//! PowerPC TextEdit records, styling, layout, drawing, and editing.

use super::graphics::*;
use super::*;

// Inside Macintosh: Text (1993), pp. 2-72 and 2-78 through 2-80. These are
// the public TERec and TEStyleRec layouts used by native PowerPC clients.
pub(crate) const PPC_TE_DEST_RECT_OFFSET: u32 = 0x00;
pub(crate) const PPC_TE_VIEW_RECT_OFFSET: u32 = 0x08;
pub(crate) const PPC_TE_SEL_RECT_OFFSET: u32 = 0x10;
pub(crate) const PPC_TE_LINE_HEIGHT_OFFSET: u32 = 0x18;
pub(crate) const PPC_TE_FONT_ASCENT_OFFSET: u32 = 0x1a;
pub(crate) const PPC_TE_SEL_POINT_OFFSET: u32 = 0x1c;
pub(crate) const PPC_TE_SEL_START_OFFSET: u32 = 0x20;
pub(crate) const PPC_TE_SEL_END_OFFSET: u32 = 0x22;
pub(crate) const PPC_TE_ACTIVE_OFFSET: u32 = 0x24;
pub(crate) const PPC_TE_CLICK_TIME_OFFSET: u32 = 0x2e;
pub(crate) const PPC_TE_CLICK_LOC_OFFSET: u32 = 0x32;
pub(crate) const PPC_TE_CARET_TIME_OFFSET: u32 = 0x34;
pub(crate) const PPC_TE_CARET_STATE_OFFSET: u32 = 0x38;
pub(crate) const PPC_TE_JUST_OFFSET: u32 = 0x3a;
pub(crate) const PPC_TE_LENGTH_OFFSET: u32 = 0x3c;
pub(crate) const PPC_TE_HTEXT_OFFSET: u32 = 0x3e;
pub(crate) const PPC_TE_TX_FONT_OFFSET: u32 = 0x4a;
pub(crate) const PPC_TE_TX_FACE_OFFSET: u32 = 0x4c;
pub(crate) const PPC_TE_TX_MODE_OFFSET: u32 = 0x4e;
pub(crate) const PPC_TE_TX_SIZE_OFFSET: u32 = 0x50;
pub(crate) const PPC_TE_IN_PORT_OFFSET: u32 = 0x52;
pub(crate) const PPC_TE_N_LINES_OFFSET: u32 = 0x5e;
pub(crate) const PPC_TE_LINE_STARTS_OFFSET: u32 = 0x60;
pub(crate) const PPC_TE_REC_MIN_SIZE: u32 = 128;
pub(crate) const PPC_TE_STYLE_N_RUNS_OFFSET: u32 = 0x00;
pub(crate) const PPC_TE_STYLE_N_STYLES_OFFSET: u32 = 0x02;
pub(crate) const PPC_TE_STYLE_TABLE_OFFSET: u32 = 0x04;
pub(crate) const PPC_TE_STYLE_LH_TABLE_OFFSET: u32 = 0x08;
pub(crate) const PPC_TE_STYLE_NULL_STYLE_OFFSET: u32 = 0x10;
pub(crate) const PPC_TE_STYLE_RUNS_OFFSET: u32 = 0x14;
pub(crate) const PPC_TE_STYLE_REC_SIZE: u32 = 0x1c;
pub(crate) const PPC_TE_ST_ELEMENT_SIZE: u32 = 0x12;
pub(crate) const PPC_TE_LH_ELEMENT_SIZE: u32 = 0x04;
pub(crate) const PPC_TE_NULL_STYLE_SCRAP_OFFSET: u32 = 0x04;
pub(crate) const PPC_TE_NULL_STYLE_REC_SIZE: u32 = 0x08;
pub(crate) const PPC_TE_STYLE_SCRAP_REC_SIZE: u32 = 0x16;
pub(crate) const PPC_TE_SCRAP_N_STYLES_OFFSET: u32 = 0x00;
pub(crate) const PPC_TE_SCRAP_STYLE_TAB_OFFSET: u32 = 0x02;
pub(crate) const PPC_TE_SCRAP_STYLE_HEIGHT_OFFSET: u32 = 0x04;
pub(crate) const PPC_TE_SCRAP_STYLE_ASCENT_OFFSET: u32 = 0x06;
pub(crate) const PPC_TE_SCRAP_STYLE_FONT_OFFSET: u32 = 0x08;
pub(crate) const PPC_TE_SCRAP_STYLE_FACE_OFFSET: u32 = 0x0a;
pub(crate) const PPC_TE_SCRAP_STYLE_SIZE_OFFSET: u32 = 0x0c;
pub(crate) const PPC_TE_SCRAP_STYLE_COLOR_OFFSET: u32 = 0x0e;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcTeResolvedStyle {
    pub(crate) font: i16,
    pub(crate) face: u8,
    pub(crate) size: i16,
    pub(crate) color: PpcRgbColor,
    pub(crate) line_height: i16,
    pub(crate) ascent: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcTeStyleRun {
    pub(crate) start: usize,
    pub(crate) style_index: usize,
    pub(crate) style: PpcTeResolvedStyle,
}

pub(super) fn ppc_te_read_rect(memory: &mut PpcSectionMem, rect_ptr: u32) -> Option<[u16; 4]> {
    Some([
        memory.read_u16_be(rect_ptr)?,
        memory.read_u16_be(rect_ptr.checked_add(2)?)?,
        memory.read_u16_be(rect_ptr.checked_add(4)?)?,
        memory.read_u16_be(rect_ptr.checked_add(6)?)?,
    ])
}

pub(super) fn ppc_te_write_rect(memory: &mut PpcSectionMem, rect_ptr: u32, rect: [u16; 4]) -> bool {
    rect.iter().copied().enumerate().all(|(index, value)| {
        memory
            .write_u16_be(rect_ptr + index as u32 * 2, value)
            .is_some()
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_te_initialize_record(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    dest_rect_ptr: u32,
    view_rect_ptr: u32,
    current_port: u32,
    tick_count: u32,
    text_mode: i16,
    text_size: i16,
    fore_color: PpcRgbColor,
    styled: bool,
) -> u32 {
    let (Some(dest_rect), Some(view_rect)) = (
        ppc_te_read_rect(memory, dest_rect_ptr),
        ppc_te_read_rect(memory, view_rect_ptr),
    ) else {
        return 0;
    };
    let te_handle = ppc_allocator_view_allocate_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        PPC_TE_REC_MIN_SIZE,
        true,
    );
    if te_handle == 0 {
        return 0;
    }
    let Some(te_ptr) = memory.read_u32_be(te_handle).filter(|ptr| *ptr != 0) else {
        ppc_te_cleanup_handles(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            &[te_handle],
        );
        return 0;
    };
    let mut allocated_handles = vec![te_handle];
    let h_text = ppc_allocator_view_allocate_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        0,
        true,
    );
    if h_text == 0 {
        ppc_te_cleanup_handles(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            &allocated_handles,
        );
        return 0;
    }
    allocated_handles.push(h_text);
    let text_font = ppc_current_text_font(memory, current_port);
    let text_face = memory
        .read_u8(current_port.wrapping_add(PPC_CGRAF_PORT_TX_FACE_OFFSET))
        .unwrap_or(0);
    let (font_face, scale) = get_font_face_scaled(text_font, text_size);
    let metrics = font_face.metrics;
    let ascent = metrics.ascent.saturating_mul(scale);
    let line_height = ascent
        .saturating_add(metrics.descent.saturating_mul(scale))
        .saturating_add(metrics.leading.saturating_mul(scale));

    let _ = ppc_te_write_rect(memory, te_ptr + PPC_TE_DEST_RECT_OFFSET, dest_rect);
    let _ = ppc_te_write_rect(memory, te_ptr + PPC_TE_VIEW_RECT_OFFSET, view_rect);
    let _ = ppc_te_write_rect(memory, te_ptr + PPC_TE_SEL_RECT_OFFSET, dest_rect);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_POINT_OFFSET, dest_rect[0]);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_POINT_OFFSET + 2, dest_rect[1]);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET, 0);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET, 0);
    let _ = memory.write_u32_be(te_ptr + PPC_TE_CARET_TIME_OFFSET, tick_count);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_CARET_STATE_OFFSET, 0);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_JUST_OFFSET, 0);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_LENGTH_OFFSET, 0);
    let _ = memory.write_u32_be(te_ptr + PPC_TE_HTEXT_OFFSET, h_text);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_TX_MODE_OFFSET, text_mode as u16);
    let _ = memory.write_u32_be(te_ptr + PPC_TE_IN_PORT_OFFSET, current_port);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_N_LINES_OFFSET, 0);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_LINE_STARTS_OFFSET, 0);

    if !styled {
        let _ = memory.write_u16_be(te_ptr + PPC_TE_LINE_HEIGHT_OFFSET, line_height as u16);
        let _ = memory.write_u16_be(te_ptr + PPC_TE_FONT_ASCENT_OFFSET, ascent as u16);
        let _ = memory.write_u16_be(te_ptr + PPC_TE_TX_FONT_OFFSET, text_font as u16);
        let _ = memory.write_u8(te_ptr + PPC_TE_TX_FACE_OFFSET, text_face);
        let _ = memory.write_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET, text_size as u16);
        return te_handle;
    }

    // Inside Macintosh: Text (1993), p. 2-78: TEStyleNew installs -1 in
    // txSize, lineHeight, and fontAscent, overlays a TEStyleHandle on
    // txFont/txFace, and creates the null style scrap used for its lifetime.
    let style_table = ppc_allocator_view_allocate_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        PPC_TE_ST_ELEMENT_SIZE,
        true,
    );
    if style_table == 0 {
        ppc_te_cleanup_handles(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            &allocated_handles,
        );
        return 0;
    }
    allocated_handles.push(style_table);
    let lh_table = ppc_allocator_view_allocate_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        PPC_TE_LH_ELEMENT_SIZE,
        true,
    );
    if lh_table == 0 {
        ppc_te_cleanup_handles(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            &allocated_handles,
        );
        return 0;
    }
    allocated_handles.push(lh_table);
    let null_scrap = ppc_allocator_view_allocate_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        PPC_TE_STYLE_SCRAP_REC_SIZE,
        true,
    );
    if null_scrap == 0 {
        ppc_te_cleanup_handles(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            &allocated_handles,
        );
        return 0;
    }
    allocated_handles.push(null_scrap);
    let null_style = ppc_allocator_view_allocate_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        PPC_TE_NULL_STYLE_REC_SIZE,
        true,
    );
    if null_style == 0 {
        ppc_te_cleanup_handles(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            &allocated_handles,
        );
        return 0;
    }
    allocated_handles.push(null_style);
    let style_handle = ppc_allocator_view_allocate_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        PPC_TE_STYLE_REC_SIZE,
        true,
    );
    if style_handle == 0 {
        ppc_te_cleanup_handles(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            &allocated_handles,
        );
        return 0;
    }
    allocated_handles.push(style_handle);

    if let Some(style_table_ptr) = memory.read_u32_be(style_table).filter(|ptr| *ptr != 0) {
        let _ = memory.write_u16_be(style_table_ptr, 1);
        let _ = memory.write_u16_be(style_table_ptr + 2, line_height as u16);
        let _ = memory.write_u16_be(style_table_ptr + 4, ascent as u16);
        let _ = memory.write_u16_be(style_table_ptr + 6, text_font as u16);
        let _ = memory.write_u8(style_table_ptr + 8, text_face);
        let _ = memory.write_u16_be(style_table_ptr + 10, text_size as u16);
        let _ = memory.write_u16_be(style_table_ptr + 12, fore_color.red);
        let _ = memory.write_u16_be(style_table_ptr + 14, fore_color.green);
        let _ = memory.write_u16_be(style_table_ptr + 16, fore_color.blue);
    }
    if let Some(lh_table_ptr) = memory.read_u32_be(lh_table).filter(|ptr| *ptr != 0) {
        let _ = memory.write_u16_be(lh_table_ptr, line_height as u16);
        let _ = memory.write_u16_be(lh_table_ptr + 2, ascent as u16);
    }
    if let Some(scrap_ptr) = memory.read_u32_be(null_scrap).filter(|ptr| *ptr != 0) {
        let _ = memory.write_u16_be(scrap_ptr, 0);
        let _ = memory.write_u32_be(scrap_ptr + 2, 0);
        let _ = memory.write_u16_be(scrap_ptr + 6, line_height as u16);
        let _ = memory.write_u16_be(scrap_ptr + 8, ascent as u16);
        let _ = memory.write_u16_be(scrap_ptr + 10, text_font as u16);
        let _ = memory.write_u8(scrap_ptr + 12, text_face);
        let _ = memory.write_u16_be(scrap_ptr + 14, text_size as u16);
        let _ = memory.write_u16_be(scrap_ptr + 16, fore_color.red);
        let _ = memory.write_u16_be(scrap_ptr + 18, fore_color.green);
        let _ = memory.write_u16_be(scrap_ptr + 20, fore_color.blue);
    }
    if let Some(null_style_ptr) = memory.read_u32_be(null_style).filter(|ptr| *ptr != 0) {
        let _ = memory.write_u32_be(null_style_ptr + PPC_TE_NULL_STYLE_SCRAP_OFFSET, null_scrap);
    }
    if let Some(style_ptr) = memory.read_u32_be(style_handle).filter(|ptr| *ptr != 0) {
        let _ = memory.write_u16_be(style_ptr + PPC_TE_STYLE_N_RUNS_OFFSET, 1);
        let _ = memory.write_u16_be(style_ptr + PPC_TE_STYLE_N_STYLES_OFFSET, 1);
        let _ = memory.write_u32_be(style_ptr + PPC_TE_STYLE_TABLE_OFFSET, style_table);
        let _ = memory.write_u32_be(style_ptr + PPC_TE_STYLE_LH_TABLE_OFFSET, lh_table);
        let _ = memory.write_u32_be(style_ptr + PPC_TE_STYLE_NULL_STYLE_OFFSET, null_style);
        let _ = memory.write_u16_be(style_ptr + PPC_TE_STYLE_RUNS_OFFSET, 0);
        let _ = memory.write_u16_be(style_ptr + PPC_TE_STYLE_RUNS_OFFSET + 2, 0);
        let _ = memory.write_u16_be(style_ptr + PPC_TE_STYLE_RUNS_OFFSET + 4, 1);
        let _ = memory.write_u16_be(style_ptr + PPC_TE_STYLE_RUNS_OFFSET + 6, 0xffff);
    }
    let _ = memory.write_u16_be(te_ptr + PPC_TE_LINE_HEIGHT_OFFSET, 0xffff);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_FONT_ASCENT_OFFSET, 0xffff);
    let _ = memory.write_u32_be(te_ptr + PPC_TE_TX_FONT_OFFSET, style_handle);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET, 0xffff);
    te_handle
}

pub(super) fn ppc_te_record_ptr(memory: &mut PpcSectionMem, te_handle: u32) -> Option<u32> {
    memory.read_u32_be(te_handle).filter(|ptr| *ptr != 0)
}

pub(super) fn ppc_te_text_bytes(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    te_handle: u32,
) -> Option<Vec<u8>> {
    let te_ptr = ppc_te_record_ptr(memory, te_handle)?;
    let text_handle = memory.read_u32_be(te_ptr + PPC_TE_HTEXT_OFFSET)?;
    let text_ptr = memory.read_u32_be(text_handle)?;
    let length = u32::from(memory.read_u16_be(te_ptr + PPC_TE_LENGTH_OFFSET)?);
    ppc_memory_read_bytes(memory, text_ptr, length).or_else(|| {
        handles
            .iter()
            .find(|record| record.handle == text_handle && record.size == 0)
            .map(|_| Vec::new())
    })
}

pub(super) fn ppc_te_font_lookup_size(size: i16) -> i16 {
    if size == 0 {
        PPC_QD_TEXT_SIZE_SYSTEM
    } else {
        size.max(1)
    }
}

pub(super) fn ppc_te_resolved_style_from_parts(
    font: i16,
    face: u8,
    size: i16,
    color: PpcRgbColor,
    line_height: i16,
    ascent: i16,
) -> PpcTeResolvedStyle {
    let size = ppc_te_font_lookup_size(size);
    let metrics = get_font_metrics(font, size);
    let fallback_height = metrics
        .ascent
        .saturating_add(metrics.descent)
        .saturating_add(metrics.leading);
    PpcTeResolvedStyle {
        font,
        face,
        size,
        color,
        line_height: if line_height > 0 {
            line_height
        } else {
            fallback_height.max(1)
        },
        ascent: if ascent > 0 {
            ascent
        } else {
            metrics.ascent.max(0)
        },
    }
}

pub(super) fn ppc_te_style_from_table_element(
    memory: &mut PpcSectionMem,
    style_ptr: u32,
) -> Option<PpcTeResolvedStyle> {
    Some(ppc_te_resolved_style_from_parts(
        memory.read_u16_be(style_ptr + 6)? as i16,
        memory.read_u8(style_ptr + 8)?,
        memory.read_u16_be(style_ptr + 10)? as i16,
        PpcRgbColor {
            red: memory.read_u16_be(style_ptr + 12)?,
            green: memory.read_u16_be(style_ptr + 14)?,
            blue: memory.read_u16_be(style_ptr + 16)?,
        },
        memory.read_u16_be(style_ptr + 2)? as i16,
        memory.read_u16_be(style_ptr + 4)? as i16,
    ))
}

pub(super) fn ppc_te_null_style_scrap_handle(memory: &mut PpcSectionMem, te_handle: u32) -> u32 {
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return 0;
    };
    let style_handle = memory
        .read_u32_be(te_ptr + PPC_TE_TX_FONT_OFFSET)
        .unwrap_or(0);
    let style_ptr = memory
        .read_u32_be(style_handle)
        .filter(|ptr| *ptr != 0)
        .unwrap_or(0);
    let null_style_handle = memory
        .read_u32_be(style_ptr + PPC_TE_STYLE_NULL_STYLE_OFFSET)
        .unwrap_or(0);
    let null_style_ptr = memory
        .read_u32_be(null_style_handle)
        .filter(|ptr| *ptr != 0)
        .unwrap_or(0);
    memory
        .read_u32_be(null_style_ptr + PPC_TE_NULL_STYLE_SCRAP_OFFSET)
        .unwrap_or(0)
}

pub(super) fn ppc_te_null_style_resolved_style(
    memory: &mut PpcSectionMem,
    te_handle: u32,
) -> Option<PpcTeResolvedStyle> {
    let scrap_handle = ppc_te_null_style_scrap_handle(memory, te_handle);
    let scrap_ptr = memory.read_u32_be(scrap_handle).filter(|ptr| *ptr != 0)?;
    if memory.read_u16_be(scrap_ptr + PPC_TE_SCRAP_N_STYLES_OFFSET)? == 0 {
        return None;
    }
    let element = scrap_ptr + PPC_TE_SCRAP_STYLE_TAB_OFFSET;
    Some(ppc_te_resolved_style_from_parts(
        memory.read_u16_be(element + PPC_TE_SCRAP_STYLE_FONT_OFFSET)? as i16,
        memory.read_u8(element + PPC_TE_SCRAP_STYLE_FACE_OFFSET)?,
        memory.read_u16_be(element + PPC_TE_SCRAP_STYLE_SIZE_OFFSET)? as i16,
        PpcRgbColor {
            red: memory.read_u16_be(element + PPC_TE_SCRAP_STYLE_COLOR_OFFSET)?,
            green: memory.read_u16_be(element + PPC_TE_SCRAP_STYLE_COLOR_OFFSET + 2)?,
            blue: memory.read_u16_be(element + PPC_TE_SCRAP_STYLE_COLOR_OFFSET + 4)?,
        },
        memory.read_u16_be(element + PPC_TE_SCRAP_STYLE_HEIGHT_OFFSET)? as i16,
        memory.read_u16_be(element + PPC_TE_SCRAP_STYLE_ASCENT_OFFSET)? as i16,
    ))
}

pub(super) fn ppc_te_set_null_style(
    memory: &mut PpcSectionMem,
    te_handle: u32,
    mode: u16,
    text_style_ptr: u32,
) -> bool {
    if text_style_ptr == 0 {
        return false;
    }
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return false;
    };
    if memory.read_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET) != Some(0xffff) {
        return false;
    }
    let scrap_handle = ppc_te_null_style_scrap_handle(memory, te_handle);
    let Some(scrap_ptr) = memory.read_u32_be(scrap_handle).filter(|ptr| *ptr != 0) else {
        return false;
    };
    let element = scrap_ptr + PPC_TE_SCRAP_STYLE_TAB_OFFSET;
    let source_font = memory.read_u16_be(text_style_ptr).unwrap_or(0);
    let source_face = memory.read_u8(text_style_ptr + 2).unwrap_or(0);
    let source_size = memory.read_u16_be(text_style_ptr + 4).unwrap_or(0);
    let source_color = [
        memory.read_u16_be(text_style_ptr + 6).unwrap_or(0),
        memory.read_u16_be(text_style_ptr + 8).unwrap_or(0),
        memory.read_u16_be(text_style_ptr + 10).unwrap_or(0),
    ];
    let _ = memory.write_u16_be(scrap_ptr + PPC_TE_SCRAP_N_STYLES_OFFSET, 1);
    if mode & 0x0001 != 0 {
        let _ = memory.write_u16_be(
            element + PPC_TE_SCRAP_STYLE_FONT_OFFSET,
            source_font,
        );
    }
    if mode & 0x0002 != 0 {
        let _ = memory.write_u8(
            element + PPC_TE_SCRAP_STYLE_FACE_OFFSET,
            source_face,
        );
    }
    if mode & 0x0004 != 0 {
        let _ = memory.write_u16_be(
            element + PPC_TE_SCRAP_STYLE_SIZE_OFFSET,
            source_size,
        );
    }
    if mode & 0x0008 != 0 {
        for (offset, color) in [0u32, 2, 4].into_iter().zip(source_color) {
            let _ = memory.write_u16_be(
                element + PPC_TE_SCRAP_STYLE_COLOR_OFFSET + offset,
                color,
            );
        }
    }
    let font = memory
        .read_u16_be(element + PPC_TE_SCRAP_STYLE_FONT_OFFSET)
        .unwrap_or(0) as i16;
    let size = ppc_te_font_lookup_size(
        memory
            .read_u16_be(element + PPC_TE_SCRAP_STYLE_SIZE_OFFSET)
            .unwrap_or(0) as i16,
    );
    let metrics = get_font_metrics(font, size);
    let _ = memory.write_u16_be(
        element + PPC_TE_SCRAP_STYLE_HEIGHT_OFFSET,
        metrics
            .ascent
            .saturating_add(metrics.descent)
            .saturating_add(metrics.leading) as u16,
    );
    let _ = memory.write_u16_be(
        element + PPC_TE_SCRAP_STYLE_ASCENT_OFFSET,
        metrics.ascent as u16,
    );
    true
}

pub(super) fn ppc_te_fallback_style(memory: &mut PpcSectionMem, te_ptr: u32) -> PpcTeResolvedStyle {
    let font = memory
        .read_u16_be(te_ptr + PPC_TE_TX_FONT_OFFSET)
        .unwrap_or(PPC_QD_TEXT_FONT_DEFAULT as u16) as i16;
    let size = memory
        .read_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET)
        .unwrap_or(PPC_QD_TEXT_SIZE_SYSTEM as u16) as i16;
    let face = memory.read_u8(te_ptr + PPC_TE_TX_FACE_OFFSET).unwrap_or(0);
    let size = ppc_te_font_lookup_size(size);
    let (font_face, scale) = get_font_face_scaled(font, size);
    let ascent = font_face.metrics.ascent.saturating_mul(scale);
    let line_height = ascent
        .saturating_add(font_face.metrics.descent.saturating_mul(scale))
        .saturating_add(font_face.metrics.leading.saturating_mul(scale));
    ppc_te_resolved_style_from_parts(font, face, size, PPC_RGB_BLACK, line_height, ascent)
}

pub(super) fn ppc_te_style_at_offset(runs: &[PpcTeStyleRun], offset: usize) -> PpcTeResolvedStyle {
    let mut style = runs
        .first()
        .map(|run| run.style)
        .unwrap_or_else(|| ppc_te_resolved_style_from_parts(0, 0, 0, PPC_RGB_BLACK, 0, 0));
    for run in runs {
        if run.start > offset {
            break;
        }
        style = run.style;
    }
    style
}

pub(super) fn ppc_te_style_runs(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    te_handle: u32,
    text_len: usize,
) -> Vec<PpcTeStyleRun> {
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return Vec::new();
    };
    let fallback = ppc_te_fallback_style(memory, te_ptr);
    if memory.read_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET) != Some(0xffff) {
        return vec![PpcTeStyleRun {
            start: 0,
            style_index: 0,
            style: fallback,
        }];
    }
    let style_handle = memory
        .read_u32_be(te_ptr + PPC_TE_TX_FONT_OFFSET)
        .unwrap_or(0);
    let Some(style_ptr) = memory.read_u32_be(style_handle).filter(|ptr| *ptr != 0) else {
        return vec![PpcTeStyleRun {
            start: 0,
            style_index: 0,
            style: fallback,
        }];
    };
    let Some(style_table_handle) = memory
        .read_u32_be(style_ptr + PPC_TE_STYLE_TABLE_OFFSET)
        .filter(|handle| *handle != 0)
    else {
        return vec![PpcTeStyleRun {
            start: 0,
            style_index: 0,
            style: fallback,
        }];
    };
    let Some(style_table_ptr) = memory
        .read_u32_be(style_table_handle)
        .filter(|ptr| *ptr != 0)
    else {
        return vec![PpcTeStyleRun {
            start: 0,
            style_index: 0,
            style: fallback,
        }];
    };
    let n_runs = usize::from(
        memory
            .read_u16_be(style_ptr + PPC_TE_STYLE_N_RUNS_OFFSET)
            .unwrap_or(0),
    );
    let n_styles = usize::from(
        memory
            .read_u16_be(style_ptr + PPC_TE_STYLE_N_STYLES_OFFSET)
            .unwrap_or(0),
    );
    let max_runs = handles
        .iter()
        .find(|record| record.handle == style_handle)
        .map(|record| {
            usize::try_from(record.size.saturating_sub(PPC_TE_STYLE_RUNS_OFFSET) / 4)
                .unwrap_or(usize::MAX)
        })
        .unwrap_or(n_runs);
    let max_styles = handles
        .iter()
        .find(|record| record.handle == style_table_handle)
        .map(|record| usize::try_from(record.size / PPC_TE_ST_ELEMENT_SIZE).unwrap_or(usize::MAX))
        .unwrap_or(n_styles);
    let run_count = n_runs.min(max_runs).min(4096);
    let style_count = n_styles.min(max_styles).min(4096);
    if run_count == 0 || style_count == 0 {
        return vec![PpcTeStyleRun {
            start: 0,
            style_index: 0,
            style: fallback,
        }];
    }

    let mut runs = Vec::with_capacity(run_count);
    for run_index in 0..run_count {
        let run_ptr = style_ptr + PPC_TE_STYLE_RUNS_OFFSET + (run_index as u32 * 4);
        let style_index_word = memory.read_u16_be(run_ptr + 2).unwrap_or(0xffff);
        if style_index_word == 0xffff {
            break;
        }
        let style_index = usize::from(style_index_word);
        if style_index >= style_count {
            continue;
        }
        let element_ptr = style_table_ptr + style_index as u32 * PPC_TE_ST_ELEMENT_SIZE;
        let Some(style) = ppc_te_style_from_table_element(memory, element_ptr) else {
            continue;
        };
        runs.push(PpcTeStyleRun {
            start: usize::from(memory.read_u16_be(run_ptr).unwrap_or(0)).min(text_len),
            style_index,
            style,
        });
    }
    if runs.is_empty() {
        return vec![PpcTeStyleRun {
            start: 0,
            style_index: 0,
            style: fallback,
        }];
    }
    runs.sort_by_key(|run| run.start);
    let mut folded: Vec<PpcTeStyleRun> = Vec::with_capacity(runs.len() + 1);
    for run in runs {
        if let Some(last) = folded.last_mut() {
            if last.start == run.start {
                *last = run;
                continue;
            }
            if last.style == run.style {
                continue;
            }
        }
        folded.push(run);
    }
    if folded.first().is_none_or(|run| run.start != 0) {
        folded.insert(
            0,
            PpcTeStyleRun {
                start: 0,
                style_index: 0,
                style: fallback,
            },
        );
    }
    folded
}

pub(super) fn ppc_te_measure_text_width_styled(
    runs: &[PpcTeStyleRun],
    text: &[u8],
    start: usize,
    end: usize,
) -> i16 {
    let start = start.min(text.len());
    let end = end.min(text.len());
    text[start..end]
        .iter()
        .enumerate()
        .fold(0i16, |width, (index, byte)| {
            let style = ppc_te_style_at_offset(runs, start + index);
            width.saturating_add(ppc_text_width_bytes(
                style.font,
                style.size,
                style.face,
                &[*byte],
            ))
        })
}

pub(super) fn ppc_te_line_metrics_for_range(runs: &[PpcTeStyleRun], start: usize, end: usize) -> (i16, i16) {
    if start >= end {
        let style = ppc_te_style_at_offset(runs, start);
        return (style.line_height, style.ascent);
    }
    let mut line_height = 1i16;
    let mut ascent = 0i16;
    for offset in start..end {
        let style = ppc_te_style_at_offset(runs, offset);
        line_height = line_height.max(style.line_height);
        ascent = ascent.max(style.ascent);
    }
    (line_height, ascent)
}

pub(super) fn ppc_te_primary_font_and_size(memory: &mut PpcSectionMem, te_ptr: u32) -> (i16, i16) {
    if memory.read_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET) == Some(0xffff) {
        let style_handle = memory
            .read_u32_be(te_ptr + PPC_TE_TX_FONT_OFFSET)
            .unwrap_or(0);
        let style_table = memory
            .read_u32_be(style_handle)
            .filter(|ptr| *ptr != 0)
            .and_then(|style_ptr| memory.read_u32_be(style_ptr + PPC_TE_STYLE_TABLE_OFFSET))
            .unwrap_or(0);
        if let Some(table_ptr) = memory.read_u32_be(style_table).filter(|ptr| *ptr != 0) {
            return (
                memory.read_u16_be(table_ptr + 6).unwrap_or(0) as i16,
                memory.read_u16_be(table_ptr + 10).unwrap_or(0) as i16,
            );
        }
    }
    (
        memory
            .read_u16_be(te_ptr + PPC_TE_TX_FONT_OFFSET)
            .unwrap_or(0) as i16,
        memory
            .read_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET)
            .unwrap_or(0) as i16,
    )
}

pub(super) fn ppc_te_recalculate_layout(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    te_handle: u32,
) -> i16 {
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return PPC_NIL_HANDLE_ERR;
    };
    let Some(text) = ppc_te_text_bytes(memory, handles, te_handle) else {
        return PPC_PARAM_ERR;
    };
    let (_, left, _, right) =
        ppc_read_rect(memory, te_ptr + PPC_TE_DEST_RECT_OFFSET).unwrap_or((0, 0, 0, i16::MAX));
    let max_width = right.saturating_sub(left).max(1);
    let styled = memory.read_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET) == Some(0xffff);
    let style_runs = if styled {
        ppc_te_style_runs(memory, handles, te_handle, text.len())
    } else {
        Vec::new()
    };
    let (font, size) = ppc_te_primary_font_and_size(memory, te_ptr);
    let lines = crate::quickdraw::text::wrap_classic_text(&text, max_width, |index, byte| {
        if styled {
            let style = ppc_te_style_at_offset(&style_runs, index);
            ppc_text_width_bytes(style.font, style.size, style.face, &[byte])
        } else {
            ppc_text_byte_advance_for_font(byte, font, size)
        }
    });
    let starts = lines
        .iter()
        .map(|line| line.start.min(u16::MAX as usize) as u16)
        .collect::<Vec<_>>();
    let required_size = PPC_TE_REC_MIN_SIZE.max(
        PPC_TE_LINE_STARTS_OFFSET
            .saturating_add((starts.len() as u32).saturating_add(2).saturating_mul(2)),
    );
    let result = ppc_allocator_view_resize_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        te_handle,
        required_size,
    );
    if result != PPC_NO_ERR {
        return result;
    }
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return PPC_PARAM_ERR;
    };
    let _ = memory.write_u16_be(
        te_ptr + PPC_TE_N_LINES_OFFSET,
        starts.len().min(u16::MAX as usize) as u16,
    );
    for (index, start) in starts.iter().copied().enumerate() {
        let _ = memory.write_u16_be(te_ptr + PPC_TE_LINE_STARTS_OFFSET + index as u32 * 2, start);
    }
    let _ = memory.write_u16_be(
        te_ptr + PPC_TE_LINE_STARTS_OFFSET + starts.len() as u32 * 2,
        text.len().min(u16::MAX as usize) as u16,
    );
    let _ = memory.write_u16_be(
        te_ptr + PPC_TE_LINE_STARTS_OFFSET + starts.len().saturating_add(1) as u32 * 2,
        0,
    );
    if styled {
        let style_handle = memory
            .read_u32_be(te_ptr + PPC_TE_TX_FONT_OFFSET)
            .unwrap_or(0);
        if let Some(style_ptr) = memory.read_u32_be(style_handle).filter(|ptr| *ptr != 0) {
            let run_count = usize::from(
                memory
                    .read_u16_be(style_ptr + PPC_TE_STYLE_N_RUNS_OFFSET)
                    .unwrap_or(0),
            );
            let _ = memory.write_u16_be(
                style_ptr
                    .saturating_add(PPC_TE_STYLE_RUNS_OFFSET)
                    .saturating_add((run_count as u32).saturating_mul(4)),
                    text.len()
                        .saturating_add(1)
                        .min(u16::MAX as usize) as u16,
            );
            let _ = memory.write_u16_be(
                style_ptr
                    .saturating_add(PPC_TE_STYLE_RUNS_OFFSET)
                    .saturating_add((run_count as u32).saturating_mul(4))
                    .saturating_add(2),
                0xffff,
            );

            let lh_handle = memory
                .read_u32_be(style_ptr + PPC_TE_STYLE_LH_TABLE_OFFSET)
                .unwrap_or(0);
            if lh_handle != 0 {
                let required_lh_size = (lines.len().saturating_add(1) as u32)
                    .saturating_mul(PPC_TE_LH_ELEMENT_SIZE);
                let result = ppc_allocator_view_resize_handle(
                    allocator.as_deref_mut(),
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    lh_handle,
                    required_lh_size,
                );
                if result == PPC_NO_ERR {
                    if let Some(lh_ptr) = memory.read_u32_be(lh_handle).filter(|ptr| *ptr != 0) {
                        for line_index in 0..=lines.len() {
                            let (line_height, ascent) = if let Some(line) = lines.get(line_index) {
                                ppc_te_line_metrics_for_range(
                                    &style_runs,
                                    line.start.min(text.len()),
                                    line.next.min(text.len()),
                                )
                            } else {
                                ppc_te_line_metrics_for_range(
                                    &style_runs,
                                    text.len(),
                                    text.len(),
                                )
                            };
                            let offset =
                                lh_ptr.saturating_add(line_index as u32 * PPC_TE_LH_ELEMENT_SIZE);
                            let _ = memory.write_u16_be(offset, line_height as u16);
                            let _ = memory.write_u16_be(offset + 2, ascent as u16);
                        }
                    }
                }
            }
        }
    }
    PPC_NO_ERR
}

pub(super) fn ppc_te_write_style_table_element(
    memory: &mut PpcSectionMem,
    style_ptr: u32,
    style: PpcTeResolvedStyle,
) {
    let _ = memory.write_u16_be(style_ptr, 1);
    let _ = memory.write_u16_be(style_ptr + 2, style.line_height as u16);
    let _ = memory.write_u16_be(style_ptr + 4, style.ascent as u16);
    let _ = memory.write_u16_be(style_ptr + 6, style.font as u16);
    let _ = memory.write_u8(style_ptr + 8, style.face);
    let _ = memory.write_u16_be(style_ptr + 10, style.size as u16);
    let _ = memory.write_u16_be(style_ptr + 12, style.color.red);
    let _ = memory.write_u16_be(style_ptr + 14, style.color.green);
    let _ = memory.write_u16_be(style_ptr + 16, style.color.blue);
}

pub(super) fn ppc_te_set_style_for_range(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    te_handle: u32,
    range_start: usize,
    range_end: usize,
    mode: u16,
    text_style_ptr: u32,
) -> bool {
    if text_style_ptr == 0 {
        return false;
    }
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return false;
    };
    if memory.read_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET) != Some(0xffff) {
        return false;
    }
    let Some(text) = ppc_te_text_bytes(memory, handles, te_handle) else {
        return false;
    };
    let text_len = text.len();
    let range_start = range_start.min(text_len);
    let range_end = range_end.min(text_len);
    if range_start >= range_end {
        return false;
    }
    let existing_runs = ppc_te_style_runs(memory, handles, te_handle, text_len);
    if existing_runs.is_empty() {
        return false;
    }
    let requested_face = memory.read_u8(text_style_ptr + 2).unwrap_or(0);
    let toggle_face = mode & 0x0020 != 0 && mode & 0x0002 != 0;
    let remove_toggled_face = toggle_face
        && existing_runs.iter().enumerate().all(|(index, run)| {
            let run_end = existing_runs
                .get(index + 1)
                .map(|next| next.start)
                .unwrap_or(text_len);
            run_end <= range_start
                || run.start >= range_end
                || run.style.face & requested_face == requested_face
        });
    let mut transform = |mut style: PpcTeResolvedStyle| {
        if mode & 0x0001 != 0 {
            style.font = memory.read_u16_be(text_style_ptr).unwrap_or(0) as i16;
        }
        if mode & 0x0002 != 0 {
            style.face = if toggle_face {
                if remove_toggled_face {
                    style.face & !requested_face
                } else {
                    style.face | requested_face
                }
            } else {
                requested_face
            };
        }
        if mode & 0x0010 != 0 {
            let delta = memory.read_u16_be(text_style_ptr + 4).unwrap_or(0) as i16;
            style.size = style.size.saturating_add(delta).max(1);
        } else if mode & 0x0004 != 0 {
            style.size = memory.read_u16_be(text_style_ptr + 4).unwrap_or(0) as i16;
        }
        if mode & 0x0008 != 0 {
            style.color = PpcRgbColor {
                red: memory
                    .read_u16_be(text_style_ptr + 6)
                    .unwrap_or(style.color.red),
                green: memory
                    .read_u16_be(text_style_ptr + 8)
                    .unwrap_or(style.color.green),
                blue: memory
                    .read_u16_be(text_style_ptr + 10)
                    .unwrap_or(style.color.blue),
            };
        }
        let metrics = get_font_metrics(style.font, ppc_te_font_lookup_size(style.size));
        style.size = ppc_te_font_lookup_size(style.size);
        style.line_height = metrics
            .ascent
            .saturating_add(metrics.descent)
            .saturating_add(metrics.leading)
            .max(1);
        style.ascent = metrics.ascent.max(0);
        style
    };
    let old_range_end_style = ppc_te_style_at_offset(&existing_runs, range_end);
    let mut merged = Vec::with_capacity(existing_runs.len() + 2);
    for run in &existing_runs {
        if run.start < range_start || run.start >= range_end {
            merged.push((run.start, run.style));
        } else {
            merged.push((run.start, transform(run.style)));
        }
    }
    if !existing_runs.iter().any(|run| run.start == range_start) {
        merged.push((
            range_start,
            transform(ppc_te_style_at_offset(&existing_runs, range_start)),
        ));
    }
    if range_end < text_len && !existing_runs.iter().any(|run| run.start == range_end) {
        merged.push((range_end, old_range_end_style));
    }
    merged.sort_by_key(|(start, _)| *start);
    let mut folded = Vec::with_capacity(merged.len());
    for (start, style) in merged {
        if let Some((last_start, last_style)) = folded.last_mut() {
            if *last_start == start {
                *last_style = style;
                continue;
            }
            if *last_style == style {
                continue;
            }
        }
        folded.push((start, style));
    }
    if folded.first().is_none_or(|(start, _)| *start != 0) {
        folded.insert(0, (0, ppc_te_style_at_offset(&existing_runs, 0)));
    }
    let run_count = folded.len().min(u16::MAX as usize);
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return false;
    };
    let style_handle = memory
        .read_u32_be(te_ptr + PPC_TE_TX_FONT_OFFSET)
        .unwrap_or(0);
    if style_handle == 0 {
        return false;
    }
    let required_style_size =
        PPC_TE_STYLE_RUNS_OFFSET.saturating_add((run_count as u32 + 1).saturating_mul(4));
    if ppc_allocator_view_resize_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        style_handle,
        required_style_size,
    ) != PPC_NO_ERR
    {
        return false;
    }
    let Some(style_ptr) = memory.read_u32_be(style_handle).filter(|ptr| *ptr != 0) else {
        return false;
    };
    let mut style_table_handle = memory
        .read_u32_be(style_ptr + PPC_TE_STYLE_TABLE_OFFSET)
        .unwrap_or(0);
    if style_table_handle == 0 {
        style_table_handle = ppc_allocator_view_allocate_handle(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            (run_count as u32).saturating_mul(PPC_TE_ST_ELEMENT_SIZE),
            true,
        );
        if style_table_handle == 0 {
            return false;
        }
        let _ = memory.write_u32_be(style_ptr + PPC_TE_STYLE_TABLE_OFFSET, style_table_handle);
    }
    if ppc_allocator_view_resize_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        style_table_handle,
        (run_count as u32).saturating_mul(PPC_TE_ST_ELEMENT_SIZE),
    ) != PPC_NO_ERR
    {
        return false;
    }
    let Some(style_ptr) = memory.read_u32_be(style_handle).filter(|ptr| *ptr != 0) else {
        return false;
    };
    let Some(style_table_ptr) = memory
        .read_u32_be(style_table_handle)
        .filter(|ptr| *ptr != 0)
    else {
        return false;
    };
    let _ = memory.write_u16_be(style_ptr + PPC_TE_STYLE_N_RUNS_OFFSET, run_count as u16);
    let _ = memory.write_u16_be(style_ptr + PPC_TE_STYLE_N_STYLES_OFFSET, run_count as u16);
    for (index, (start, style)) in folded.iter().take(run_count).enumerate() {
        ppc_te_write_style_table_element(
            memory,
            style_table_ptr + index as u32 * PPC_TE_ST_ELEMENT_SIZE,
            *style,
        );
        let run_ptr = style_ptr + PPC_TE_STYLE_RUNS_OFFSET + index as u32 * 4;
        let _ = memory.write_u16_be(run_ptr, (*start).min(u16::MAX as usize) as u16);
        let _ = memory.write_u16_be(run_ptr + 2, index as u16);
    }
    let sentinel = style_ptr + PPC_TE_STYLE_RUNS_OFFSET + run_count as u32 * 4;
    let _ = memory.write_u16_be(
        sentinel,
        text_len.saturating_add(1).min(u16::MAX as usize) as u16,
    );
    let _ = memory.write_u16_be(sentinel + 2, 0xffff);
    true
}

pub(super) fn ppc_te_continuous_style(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    mode_ptr: u32,
    style_ptr: u32,
    te_handle: u32,
) -> bool {
    if mode_ptr == 0 || style_ptr == 0 {
        return false;
    }
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return false;
    };
    let Some(text) = ppc_te_text_bytes(memory, handles, te_handle) else {
        return false;
    };
    let requested_mode = memory.read_u16_be(mode_ptr).unwrap_or(0);
    let mut start = usize::from(
        memory
            .read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET)
            .unwrap_or(0),
    );
    let mut end = usize::from(
        memory
            .read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET)
            .unwrap_or(0),
    );
    if end < start {
        std::mem::swap(&mut start, &mut end);
    }
    start = start.min(text.len());
    end = end.min(text.len());
    let runs = ppc_te_style_runs(memory, handles, te_handle, text.len());
    let first = if start == end {
        ppc_te_null_style_resolved_style(memory, te_handle).unwrap_or_else(|| {
            ppc_te_style_at_offset(
                &runs,
                start.saturating_sub(1).min(text.len().saturating_sub(1)),
            )
        })
    } else {
        ppc_te_style_at_offset(&runs, start)
    };
    let mut common_mode = requested_mode;
    let mut common_face = first.face;
    let mut all_faces_equal = true;
    if start < end {
        for offset in start..end {
            let style = ppc_te_style_at_offset(&runs, offset);
            if requested_mode & 0x0001 != 0 && style.font != first.font {
                common_mode &= !0x0001;
            }
            if requested_mode & 0x0002 != 0 {
                if style.face != first.face {
                    all_faces_equal = false;
                }
                common_face &= style.face;
            }
            if requested_mode & 0x0004 != 0 && style.size != first.size {
                common_mode &= !0x0004;
            }
            if requested_mode & 0x0008 != 0 && style.color != first.color {
                common_mode &= !0x0008;
            }
        }
    }
    if requested_mode & 0x0002 != 0 && !all_faces_equal && common_face == 0 {
        common_mode &= !0x0002;
    }
    let _ = memory.write_u16_be(mode_ptr, common_mode);
    if requested_mode & 0x0001 != 0 {
        let _ = memory.write_u16_be(style_ptr, first.font as u16);
    }
    if requested_mode & 0x0002 != 0 {
        let _ = memory.write_u8(style_ptr + 2, common_face);
    }
    if requested_mode & 0x0004 != 0 {
        let _ = memory.write_u16_be(style_ptr + 4, first.size as u16);
    }
    if requested_mode & 0x0008 != 0 {
        let _ = memory.write_u16_be(style_ptr + 6, first.color.red);
        let _ = memory.write_u16_be(style_ptr + 8, first.color.green);
        let _ = memory.write_u16_be(style_ptr + 10, first.color.blue);
    }
    common_mode == requested_mode
}

pub(super) fn ppc_measure_text(
    memory: &mut PpcSectionMem,
    count: i16,
    text_ptr: u32,
    char_locs_ptr: u32,
    text_font: i16,
    text_size: i16,
    text_face: u8,
) {
    if count < 0 || char_locs_ptr == 0 {
        return;
    }
    let count = count as u32;
    let mut width = 0i16;
    let _ = memory.write_u16_be(char_locs_ptr, 0);
    for index in 0..count {
        let byte = memory.read_u8(text_ptr.saturating_add(index)).unwrap_or(0);
        width = width.saturating_add(ppc_text_width_bytes(
            text_font,
            text_size,
            text_face,
            &[byte],
        ));
        let _ = memory.write_u16_be(
            char_locs_ptr.saturating_add((index + 1).saturating_mul(2)),
            width as u16,
        );
    }
}

pub(super) fn ppc_te_set_text(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    te_handle: u32,
    text: &[u8],
) -> i16 {
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return PPC_NIL_HANDLE_ERR;
    };
    let text_handle = memory
        .read_u32_be(te_ptr + PPC_TE_HTEXT_OFFSET)
        .unwrap_or(0);
    if text_handle == 0 {
        return PPC_NIL_HANDLE_ERR;
    }
    let length = text.len().min(i16::MAX as usize);
    let result = ppc_allocator_view_resize_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        text_handle,
        length as u32,
    );
    if result != PPC_NO_ERR {
        return result;
    }
    let Some(text_ptr) = memory.read_u32_be(text_handle) else {
        return PPC_PARAM_ERR;
    };
    if !text.is_empty() && memory.write_bytes(text_ptr, &text[..length]).is_none() {
        return PPC_PARAM_ERR;
    }
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return PPC_PARAM_ERR;
    };
    let _ = memory.write_u16_be(te_ptr + PPC_TE_LENGTH_OFFSET, length as u16);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET, length as u16);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET, length as u16);
    ppc_te_recalculate_layout(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        te_handle,
    )
}

pub(super) fn ppc_te_replace_selection(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    te_handle: u32,
    inserted: &[u8],
) -> i16 {
    let Some(mut buffer) = ppc_te_edit_buffer(memory, handles, te_handle) else {
        return PPC_NIL_HANDLE_ERR;
    };
    buffer.replace_selection(inserted);
    ppc_te_commit_edit_buffer(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        te_handle,
        &buffer,
    )
}

pub(super) fn ppc_te_edit_buffer(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    te_handle: u32,
) -> Option<crate::text_edit::TextEditBuffer> {
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return None;
    };
    Some(crate::text_edit::TextEditBuffer::new(
        ppc_te_text_bytes(memory, handles, te_handle)?,
        usize::from(memory.read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET)?),
        usize::from(memory.read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET)?),
    ))
}

pub(super) fn ppc_te_commit_edit_buffer(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    te_handle: u32,
    buffer: &crate::text_edit::TextEditBuffer,
) -> i16 {
    let result = ppc_te_set_text(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        te_handle,
        buffer.text(),
    );
    if result != PPC_NO_ERR {
        return result;
    }
    if let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) {
        let selection = buffer.selection();
        let _ = memory.write_u16_be(
            te_ptr + PPC_TE_SEL_START_OFFSET,
            selection.start.min(i16::MAX as usize) as u16,
        );
        let _ = memory.write_u16_be(
            te_ptr + PPC_TE_SEL_END_OFFSET,
            selection.end.min(i16::MAX as usize) as u16,
        );
    }
    PPC_NO_ERR
}

pub(super) fn ppc_te_key(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    te_handle: u32,
    key: u8,
) -> i16 {
    let Some(mut buffer) = ppc_te_edit_buffer(memory, handles, te_handle) else {
        return PPC_NIL_HANDLE_ERR;
    };
    buffer.apply_key(key);
    ppc_te_commit_edit_buffer(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        te_handle,
        &buffer,
    )
}

pub(super) fn ppc_te_line_range(
    memory: &mut PpcSectionMem,
    te_ptr: u32,
    line: usize,
    text_len: usize,
) -> Option<(usize, usize)> {
    let line_count = usize::from(memory.read_u16_be(te_ptr + PPC_TE_N_LINES_OFFSET)?);
    if line >= line_count {
        return None;
    }
    let start =
        usize::from(memory.read_u16_be(te_ptr + PPC_TE_LINE_STARTS_OFFSET + line as u32 * 2)?)
            .min(text_len);
    let end = if line + 1 < line_count {
        usize::from(memory.read_u16_be(te_ptr + PPC_TE_LINE_STARTS_OFFSET + (line + 1) as u32 * 2)?)
            .min(text_len)
    } else {
        text_len
    };
    Some((start, end.max(start)))
}

pub(super) fn ppc_te_metrics(memory: &mut PpcSectionMem, te_ptr: u32) -> (i16, i16, i16, i16) {
    let (font, size) = ppc_te_primary_font_and_size(memory, te_ptr);
    let mut line_height = memory
        .read_u16_be(te_ptr + PPC_TE_LINE_HEIGHT_OFFSET)
        .unwrap_or(0) as i16;
    let mut ascent = memory
        .read_u16_be(te_ptr + PPC_TE_FONT_ASCENT_OFFSET)
        .unwrap_or(0) as i16;
    if line_height <= 0 || ascent < 0 {
        let (face, scale) = get_font_face_scaled(font, size);
        ascent = face.metrics.ascent.saturating_mul(scale);
        line_height = ascent
            .saturating_add(face.metrics.descent.saturating_mul(scale))
            .saturating_add(face.metrics.leading.saturating_mul(scale));
    }
    (font, size, line_height.max(1), ascent.max(0))
}

pub(super) fn ppc_te_point_to_offset(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    te_handle: u32,
    v: i16,
    h: i16,
) -> Option<usize> {
    let te_ptr = ppc_te_record_ptr(memory, te_handle)?;
    let text = ppc_te_text_bytes(memory, handles, te_handle)?;
    let (top, left, _, right) =
        ppc_read_rect(memory, te_ptr + PPC_TE_DEST_RECT_OFFSET).unwrap_or((0, 0, 0, 0));
    let styled = memory.read_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET) == Some(0xffff);
    let style_runs = if styled {
        ppc_te_style_runs(memory, handles, te_handle, text.len())
    } else {
        Vec::new()
    };
    let (font, size, fallback_line_height, _) = ppc_te_metrics(memory, te_ptr);
    let line_count = usize::from(
        memory
            .read_u16_be(te_ptr + PPC_TE_N_LINES_OFFSET)
            .unwrap_or(0),
    );
    let target_v = i32::from(v.saturating_sub(top)).max(0);
    let mut line = 0;
    if line_count != 0 {
        let mut line_top = 0i32;
        for candidate in 0..line_count {
            let candidate_height = if styled {
                ppc_te_line_height(memory, te_ptr, candidate).max(1)
            } else {
                fallback_line_height
            };
            if target_v < line_top.saturating_add(i32::from(candidate_height))
                || candidate + 1 == line_count
            {
                line = candidate;
                break;
            }
            line_top = line_top.saturating_add(i32::from(candidate_height));
        }
    }
    let (start, end) =
        ppc_te_line_range(memory, te_ptr, line, text.len()).unwrap_or((text.len(), text.len()));
    let visible_end = (start..end)
        .rev()
        .find(|index| !matches!(text[*index], b'\r' | b'\n'))
        .map_or(start, |index| index + 1);
    let width = if styled {
        ppc_te_measure_text_width_styled(&style_runs, &text, start, visible_end)
    } else {
        ppc_text_bytes_advance_for_font(&text[start..visible_end], font, size)
    };
    let alignment = memory.read_u16_be(te_ptr + PPC_TE_JUST_OFFSET).unwrap_or(0) as i16;
    let line_left = crate::text_edit::aligned_line_left(left, right, width, alignment, 1);
    let target_x = h.saturating_sub(line_left);
    let mut offset = start;
    let mut advance = 0i16;
    for (index, byte) in text[start..visible_end].iter().copied().enumerate() {
        let char_width = if styled {
            let style = ppc_te_style_at_offset(&style_runs, start + index);
            ppc_text_width_bytes(style.font, style.size, style.face, &[byte])
        } else {
            ppc_text_byte_advance_for_font(byte, font, size)
        };
        if target_x < advance.saturating_add(char_width / 2) {
            offset = start + index;
            break;
        }
        advance = advance.saturating_add(char_width);
        offset = start + index + 1;
    }

    Some(offset)
}

pub(super) fn ppc_te_click(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    te_handle: u32,
    v: i16,
    h: i16,
    extend: bool,
    tick_count: u32,
) {
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return;
    };
    let Some(text) = ppc_te_text_bytes(memory, handles, te_handle) else {
        return;
    };
    let Some(offset) = ppc_te_point_to_offset(memory, handles, te_handle, v, h) else {
        return;
    };

    let previous_time = memory
        .read_u32_be(te_ptr + PPC_TE_CLICK_TIME_OFFSET)
        .unwrap_or(0);
    let previous_offset = memory
        .read_u16_be(te_ptr + PPC_TE_CLICK_LOC_OFFSET)
        .unwrap_or(u16::MAX);
    let double_time = memory
        .read_u32_be(crate::memory::globals::addr::DOUBLE_TIME)
        .unwrap_or(PPC_DEFAULT_DOUBLE_TIME_TICKS);
    let is_double = previous_time != 0
        && previous_offset == offset as u16
        && tick_count.wrapping_sub(previous_time) <= double_time;
    let (selection_start, selection_end) = if is_double && !text.is_empty() {
        let mut word_start = offset.min(text.len().saturating_sub(1));
        let mut word_end = word_start;
        while word_start > 0 && text[word_start - 1] > b' ' {
            word_start -= 1;
        }
        while word_end < text.len() && text[word_end] > b' ' {
            word_end += 1;
        }
        (word_start, word_end)
    } else if extend {
        let old_start = usize::from(
            memory
                .read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET)
                .unwrap_or(0),
        );
        let old_end = usize::from(
            memory
                .read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET)
                .unwrap_or(0),
        );
        if offset < old_start {
            (offset, old_end)
        } else {
            (old_start, offset)
        }
    } else {
        (offset, offset)
    };
    let _ = memory.write_u16_be(
        te_ptr + PPC_TE_SEL_START_OFFSET,
        selection_start.min(i16::MAX as usize) as u16,
    );
    let _ = memory.write_u16_be(
        te_ptr + PPC_TE_SEL_END_OFFSET,
        selection_end.min(i16::MAX as usize) as u16,
    );
    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_POINT_OFFSET, v as u16);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_POINT_OFFSET + 2, h as u16);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_CLICK_LOC_OFFSET, offset as u16);
    let _ = memory.write_u32_be(te_ptr + PPC_TE_CLICK_TIME_OFFSET, tick_count);
    let _ = memory.write_u32_be(te_ptr + PPC_TE_CARET_TIME_OFFSET, tick_count);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_te_draw_text_box(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    port: u32,
    text: &[u8],
    rect_ptr: u32,
    alignment: i16,
    text_mode: i16,
    text_size: i16,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
) {
    let Some((top, left, bottom, right)) = ppc_read_rect(memory, rect_ptr) else {
        return;
    };
    let font = ppc_current_text_font(memory, port);
    let (face, scale) = get_font_face_scaled(font, text_size);
    let ascent = face.metrics.ascent.saturating_mul(scale);
    let line_height = ascent
        .saturating_add(face.metrics.descent.saturating_mul(scale))
        .saturating_add(face.metrics.leading.saturating_mul(scale))
        .max(1);
    let max_width = right.saturating_sub(left).max(1);
    let lines = crate::quickdraw::text::wrap_classic_text(text, max_width, |_, byte| {
        ppc_text_byte_advance_for_font(byte, font, text_size)
    });
    for (line_index, line) in lines.into_iter().enumerate() {
        let baseline = top
            .saturating_add(ascent)
            .saturating_add((line_index as i16).saturating_mul(line_height));
        if baseline.saturating_sub(ascent) >= bottom {
            break;
        }
        let bytes = &text[line.start..line.visible_end];
        let width = ppc_text_bytes_advance_for_font(bytes, font, text_size);
        let x = crate::text_edit::aligned_line_left(left, right, width, alignment, 1);
        let _ = ppc_draw_text_bytes(
            memory,
            gworlds,
            port,
            (x, baseline),
            font,
            text_size,
            text_mode,
            color,
            explicit_index,
            bytes,
        );
    }
}

pub(super) fn ppc_te_line_height(memory: &mut PpcSectionMem, te_ptr: u32, line: usize) -> i16 {
    if memory.read_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET) != Some(0xffff) {
        return memory
            .read_u16_be(te_ptr + PPC_TE_LINE_HEIGHT_OFFSET)
            .unwrap_or(0) as i16;
    }
    let line_height_table = memory
        .read_u32_be(te_ptr + PPC_TE_TX_FONT_OFFSET)
        .filter(|handle| *handle != 0)
        .and_then(|handle| memory.read_u32_be(handle))
        .filter(|ptr| *ptr != 0)
        .and_then(|ptr| memory.read_u32_be(ptr + PPC_TE_STYLE_LH_TABLE_OFFSET))
        .filter(|handle| *handle != 0)
        .and_then(|handle| memory.read_u32_be(handle))
        .filter(|ptr| *ptr != 0);
    line_height_table
        .and_then(|ptr| memory.read_u16_be(ptr.wrapping_add(line as u32 * 4)))
        .unwrap_or(0) as i16
}

pub(super) fn ppc_te_get_height(
    memory: &mut PpcSectionMem,
    te_handle: u32,
    start_line: i32,
    end_line: i32,
) -> u32 {
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return 0;
    };
    let line_count = i32::from(
        memory
            .read_u16_be(te_ptr + PPC_TE_N_LINES_OFFSET)
            .unwrap_or(0),
    );
    // Classic applications commonly pass 0 for the first line when asking
    // for the full text height (for example TEGetHeight(32767, 0, hTE)).
    // Line 0 is the first line in that convention.
    if start_line < 0 || end_line < start_line || line_count == 0 {
        return 0;
    }
    let start = (start_line.max(1) - 1).min(line_count) as usize;
    let end = end_line.min(line_count) as usize;
    let (_, _, fallback_height, _) = ppc_te_metrics(memory, te_ptr);
    let height = (start..end).fold(0i32, |height, line| {
        let line_height = ppc_te_line_height(memory, te_ptr, line);
        height.saturating_add(i32::from(if line_height > 0 {
            line_height
        } else {
            fallback_height
        }))
    });
    height.max(0) as u32
}

pub(super) fn ppc_te_get_point(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    te_handle: u32,
    offset: u16,
) -> u32 {
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return 0;
    };
    let text = ppc_te_text_bytes(memory, handles, te_handle).unwrap_or_default();
    let offset = usize::from(offset).min(text.len());
    let line_count = usize::from(
        memory
            .read_u16_be(te_ptr + PPC_TE_N_LINES_OFFSET)
            .unwrap_or(0),
    );
    let mut line = 0usize;
    for candidate in 1..line_count {
        let start = usize::from(
            memory
                .read_u16_be(te_ptr + PPC_TE_LINE_STARTS_OFFSET + candidate as u32 * 2)
                .unwrap_or(u16::MAX),
        );
        if start <= offset {
            line = candidate;
        } else {
            break;
        }
    }
    let start = if line_count == 0 {
        0
    } else {
        ppc_te_line_range(memory, te_ptr, line, text.len())
            .map(|range| range.0)
            .unwrap_or(0)
    };
    let (top, left, _, right) =
        ppc_read_rect(memory, te_ptr + PPC_TE_DEST_RECT_OFFSET).unwrap_or((0, 0, 0, 0));
    let styled = memory.read_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET) == Some(0xffff);
    let style_runs = if styled {
        ppc_te_style_runs(memory, handles, te_handle, text.len())
    } else {
        Vec::new()
    };
    let (font, size, fallback_line_height, _) = ppc_te_metrics(memory, te_ptr);
    let line_end = if line_count == 0 {
        0
    } else {
        ppc_te_line_range(memory, te_ptr, line, text.len())
            .map(|range| range.1)
            .unwrap_or(text.len())
    };
    let visible_end = (start..line_end)
        .rev()
        .find(|index| !matches!(text[*index], b'\r' | b'\n'))
        .map_or(start, |index| index + 1);
    let line_width = if styled {
        ppc_te_measure_text_width_styled(&style_runs, &text, start, visible_end)
    } else {
        ppc_text_bytes_advance_for_font(&text[start..visible_end], font, size)
    };
    let alignment = memory.read_u16_be(te_ptr + PPC_TE_JUST_OFFSET).unwrap_or(0) as i16;
    let line_left = crate::text_edit::aligned_line_left(left, right, line_width, alignment, 1);
    let prefix_end = offset.min(visible_end);
    let prefix_width = if styled {
        ppc_te_measure_text_width_styled(&style_runs, &text, start, prefix_end)
    } else {
        ppc_text_bytes_advance_for_font(&text[start..prefix_end], font, size)
    };
    let h = line_left.saturating_add(prefix_width);
    let mut v = top;
    for previous_line in 0..line {
        let previous_height = if styled {
            ppc_te_line_height(memory, te_ptr, previous_line).max(1)
        } else {
            fallback_line_height
        };
        v = v.saturating_add(previous_height);
    }
    (u32::from(v as u16) << 16) | u32::from(h as u16)
}

/// Moves destRect by (dh, dv), pinned to the text when requested. Returns
/// whether destRect actually moved.
pub(super) fn ppc_te_scroll(memory: &mut PpcSectionMem, te_handle: u32, dh: i16, dv: i16, pinned: bool) -> bool {
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return false;
    };
    let Some((top, left, bottom, right)) = ppc_read_rect(memory, te_ptr + PPC_TE_DEST_RECT_OFFSET)
    else {
        return false;
    };
    let mut next_top = top.saturating_add(dv);
    let mut next_left = left.saturating_add(dh);
    let height = bottom.saturating_sub(top);
    let width = right.saturating_sub(left);
    if pinned {
        let (view_top, view_left, view_bottom, view_right) =
            ppc_read_rect(memory, te_ptr + PPC_TE_VIEW_RECT_OFFSET)
                .unwrap_or((top, left, bottom, right));
        let line_count = usize::from(
            memory
                .read_u16_be(te_ptr + PPC_TE_N_LINES_OFFSET)
                .unwrap_or(0),
        );
        let (_, _, fallback_height, _) = ppc_te_metrics(memory, te_ptr);
        let content_height = (0..line_count).fold(0i16, |height, line| {
            let line_height = ppc_te_line_height(memory, te_ptr, line);
            height.saturating_add(if line_height > 0 {
                line_height
            } else {
                fallback_height
            })
        });
        next_top = if content_height <= view_bottom.saturating_sub(view_top) {
            view_top
        } else {
            next_top.clamp(view_bottom.saturating_sub(content_height), view_top)
        };
        next_left = if width <= view_right.saturating_sub(view_left) {
            view_left
        } else {
            next_left.clamp(view_right.saturating_sub(width), view_left)
        };
    }
    let _ = ppc_write_rect(
        memory,
        te_ptr + PPC_TE_DEST_RECT_OFFSET,
        next_top,
        next_left,
        next_top.saturating_add(height),
        next_left.saturating_add(width),
    );
    (next_top, next_left) != (top, left)
}

pub(super) fn ppc_te_scrap_handle(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> u32 {
    let existing = memory
        .read_u32_be(crate::memory::globals::addr::TE_SCRP_HANDLE)
        .unwrap_or(0);
    if existing != 0 && memory.read_u32_be(existing).is_some() {
        return existing;
    }
    let handle = ppc_allocator_view_allocate_handle(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        0,
        true,
    );
    if handle != 0 {
        let _ = memory.write_u32_be(crate::memory::globals::addr::TE_SCRP_HANDLE, handle);
        let _ = memory.write_u16_be(crate::memory::globals::addr::TE_SCRP_LENGTH, 0);
    }
    handle
}

pub(super) fn ppc_te_scrap_bytes(memory: &mut PpcSectionMem) -> Vec<u8> {
    let handle = memory
        .read_u32_be(crate::memory::globals::addr::TE_SCRP_HANDLE)
        .unwrap_or(0);
    let length = usize::from(
        memory
            .read_u16_be(crate::memory::globals::addr::TE_SCRP_LENGTH)
            .unwrap_or(0),
    );
    let Some(ptr) = memory.read_u32_be(handle).filter(|ptr| *ptr != 0) else {
        return Vec::new();
    };
    u32::try_from(length)
        .ok()
        .and_then(|length| ppc_memory_read_bytes(memory, ptr, length))
        .unwrap_or_default()
}

pub(super) fn ppc_te_set_scrap_bytes(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    bytes: &[u8],
) -> i16 {
    let handle = ppc_te_scrap_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    if handle == 0 {
        return PPC_MEM_FULL_ERR;
    }
    let length = bytes.len().min(i16::MAX as usize);
    let result = ppc_allocator_view_resize_handle(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        handle,
        length as u32,
    );
    if result != PPC_NO_ERR {
        return result;
    }
    if length != 0 {
        let Some(ptr) = memory.read_u32_be(handle).filter(|ptr| *ptr != 0) else {
            return PPC_PARAM_ERR;
        };
        if memory.write_bytes(ptr, &bytes[..length]).is_none() {
            return PPC_PARAM_ERR;
        }
    }
    let _ = memory.write_u16_be(
        crate::memory::globals::addr::TE_SCRP_LENGTH,
        length as u16,
    );
    PPC_NO_ERR
}

pub(super) fn ppc_te_selected_bytes(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    te_handle: u32,
) -> Option<Vec<u8>> {
    Some(
        ppc_te_edit_buffer(memory, handles, te_handle)?
            .selected_text()
            .to_vec(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_te_copy_or_cut(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    te_handle: u32,
    cut: bool,
) -> i16 {
    let Some(selected) = ppc_te_selected_bytes(memory, handles, te_handle) else {
        return PPC_NIL_HANDLE_ERR;
    };
    if selected.is_empty() {
        return PPC_NO_ERR;
    }
    let result = ppc_te_set_scrap_bytes(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        &selected,
    );
    if result != PPC_NO_ERR || !cut {
        return result;
    }
    ppc_te_replace_selection(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        te_handle,
        &[],
    )
}

pub(super) fn ppc_te_draw(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    gworlds: &[PpcGWorldRecord],
    te_handle: u32,
    fallback_port: u32,
    fallback_color: PpcRgbColor,
    fore_indices: &HashMap<u32, u8>,
) {
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return;
    };
    let Some(text) = ppc_te_text_bytes(memory, handles, te_handle) else {
        return;
    };
    let (top, left, _, right) =
        ppc_read_rect(memory, te_ptr + PPC_TE_DEST_RECT_OFFSET).unwrap_or((0, 0, 0, 0));
    let styled = memory.read_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET) == Some(0xffff);
    let (font, size, fallback_line_height, fallback_ascent) = ppc_te_metrics(memory, te_ptr);
    let style_runs = if styled {
        ppc_te_style_runs(memory, handles, te_handle, text.len())
    } else {
        Vec::new()
    };
    let port = memory
        .read_u32_be(te_ptr + PPC_TE_IN_PORT_OFFSET)
        .filter(|port| *port != 0 && gworlds.iter().any(|g| g.port == *port))
        .unwrap_or(fallback_port);
    let explicit_index = (!styled)
        .then(|| fore_indices.get(&port).copied())
        .flatten();
    let mode = memory
        .read_u16_be(te_ptr + PPC_TE_TX_MODE_OFFSET)
        .unwrap_or(PPC_QD_TEXT_MODE_SRC_OR as u16) as i16;
    let line_count = usize::from(
        memory
            .read_u16_be(te_ptr + PPC_TE_N_LINES_OFFSET)
            .unwrap_or(0),
    );
    let active = memory
        .read_u16_be(te_ptr + PPC_TE_ACTIVE_OFFSET)
        .unwrap_or(0)
        != 0;
    let selection_start = memory
        .read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET)
        .unwrap_or(0) as usize;
    let selection_end = memory
        .read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET)
        .unwrap_or(0) as usize;
    // Text (1993), pp. 2-16 and 2-29: viewRect bounds the visible portion of
    // the text, so drawing is clipped to it.
    let view_clip = ppc_read_rect(memory, te_ptr + PPC_TE_VIEW_RECT_OFFSET);
    let view = view_clip.unwrap_or((0, 0, 0, 0));
    for line in 0..line_count {
        let start = usize::from(
            memory
                .read_u16_be(te_ptr + PPC_TE_LINE_STARTS_OFFSET + line as u32 * 2)
                .unwrap_or(0),
        )
        .min(text.len());
        let end = if line + 1 < line_count {
            usize::from(
                memory
                    .read_u16_be(te_ptr + PPC_TE_LINE_STARTS_OFFSET + (line + 1) as u32 * 2)
                    .unwrap_or(text.len() as u16),
            )
            .min(text.len())
        } else {
            text.len()
        };
        let mut visible_end = end;
        while visible_end > start && matches!(text[visible_end - 1], b' ' | b'\r' | b'\n') {
            visible_end -= 1;
        }
        let width = if styled {
            ppc_te_measure_text_width_styled(&style_runs, &text, start, visible_end)
        } else {
            ppc_text_width_bytes(
                font,
                size,
                ppc_current_text_style(memory, port),
                &text[start..visible_end],
            )
        };
        let alignment = memory.read_u16_be(te_ptr + PPC_TE_JUST_OFFSET).unwrap_or(0) as i16;
        let line_left = crate::text_edit::aligned_line_left(left, right, width, alignment, 1);
        let (line_height, ascent) = if styled {
            ppc_te_line_metrics_for_range(&style_runs, start, end)
        } else {
            (fallback_line_height, fallback_ascent)
        };
        let baseline = top
            .saturating_add(ascent)
            .saturating_add((line as i16).saturating_mul(line_height));
        if styled {
            let mut offset = start;
            let mut pen = line_left;
            while offset < visible_end {
                let style = ppc_te_style_at_offset(&style_runs, offset);
                let run_end = style_runs
                    .iter()
                    .find(|run| run.start > offset)
                    .map(|run| run.start)
                    .unwrap_or(visible_end)
                    .min(visible_end)
                    .max(offset + 1);
                let advance = ppc_draw_text_bytes_styled_clipped(
                    memory,
                    gworlds,
                    port,
                    (pen, baseline),
                    style.font,
                    style.size,
                    mode,
                    style.color,
                    None,
                    style.face,
                    view_clip,
                    &text[offset..run_end],
                );
                pen = pen.saturating_add(advance);
                offset = run_end;
            }
        } else {
            let _ = ppc_draw_text_bytes_clipped(
                memory,
                gworlds,
                port,
                (line_left, baseline),
                font,
                size,
                mode,
                fallback_color,
                explicit_index,
                view_clip,
                &text[start..visible_end],
            );
        }
        if active && selection_start != selection_end {
            // Text (1993), pp. 2-51--2-52: highlight each selected line segment.
            let selected_start = selection_start.min(selection_end).max(start);
            let selected_end = selection_start.max(selection_end).min(visible_end);
            if selected_start < selected_end {
                let measure = |end| {
                    if styled {
                        ppc_te_measure_text_width_styled(&style_runs, &text, start, end)
                    } else {
                        ppc_text_bytes_advance_for_font(&text[start..end], font, size)
                    }
                };
                let selection_left = if selected_start == start && matches!(alignment, 0 | -2) {
                    left
                } else {
                    line_left.saturating_add(measure(selected_start))
                };
                let selection_right = line_left.saturating_add(measure(selected_end));
                let line_top = baseline.saturating_sub(ascent);
                let selection = (
                    line_top.max(view.0),
                    selection_left.max(view.1),
                    line_top.saturating_add(line_height).min(view.2),
                    selection_right.min(view.3),
                );
                if !ppc_draw_themed_selection(memory, gworlds, port, selection) {
                    ppc_invert_rect_bounds(memory, gworlds, port, selection);
                }
            }
        }
    }

    let sel_start = usize::from(
        memory
            .read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET)
            .unwrap_or(0),
    );
    let sel_end = usize::from(
        memory
            .read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET)
            .unwrap_or(0),
    );
    if active && sel_start == sel_end {
        for line in 0..line_count {
            let start = usize::from(
                memory
                    .read_u16_be(te_ptr + PPC_TE_LINE_STARTS_OFFSET + line as u32 * 2)
                    .unwrap_or(0),
            )
            .min(text.len());
            let end = if line + 1 < line_count {
                usize::from(
                    memory
                        .read_u16_be(te_ptr + PPC_TE_LINE_STARTS_OFFSET + (line + 1) as u32 * 2)
                        .unwrap_or(text.len() as u16),
                )
                .min(text.len())
            } else {
                text.len()
            };
            if sel_start >= start && (sel_start <= end || line + 1 == line_count) {
                let mut visible_end = end;
                while visible_end > start && matches!(text[visible_end - 1], b' ' | b'\r' | b'\n') {
                    visible_end -= 1;
                }
                let (line_height, _) = if styled {
                    ppc_te_line_metrics_for_range(&style_runs, start, end)
                } else {
                    (fallback_line_height, fallback_ascent)
                };
                let width = if styled {
                    ppc_te_measure_text_width_styled(&style_runs, &text, start, visible_end)
                } else {
                    ppc_text_width_bytes(
                        font,
                        size,
                        ppc_current_text_style(memory, port),
                        &text[start..visible_end],
                    )
                };
                let alignment = memory.read_u16_be(te_ptr + PPC_TE_JUST_OFFSET).unwrap_or(0) as i16;
                let line_left =
                    crate::text_edit::aligned_line_left(left, right, width, alignment, 1);
                let caret_offset = sel_start.min(visible_end).max(start);
                let measured = if styled {
                    ppc_te_measure_text_width_styled(&style_runs, &text, start, caret_offset)
                } else {
                    ppc_text_width_bytes(
                        font,
                        size,
                        ppc_current_text_style(memory, port),
                        &text[start..caret_offset],
                    )
                };
                let mut caret_x = line_left + measured;
                if caret_offset > start {
                    caret_x = caret_x.saturating_sub(1);
                }
                let line_top = top.saturating_add((line as i16).saturating_mul(line_height));
                let line_bottom = line_top.saturating_add(line_height);
                let _ = ppc_paint_rect_bounds(
                    memory,
                    gworlds,
                    port,
                    (line_top, caret_x, line_bottom, caret_x.saturating_add(1)),
                    if ppc_ui_theme(gworlds) != UiThemeId::ClassicSystem7 {
                        ppc_theme_rgb(ppc_ui_theme(gworlds).provider().palette().selection)
                    } else if styled {
                        ppc_te_style_at_offset(&style_runs, caret_offset).color
                    } else {
                        fallback_color
                    },
                    explicit_index,
                );
                break;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_te_cleanup_handles(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    allocated: &[u32],
) {
    for handle in allocated.iter().copied().rev() {
        ppc_te_forget_handle(
            allocator.as_deref_mut(),
            None,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            handle,
        );
    }
}

pub(super) fn ppc_te_forget_handle(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    legacy_handle_states: Option<&mut Vec<PpcHandleStateRecord>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    handle: u32,
) {
    if handle == 0 {
        return;
    }
    if let Some(allocator) = allocator {
        let _ = allocator.dispose_handle(
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            handle,
        );
    } else {
        if let Some(index) = handles.iter().position(|record| record.handle == handle) {
            handles.remove(index);
            let _ = memory.write_u32_be(handle, 0);
        }
        if let Some(handle_states) = legacy_handle_states {
            ppc_forget_handle_state(handle, handle_states);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_te_dispose(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    mut legacy_handle_states: Option<&mut Vec<PpcHandleStateRecord>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    te_handle: u32,
) {
    let Some(te_ptr) = memory.read_u32_be(te_handle).filter(|ptr| *ptr != 0) else {
        return;
    };
    let h_text = memory
        .read_u32_be(te_ptr + PPC_TE_HTEXT_OFFSET)
        .unwrap_or(0);
    let styled = memory
        .read_u16_be(te_ptr + PPC_TE_TX_SIZE_OFFSET)
        .is_some_and(|size| size == 0xffff);
    if styled {
        let style_handle = memory
            .read_u32_be(te_ptr + PPC_TE_TX_FONT_OFFSET)
            .unwrap_or(0);
        if let Some(style_ptr) = memory.read_u32_be(style_handle).filter(|ptr| *ptr != 0) {
            let style_table = memory
                .read_u32_be(style_ptr + PPC_TE_STYLE_TABLE_OFFSET)
                .unwrap_or(0);
            let lh_table = memory
                .read_u32_be(style_ptr + PPC_TE_STYLE_LH_TABLE_OFFSET)
                .unwrap_or(0);
            let null_style = memory
                .read_u32_be(style_ptr + PPC_TE_STYLE_NULL_STYLE_OFFSET)
                .unwrap_or(0);
            let null_scrap = memory
                .read_u32_be(null_style)
                .filter(|ptr| *ptr != 0)
                .and_then(|ptr| memory.read_u32_be(ptr + PPC_TE_NULL_STYLE_SCRAP_OFFSET))
                .unwrap_or(0);
            for nested in [style_table, lh_table, null_scrap, null_style, style_handle] {
                ppc_te_forget_handle(
                    allocator.as_deref_mut(),
                    legacy_handle_states.as_deref_mut(),
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    nested,
                );
            }
        }
    }
    ppc_te_forget_handle(
        allocator.as_deref_mut(),
        legacy_handle_states.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        h_text,
    );
    ppc_te_forget_handle(
        allocator,
        legacy_handle_states,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        te_handle,
    );
}

