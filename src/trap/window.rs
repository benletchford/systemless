//! Window Manager trap handlers.

use crate::cpu::{CpuOps, Register};
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::trap::dispatch::{DrawOldState, PortDrawState, QueuedEvent};
use crate::trap::quickdraw::RegionBooleanOp;
use crate::trap::types::{Rect, ShapeOp};
use crate::Result;
use std::sync::OnceLock;

static TRACE_INVAL: OnceLock<bool> = OnceLock::new();
static TRACE_DRAGWINDOW: OnceLock<bool> = OnceLock::new();

type WindowRect = (i16, i16, i16, i16);
type HiddenWindowLocalRegions = (WindowRect, Option<WindowRect>);

struct WindTemplate {
    bounds: WindowRect,
    proc_id: i16,
    visible: bool,
    go_away: bool,
    ref_con: u32,
    title: String,
    position: u16,
}

fn trace_inval_enabled() -> bool {
    *TRACE_INVAL.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_INVAL").is_some())
}

fn trace_dragwindow_enabled() -> bool {
    *TRACE_DRAGWINDOW.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DRAGWINDOW").is_some())
}

impl super::TrapDispatcher {
    const DRAG_NO_DRAG_SENTINEL: u32 = 0x8000_8000;
    const DRAG_NO_CONSTRAINT: i16 = 0;
    const DRAG_H_AXIS_ONLY: i16 = 1;
    const DRAG_V_AXIS_ONLY: i16 = 2;
    const USER_WINDOW_KIND: u16 = 8;
    const WINDOW_KIND_OFFSET: u32 = 108;
    const WINDOW_VISIBLE_OFFSET: u32 = 110;
    const WINDOW_HILITED_OFFSET: u32 = 111;
    const WINDOW_GO_AWAY_FLAG_OFFSET: u32 = 112;
    const WINDOW_SPARE_FLAG_OFFSET: u32 = 113;
    const WINDOW_STRUC_RGN_OFFSET: u32 = 114;
    const WINDOW_CONT_RGN_OFFSET: u32 = 118;
    const WINDOW_UPDATE_RGN_OFFSET: u32 = 122;
    const WINDOW_DEF_PROC_OFFSET: u32 = 126;
    const WINDOW_DATA_HANDLE_OFFSET: u32 = 130;
    const WINDOW_TITLE_HANDLE_OFFSET: u32 = 134;
    const WINDOW_TITLE_WIDTH_OFFSET: u32 = 138;
    const WINDOW_CONTROL_LIST_OFFSET: u32 = 140;
    const WINDOW_NEXT_WINDOW_OFFSET: u32 = 144;
    const WINDOW_PIC_OFFSET: u32 = 148;
    const WINDOW_REFCON_OFFSET: u32 = 152;
    const LOWMEM_WINDOW_LIST: u32 = 0x09D6;
    const LOWMEM_CUR_ACTIVATE: u32 = 0x0A64;
    const LOWMEM_CUR_DEACTIVATE: u32 = 0x0A68;
    const LOWMEM_WMGR_PORT: u32 = 0x09DE;
    const LOWMEM_GRAY_RGN: u32 = 0x09EE;
    const WDEF_WDRAW_MSG: i16 = 0;
    const WDEF_WCALC_RGNS_MSG: i16 = 2;
    const WDEF_WNEW_MSG: i16 = 3;
    const WDEF_FIRST_APPLICATION_RESOURCE_ID: i16 = 128;
    const WDEF_TRAMPOLINE_SIZE: u32 = 68;
    const AUX_WIN_NEXT_OFFSET: u32 = 0;
    const AUX_WIN_OWNER_OFFSET: u32 = 4;
    pub(crate) const AUX_WIN_CTABLE_OFFSET: u32 = 8;
    const AUX_WIN_DIALOG_CITEM_OFFSET: u32 = 12;
    const AUX_WIN_FLAGS_OFFSET: u32 = 16;
    const AUX_WIN_RESERVED_OFFSET: u32 = 20;
    const AUX_WIN_REFCON_OFFSET: u32 = 24;
    const AUX_WIN_RECORD_SIZE: u32 = 28;

    fn parse_wind_template(bus: &MacMemoryBus, ptr: u32, data_len: u32) -> Option<WindTemplate> {
        // A classic WIND ends after its Pascal title. System 7 optionally
        // appends a word-aligned positioning specification after that title.
        if data_len < 19 {
            return None;
        }
        let title_len = bus.read_byte(ptr + 18) as u32;
        let title_end = 19u32.checked_add(title_len)?;
        if title_end > data_len {
            return None;
        }
        let padded_title_end = title_end.checked_add(1)? & !1;
        let position = if padded_title_end
            .checked_add(2)
            .is_some_and(|end| end <= data_len)
        {
            bus.read_word(ptr + padded_title_end)
        } else {
            0
        };
        let title =
            String::from_utf8_lossy(&bus.read_bytes(ptr + 19, title_len as usize)).into_owned();
        Some(WindTemplate {
            bounds: Self::read_rect(bus, ptr),
            proc_id: bus.read_word(ptr + 8) as i16,
            visible: bus.read_byte(ptr + 10) != 0,
            go_away: bus.read_byte(ptr + 12) != 0,
            ref_con: bus.read_long(ptr + 14),
            title,
            position,
        })
    }

    pub(crate) fn finish_drag_result<C: CpuOps>(
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        sp: u32,
        result: u32,
    ) {
        // DragGrayRgn and DragTheRgn share the same 22-byte frame; only the
        // returned LONGINT sentinel differs between the gray and custom-outline
        // aliases.
        bus.write_long(sp + 22, result);
        cpu.write_reg(Register::A7, sp + 22);
    }

    pub(crate) fn drag_region_result(&self, bus: &MacMemoryBus, sp: u32) -> u32 {
        let axis = bus.read_word(sp + 4) as i16;
        let slop_rect_ptr = bus.read_long(sp + 6);
        let limit_rect_ptr = bus.read_long(sp + 10);
        let start_v = bus.read_word(sp + 14) as i16;
        let start_h = bus.read_word(sp + 16) as i16;

        if slop_rect_ptr == 0 {
            return Self::DRAG_NO_DRAG_SENTINEL;
        }
        let slop_rect = Self::read_rect(bus, slop_rect_ptr);
        let (release_v, release_h) = self.current_mouse_local_point(bus);
        if !Self::point_in_rect(release_v, release_h, slop_rect) {
            return Self::DRAG_NO_DRAG_SENTINEL;
        }

        let (mut offset_v, mut offset_h) = if limit_rect_ptr != 0 {
            let limit_rect = Self::read_rect(bus, limit_rect_ptr);
            if Self::rect_is_empty(limit_rect) {
                (release_v, release_h)
            } else {
                Self::clamp_point_to_rect(release_v, release_h, limit_rect)
            }
        } else {
            (release_v, release_h)
        };

        match axis {
            Self::DRAG_H_AXIS_ONLY => offset_v = start_v,
            Self::DRAG_V_AXIS_ONLY => offset_h = start_h,
            Self::DRAG_NO_CONSTRAINT => {}
            _ => {}
        }

        let dv = offset_v.wrapping_sub(start_v) as u16 as u32;
        let dh = offset_h.wrapping_sub(start_h) as u16 as u32;
        (dv << 16) | dh
    }

    fn rect_is_empty(rect: (i16, i16, i16, i16)) -> bool {
        rect.2 <= rect.0 || rect.3 <= rect.1
    }

    fn read_rect(bus: &MacMemoryBus, rect_ptr: u32) -> (i16, i16, i16, i16) {
        (
            bus.read_word(rect_ptr) as i16,
            bus.read_word(rect_ptr + 2) as i16,
            bus.read_word(rect_ptr + 4) as i16,
            bus.read_word(rect_ptr + 6) as i16,
        )
    }

    fn point_in_rect(v: i16, h: i16, rect: (i16, i16, i16, i16)) -> bool {
        v >= rect.0 && v < rect.2 && h >= rect.1 && h < rect.3
    }

    fn clamp_point_to_rect(v: i16, h: i16, rect: (i16, i16, i16, i16)) -> (i16, i16) {
        (
            v.clamp(rect.0, rect.2.wrapping_sub(1)),
            h.clamp(rect.1, rect.3.wrapping_sub(1)),
        )
    }

    fn current_mouse_local_point(&self, bus: &MacMemoryBus) -> (i16, i16) {
        let (v, h) = self.mouse_pos;
        if self.current_port == 0 {
            return (v, h);
        }
        let (bounds_top, bounds_left) = self.port_bounds_top_left(bus, self.current_port);
        (v.wrapping_add(bounds_top), h.wrapping_add(bounds_left))
    }

    fn copy_color_table_resource(
        &mut self,
        bus: &mut MacMemoryBus,
        resource_type: [u8; 4],
        table_id: i16,
    ) -> Option<u32> {
        let resource_ptr = self
            .find_or_load_resource_any(bus, resource_type, table_id)
            .map(|(_, ptr)| ptr)?;

        if bus.get_alloc_size(resource_ptr).unwrap_or(0) < 8 {
            return None;
        }

        let table_size = bus.read_word(resource_ptr + 6);
        if table_size == 0xFFFF {
            return Some(self.default_window_color_table_handle(bus));
        }

        let entry_count = u32::from(table_size) + 1;
        let byte_size = 8 + entry_count * 8;
        if bus.get_alloc_size(resource_ptr).unwrap_or(0) < byte_size {
            return None;
        }
        let ctab_ptr = bus.alloc(byte_size);
        for offset in 0..byte_size {
            bus.write_byte(ctab_ptr + offset, bus.read_byte(resource_ptr + offset));
        }
        let seed = self.next_ct_seed;
        self.next_ct_seed = self.next_ct_seed.wrapping_add(1);
        if self.next_ct_seed == 0 {
            self.next_ct_seed = 1;
        }
        bus.write_long(ctab_ptr, seed);
        let ctab_handle = bus.alloc(4);
        bus.write_long(ctab_handle, ctab_ptr);
        Some(ctab_handle)
    }

    fn copy_window_color_table_resource(&mut self, bus: &mut MacMemoryBus, table_id: i16) -> u32 {
        self.copy_color_table_resource(bus, *b"wctb", table_id)
            .unwrap_or(0)
    }

    pub(crate) fn copy_dialog_color_table_resource(
        &mut self,
        bus: &mut MacMemoryBus,
        table_id: i16,
    ) -> Option<u32> {
        self.copy_color_table_resource(bus, *b"dctb", table_id)
    }

    fn window_content_color(bus: &MacMemoryBus, ctab_handle: u32) -> Option<(u16, u16, u16)> {
        if ctab_handle == 0 {
            return None;
        }
        let ctab_ptr = bus.read_long(ctab_handle);
        if ctab_ptr == 0 {
            return None;
        }

        let entry_count = u32::from(bus.read_word(ctab_ptr + 6)) + 1;
        for index in 0..entry_count {
            let entry = ctab_ptr + 8 + index * 8;
            if bus.read_word(entry) == 0 {
                return Some((
                    bus.read_word(entry + 2),
                    bus.read_word(entry + 4),
                    bus.read_word(entry + 6),
                ));
            }
        }

        None
    }

    pub(crate) fn window_semantic_color(
        &self,
        bus: &MacMemoryBus,
        window_ptr: u32,
        part: u16,
    ) -> Option<(u16, u16, u16)> {
        let aux_handle = *self.window_aux_records.get(&window_ptr)?;
        let aux_ptr = bus.read_long(aux_handle);
        let ctab_handle = bus.read_long(aux_ptr + Self::AUX_WIN_CTABLE_OFFSET);
        let ctab_ptr = bus.read_long(ctab_handle);
        if ctab_ptr == 0 {
            return None;
        }

        // WCTab/DCTab resources contain the five semantic Window Manager
        // roles. A device ColorTable instead contains indexed palette entries
        // and must not be interpreted as window-part identifiers.
        let table_size = bus.read_word(ctab_ptr + 6);
        if table_size == 0xFFFF || table_size > 4 {
            return None;
        }
        for index in 0..=u32::from(table_size) {
            let entry = ctab_ptr + 8 + index * 8;
            if bus.read_word(entry) == part {
                return Some((
                    bus.read_word(entry + 2),
                    bus.read_word(entry + 4),
                    bus.read_word(entry + 6),
                ));
            }
        }
        None
    }

    pub(crate) fn apply_window_color_table(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        ctab_handle: u32,
    ) {
        let Some((r, g, b)) = Self::window_content_color(bus, ctab_handle) else {
            return;
        };

        bus.write_word(window_ptr + 42, r);
        bus.write_word(window_ptr + 44, g);
        bus.write_word(window_ptr + 46, b);
        let legacy_color = if (r, g, b) == (0, 0, 0) {
            0x00000021
        } else if (r, g, b) == (0xFFFF, 0xFFFF, 0xFFFF) {
            0x0000001E
        } else {
            0
        };
        bus.write_long(window_ptr + 84, legacy_color);

        let mut state = self
            .port_draw_states
            .get(&window_ptr)
            .copied()
            .unwrap_or_else(PortDrawState::default);
        state.bg_color = (r, g, b);
        self.port_draw_states.insert(window_ptr, state);

        if self.current_port == window_ptr {
            self.bg_color = (r, g, b);
            self.sync_current_port_draw_state(bus);
        }
    }

    fn default_window_color_table_handle(&mut self, bus: &mut MacMemoryBus) -> u32 {
        let gd_handle = self.ensure_main_gdevice(bus);
        let gd_ptr = bus.read_long(gd_handle);
        let gd_pmap_handle = bus.read_long(gd_ptr + 22);
        let gd_pmap = bus.read_long(gd_pmap_handle);
        bus.read_long(gd_pmap + 42)
    }

    pub(crate) fn ensure_window_aux_record(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        ctab_handle: u32,
    ) -> u32 {
        if window_ptr == 0 {
            return 0;
        }

        let ctab_handle = if ctab_handle != 0 {
            ctab_handle
        } else {
            self.default_window_color_table_handle(bus)
        };

        if let Some(&aux_handle) = self.window_aux_records.get(&window_ptr) {
            let aux_ptr = bus.read_long(aux_handle);
            if aux_ptr != 0 {
                bus.write_long(aux_ptr + Self::AUX_WIN_OWNER_OFFSET, window_ptr);
                bus.write_long(aux_ptr + Self::AUX_WIN_CTABLE_OFFSET, ctab_handle);
            }
            return aux_handle;
        }

        let aux_ptr = bus.alloc(Self::AUX_WIN_RECORD_SIZE);
        let aux_handle = bus.alloc(4);
        bus.write_long(aux_handle, aux_ptr);
        bus.write_long(aux_ptr + Self::AUX_WIN_NEXT_OFFSET, 0);
        bus.write_long(aux_ptr + Self::AUX_WIN_OWNER_OFFSET, window_ptr);
        bus.write_long(aux_ptr + Self::AUX_WIN_CTABLE_OFFSET, ctab_handle);
        bus.write_long(aux_ptr + Self::AUX_WIN_DIALOG_CITEM_OFFSET, 0);
        bus.write_long(aux_ptr + Self::AUX_WIN_FLAGS_OFFSET, 0);
        bus.write_long(aux_ptr + Self::AUX_WIN_RESERVED_OFFSET, 0);
        bus.write_long(aux_ptr + Self::AUX_WIN_REFCON_OFFSET, 0);
        self.window_aux_records.insert(window_ptr, aux_handle);
        aux_handle
    }

    pub(crate) fn set_window_dialog_item_color_table(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        item_color_table: u32,
    ) {
        let aux_handle = self
            .window_aux_records
            .get(&window_ptr)
            .copied()
            .unwrap_or_else(|| self.ensure_window_aux_record(bus, window_ptr, 0));
        let aux_ptr = bus.read_long(aux_handle);
        if aux_ptr != 0 {
            bus.write_long(
                aux_ptr + Self::AUX_WIN_DIALOG_CITEM_OFFSET,
                item_color_table,
            );
        }
    }

    pub(crate) fn window_dialog_item_color_table(
        &self,
        bus: &MacMemoryBus,
        window_ptr: u32,
    ) -> u32 {
        self.window_aux_records
            .get(&window_ptr)
            .map(|handle| bus.read_long(*handle))
            .filter(|ptr| *ptr != 0)
            .map(|ptr| bus.read_long(ptr + Self::AUX_WIN_DIALOG_CITEM_OFFSET))
            .unwrap_or(0)
    }

    pub(crate) fn rect_intersection(
        a: (i16, i16, i16, i16),
        b: (i16, i16, i16, i16),
    ) -> Option<(i16, i16, i16, i16)> {
        let rect = (a.0.max(b.0), a.1.max(b.1), a.2.min(b.2), a.3.min(b.3));
        (!Self::rect_is_empty(rect)).then_some(rect)
    }

    fn region_handle_rect_with_min_size(
        bus: &MacMemoryBus,
        handle: u32,
        min_size: u16,
    ) -> Option<(i16, i16, i16, i16)> {
        if handle == 0 {
            return None;
        }
        let ptr = bus.read_long(handle);
        if ptr == 0 || bus.read_word(ptr) < min_size {
            return None;
        }
        Self::region_handle_rect(bus, handle)
    }

    fn rect_union(
        a: Option<(i16, i16, i16, i16)>,
        b: Option<(i16, i16, i16, i16)>,
    ) -> Option<(i16, i16, i16, i16)> {
        match (a, b) {
            (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    fn rect_union_all(rects: [Option<(i16, i16, i16, i16)>; 4]) -> Option<(i16, i16, i16, i16)> {
        rects
            .into_iter()
            .fold(None, |acc, rect| Self::rect_union(acc, rect))
    }

    fn merge_window_update_region(
        &self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        rect: (i16, i16, i16, i16),
    ) {
        let update_handle = bus.read_long(window_ptr + Self::WINDOW_UPDATE_RGN_OFFSET);
        let merged = Self::rect_union(Self::region_handle_rect(bus, update_handle), Some(rect));
        Self::write_region_handle_rect(bus, update_handle, merged);
    }

    fn rect_difference_bbox(
        src: (i16, i16, i16, i16),
        cut: (i16, i16, i16, i16),
    ) -> Option<(i16, i16, i16, i16)> {
        let Some(intersection) = Self::rect_intersection(src, cut) else {
            return Some(src);
        };

        let mut remaining = None;
        for rect in [
            (src.0, src.1, intersection.0, src.3),
            (intersection.2, src.1, src.2, src.3),
            (intersection.0, src.1, intersection.2, intersection.1),
            (intersection.0, intersection.3, intersection.2, src.3),
        ] {
            if !Self::rect_is_empty(rect) {
                remaining = Self::rect_union(remaining, Some(rect));
            }
        }
        remaining
    }

    fn alloc_rect_region_handle(bus: &mut MacMemoryBus, rect: Option<(i16, i16, i16, i16)>) -> u32 {
        let ptr = bus.alloc(10);
        let handle = bus.alloc(4);
        bus.write_long(handle, ptr);
        Self::write_region_handle_rect(bus, handle, rect);
        handle
    }

    fn window_def_resource_id(proc_id: i16) -> i16 {
        proc_id >> 4
    }

    fn is_application_window_def_proc_id(proc_id: i16) -> bool {
        Self::window_def_resource_id(proc_id) >= Self::WDEF_FIRST_APPLICATION_RESOURCE_ID
    }

    pub(crate) fn window_uses_custom_def_proc(&self, bus: &MacMemoryBus, window_ptr: u32) -> bool {
        if window_ptr == 0 {
            return false;
        }
        let proc_id = self.window_proc_ids.get(&window_ptr).copied().unwrap_or(0);
        Self::is_application_window_def_proc_id(proc_id)
            && bus.read_long(window_ptr + Self::WINDOW_DEF_PROC_OFFSET) != 0
    }

    fn window_def_proc_handle(&mut self, bus: &mut MacMemoryBus, proc_id: i16) -> u32 {
        let wdef_id = Self::window_def_resource_id(proc_id);
        if Self::is_application_window_def_proc_id(proc_id) {
            return self
                .find_or_load_resource_any(bus, *b"WDEF", wdef_id)
                .map(|(_, ptr)| ptr)
                .map(|ptr| self.get_or_create_resource_handle(bus, *b"WDEF", wdef_id, ptr))
                .unwrap_or(0);
        }

        // WindowRecord.windowDefProc is guest-visible state, including for
        // the standard ROM-backed definitions. Macintosh Toolbox Essentials
        // (1992), pp. 4-66 and 4-145, says the Window Manager resolves the
        // WDEF and stores its handle in this field when creating a window.
        self.synthesize_system_wdef(bus, wdef_id)
            .map(|ptr| self.get_or_create_resource_handle_in_file(bus, *b"WDEF", wdef_id, ptr, 0))
            .unwrap_or(0)
    }

    fn window_def_proc_addr(bus: &MacMemoryBus, def_handle: u32) -> u32 {
        if def_handle == 0 {
            return 0;
        }
        let def_ptr = bus.read_long(def_handle);
        if def_ptr != 0 {
            def_ptr
        } else {
            def_handle
        }
    }

    fn window_def_entry_looks_callable(bus: &MacMemoryBus, proc_addr: u32) -> bool {
        if proc_addr == 0 {
            return false;
        }
        matches!(
            bus.read_word(proc_addr),
            0x4E56 // LINK.W A6,#imm
                | 0x48E7 // MOVEM.L regs,-(SP)
                | 0x4EF9 // JMP abs.L
                | 0x4EFA // JMP pc-relative
                | 0x6000..=0x60FF // BRA/BRA.S to the real entry
        )
    }

    fn get_or_create_window_def_trampoline(&mut self, bus: &mut MacMemoryBus) -> u32 {
        if self.window_def_trampoline != 0 {
            return self.window_def_trampoline;
        }

        let tramp = bus.alloc(Self::WDEF_TRAMPOLINE_SIZE);
        bus.write_word(tramp, 0x48E7); // MOVEM.L D0-D3/A0-A3,-(SP)
        bus.write_word(tramp + 2, 0xF0F0);
        bus.write_word(tramp + 4, 0x2F3C); // MOVE.L #result,-(SP)
        bus.write_word(tramp + 10, 0x3F3C); // MOVE.W #varCode,-(SP)
        bus.write_word(tramp + 14, 0x2F3C); // MOVE.L #theWindow,-(SP)
        bus.write_word(tramp + 20, 0x3F3C); // MOVE.W #message,-(SP)
        bus.write_word(tramp + 24, 0x2F3C); // MOVE.L #param,-(SP)
        bus.write_word(tramp + 30, 0x4EB9); // JSR abs.L
        bus.write_word(tramp + 36, 0x2E7C); // MOVEA.L #savedRegsSP,A7
        bus.write_word(tramp + 42, 0x4CDF); // MOVEM.L (SP)+,D0-D3/A0-A3
        bus.write_word(tramp + 44, 0x0F0F);
        bus.write_word(tramp + 46, 0x4E75); // RTS (patched to JMP for chains)
        self.window_def_trampoline = tramp;
        tramp
    }

    fn write_window_def_trampoline(
        bus: &mut MacMemoryBus,
        tramp: u32,
        variant: i16,
        window_ptr: u32,
        message: i16,
        param: u32,
        proc_addr: u32,
        return_slot: u32,
        next_trampoline: Option<u32>,
        restore_cgraf_fields: Option<(u32, u32)>,
    ) {
        bus.write_word(tramp, 0x48E7);
        bus.write_word(tramp + 2, 0xF0F0);
        bus.write_word(tramp + 4, 0x2F3C);
        bus.write_long(tramp + 6, 0);
        bus.write_word(tramp + 10, 0x3F3C);
        bus.write_word(tramp + 12, variant as u16);
        bus.write_word(tramp + 14, 0x2F3C);
        bus.write_long(tramp + 16, window_ptr);
        bus.write_word(tramp + 20, 0x3F3C);
        bus.write_word(tramp + 22, message as u16);
        bus.write_word(tramp + 24, 0x2F3C);
        bus.write_long(tramp + 26, param);
        bus.write_word(tramp + 30, 0x4EB9);
        bus.write_long(tramp + 32, proc_addr);
        bus.write_word(tramp + 36, 0x2E7C);
        bus.write_long(tramp + 38, return_slot.wrapping_sub(32));
        bus.write_word(tramp + 42, 0x4CDF);
        bus.write_word(tramp + 44, 0x0F0F);
        match next_trampoline {
            Some(next) => {
                bus.write_word(tramp + 46, 0x4EF9); // JMP abs.L
                bus.write_long(tramp + 48, next);
            }
            None => {
                if let Some((graf_vars, color_fields)) = restore_cgraf_fields {
                    // MOVE.L #grafVars,window+8
                    bus.write_word(tramp + 46, 0x23FC);
                    bus.write_long(tramp + 48, graf_vars);
                    bus.write_long(tramp + 52, window_ptr + 8);
                    // MOVE.L #chExtra/pnLocHFrac,window+12
                    bus.write_word(tramp + 56, 0x23FC);
                    bus.write_long(tramp + 58, color_fields);
                    bus.write_long(tramp + 62, window_ptr + 12);
                    bus.write_word(tramp + 66, 0x4E75); // RTS
                } else {
                    bus.write_word(tramp + 46, 0x4E75); // RTS
                    bus.write_long(tramp + 48, 0);
                }
            }
        }
    }

    fn arm_window_def_messages<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        proc_id: i16,
        messages: &[(i16, u32)],
    ) -> bool {
        if messages.is_empty() || !Self::is_application_window_def_proc_id(proc_id) {
            return false;
        }

        let def_handle = bus.read_long(window_ptr + Self::WINDOW_DEF_PROC_OFFSET);
        let proc_addr = Self::window_def_proc_addr(bus, def_handle);
        if !Self::window_def_entry_looks_callable(bus, proc_addr) {
            return false;
        }

        // Window definition functions are Pascal functions:
        // FUNCTION MyWindow(varCode: Integer; theWindow: WindowPtr;
        //                   message: Integer; param: LongInt): LongInt;
        // They live in 'WDEF' resources, receive the variation code from the
        // low four bits of the window definition ID, and draw wDraw in the
        // Window Manager port. Macintosh Toolbox Essentials 1992, pp. 4-120
        // through 4-127; Inside Macintosh Volume I 1985, pp. I-282, I-304.
        let wmgr_port = self.ensure_color_window_manager_port(bus);
        self.set_current_port_state(bus, cpu, wmgr_port, None);

        // Pre-Color QuickDraw WDEFs receive WindowPeek and commonly read the
        // classic inline portBits.bounds at offsets +8..+15 to turn portRect
        // into global strucRgn/contRgn coordinates. A CWindowRecord stores
        // grafVars/chExtra/pnLocHFrac there instead. System 7 preserves this
        // legacy WDEF contract, so expose the authoritative portPixMap bounds
        // only for the duration of the callback chain and restore the color
        // fields in the final trampoline before guest execution resumes.
        let restore_cgraf_fields = if (bus.read_word(window_ptr + 6) & 0xC000) != 0 {
            let pixmap_handle = bus.read_long(window_ptr + 2);
            let pixmap = bus.read_long(pixmap_handle);
            if pixmap != 0 {
                let saved = (
                    bus.read_long(window_ptr + 8),
                    bus.read_long(window_ptr + 12),
                );
                bus.write_long(window_ptr + 8, bus.read_long(pixmap + 6));
                bus.write_long(window_ptr + 12, bus.read_long(pixmap + 10));
                Some(saved)
            } else {
                None
            }
        } else {
            None
        };

        let final_sp = cpu.read_reg(Register::A7);
        let return_pc = cpu.read_reg(Register::PC);
        let return_slot = final_sp.wrapping_sub(4);
        let variant = proc_id & 0xF;
        let trampolines: Vec<u32> = (0..messages.len())
            .map(|idx| {
                if idx == 0 {
                    self.get_or_create_window_def_trampoline(bus)
                } else {
                    bus.alloc(Self::WDEF_TRAMPOLINE_SIZE)
                }
            })
            .collect();

        for (idx, &(message, param)) in messages.iter().enumerate() {
            let next = trampolines.get(idx + 1).copied();
            let final_restore = if next.is_none() {
                restore_cgraf_fields
            } else {
                None
            };
            Self::write_window_def_trampoline(
                bus,
                trampolines[idx],
                variant,
                window_ptr,
                message,
                param,
                proc_addr,
                return_slot,
                next,
                final_restore,
            );
        }

        bus.write_long(return_slot, return_pc);
        cpu.write_reg(Register::A7, return_slot);
        cpu.write_reg(Register::PC, trampolines[0]);
        true
    }

    fn arm_window_def_on_create<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        proc_id: i16,
        draw_initial_frame: bool,
        visible: bool,
    ) -> bool {
        let mut messages = Vec::with_capacity(3);
        messages.push((Self::WDEF_WNEW_MSG, 0));
        if draw_initial_frame && visible {
            messages.push((Self::WDEF_WCALC_RGNS_MSG, 0));
            messages.push((Self::WDEF_WDRAW_MSG, 0));
        }
        self.arm_window_def_messages(cpu, bus, window_ptr, proc_id, &messages)
    }

    fn arm_window_def_draw<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
    ) -> bool {
        let proc_id = self.window_proc_ids.get(&window_ptr).copied().unwrap_or(0);
        self.arm_window_def_messages(
            cpu,
            bus,
            window_ptr,
            proc_id,
            &[(Self::WDEF_WCALC_RGNS_MSG, 0), (Self::WDEF_WDRAW_MSG, 0)],
        )
    }

    fn desktop_gray_region_rect(&self, bus: &MacMemoryBus) -> (i16, i16, i16, i16) {
        let (_, _, width, height, _) = self.screen_mode;
        let screen_w = width.min(i16::MAX as u16) as i16;
        let screen_h = height.min(i16::MAX as u16) as i16;
        let mbar_h = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
        let top = if mbar_h > 0 && mbar_h < screen_h {
            mbar_h
        } else {
            0
        };
        (top, 0, screen_h, screen_w)
    }

    fn ensure_gray_region(&self, bus: &mut MacMemoryBus) -> u32 {
        // InitWindows creates GrayRgn as the desktop region: the active
        // screen area minus the menu bar. GetGrayRgn is a Universal Headers
        // macro that reads this handle directly from low memory at $09EE.
        // Inside Macintosh Volume I, p. I-282; Volume V, p. V-121;
        // Macintosh Toolbox Essentials 1992, pp. 4-113..4-114.
        let rect = self.desktop_gray_region_rect(bus);
        let handle = bus.read_long(Self::LOWMEM_GRAY_RGN);
        if handle != 0 && bus.read_long(handle) != 0 {
            Self::write_region_handle_rect(bus, handle, Some(rect));
            return handle;
        }

        let handle = Self::alloc_rect_region_handle(bus, Some(rect));
        bus.write_long(Self::LOWMEM_GRAY_RGN, handle);
        handle
    }

    pub(crate) fn ensure_window_manager_port(&mut self, bus: &mut MacMemoryBus) -> u32 {
        if self.window_manager_port != 0 {
            bus.write_long(Self::LOWMEM_WMGR_PORT, self.window_manager_port);
            let _ = self.ensure_gray_region(bus);
            return self.window_manager_port;
        }

        let lowmem_port = bus.read_long(Self::LOWMEM_WMGR_PORT);
        if lowmem_port != 0 {
            self.window_manager_port = lowmem_port;
            let _ = self.ensure_gray_region(bus);
            return lowmem_port;
        }

        let (screen_base, row_bytes, width, height, _) = self.screen_mode;
        let bounds = (0i16, 0i16, height as i16, width as i16);
        let port_ptr = bus.alloc(256);

        let effective_screen_base = if screen_base != 0 {
            screen_base
        } else {
            bus.read_long(0x0824)
        };
        // GetWMgrPort returns a GrafPtr, not a CGrafPtr. Publish the classic
        // GrafPort layout with its 14-byte inline BitMap so legacy callers can
        // inspect portBits exactly as documented. In particular, the bytes at
        // +8..+15 must be screenBits.bounds, not the grafVars/chExtra/
        // pnLocHFrac fields that occupy the same offsets in a CGrafPort.
        //
        // Inside Macintosh Volume I (1985), p. I-282;
        // Imaging With QuickDraw (1994), pp. 2-30, 2-38, 4-8 to 4-9.
        bus.write_word(port_ptr, 0); // device
        bus.write_long(port_ptr + 2, effective_screen_base); // portBits.baseAddr
        bus.write_word(port_ptr + 6, (row_bytes as u16) & 0x3FFF); // portBits.rowBytes
        bus.write_word(port_ptr + 8, bounds.0 as u16); // portBits.bounds.top
        bus.write_word(port_ptr + 10, bounds.1 as u16); // portBits.bounds.left
        bus.write_word(port_ptr + 12, bounds.2 as u16); // portBits.bounds.bottom
        bus.write_word(port_ptr + 14, bounds.3 as u16); // portBits.bounds.right
        bus.write_word(port_ptr + 16, bounds.0 as u16); // portRect.top
        bus.write_word(port_ptr + 18, bounds.1 as u16); // portRect.left
        bus.write_word(port_ptr + 20, bounds.2 as u16); // portRect.bottom
        bus.write_word(port_ptr + 22, bounds.3 as u16); // portRect.right

        let vis_rgn = Self::alloc_rect_region_handle(bus, Some(bounds));
        let clip_rgn =
            Self::alloc_rect_region_handle(bus, Some((i16::MIN, i16::MIN, i16::MAX, i16::MAX)));
        bus.write_long(port_ptr + 24, vis_rgn);
        bus.write_long(port_ptr + 28, clip_rgn);

        // OpenPort defaults for the remainder of the basic GrafPort.
        // Imaging With QuickDraw (1994), Table 2-2, p. 2-38.
        for offset in 32..40 {
            bus.write_byte(port_ptr + offset, 0); // bkPat = white
        }
        for offset in 40..48 {
            bus.write_byte(port_ptr + offset, 0xFF); // fillPat = black
        }
        bus.write_word(port_ptr + 48, 0); // pnLoc.v
        bus.write_word(port_ptr + 50, 0); // pnLoc.h
        bus.write_word(port_ptr + 52, 1); // pnSize.v
        bus.write_word(port_ptr + 54, 1); // pnSize.h
        bus.write_word(port_ptr + 56, 8); // pnMode = patCopy
        for offset in 58..66 {
            bus.write_byte(port_ptr + offset, 0xFF); // pnPat = black
        }
        bus.write_word(port_ptr + 66, 0); // pnVis
        bus.write_word(port_ptr + 68, 0); // txFont
        bus.write_word(port_ptr + 70, 0); // txFace
        bus.write_word(port_ptr + 72, 1); // txMode = srcOr
        bus.write_word(port_ptr + 74, 0); // txSize
        bus.write_long(port_ptr + 76, 0); // spExtra
        bus.write_long(port_ptr + 80, 0x0000_0021); // fgColor = blackColor
        bus.write_long(port_ptr + 84, 0x0000_001E); // bkColor = whiteColor
        bus.write_word(port_ptr + 88, 0); // colrBit
        bus.write_word(port_ptr + 90, 0); // patStretch
        bus.write_long(port_ptr + 92, 0); // picSave
        bus.write_long(port_ptr + 96, 0); // rgnSave
        bus.write_long(port_ptr + 100, 0); // polySave
        bus.write_long(port_ptr + 104, 0); // grafProcs
        self.port_draw_states
            .insert(port_ptr, PortDrawState::default());

        self.window_manager_port = port_ptr;
        bus.write_long(Self::LOWMEM_WMGR_PORT, port_ptr);
        let _ = self.ensure_gray_region(bus);
        port_ptr
    }

    pub(crate) fn ensure_color_window_manager_port(&mut self, bus: &mut MacMemoryBus) -> u32 {
        if self.window_manager_cport != 0 {
            return self.window_manager_cport;
        }

        let _ = self.ensure_window_manager_port(bus);
        let (_, _, width, height, _) = self.screen_mode;
        let bounds = (0i16, 0i16, height as i16, width as i16);
        let gd_handle = self.ensure_main_gdevice(bus);
        let gd_ptr = bus.read_long(gd_handle);
        let gd_pmap_handle = bus.read_long(gd_ptr + 22);
        let gd_pmap = bus.read_long(gd_pmap_handle);
        let pixmap = bus.alloc(50);
        for offset in 0..50u32 {
            bus.write_byte(pixmap + offset, bus.read_byte(gd_pmap + offset));
        }
        let pixmap_handle = bus.alloc(4);
        bus.write_long(pixmap_handle, pixmap);

        let color_port = bus.alloc(256);
        bus.write_word(color_port, 0); // device
        bus.write_long(color_port + 2, pixmap_handle); // portPixMap
        bus.write_word(color_port + 6, 0xC000); // portVersion
        bus.write_long(color_port + 8, 0); // grafVars
        bus.write_word(color_port + 12, 0); // chExtra
        bus.write_word(color_port + 14, 0x8000); // pnLocHFrac
        bus.write_word(color_port + 16, bounds.0 as u16);
        bus.write_word(color_port + 18, bounds.1 as u16);
        bus.write_word(color_port + 20, bounds.2 as u16);
        bus.write_word(color_port + 22, bounds.3 as u16);
        let vis_rgn = Self::alloc_rect_region_handle(bus, Some(bounds));
        let clip_rgn =
            Self::alloc_rect_region_handle(bus, Some((i16::MIN, i16::MIN, i16::MAX, i16::MAX)));
        bus.write_long(color_port + 24, vis_rgn);
        bus.write_long(color_port + 28, clip_rgn);
        self.init_cgraf_port_defaults(color_port, bus);

        self.window_manager_cport = color_port;
        color_port
    }

    pub(crate) fn region_handle_rect(
        bus: &MacMemoryBus,
        handle: u32,
    ) -> Option<(i16, i16, i16, i16)> {
        if handle == 0 {
            return None;
        }
        let ptr = bus.read_long(handle);
        if ptr == 0 {
            return None;
        }
        let rect = (
            bus.read_word(ptr + 2) as i16,
            bus.read_word(ptr + 4) as i16,
            bus.read_word(ptr + 6) as i16,
            bus.read_word(ptr + 8) as i16,
        );
        (!Self::rect_is_empty(rect)).then_some(rect)
    }

    fn write_region_handle_rect(
        bus: &mut MacMemoryBus,
        handle: u32,
        rect: Option<(i16, i16, i16, i16)>,
    ) {
        if handle == 0 {
            return;
        }
        let ptr = bus.read_long(handle);
        if ptr == 0 {
            return;
        }
        bus.write_word(ptr, 10);
        if let Some((top, left, bottom, right)) = rect.filter(|r| !Self::rect_is_empty(*r)) {
            bus.write_word(ptr + 2, top as u16);
            bus.write_word(ptr + 4, left as u16);
            bus.write_word(ptr + 6, bottom as u16);
            bus.write_word(ptr + 8, right as u16);
        } else {
            bus.write_long(ptr + 2, 0);
            bus.write_long(ptr + 6, 0);
        }
    }

    pub(super) fn window_content_global_rect(
        &self,
        bus: &MacMemoryBus,
        window_ptr: u32,
    ) -> Option<(i16, i16, i16, i16)> {
        Self::region_handle_rect(
            bus,
            bus.read_long(window_ptr + Self::WINDOW_CONT_RGN_OFFSET),
        )
    }

    fn window_content_rect(
        &self,
        bus: &MacMemoryBus,
        window_ptr: u32,
    ) -> Option<(i16, i16, i16, i16)> {
        self.window_content_global_rect(bus, window_ptr)
            .map(|rect| self.global_rect_to_window_local(bus, window_ptr, rect))
    }

    fn window_port_rect(&self, bus: &MacMemoryBus, window_ptr: u32) -> (i16, i16, i16, i16) {
        (
            bus.read_word(window_ptr + 16) as i16,
            bus.read_word(window_ptr + 18) as i16,
            bus.read_word(window_ptr + 20) as i16,
            bus.read_word(window_ptr + 22) as i16,
        )
    }

    fn window_global_port_rect(&self, bus: &MacMemoryBus, window_ptr: u32) -> (i16, i16, i16, i16) {
        let (top, left, bottom, right) = self.window_port_rect(bus, window_ptr);
        let (bounds_top, bounds_left) = self.port_bounds_top_left(bus, window_ptr);
        (
            top.wrapping_sub(bounds_top),
            left.wrapping_sub(bounds_left),
            bottom.wrapping_sub(bounds_top),
            right.wrapping_sub(bounds_left),
        )
    }

    fn global_rect_to_window_local(
        &self,
        bus: &MacMemoryBus,
        window_ptr: u32,
        rect: (i16, i16, i16, i16),
    ) -> (i16, i16, i16, i16) {
        let (bounds_top, bounds_left) = self.port_bounds_top_left(bus, window_ptr);
        (
            rect.0.wrapping_add(bounds_top),
            rect.1.wrapping_add(bounds_left),
            rect.2.wrapping_add(bounds_top),
            rect.3.wrapping_add(bounds_left),
        )
    }

    fn window_local_rect_to_global(
        &self,
        bus: &MacMemoryBus,
        window_ptr: u32,
        rect: (i16, i16, i16, i16),
    ) -> (i16, i16, i16, i16) {
        let (bounds_top, bounds_left) = self.port_bounds_top_left(bus, window_ptr);
        (
            rect.0.wrapping_sub(bounds_top),
            rect.1.wrapping_sub(bounds_left),
            rect.2.wrapping_sub(bounds_top),
            rect.3.wrapping_sub(bounds_left),
        )
    }

    pub(super) fn hidden_window_local_regions_for_origin_change(
        &self,
        bus: &MacMemoryBus,
        window_ptr: u32,
    ) -> Option<HiddenWindowLocalRegions> {
        if window_ptr == 0
            || self.window_visible(bus, window_ptr)
            || !self.window_list.contains(&window_ptr)
        {
            return None;
        }

        let content_local = self.window_content_rect(bus, window_ptr)?;
        let update_local = self
            .window_update_rect(bus, window_ptr)
            .map(|rect| self.global_rect_to_window_local(bus, window_ptr, rect));
        Some((content_local, update_local))
    }

    pub(super) fn sync_hidden_window_regions_after_origin_change(
        &self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        local_regions: Option<HiddenWindowLocalRegions>,
    ) {
        let Some((content_local, update_local)) = local_regions else {
            return;
        };
        if window_ptr == 0
            || self.window_visible(bus, window_ptr)
            || !self.window_list.contains(&window_ptr)
        {
            return;
        }

        // WindowRecord strucRgn, contRgn, and updateRgn are global
        // coordinates (Inside Macintosh Volume I, p. I-278). If a hidden
        // window's port origin changes before ShowWindow, preserve the
        // caller's local content/update boxes and re-express them in the new
        // global coordinate system. Visible windows keep normal SetOrigin
        // scrolling semantics.
        let global_content = self.window_local_rect_to_global(bus, window_ptr, content_local);
        let global_structure =
            self.window_structure_global_rect_for_window(bus, window_ptr, global_content);
        Self::write_region_handle_rect(
            bus,
            bus.read_long(window_ptr + Self::WINDOW_CONT_RGN_OFFSET),
            Some(global_content),
        );
        Self::write_region_handle_rect(
            bus,
            bus.read_long(window_ptr + Self::WINDOW_STRUC_RGN_OFFSET),
            Some(global_structure),
        );

        let update_global =
            update_local.map(|rect| self.window_local_rect_to_global(bus, window_ptr, rect));
        Self::write_region_handle_rect(
            bus,
            bus.read_long(window_ptr + Self::WINDOW_UPDATE_RGN_OFFSET),
            update_global,
        );
    }

    fn window_structure_global_rect_for_content(
        &self,
        bus: &MacMemoryBus,
        content_rect: (i16, i16, i16, i16),
    ) -> (i16, i16, i16, i16) {
        let mbar_h = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
        let title_top = if self.menu_bar_hidden {
            content_rect.0.saturating_sub(19).max(0)
        } else {
            content_rect.0.saturating_sub(19).max(mbar_h)
        };
        (
            title_top,
            content_rect.1.saturating_sub(1),
            content_rect.2.saturating_add(2),
            content_rect.3.saturating_add(2),
        )
    }

    pub(super) fn window_structure_global_rect_for_proc(
        &self,
        bus: &MacMemoryBus,
        content_rect: (i16, i16, i16, i16),
        proc_id: i16,
    ) -> (i16, i16, i16, i16) {
        match proc_id {
            // Match the standard WDEF chrome drawn by the HLE paths. A
            // dBoxProc must not transiently inherit the 19-pixel title area
            // of a document window while a frontend is sizing its viewport.
            1 => (
                content_rect.0.saturating_sub(8),
                content_rect.1.saturating_sub(8),
                content_rect.2.saturating_add(8),
                content_rect.3.saturating_add(8),
            ),
            2 => (
                content_rect.0.saturating_sub(1),
                content_rect.1.saturating_sub(1),
                content_rect.2.saturating_add(1),
                content_rect.3.saturating_add(1),
            ),
            3 => (
                content_rect.0.saturating_sub(1),
                content_rect.1.saturating_sub(1),
                content_rect.2.saturating_add(3),
                content_rect.3.saturating_add(3),
            ),
            _ => self.window_structure_global_rect_for_content(bus, content_rect),
        }
    }

    fn window_structure_global_rect_for_window(
        &self,
        bus: &MacMemoryBus,
        window_ptr: u32,
        content_rect: (i16, i16, i16, i16),
    ) -> (i16, i16, i16, i16) {
        let proc_id = self.window_proc_ids.get(&window_ptr).copied().unwrap_or(0);
        self.window_structure_global_rect_for_proc(bus, content_rect, proc_id)
    }

    pub(super) fn window_structure_rect(
        &self,
        bus: &MacMemoryBus,
        window_ptr: u32,
    ) -> Option<(i16, i16, i16, i16)> {
        if window_ptr == 0 {
            return None;
        }
        if let Some(rect) = Self::region_handle_rect(
            bus,
            bus.read_long(window_ptr + Self::WINDOW_STRUC_RGN_OFFSET),
        ) {
            return Some(rect);
        }
        self.window_content_global_rect(bus, window_ptr)
            .map(|content| self.window_structure_global_rect_for_window(bus, window_ptr, content))
    }

    fn erase_exposed_desktop_rect(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        if self.menu_bar_hidden || self.fullscreen_locked {
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top,
                left,
                bottom,
                right,
                true,
            );
        } else {
            // SetDeskCPat defines the desktop as the Window Manager's
            // patterned background. Systemless uses the standard QuickDraw
            // gray pattern when app-style hosting exposes desktop areas.
            // Inside Macintosh Volume V, V-210
            Self::fb_fill_pattern_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top,
                left,
                bottom,
                right,
                [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
            );
        }
    }

    fn erase_window_content_rect(
        &self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        rect: (i16, i16, i16, i16),
    ) {
        let Some(local_rect) = self
            .window_content_rect(bus, window_ptr)
            .and_then(|content| Self::rect_intersection(content, rect))
        else {
            return;
        };
        let (global_top, global_left, _, _) = self.window_global_port_rect(bus, window_ptr);
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            global_top.saturating_add(local_rect.0),
            global_left.saturating_add(local_rect.1),
            global_top.saturating_add(local_rect.2),
            global_left.saturating_add(local_rect.3),
            false,
        );
    }

    pub(super) fn save_screen_rect_pixels(
        &self,
        bus: &MacMemoryBus,
        rect: (i16, i16, i16, i16),
    ) -> Option<(i16, i16, i16, i16, Vec<u8>)> {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let top = rect.0.max(0).min(screen_height);
        let left = rect.1.max(0).min(screen_width);
        let bottom = rect.2.max(0).min(screen_height);
        let right = rect.3.max(0).min(screen_width);
        if top >= bottom || left >= right {
            return None;
        }

        let width = right - left;
        let height = bottom - top;
        let mut pixels = Vec::with_capacity(width as usize * height as usize);
        for y in top..bottom {
            for x in left..right {
                match pixel_size {
                    8 => {
                        pixels.push(bus.read_byte(screen_base + y as u32 * row_bytes + x as u32));
                    }
                    bits @ (2 | 4) => {
                        let pixels_per_byte = 8 / u32::from(bits);
                        let byte_offset = y as u32 * row_bytes + x as u32 / pixels_per_byte;
                        let shift =
                            8 - u32::from(bits) - (x as u32 % pixels_per_byte) * u32::from(bits);
                        pixels.push(
                            (bus.read_byte(screen_base + byte_offset) >> shift)
                                & ((1u8 << bits) - 1),
                        );
                    }
                    _ => {
                        let byte_offset = y as u32 * row_bytes + x as u32 / 8;
                        let bit = 7 - (x as u32 % 8);
                        pixels.push((bus.read_byte(screen_base + byte_offset) >> bit) & 1);
                    }
                }
            }
        }

        Some((top, left, width, height, pixels))
    }

    pub(super) fn restore_screen_rect_pixels(
        &self,
        bus: &mut MacMemoryBus,
        dst_top: i16,
        dst_left: i16,
        width: i16,
        height: i16,
        pixels: &[u8],
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let mut idx = 0usize;
        for dy in 0..height {
            let y = dst_top + dy;
            for dx in 0..width {
                let x = dst_left + dx;
                if idx >= pixels.len() {
                    return;
                }
                let pixel = pixels[idx];
                idx += 1;
                if x < 0 || y < 0 || x >= screen_width || y >= screen_height {
                    continue;
                }
                if pixel_size == 8 {
                    bus.write_byte(screen_base + y as u32 * row_bytes + x as u32, pixel);
                } else if matches!(pixel_size, 2 | 4) {
                    Self::fb_set_pixel_index(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        x,
                        y,
                        pixel,
                    );
                } else {
                    Self::fb_set_pixel(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        x,
                        y,
                        pixel != 0,
                    );
                }
            }
        }
    }

    pub(crate) fn window_is_document_proc(proc_id: i16) -> bool {
        matches!(proc_id, 0 | 4 | 8 | 12 | 16)
    }

    fn window_uses_save_under(&self, proc_id: i16) -> bool {
        !Self::window_is_document_proc(proc_id)
    }

    fn save_window_under_pixels_for_proc(
        &mut self,
        bus: &MacMemoryBus,
        window_ptr: u32,
        proc_id: i16,
    ) {
        if window_ptr == 0
            || !self.window_uses_save_under(proc_id)
            || self.window_saved_under_pixels.contains_key(&window_ptr)
        {
            return;
        }

        let Some(rect) = self.window_structure_rect(bus, window_ptr) else {
            return;
        };
        if let Some(saved) = self.save_screen_rect_pixels(bus, rect) {
            self.window_saved_under_pixels.insert(window_ptr, saved);
        }
    }

    fn save_window_under_pixels(&mut self, bus: &MacMemoryBus, window_ptr: u32) {
        let proc_id = self
            .window_proc_ids
            .get(&window_ptr)
            .copied()
            .unwrap_or(self.window_proc_id);
        self.save_window_under_pixels_for_proc(bus, window_ptr, proc_id);
    }

    fn restore_window_under_pixels(&mut self, bus: &mut MacMemoryBus, window_ptr: u32) -> bool {
        let Some((top, left, width, height, pixels)) =
            self.window_saved_under_pixels.remove(&window_ptr)
        else {
            return false;
        };
        self.restore_screen_rect_pixels(bus, top, left, width, height, &pixels);
        true
    }

    pub(super) fn fill_kiosk_stage_for_centered_game_surface(
        &self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
    ) {
        let valid_front_window = window_ptr != 0
            && window_ptr == self.front_window
            && self.window_visible(bus, window_ptr);
        if valid_front_window {
            let front_proc_id = self.window_proc_id(window_ptr);
            let front_rect = self.window_global_port_rect(bus, window_ptr);
            if front_proc_id == 2 && self.fill_kiosk_stage_around_rect(bus, front_rect) {
                return;
            }

            if !Self::window_is_document_proc(front_proc_id) {
                if let (Some(rect), Some(front_structure)) = (
                    self.last_screen_copybits_rect,
                    self.window_structure_rect(bus, window_ptr),
                ) {
                    let destination =
                        (rect.dst_top, rect.dst_left, rect.dst_bottom, rect.dst_right);
                    // The structure region includes the frame and content. Checking only the
                    // content port could let the kiosk fill overwrite a dialog border or shadow.
                    // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 4-12
                    let contains_front = destination.0 <= front_structure.0
                        && destination.1 <= front_structure.1
                        && destination.2 >= front_structure.2
                        && destination.3 >= front_structure.3;
                    if contains_front
                        && self.kiosk_stage_margins_are_uniform(bus, destination)
                        && self.fill_kiosk_stage_around_rect(bus, destination)
                    {
                        return;
                    }
                }
            }
            return;
        }

        // A game may replace or dispose its WindowRecord after presenting a
        // centered surface with CopyBits. Keep that explicit guest destination
        // as the aperture when its exposed bands remain one uniform index.
        if let Some(rect) = self.last_screen_copybits_rect {
            let destination = (rect.dst_top, rect.dst_left, rect.dst_bottom, rect.dst_right);
            if self.kiosk_stage_margins_are_uniform(bus, destination) {
                self.fill_kiosk_stage_around_rect(bus, destination);
            }
        }
    }

    fn move_window_to_global(
        &mut self,
        bus: &mut MacMemoryBus,
        the_window: u32,
        h_global: i16,
        v_global: i16,
        front_flag: bool,
    ) {
        if the_window == 0 {
            return;
        }

        let (_, _, screen_w, screen_h, _) = self.screen_mode;
        let old_port_rect = self.window_global_port_rect(bus, the_window);
        let old_structure = if self.window_visible(bus, the_window) {
            self.window_structure_rect(bus, the_window)
        } else {
            None
        };
        let local_content_rect = self
            .window_content_rect(bus, the_window)
            .unwrap_or_else(|| self.window_port_rect(bus, the_window));
        let old_update_rect = self.window_update_rect(bus, the_window);
        let moved_pixels = old_structure.and_then(|rect| self.save_screen_rect_pixels(bus, rect));
        let delta_v = v_global.wrapping_sub(old_port_rect.0);
        let delta_h = h_global.wrapping_sub(old_port_rect.1);

        // portRect stays in local coordinates (0,0,h,w) — unchanged.
        // Update pixmap bounds so local (0,0) maps to the new screen position.
        // Per Executor windInit.cpp lines 370-373 and
        // Inside Macintosh Volume I, I-289 (SetOrigin)
        let port_version = bus.read_word(the_window + 6);
        let is_cgraf = (port_version & 0xC000) == 0xC000;
        if is_cgraf {
            let pixmap_handle = bus.read_long(the_window + 2);
            let pixmap = bus.read_long(pixmap_handle);
            bus.write_word(pixmap + 6, (-v_global) as u16);
            bus.write_word(pixmap + 8, (-h_global) as u16);
            bus.write_word(pixmap + 10, (screen_h as i16 - v_global) as u16);
            bus.write_word(pixmap + 12, (screen_w as i16 - h_global) as u16);
        } else {
            // GrafPort: portBits.bounds at offset 2+6=8
            bus.write_word(the_window + 8, (-v_global) as u16);
            bus.write_word(the_window + 10, (-h_global) as u16);
            bus.write_word(the_window + 12, (screen_h as i16 - v_global) as u16);
            bus.write_word(the_window + 14, (screen_w as i16 - h_global) as u16);
        }

        // portRect, visRgn, clipRgn stay in local coords — no update needed.
        let global_content = self.window_local_rect_to_global(bus, the_window, local_content_rect);
        let global_structure =
            self.window_structure_global_rect_for_window(bus, the_window, global_content);
        Self::write_region_handle_rect(
            bus,
            bus.read_long(the_window + Self::WINDOW_CONT_RGN_OFFSET),
            Some(global_content),
        );
        Self::write_region_handle_rect(
            bus,
            bus.read_long(the_window + Self::WINDOW_STRUC_RGN_OFFSET),
            Some(global_structure),
        );
        if let Some(update_rect) = old_update_rect {
            Self::write_region_handle_rect(
                bus,
                bus.read_long(the_window + Self::WINDOW_UPDATE_RGN_OFFSET),
                Some((
                    update_rect.0.wrapping_add(delta_v),
                    update_rect.1.wrapping_add(delta_h),
                    update_rect.2.wrapping_add(delta_v),
                    update_rect.3.wrapping_add(delta_h),
                )),
            );
        }

        // If front=TRUE, bring theWindow to the front
        // (equivalent to SelectWindow per IM:I I-287).
        if front_flag {
            self.activate_as_front_window(bus, the_window);
        }

        // Moving a structure changes the visibility of every window behind
        // it. Rebuild their visible regions before invalidating the old
        // structure area; otherwise BeginUpdate keeps the stale hole at the
        // window's former position and the application can never repaint the
        // newly exposed content. Inside Macintosh Volume I, I-289, I-293,
        // I-297.
        self.recalculate_window_vis_regions(bus);
        if let Some(old_structure) = old_structure {
            let windows = self.window_list.clone();
            if let Some(moved_index) = windows.iter().position(|&window| window == the_window) {
                for &window in windows.iter().skip(moved_index + 1) {
                    if self.window_visible(bus, window) {
                        self.invalidate_window_global_rect(bus, window, old_structure);
                    }
                }
            }
        }

        // The screen copy preserves the pixels that were visible before the
        // move. Queue a content update as well so portions entering from
        // offscreen, or formerly covered by another window, are supplied by
        // the owning application rather than left as stale copied pixels.
        if self.window_visible(bus, the_window) {
            if let Some(content_rect) = self.window_content_rect(bus, the_window) {
                self.invalidate_window_rect(bus, the_window, content_rect);
            }
        }

        // Keep FindWindow hit-test bounds in sync (screen coords).
        let port_h = bus.read_word(the_window + 20) as i16;
        let port_w = bus.read_word(the_window + 22) as i16;
        if the_window == self.front_window {
            self.window_bounds = (v_global, h_global, v_global + port_h, h_global + port_w);
        }

        if let Some((top, left, bottom, right)) = old_structure {
            self.erase_exposed_desktop_rect(bus, top, left, bottom, right);
        }
        if let Some((top, left, width, height, pixels)) = moved_pixels {
            self.restore_screen_rect_pixels(
                bus,
                top.wrapping_add(delta_v),
                left.wrapping_add(delta_h),
                width,
                height,
                &pixels,
            );
        }

        if self.window_visible(bus, the_window) {
            let hilited = bus.read_byte(the_window + Self::WINDOW_HILITED_OFFSET) != 0;
            self.draw_single_window_chrome_inline(bus, the_window, hilited);
        }
    }

    fn find_window_at_point(
        &self,
        bus: &MacMemoryBus,
        pt_v: i16,
        pt_h: i16,
        mbar_h: i16,
    ) -> (i16, u32) {
        for &window_ptr in &self.window_list {
            if !self.window_visible(bus, window_ptr) {
                continue;
            }
            let (top, left, bottom, right) = self.window_global_port_rect(bus, window_ptr);
            if Self::point_in_rect(pt_v, pt_h, (top, left, bottom, right)) {
                return (3, window_ptr);
            }

            // The title/drag region sits above the content rect for ordinary
            // titled windows. FindWindow receives a global point and reports
            // a WindowPtr without activating it. Inside Macintosh Volume I,
            // I-287; MTE 1992 p. 4-91.
            let title_top = top.saturating_sub(20).max(mbar_h);
            if pt_v >= title_top && pt_v < top && pt_h >= left && pt_h <= right {
                return (4, window_ptr);
            }
        }
        (0, 0)
    }

    fn window_update_rect(
        &self,
        bus: &MacMemoryBus,
        window_ptr: u32,
    ) -> Option<(i16, i16, i16, i16)> {
        Self::region_handle_rect(
            bus,
            bus.read_long(window_ptr + Self::WINDOW_UPDATE_RGN_OFFSET),
        )
    }

    pub(super) fn window_has_pending_update(&self, bus: &MacMemoryBus, window_ptr: u32) -> bool {
        self.window_update_rect(bus, window_ptr).is_some()
    }

    /// Redraw dirty visible windows that have a picture installed through
    /// SetWindowPic, then leave ordinary dirty windows for update-event
    /// delivery.
    ///
    /// CheckUpdate ($A911)
    /// Scans windows from front to back; it redraws a dirty window whose
    /// windowPic field is non-NIL and continues scanning instead of returning
    /// an update event for that window.
    /// FUNCTION CheckUpdate (VAR theEvent: EventRecord): BOOLEAN;
    /// Macintosh Toolbox Essentials (1992), p. 4-116
    pub(crate) fn service_window_picture_updates<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<u32> {
        let windows = self.window_list.clone();
        for window in windows {
            if !self.window_visible(bus, window) || !self.window_has_pending_update(bus, window) {
                continue;
            }
            let picture = bus.read_long(window + Self::WINDOW_PIC_OFFSET);
            if picture == 0 || bus.read_long(picture) == 0 {
                return Some(window);
            }

            let old_port = self.current_port;
            let old_gdevice = self.current_gdevice;
            let old_sp = cpu.read_reg(Register::A7);
            self.set_current_port_state(bus, cpu, window, None);
            self.begin_update_window(bus, window);

            // DrawPicture(myPicture, &theWindow->portRect). Use temporary
            // guest-stack storage so the normal DrawPicture implementation
            // supplies PICT parsing, palette handling, and region clipping.
            let draw_sp = old_sp.wrapping_sub(16);
            let dst_rect = draw_sp + 8;
            let (top, left, bottom, right) = self.window_port_rect(bus, window);
            bus.write_word(dst_rect, top as u16);
            bus.write_word(dst_rect + 2, left as u16);
            bus.write_word(dst_rect + 4, bottom as u16);
            bus.write_word(dst_rect + 6, right as u16);
            bus.write_long(draw_sp, dst_rect);
            bus.write_long(draw_sp + 4, picture);
            cpu.write_reg(Register::A7, draw_sp);
            let _ = self.dispatch_quickdraw(true, 0x0F6, cpu, bus);

            self.end_update_window(bus, window);
            cpu.write_reg(Register::A7, old_sp);
            self.set_current_port_state(bus, cpu, old_port, Some(old_gdevice));
        }
        None
    }

    pub(crate) fn begin_update_window(&mut self, bus: &mut MacMemoryBus, window: u32) {
        if window == 0 {
            return;
        }
        if trace_inval_enabled() {
            let update_handle = bus.read_long(window + Self::WINDOW_UPDATE_RGN_OFFSET);
            let rect = Self::region_handle_rect(bus, update_handle);
            eprintln!(
                "[INVAL] BeginUpdate window=${:08X} update_handle=${:08X} update_rect_before={:?} tick={}",
                window, update_handle, rect, self.tick_count
            );
        }
        let Some(update_rect) = self.window_update_rect(bus, window) else {
            self.clear_queued_update_events(window);
            return;
        };
        if let Some(saved_vis) = Self::region_handle_rect(bus, bus.read_long(window + 24)) {
            self.saved_vis_regions.insert(window, saved_vis);
        }
        let vis_handle = bus.read_long(window + 24);
        let update_handle = bus.read_long(window + Self::WINDOW_UPDATE_RGN_OFFSET);
        // BeginUpdate sets visRgn to the intersection of visRgn and updateRgn,
        // so drawing during the update is confined to the parts that actually
        // need repainting.
        // BeginUpdate ($A922)
        // PROCEDURE BeginUpdate (theWindow: WindowPtr);
        // Inside Macintosh Volume I, I-292
        //
        // This has to be a true region intersection, not a bounding-box one:
        // visRgn already excludes any window sitting in front (CalcVis), and a
        // bounding-box intersection would hand those pixels back to the window
        // underneath, letting it paint over a modal dialog.
        if vis_handle != 0 && update_handle != 0 {
            // updateRgn is kept in global coordinates, visRgn in the window's
            // local ones. IM:I I-278. Shift the update region into local space
            // for the intersection; BeginUpdate empties it immediately after,
            // so the in-place offset is not observable.
            let (bounds_top, bounds_left) = self.port_bounds_top_left(bus, window);
            if bounds_top != 0 || bounds_left != 0 {
                Self::offset_region(bus, update_handle, bounds_left, bounds_top);
            }
            Self::write_region_boolean_op(
                bus,
                vis_handle,
                vis_handle,
                update_handle,
                RegionBooleanOp::Intersection,
            );
        } else {
            let update_rect = self.global_rect_to_window_local(bus, window, update_rect);
            let new_vis = Self::rect_intersection(
                Self::region_handle_rect(bus, vis_handle).unwrap_or((0, 0, 0, 0)),
                update_rect,
            );
            Self::write_region_handle_rect(bus, vis_handle, new_vis);
        }
        Self::write_region_handle_rect(
            bus,
            bus.read_long(window + Self::WINDOW_UPDATE_RGN_OFFSET),
            None,
        );
        self.clear_queued_update_events(window);
    }

    pub(crate) fn end_update_window(&mut self, bus: &mut MacMemoryBus, window: u32) {
        if let Some(saved_vis) = self.saved_vis_regions.remove(&window) {
            // EndUpdate restores the window's normal visRgn.
            // EndUpdate ($A923)
            // PROCEDURE EndUpdate (theWindow: WindowPtr);
            // Inside Macintosh Volume I, I-292
            //
            // Recompute it from the window list rather than replaying the
            // bounding box saved at BeginUpdate — the real visRgn can have
            // holes where other windows overlap, which a rect cannot carry.
            if !self.calc_window_vis_region(bus, window) {
                Self::write_region_handle_rect(bus, bus.read_long(window + 24), Some(saved_vis));
            }
        }
    }

    fn set_window_vis_from_content(&self, bus: &mut MacMemoryBus, window_ptr: u32, visible: bool) {
        let rect = if visible {
            self.window_content_rect(bus, window_ptr)
        } else {
            None
        };
        Self::write_region_handle_rect(bus, bus.read_long(window_ptr + 24), rect);
        self.recalculate_window_vis_regions(bus);
    }

    /// Recompute one window's visRgn: its content region minus the structure
    /// regions of every visible window in front of it.
    ///
    /// CalcVis ($A909)
    /// Calculates the visRgn of theWindow by subtracting the structure regions
    /// of all windows in front of it from its content region.
    /// PROCEDURE CalcVis (theWindow: WindowPeek);
    /// Inside Macintosh Volume I, I-297
    ///
    /// Without this, a background window's drawing is clipped only to its own
    /// content rect, so a full-window PaintRect from behind erases whatever a
    /// modal dialog (or any front window) has already painted on top of it.
    /// Returns `false` when the window carries no usable content region, so
    /// callers that need a definite visRgn can fall back.
    fn calc_window_vis_region(&self, bus: &mut MacMemoryBus, window_ptr: u32) -> bool {
        let vis_handle = bus.read_long(window_ptr + 24);
        if vis_handle == 0 {
            return false;
        }
        // CalcVis only ever runs on visible windows; ShowWindow/HideWindow own
        // the empty-visRgn case for hidden ones. IM:I I-283.
        if !self.window_visible(bus, window_ptr) {
            return false;
        }
        let cont_handle = bus.read_long(window_ptr + Self::WINDOW_CONT_RGN_OFFSET);
        if cont_handle == 0 || Self::region_handle_rect(bus, cont_handle).is_none() {
            return false;
        }

        // Start from a copy of the content region in global coordinates.
        Self::write_region_boolean_op(
            bus,
            vis_handle,
            cont_handle,
            cont_handle,
            RegionBooleanOp::Union,
        );

        let ghost_window = bus.read_long(crate::memory::globals::addr::GHOST_WINDOW);
        for &front in &self.window_list {
            if front == window_ptr {
                break;
            }
            if front == ghost_window || !self.window_visible(bus, front) {
                continue;
            }
            // Windows parked wholly off-screen never occlude anything on the
            // real screen, and their structure regions are far outside it.
            if self.windows_placed_offscreen.contains(&front) {
                continue;
            }
            let struc_handle = bus.read_long(front + Self::WINDOW_STRUC_RGN_OFFSET);
            if struc_handle == 0 || Self::region_handle_rect(bus, struc_handle).is_none() {
                continue;
            }
            Self::write_region_boolean_op(
                bus,
                vis_handle,
                vis_handle,
                struc_handle,
                RegionBooleanOp::Difference,
            );
        }

        // The Window Manager keeps the menu bar out of every window's visRgn.
        // Inside Macintosh Volume V, V-245. Apply it in global coordinates so
        // it tracks the window rather than following it around as a fixed
        // local inset.
        let mbar_h = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
        if mbar_h > 0 && !self.menu_bar_hidden {
            if let Some((top, left, bottom, right)) = Self::region_handle_rect(bus, vis_handle) {
                if top < mbar_h {
                    let clipped = (top.max(mbar_h), left, bottom, right);
                    let menu_bar_rgn = Self::alloc_rect_region_handle(bus, Some(clipped));
                    Self::write_region_boolean_op(
                        bus,
                        vis_handle,
                        vis_handle,
                        menu_bar_rgn,
                        RegionBooleanOp::Intersection,
                    );
                }
            }
        }

        // visRgn is kept in the window's local coordinates.
        let (bounds_top, bounds_left) = self.port_bounds_top_left(bus, window_ptr);
        if bounds_top != 0 || bounds_left != 0 {
            Self::offset_region(bus, vis_handle, bounds_left, bounds_top);
        }
        true
    }

    /// Apply [`Self::calc_window_vis_region`] to every tracked window.
    ///
    /// CalcVisBehind ($A90A)
    /// Recalculates the visible regions of startWindow and the windows behind
    /// it.
    /// PROCEDURE CalcVisBehind (startWindow: WindowPeek; clobberedRgn: RgnHandle);
    /// Inside Macintosh Volume I, I-297
    pub(crate) fn recalculate_window_vis_regions(&self, bus: &mut MacMemoryBus) {
        for &window_ptr in &self.window_list {
            // A window inside BeginUpdate/EndUpdate has a temporarily narrowed
            // visRgn that EndUpdate restores; recomputing it here would drop
            // the update clip. IM:I I-292.
            if self.saved_vis_regions.contains_key(&window_ptr) {
                continue;
            }
            let _ = self.calc_window_vis_region(bus, window_ptr);
        }
    }

    fn recalculate_window_regions_from_rect(
        &self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        source_rect: (i16, i16, i16, i16),
    ) {
        // CalcVis / CalcVisBehind clamp the top edge against the menu bar.
        let mbar_h = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
        let (bounds_top, _) = self.port_bounds_top_left(bus, window_ptr);
        let local_mbar_bottom = mbar_h.saturating_add(bounds_top);
        let local_vis_top = source_rect.0.max(local_mbar_bottom);
        let local_rect = (local_vis_top, source_rect.1, source_rect.2, source_rect.3);
        let global_content = self.window_local_rect_to_global(bus, window_ptr, local_rect);
        let global_structure =
            self.window_structure_global_rect_for_window(bus, window_ptr, global_content);

        Self::write_region_handle_rect(
            bus,
            bus.read_long(window_ptr + Self::WINDOW_CONT_RGN_OFFSET),
            Some(global_content),
        );
        Self::write_region_handle_rect(
            bus,
            bus.read_long(window_ptr + Self::WINDOW_STRUC_RGN_OFFSET),
            Some(global_structure),
        );
        for offset in [24u32, 28u32] {
            Self::write_region_handle_rect(
                bus,
                bus.read_long(window_ptr + offset),
                Some(local_rect),
            );
        }
    }

    fn init_window_manager_fields(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        content_rect: (i16, i16, i16, i16),
        proc_id: i16,
        visible: bool,
        go_away_flag: bool,
        ref_con: u32,
    ) {
        bus.write_word(
            window_ptr + Self::WINDOW_KIND_OFFSET,
            Self::USER_WINDOW_KIND,
        );
        bus.write_byte(
            window_ptr + Self::WINDOW_VISIBLE_OFFSET,
            if visible { 0xFF } else { 0x00 },
        );
        bus.write_byte(window_ptr + Self::WINDOW_HILITED_OFFSET, 0);
        bus.write_byte(
            window_ptr + Self::WINDOW_GO_AWAY_FLAG_OFFSET,
            if go_away_flag { 0xFF } else { 0x00 },
        );
        bus.write_byte(window_ptr + Self::WINDOW_SPARE_FLAG_OFFSET, 0);
        // WindowRecord strucRgn, contRgn, and updateRgn are maintained in
        // global coordinates. Inside Macintosh Volume I, p. I-278.
        let global_content = self.window_local_rect_to_global(bus, window_ptr, content_rect);
        let global_structure =
            self.window_structure_global_rect_for_proc(bus, global_content, proc_id);
        let struc_rgn = Self::alloc_rect_region_handle(bus, Some(global_structure));
        let cont_rgn = Self::alloc_rect_region_handle(bus, Some(global_content));
        let update_rgn = Self::alloc_rect_region_handle(bus, visible.then_some(global_content));
        bus.write_long(window_ptr + Self::WINDOW_STRUC_RGN_OFFSET, struc_rgn);
        bus.write_long(window_ptr + Self::WINDOW_CONT_RGN_OFFSET, cont_rgn);
        bus.write_long(window_ptr + Self::WINDOW_UPDATE_RGN_OFFSET, update_rgn);
        bus.write_long(window_ptr + Self::WINDOW_DEF_PROC_OFFSET, 0);
        bus.write_long(window_ptr + Self::WINDOW_DATA_HANDLE_OFFSET, 0);
        bus.write_long(window_ptr + Self::WINDOW_TITLE_HANDLE_OFFSET, 0);
        bus.write_word(window_ptr + Self::WINDOW_TITLE_WIDTH_OFFSET, 0);
        bus.write_long(window_ptr + Self::WINDOW_CONTROL_LIST_OFFSET, 0);
        bus.write_long(window_ptr + Self::WINDOW_PIC_OFFSET, 0);
        bus.write_long(window_ptr + Self::WINDOW_REFCON_OFFSET, ref_con);
        self.set_window_vis_from_content(bus, window_ptr, visible);
        self.track_window_front(bus, window_ptr);
    }

    pub(crate) fn window_visible(&self, bus: &MacMemoryBus, window_ptr: u32) -> bool {
        window_ptr != 0 && bus.read_byte(window_ptr + Self::WINDOW_VISIBLE_OFFSET) != 0
    }

    fn frontmost_visible_window_in_list(&self, bus: &MacMemoryBus) -> u32 {
        let ghost_window = bus.read_long(crate::memory::globals::addr::GHOST_WINDOW);
        self.window_list
            .iter()
            .copied()
            .find(|&w| w != ghost_window && self.window_visible(bus, w))
            .unwrap_or(0)
    }

    fn front_window_for_internal_state(&self, bus: &MacMemoryBus) -> u32 {
        let visible_window = self.frontmost_visible_window_in_list(bus);
        if visible_window != 0 {
            visible_window
        } else {
            self.window_list.first().copied().unwrap_or(0)
        }
    }

    pub(super) fn front_window_for_trap(&self, bus: &MacMemoryBus) -> u32 {
        self.frontmost_visible_window_in_list(bus)
    }

    fn frontmost_tracked_window(&self, bus: &MacMemoryBus) -> u32 {
        let ghost_window = bus.read_long(crate::memory::globals::addr::GHOST_WINDOW);
        self.window_list
            .iter()
            .copied()
            .find(|&w| w != ghost_window)
            .unwrap_or(0)
    }

    fn window_proc_id(&self, window_ptr: u32) -> i16 {
        self.window_proc_ids.get(&window_ptr).copied().unwrap_or(0)
    }

    fn window_is_custom_utility_layer_candidate(
        &self,
        bus: &MacMemoryBus,
        window_ptr: u32,
    ) -> bool {
        let proc_id = self.window_proc_id(window_ptr);
        self.window_visible(bus, window_ptr)
            && !Self::window_is_document_proc(proc_id)
            && self.window_uses_custom_def_proc(bus, window_ptr)
    }

    fn document_should_remain_active_behind_custom_utility(
        &self,
        bus: &MacMemoryBus,
        window_ptr: u32,
        behind: u32,
    ) -> bool {
        behind != 0
            && behind != 0xFFFF_FFFF
            && Self::window_is_document_proc(self.window_proc_id(window_ptr))
            && self.window_is_custom_utility_layer_candidate(bus, behind)
    }

    /// Allocate (or replace) the title StringHandle for a window and write
    /// the given bytes as a Pascal string into it.
    /// Inside Macintosh Volume I, I-276: titleHandle is a StringHandle.
    fn set_title_handle(bus: &mut MacMemoryBus, window_ptr: u32, title: &[u8]) {
        let len = title.len().min(255) as u8;
        let str_block = bus.alloc((1 + len as u32).max(2));
        bus.write_byte(str_block, len);
        for (i, &b) in title.iter().take(len as usize).enumerate() {
            bus.write_byte(str_block + 1 + i as u32, b);
        }
        let handle = bus.alloc(4);
        bus.write_long(handle, str_block);
        bus.write_long(window_ptr + Self::WINDOW_TITLE_HANDLE_OFFSET, handle);
    }

    fn sync_window_list_links(&self, bus: &mut MacMemoryBus) {
        for (index, &window_ptr) in self.window_list.iter().enumerate() {
            let next = self.window_list.get(index + 1).copied().unwrap_or(0);
            bus.write_long(window_ptr + Self::WINDOW_NEXT_WINDOW_OFFSET, next);
        }
        bus.write_long(
            Self::LOWMEM_WINDOW_LIST,
            self.window_list.first().copied().unwrap_or(0),
        );
        // Reordering the window list changes who occludes whom, so every
        // tracked window's visRgn has to be recalculated. IM:I I-297.
        self.recalculate_window_vis_regions(bus);
    }

    pub(crate) fn track_window_front(&mut self, bus: &mut MacMemoryBus, window_ptr: u32) {
        self.window_list.retain(|&tracked| tracked != window_ptr);
        self.window_list.insert(0, window_ptr);
        self.sync_window_list_links(bus);
    }

    /// Shared activation sequence for SelectWindow ($A91F) and
    /// MoveWindow(front=TRUE) ($A91B). Per IM:I I-286/I-287:
    ///   1. Unhilite the currently-active window + queue a deactivate
    ///      event (what=8, modifiers.activeFlag=0).
    ///   2. track_window_front(the_window) + update self.front_window.
    ///   3. Hilite the_window + queue an activate event
    ///      (modifiers.activeFlag=1).
    ///   4. Activate the palette for the new front.
    ///
    /// Idempotent when `the_window` is already the front or NIL.
    pub(crate) fn activate_as_front_window(&mut self, bus: &mut MacMemoryBus, the_window: u32) {
        if the_window == 0 || the_window == self.front_window {
            return;
        }
        let old_front = self.front_window;
        if old_front != 0 {
            bus.write_byte(old_front + Self::WINDOW_HILITED_OFFSET, 0x00);
            self.queue_window_activation_event(bus, old_front, false);
        }
        self.track_window_front(bus, the_window);
        self.front_window = the_window;
        self.sync_cached_front_window_render_state(bus);
        bus.write_byte(the_window + Self::WINDOW_HILITED_OFFSET, 0xFF);
        self.queue_window_activation_event(bus, the_window, true);
        self.activate_palette_for_window(bus, the_window);
    }

    fn activate_created_front_window(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        old_front: u32,
    ) {
        if window_ptr == 0 || window_ptr == old_front {
            return;
        }
        if old_front != 0 {
            bus.write_byte(old_front + Self::WINDOW_HILITED_OFFSET, 0x00);
            self.queue_window_activation_event(bus, old_front, false);
        }
        self.front_window = window_ptr;
        self.sync_cached_front_window_render_state(bus);
        bus.write_byte(window_ptr + Self::WINDOW_HILITED_OFFSET, 0xFF);
        self.queue_window_activation_event(bus, window_ptr, true);
        self.activate_palette_for_window(bus, window_ptr);
    }

    fn activate_frontmost_created_window_if_needed(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        _visible: bool,
        behind: u32,
        old_front: u32,
    ) {
        if behind == 0xFFFF_FFFF {
            // A frontmost NewWindow/GetNewWindow activates even when created
            // invisible. In that case its frame is not seen until ShowWindow,
            // but its activation still replaces the Window Manager's pending
            // CurActivate/CurDeactive pair.
            // Inside Macintosh: Macintosh Toolbox Essentials (1992),
            // pp. 4-76, 4-78, 4-81, and 4-84.
            self.activate_created_front_window(bus, window_ptr, old_front);
        }
    }

    fn activate_shown_front_window(&mut self, bus: &mut MacMemoryBus, the_window: u32) {
        if the_window == 0 {
            return;
        }
        self.front_window = the_window;
        self.sync_cached_front_window_render_state(bus);
        bus.write_byte(the_window + Self::WINDOW_HILITED_OFFSET, 0xFF);
        self.queue_window_activation_event(bus, the_window, true);
        self.activate_palette_for_window(bus, the_window);
    }

    /// Record one of the Window Manager's two pending activation events.
    ///
    /// Activate events bypass the Operating System event queue. The classic
    /// Window Manager instead exposes one pending activation and one pending
    /// deactivation through the `CurActivate` and `CurDeactive` low-memory
    /// globals ($0A64/$0A68). Replacing an occupied slot is important when an
    /// application changes front windows repeatedly before polling events: an
    /// unbounded FIFO can later name a window whose application-owned refCon
    /// storage has already been released.
    ///
    /// Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 2-51 and
    /// 4-51; A/UX Toolbox: Macintosh ROM Interface (1990), Appendix D.
    pub(crate) fn queue_window_activation_event(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        activating: bool,
    ) {
        if window_ptr == 0 {
            return;
        }
        let active_flag = u16::from(activating);
        self.event_queue
            .retain(|event| event.what != 8 || (event.modifiers & 1) != active_flag);
        bus.write_long(
            if activating {
                Self::LOWMEM_CUR_ACTIVATE
            } else {
                Self::LOWMEM_CUR_DEACTIVATE
            },
            window_ptr,
        );
        self.event_queue.push_back(QueuedEvent {
            what: 8,
            message: window_ptr,
            where_v: 0,
            where_h: 0,
            modifiers: active_flag,
        });
    }

    pub(crate) fn acknowledge_window_activation_event(
        &self,
        bus: &mut MacMemoryBus,
        event: &QueuedEvent,
    ) {
        if event.what != 8 {
            return;
        }
        let global = if (event.modifiers & 1) != 0 {
            Self::LOWMEM_CUR_ACTIVATE
        } else {
            Self::LOWMEM_CUR_DEACTIVATE
        };
        if bus.read_long(global) == event.message {
            bus.write_long(global, 0);
        }
    }

    /// Apply the Pascal `behind` parameter from NewWindow / NewCWindow /
    /// GetNewWindow / GetNewCWindow to reposition `window_ptr` in
    /// `window_list` per IM:I I-299:
    ///   behind == (WindowPtr)-1 (0xFFFFFFFF) → frontmost (default)
    ///   behind == NIL (0)                    → backmost
    ///   behind == specific ptr               → immediately behind it
    /// Unknown pointers fall back to backmost. Re-derives `front_window`
    /// via the visible-aware walk so the reshuffle can't land an
    /// invisible pointer in front.
    pub(crate) fn apply_behind_parameter(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        behind: u32,
    ) {
        if behind == 0xFFFFFFFF {
            // Default: stay at the front where init_cgraf_window
            // already placed us.
            return;
        }
        let covered_rect = Self::region_handle_rect(
            bus,
            bus.read_long(window_ptr + Self::WINDOW_STRUC_RGN_OFFSET),
        );
        self.window_list.retain(|&w| w != window_ptr);
        if behind == 0 {
            self.window_list.push(window_ptr);
        } else if let Some(idx) = self.window_list.iter().position(|&w| w == behind) {
            self.window_list.insert(idx + 1, window_ptr);
        } else {
            self.window_list.push(window_ptr);
        }
        self.sync_window_list_links(bus);
        if self.document_should_remain_active_behind_custom_utility(bus, window_ptr, behind) {
            // Floating utility windows/palettes remain visually above document
            // windows, but the active window is still the document the user is
            // working in. Overview 1992 p. 125 notes this active/frontmost
            // exception for apps that support floating windows; HIG 1992
            // pp. 137, 144 describes utility windows and palettes as floating
            // above document windows.
            self.front_window = window_ptr;
            bus.write_byte(behind + Self::WINDOW_HILITED_OFFSET, 0x00);
            bus.write_byte(window_ptr + Self::WINDOW_HILITED_OFFSET, 0xFF);
            self.sync_cached_front_window_render_state(bus);
        } else {
            self.front_window = self.front_window_for_internal_state(bus);
        }
        if self.front_window != window_ptr {
            self.sync_cached_front_window_render_state(bus);
        }
        // Creation draws a visible window before the Pascal `behind`
        // parameter is applied. If that parameter moves it behind existing
        // windows, its freshly drawn pixels have clobbered the windows that
        // remain in front. Add the overlap to each affected window's update
        // region so its application can repaint the exposed content.
        if let Some(rect) = covered_rect {
            let windows = self.window_list.clone();
            for &window in &windows {
                if window == window_ptr {
                    break;
                }
                if self.window_visible(bus, window) {
                    self.invalidate_window_global_rect(bus, window, rect);
                }
            }
        }
    }

    /// Resizes a tracked window port's visRgn to match its current `portRect`.
    /// No-op when it already matches, or when the port is not a window we track.
    ///
    /// `PortChanged` ($AB1D selector 9) is how an application tells QuickDraw
    /// that it edited a port's fields behind its back (Imaging With QuickDraw
    /// 1994, 4-103). HyperCard resizes its card window exactly that way — it
    /// writes `portRect` directly and calls `PortChanged` instead of
    /// `SizeWindow` — and the visRgn QuickDraw clips against has to follow, or
    /// the card is cut down to whatever rect `NewWindow` was given. Myst
    /// Preview's 544x332 card was being clipped to 512x342 that way.
    ///
    /// The content and structure regions are re-derived from the port as well,
    /// so compositing, hit-testing and the cached front-window bounds all agree
    /// with the rect the application is actually drawing into.
    pub(crate) fn resync_window_geometry_from_port_rect(
        &mut self,
        bus: &mut MacMemoryBus,
        the_window: u32,
    ) {
        if the_window == 0 || !self.window_list.contains(&the_window) {
            return;
        }
        let h = bus.read_word(the_window + 20) as i16;
        let w = bus.read_word(the_window + 22) as i16;
        if h <= 0 || w <= 0 {
            return;
        }

        let vis_rgn_handle = bus.read_long(the_window + 24);
        let vis_rgn = if vis_rgn_handle != 0 {
            bus.read_long(vis_rgn_handle)
        } else {
            0
        };
        if vis_rgn != 0
            && bus.read_word(vis_rgn + 6) as i16 == h
            && bus.read_word(vis_rgn + 8) as i16 == w
        {
            return;
        }

        if vis_rgn != 0 {
            // Keep a positive existing top, which is the menu bar clipping the
            // window; clamp a negative one away. A visRgn starting above the
            // port's own origin is left over from geometry the port no longer
            // has, and it would let the application draw outside its portRect.
            let vis_top = (bus.read_word(vis_rgn + 2) as i16).max(0);
            bus.write_word(vis_rgn + 2, vis_top as u16);
            bus.write_word(vis_rgn + 4, 0u16);
            bus.write_word(vis_rgn + 6, h as u16);
            bus.write_word(vis_rgn + 8, w as u16);
        }

        // Anchor the content region on the port's own origin. Carrying over the
        // previous local top would keep the window's content region offset from
        // the rect the port actually addresses, and everything derived from it —
        // the visRgn ShowHide recomputes included — would inherit the skew.
        let content_rect = (0, 0, h, w);
        let global_content = self.window_local_rect_to_global(bus, the_window, content_rect);
        let global_structure =
            self.window_structure_global_rect_for_window(bus, the_window, global_content);
        Self::write_region_handle_rect(
            bus,
            bus.read_long(the_window + Self::WINDOW_CONT_RGN_OFFSET),
            Some(global_content),
        );
        Self::write_region_handle_rect(
            bus,
            bus.read_long(the_window + Self::WINDOW_STRUC_RGN_OFFSET),
            Some(global_structure),
        );
        if the_window == self.front_window {
            self.window_bounds = global_content;
        }
    }

    pub(crate) fn untrack_window(&mut self, bus: &mut MacMemoryBus, window_ptr: u32) {
        self.window_list.retain(|&tracked| tracked != window_ptr);
        self.sync_window_list_links(bus);
        self.dialog_visible_snapshots.remove(&window_ptr);
        self.saved_vis_regions.remove(&window_ptr);
        self.window_proc_ids.remove(&window_ptr);
        self.windows_placed_offscreen.remove(&window_ptr);
        self.window_aux_records.remove(&window_ptr);
        self.window_original_pixmaps.remove(&window_ptr);
        self.window_saved_under_pixels.remove(&window_ptr);
        self.clear_queued_update_events(window_ptr);
        if self
            .dialog_tracking
            .as_ref()
            .is_some_and(|tracking| tracking.dialog_ptr == window_ptr)
        {
            self.dialog_tracking = None;
        }
        if self.front_window == window_ptr {
            // Promote the first VISIBLE window remaining in the list.
            // CloseWindow and DisposeWindow both route through
            // untrack_window; IM:I I-286 requires the next front to be
            // visible.
            let list = self.window_list.clone();
            self.front_window = list
                .into_iter()
                .find(|&w| self.window_visible(bus, w))
                .unwrap_or_else(|| self.window_list.first().copied().unwrap_or(0));
            self.sync_cached_front_window_render_state(bus);
        }
        if self.current_port == window_ptr {
            self.current_port = self.front_window;
        }
        self.saved_draw_old_regions.remove(&window_ptr);
    }

    pub(crate) fn sync_cached_front_window_render_state(&mut self, bus: &MacMemoryBus) {
        let front_window = self.front_window;
        if front_window == 0 || !self.window_visible(bus, front_window) {
            self.window_bounds = (0, 0, 0, 0);
            self.window_title.clear();
            self.window_proc_id = -1;
            self.go_away_flag = false;
            return;
        }

        self.window_bounds = self.window_global_port_rect(bus, front_window);
        self.window_proc_id = self
            .window_proc_ids
            .get(&front_window)
            .copied()
            .unwrap_or(0);
        self.go_away_flag = bus.read_byte(front_window + Self::WINDOW_GO_AWAY_FLAG_OFFSET) != 0;

        let title_handle = bus.read_long(front_window + Self::WINDOW_TITLE_HANDLE_OFFSET);
        self.window_title = if title_handle != 0 {
            let title_ptr = bus.read_long(title_handle);
            if title_ptr != 0 {
                String::from_utf8_lossy(&bus.read_pstring(title_ptr)).into_owned()
            } else {
                String::new()
            }
        } else {
            String::new()
        };
    }

    fn erase_window_for_removal(&mut self, bus: &mut MacMemoryBus, window_ptr: u32) {
        if window_ptr == 0 || !self.window_visible(bus, window_ptr) {
            return;
        }

        if !self.restore_window_under_pixels(bus, window_ptr) {
            if let Some((top, left, bottom, right)) = self.window_structure_rect(bus, window_ptr) {
                self.erase_exposed_desktop_rect(bus, top, left, bottom, right);
            }
        }
        bus.write_byte(window_ptr + Self::WINDOW_VISIBLE_OFFSET, 0x00);
        self.set_window_vis_from_content(bus, window_ptr, false);
    }

    fn visible_windows_behind(&self, bus: &MacMemoryBus, window_ptr: u32) -> Vec<u32> {
        let Some(index) = self
            .window_list
            .iter()
            .position(|&window| window == window_ptr)
        else {
            return Vec::new();
        };
        self.window_list[index + 1..]
            .iter()
            .copied()
            .filter(|&window| self.window_visible(bus, window))
            .collect()
    }

    fn invalidate_exposed_windows(
        &mut self,
        bus: &mut MacMemoryBus,
        windows: &[u32],
        exposed_rect: Option<(i16, i16, i16, i16)>,
    ) {
        let Some(exposed_rect) = exposed_rect else {
            return;
        };
        for &window in windows {
            if let Some(intersection) = self
                .window_content_global_rect(bus, window)
                .and_then(|content| Self::rect_intersection(content, exposed_rect))
            {
                self.invalidate_window_global_rect(bus, window, intersection);
            }
        }
    }

    fn apply_closewindow_front_promotion_side_effects(
        &mut self,
        bus: &mut MacMemoryBus,
        closed_window: u32,
        was_front: bool,
    ) {
        if !was_front {
            return;
        }

        if closed_window != 0 {
            bus.write_byte(closed_window + Self::WINDOW_HILITED_OFFSET, 0x00);
        }

        let new_front = self.front_window;
        if new_front != 0 {
            bus.write_byte(new_front + Self::WINDOW_HILITED_OFFSET, 0xFF);
            self.queue_window_activation_event(bus, new_front, true);
            self.draw_single_window_chrome_inline(bus, new_front, true);
        }
    }

    pub(crate) fn queue_window_update_event(&mut self, window_ptr: u32) {
        if window_ptr == 0 {
            return;
        }
        if self
            .event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == window_ptr)
        {
            return;
        }
        if self
            .flushed_update_events
            .iter()
            .any(|event| event.what == 6 && event.message == window_ptr)
        {
            return;
        }
        if std::env::var_os("SYSTEMLESS_TRACE_INVAL").is_some() {
            eprintln!(
                "[INVAL] queue_window_update_event window=${:08X} tick={}",
                window_ptr, self.tick_count
            );
        }
        self.event_queue.push_back(QueuedEvent {
            what: 6,
            message: window_ptr,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
    }

    fn clear_queued_update_events(&mut self, window_ptr: u32) {
        self.event_queue
            .retain(|event| !(event.what == 6 && event.message == window_ptr));
        self.flushed_update_events
            .retain(|event| !(event.what == 6 && event.message == window_ptr));
    }

    fn current_window_port(&self) -> u32 {
        self.current_port
    }

    pub(super) fn invalidate_window_rect(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        rect: (i16, i16, i16, i16),
    ) {
        let Some(clipped_rect) = self
            .window_content_rect(bus, window_ptr)
            .and_then(|content| Self::rect_intersection(content, rect))
        else {
            return;
        };
        let update_handle = bus.read_long(window_ptr + Self::WINDOW_UPDATE_RGN_OFFSET);
        let clipped_global = self.window_local_rect_to_global(bus, window_ptr, clipped_rect);
        let merged = Self::rect_union(
            Self::region_handle_rect(bus, update_handle),
            Some(clipped_global),
        );
        Self::write_region_handle_rect(bus, update_handle, merged);
        if std::env::var_os("SYSTEMLESS_TRACE_INVAL").is_some() {
            eprintln!(
                "[INVAL] invalidate_window_rect window=${:08X} rect=({},{},{},{}) tick={}",
                window_ptr,
                clipped_global.0,
                clipped_global.1,
                clipped_global.2,
                clipped_global.3,
                self.tick_count
            );
        }
        self.queue_window_update_event(window_ptr);
    }

    pub(super) fn invalidate_entire_visible_window(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
    ) {
        if !self.window_visible(bus, window_ptr) {
            return;
        }
        if let Some(content) = self.window_content_rect(bus, window_ptr) {
            self.invalidate_window_rect(bus, window_ptr, content);
        }
    }

    fn invalidate_window_global_rect(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        rect: (i16, i16, i16, i16),
    ) {
        let local_rect = self.global_rect_to_window_local(bus, window_ptr, rect);
        self.invalidate_window_rect(bus, window_ptr, local_rect);
    }

    pub(crate) fn validate_window_rect(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        rect: (i16, i16, i16, i16),
    ) {
        let update_handle = bus.read_long(window_ptr + Self::WINDOW_UPDATE_RGN_OFFSET);
        let global_rect = self.window_local_rect_to_global(bus, window_ptr, rect);
        let new_rect = self
            .window_update_rect(bus, window_ptr)
            .map(|update| Self::rect_difference_bbox(update, global_rect))
            .unwrap_or(None);
        Self::write_region_handle_rect(bus, update_handle, new_rect);
        if new_rect.is_none() {
            self.clear_queued_update_events(window_ptr);
        }
    }

    pub(crate) fn pending_update_event(
        &self,
        bus: &MacMemoryBus,
        event_mask: u16,
    ) -> Option<QueuedEvent> {
        let update_mask = 1u16 << 6;
        if (event_mask & update_mask) == 0 {
            return None;
        }

        // Iterate the window list in place. This runs on the event-polling
        // path, which a Mac event loop hits constantly, so cloning the list
        // just to walk it dominated allocation in whole-application
        // profiles. The front window is a fallback only when the list is
        // empty, which `chain` expresses without a temporary.
        let fallback = if self.window_list.is_empty() && self.front_window != 0 {
            Some(self.front_window)
        } else {
            None
        };

        self.window_list
            .iter()
            .copied()
            .chain(fallback)
            .find(|&window_ptr| {
                self.window_visible(bus, window_ptr)
                    && self.window_update_rect(bus, window_ptr).is_some()
            })
            .map(|window_ptr| QueuedEvent {
                what: 6,
                message: window_ptr,
                where_v: 0,
                where_h: 0,
                modifiers: 0,
            })
    }

    /// Initialise a CGrafPort-based window record at `window_ptr`.
    /// Shared by NewCWindow (0x245) and GetNewCWindow (0x246).
    pub(crate) fn init_cgraf_window<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        window_ptr: u32,
        screen_base: u32,
        wind_top: i16,
        wind_left: i16,
        wind_bottom: i16,
        wind_right: i16,
        wind_title: &str,
        wind_proc_id: i16,
        visible: bool,
        draw_initial_frame: bool,
        go_away_flag: bool,
        ref_con: u32,
    ) {
        let (_, pm_row_bytes, screen_w, screen_h, pixel_depth) = self.screen_mode;
        // Per Executor windInit.cpp lines 370-373, the Window Manager offsets
        // portBits.bounds by (-left, -top) so that local coordinate (0,0)
        // maps to the window's top-left screen pixel.
        // Reference: Inside Macintosh Volume I, I-289 (SetOrigin)
        let bounds_top = -wind_top;
        let bounds_left = -wind_left;
        let bounds_bottom = screen_h as i16 - wind_top;
        let bounds_right = screen_w as i16 - wind_left;

        let pixmap = bus.alloc(50);
        // Some CRTs (Centaurian 1.2.1) zero out low-mem globals
        // including ScrnBase ($0824) during their init pass — if our
        // caller passed screen_base=0 (read from $0824), fall back to
        // the runner's authoritative screen_mode so the pixmap's
        // baseAddr isn't NIL.
        let effective_screen_base = if screen_base != 0 {
            screen_base
        } else {
            self.screen_mode.0
        };
        bus.write_long(pixmap, effective_screen_base); // baseAddr
        bus.write_word(pixmap + 4, (pm_row_bytes as u16) | 0x8000); // rowBytes with PixMap flag
        bus.write_word(pixmap + 6, bounds_top as u16); // bounds.top
        bus.write_word(pixmap + 8, bounds_left as u16); // bounds.left
        bus.write_word(pixmap + 10, bounds_bottom as u16); // bounds.bottom
        bus.write_word(pixmap + 12, bounds_right as u16); // bounds.right
        bus.write_word(pixmap + 30, 0); // pixelType (chunky)
        bus.write_word(pixmap + 32, pixel_depth); // pixelSize
        bus.write_word(pixmap + 34, 1); // cmpCount
        bus.write_word(pixmap + 36, pixel_depth); // cmpSize
                                                  // Share the main GDevice's color table so DrawPicture remaps
                                                  // against the screen's palette when drawing into a window.
        let gd_handle = self.ensure_main_gdevice(bus);
        let gd_ptr = bus.read_long(gd_handle);
        let gd_pmap_handle = bus.read_long(gd_ptr + 22);
        let gd_pmap = bus.read_long(gd_pmap_handle);
        let gd_ctab_handle = bus.read_long(gd_pmap + 42);
        bus.write_long(pixmap + 42, gd_ctab_handle); // pmTable
        let pixmap_handle = bus.alloc(4);
        bus.write_long(pixmap_handle, pixmap);
        self.window_original_pixmaps
            .insert(window_ptr, pixmap_handle);

        bus.write_word(window_ptr, 0); // device
        bus.write_long(window_ptr + 2, pixmap_handle); // portPixMap
        bus.write_word(window_ptr + 6, 0xC000); // portVersion (CGrafPort flag)

        // portRect — in local coordinates (origin at window top-left)
        let port_height = wind_bottom - wind_top;
        let port_width = wind_right - wind_left;
        bus.write_word(window_ptr + 16, 0u16); // top = 0
        bus.write_word(window_ptr + 18, 0u16); // left = 0
        bus.write_word(window_ptr + 20, port_height as u16); // bottom = height
        bus.write_word(window_ptr + 22, port_width as u16); // right = width

        // The Window Manager clips visRgn to exclude the menu bar.
        // Inside Macintosh Volume V, V-245
        // In local coordinates, the menu bar is at y = mbar_h - wind_top.
        let mbar_h = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
        let vis_top_local = (mbar_h - wind_top).max(0);
        let mut content_rect = (vis_top_local, 0, port_height, port_width);
        // The menu-bar exclusion is a property of visRgn, not of the window's
        // content region, so the Window-Manager regions start from the whole
        // content area. CalcVis re-applies the exclusion in global
        // coordinates every time the window list changes, which keeps it
        // correct after the window moves.
        let mut region_content_rect = (0, 0, port_height, port_width);
        if self.menu_bar_hidden
            && matches!(wind_proc_id, 1 | 2 | 3 | 5)
            && wind_top <= mbar_h.saturating_add(2)
            && wind_left <= 2
            && wind_bottom >= screen_h as i16 - 2
            && wind_right >= screen_w as i16 - 2
        {
            // Kiosk-mode expansion genuinely enlarges the content area to the
            // screen-backed PixMap, so it applies to the regions too.
            content_rect = (bounds_top, bounds_left, bounds_bottom, bounds_right);
            region_content_rect = content_rect;
        }
        eprintln!(
            "[WINDOW-INIT] init_cgraf_window: window=${:08X} bounds=({},{},{},{}) MBarHeight={} → visRgn.top={}",
            window_ptr,
            wind_top,
            wind_left,
            wind_bottom,
            wind_right,
            mbar_h,
            content_rect.0,
        );

        // visRgn — in local coordinates
        let vis_rgn = bus.alloc(10);
        bus.write_word(vis_rgn, 10);
        bus.write_word(vis_rgn + 2, content_rect.0 as u16);
        bus.write_word(vis_rgn + 4, 0u16);
        bus.write_word(vis_rgn + 6, content_rect.2 as u16);
        bus.write_word(vis_rgn + 8, content_rect.3 as u16);
        let vis_rgn_handle = bus.alloc(4);
        bus.write_long(vis_rgn_handle, vis_rgn);
        bus.write_long(window_ptr + 24, vis_rgn_handle);

        // clipRgn — in local coordinates
        let clip_rgn = bus.alloc(10);
        bus.write_word(clip_rgn, 10);
        bus.write_word(clip_rgn + 2, content_rect.0 as u16);
        bus.write_word(clip_rgn + 4, 0u16);
        bus.write_word(clip_rgn + 6, content_rect.2 as u16);
        bus.write_word(clip_rgn + 8, content_rect.3 as u16);
        let clip_rgn_handle = bus.alloc(4);
        bus.write_long(clip_rgn_handle, clip_rgn);
        bus.write_long(window_ptr + 28, clip_rgn_handle);

        // Pen state defaults
        bus.write_long(window_ptr + 48, 0); // pnLoc
        bus.write_word(window_ptr + 52, 1); // pnSize.v
        bus.write_word(window_ptr + 54, 1); // pnSize.h
        bus.write_word(window_ptr + 56, 8); // pnMode (patCopy)
        self.init_cgraf_port_defaults(window_ptr, bus);
        // The WindowRecord's contRgn is the window's whole content area. The
        // menu-bar exclusion belongs to visRgn alone (CalcVis re-derives it,
        // and re-derives it again whenever the window moves) — baking it into
        // contRgn would follow the window around in local coordinates and
        // permanently blank its top rows. SimCity 2000 opens its budget and
        // palette windows at the top of the screen and then moves them down,
        // which lost the first row of every one of them.
        // Inside Macintosh Volume I, I-273 (content region);
        // Inside Macintosh Volume V, V-245 (visRgn excludes the menu bar).
        self.init_window_manager_fields(
            bus,
            window_ptr,
            region_content_rect,
            wind_proc_id,
            visible,
            go_away_flag,
            ref_con,
        );
        let window_def_proc = self.window_def_proc_handle(bus, wind_proc_id);
        bus.write_long(window_ptr + Self::WINDOW_DEF_PROC_OFFSET, window_def_proc);
        // OpenPort/OpenCPort initializes clipRgn to an arbitrarily large
        // rectangle; only visRgn is constrained to the visible content.
        Self::write_region_handle_rect(
            bus,
            bus.read_long(window_ptr + 28),
            Some((-32767, -32767, 32767, 32767)),
        );

        // Allocate a StringHandle for the title and store it in the window record.
        // Inside Macintosh Volume I, I-276: titleHandle is a StringHandle.
        Self::set_title_handle(bus, window_ptr, wind_title.as_bytes());

        // Window creation initializes the port as the current drawing target.
        // Individual NewWindow/NewCWindow callers restore the previous port
        // for the System 7.5.3 hidden/backmost cases that preserve it.
        self.set_current_port_state(bus, cpu, window_ptr, Some(gd_handle));

        // If the previous front window was a document-style window,
        // redraw its title bar as inactive (no close box, no stripes).
        // On a real Mac, only the front window shows active chrome.
        if self.front_window != 0 && Self::window_is_document_proc(self.window_proc_id) && visible {
            self.draw_window_chrome(bus, false);
        }

        if wind_top >= screen_h as i16
            || wind_left >= screen_w as i16
            || wind_bottom <= 0
            || wind_right <= 0
        {
            self.windows_placed_offscreen.insert(window_ptr);
        } else {
            self.windows_placed_offscreen.remove(&window_ptr);
        }

        self.front_window = window_ptr;
        self.window_title = wind_title.to_string();
        self.window_bounds = (wind_top, wind_left, wind_bottom, wind_right);
        self.window_proc_id = wind_proc_id;
        self.window_proc_ids.insert(window_ptr, wind_proc_id);
        self.ensure_window_aux_record(bus, window_ptr, gd_ctab_handle);
        self.go_away_flag = go_away_flag;

        let fullscreen_visible = visible
            && wind_top <= 0
            && wind_left <= 0
            && wind_bottom >= screen_h as i16
            && wind_right >= screen_w as i16;
        if fullscreen_visible {
            // Real full-screen game windows start from an erased content area
            // using the Window Manager desktop/background. In Systemless's
            // explicit kiosk mode the Mac menu bar/desktop is suppressed, so
            // exposed areas should stay on the black host stage rather than
            // flash the classic white desktop. When callers opt into the menu
            // bar, keep the normal Mac white background.
            Self::fb_fill_rect(
                bus,
                screen_base,
                pm_row_bytes,
                pixel_depth,
                screen_w as i16,
                screen_h as i16,
                0,
                0,
                screen_h as i16,
                screen_w as i16,
                self.menu_bar_hidden,
            );

            // Keep the initial update region pending. Applications are
            // allowed to defer their first frame until they receive the
            // updateEvt generated by NewWindow, including for full-screen
            // windows that omit standard document chrome.
            // Inside Macintosh Volume I (1985), p. I-281.
        }

        eprintln!(
            "[WINDOW] CGrafWindow: window=${:08X} pixmap=${:08X} bounds=({},{},{},{}) goAway={}",
            window_ptr, pixmap, wind_top, wind_left, wind_bottom, wind_right, go_away_flag
        );

        let hidden_menu_fullscreen_top = if mbar_h > 0 {
            mbar_h.saturating_add(2)
        } else {
            22
        };
        let suppress_document_chrome = self.menu_bar_hidden
            && Self::window_is_document_proc(wind_proc_id)
            && wind_top <= hidden_menu_fullscreen_top
            && wind_left <= 2
            && wind_bottom >= screen_h as i16 - 2
            && wind_right >= screen_w as i16 - 2;
        if visible {
            self.save_window_under_pixels_for_proc(bus, window_ptr, wind_proc_id);
            if draw_initial_frame
                && !suppress_document_chrome
                && !self.window_uses_custom_def_proc(bus, window_ptr)
            {
                self.draw_window_frame(bus);
            }
            self.queue_window_update_event(window_ptr);
        }
    }

    /// Initialise an old-style WindowRecord whose first field is a GrafPort.
    ///
    /// NewWindow and GetNewWindow call OpenPort and expose an embedded BitMap
    /// at offsets 2..15. NewCWindow/GetNewCWindow instead expose a
    /// PixMapHandle at offset 2. The records have the same total size, but
    /// callers such as HyperCard legitimately inspect the embedded portBits.
    fn init_graf_window<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        window_ptr: u32,
        screen_base: u32,
        wind_top: i16,
        wind_left: i16,
        wind_bottom: i16,
        wind_right: i16,
        wind_title: &str,
        wind_proc_id: i16,
        visible: bool,
        draw_initial_frame: bool,
        go_away_flag: bool,
        ref_con: u32,
    ) {
        self.init_cgraf_window(
            bus,
            cpu,
            window_ptr,
            screen_base,
            wind_top,
            wind_left,
            wind_bottom,
            wind_right,
            wind_title,
            wind_proc_id,
            visible,
            draw_initial_frame,
            go_away_flag,
            ref_con,
        );

        // Preserve the private PixMap used by the host renderer in
        // window_original_pixmaps, while publishing the documented GrafPort
        // layout to guest code. BitMap.rowBytes does not carry PixMap's high
        // flag bit.
        let pixmap_handle = self.window_original_pixmaps[&window_ptr];
        let pixmap = bus.read_long(pixmap_handle);
        bus.write_long(window_ptr + 2, Self::offscreen_pixmap_base_ptr(bus, pixmap));
        bus.write_word(window_ptr + 6, bus.read_word(pixmap + 4) & 0x3FFF);
        for offset in 0..8 {
            bus.write_byte(window_ptr + 8 + offset, bus.read_byte(pixmap + 6 + offset));
        }
    }

    /// Paint the newly exposed content of a visible window with its
    /// background pattern after its final z-order has been established.
    ///
    /// The Window Manager's PaintOne operation draws the frame, erases the
    /// exposed content with the window background pattern, and adds that
    /// content to updateRgn (Inside Macintosh Volume I, pp. I-278, I-296).
    /// NewWindow then reports an update event for the whole content area
    /// (pp. I-282..I-283).  The erase matters even when an application draws
    /// immediately instead of waiting for that event: transparent QuickDraw
    /// text modes preserve the destination outside their glyphs.
    ///
    /// Full-screen windows are initialized separately above because kiosk
    /// mode deliberately uses a black stage instead of the classic desktop.
    fn paint_new_window_content<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        window_ptr: u32,
        visible: bool,
    ) {
        if !visible {
            return;
        }

        let (top, left, bottom, right) = self.window_global_port_rect(bus, window_ptr);
        let (_, _, screen_width, screen_height, _) = self.screen_mode;
        if top <= 0 && left <= 0 && bottom >= screen_height as i16 && right >= screen_width as i16 {
            return;
        }

        let (top, left, bottom, right) = self.window_port_rect(bus, window_ptr);
        self.draw_rect(
            cpu,
            bus,
            &Rect {
                top,
                left,
                bottom,
                right,
            },
            ShapeOp::Erase,
        );
    }

    pub(crate) fn dispatch_window<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        Some(match (is_tool, trap_num) {
            // LayerDispatch ($A829), selector 2: IsLayer.
            //
            // LayerDispatch is a private Window/Layer Manager interface rather
            // than a documented Toolbox API. Myst's 68K call sites establish
            // the Pascal ABI used here: D0 selects IsLayer, the caller pushes a
            // four-byte WindowPtr after reserving a Boolean result word, and
            // the dispatcher consumes the pointer while leaving the result.
            // System 7.5.3 returns FALSE for ordinary WindowPtrs; this also
            // matches the clean-room compatibility behavior reported by MACE.
            (true, 0x029) if cpu.read_reg(Register::D0) as u16 == 2 => {
                let sp = cpu.read_reg(Register::A7);
                let _window_or_layer = bus.read_long(sp);
                bus.write_word(sp + 4, 0);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // InitWindows ($A912)
            // PROCEDURE InitWindows;
            // Inside Macintosh Volume I (1985), p. I-281:
            // initializes the Window Manager and creates the Window Manager
            // port, retrievable via GetWMgrPort; the assembly-language note
            // on p. I-282 says it initializes GrayRgn to the desktop region.
            //
            // Systemless keeps most InitWindows side effects as no-ops, but it
            // now ensures stable WMgrPort ($09DE) and GrayRgn ($09EE)
            // low-memory globals exist for callers using GetWMgrPort or the
            // GetGrayRgn macro.
            (true, 0x112) => {
                let _ = self.ensure_window_manager_port(bus);
                Ok(())
            }

            // NewWindow ($A913)
            // FUNCTION NewWindow(wStorage: Ptr; boundsRect: Rect; title: Str255;
            //   visible: BOOLEAN; procID: INTEGER; behind: WindowPtr;
            //   goAwayFlag: BOOLEAN; refCon: LONGINT): WindowPtr;
            // Inside Macintosh Volume I, I-299
            // NewWindow ($A913): Allocates full port structure with visRgn/clipRgn, reads procID/title/bounds, sets as front window
            (true, 0x113) => {
                let sp = cpu.read_reg(Register::A7);
                // Stack layout (26 bytes params + 4 bytes result):
                //   SP+0:  refCon (4)
                //   SP+4:  goAwayFlag (2)
                //   SP+6:  behind (4)
                //   SP+10: procID (2)
                //   SP+12: visible (2)
                //   SP+14: title (4)
                //   SP+18: boundsRect (4)
                //   SP+22: wStorage (4)
                //   SP+26: result (4)
                let bounds_rect_ptr = bus.read_long(sp + 18);
                let bounds_top = bus.read_word(bounds_rect_ptr) as i16;
                let bounds_left = bus.read_word(bounds_rect_ptr + 2) as i16;
                let bounds_bottom = bus.read_word(bounds_rect_ptr + 4) as i16;
                let bounds_right = bus.read_word(bounds_rect_ptr + 6) as i16;
                let title_ptr = bus.read_long(sp + 14);
                // Read Pascal BOOLEAN as the HIGH byte of its 2-byte stack
                // slot — MPW C pushes Boolean (1-byte type) into a word-
                // aligned slot with the value at the even offset and an
                // uninitialised garbage byte at the odd offset. Inside
                // Macintosh Volume V, V-238 (Pascal calling convention).
                let visible = bus.read_byte(sp + 12) != 0;
                let proc_id = bus.read_word(sp + 10) as i16;
                let go_away = bus.read_byte(sp + 4) != 0;
                let ref_con = bus.read_long(sp);
                let storage_ptr = bus.read_long(sp + 22);

                // Read title Pascal string
                let title = if title_ptr != 0 {
                    String::from_utf8_lossy(&bus.read_pstring(title_ptr)).into_owned()
                } else {
                    String::new()
                };

                // Honor the caller's wStorage parameter per IM:I I-299:
                // "If wStorage is NIL, NewWindow allocates the necessary
                // storage itself; otherwise it uses the storage pointed
                // to by wStorage."
                let window_ptr = if storage_ptr != 0 {
                    storage_ptr
                } else {
                    bus.alloc(256)
                };
                let (screen_base, _, scr_w, scr_h, _) = self.screen_mode;

                let behind = bus.read_long(sp + 6);

                eprintln!(
                    "[WINDOW] NewWindow: window=${:08X} procID={} visible={} behind=${:08X} title=\"{}\" bounds=({},{},{},{}) screen={}x{}",
                    window_ptr,
                    proc_id,
                    visible,
                    behind,
                    title,
                    bounds_top,
                    bounds_left,
                    bounds_bottom,
                    bounds_right,
                    scr_w,
                    scr_h,
                );

                let old_front = self.front_window;
                let old_current_port = self.current_port;
                let old_current_gdevice = self.current_gdevice;
                self.init_graf_window(
                    bus,
                    cpu,
                    window_ptr,
                    screen_base,
                    bounds_top,
                    bounds_left,
                    bounds_bottom,
                    bounds_right,
                    &title,
                    proc_id,
                    visible,
                    true,
                    go_away,
                    ref_con,
                );

                // Honor the Pascal `behind` parameter at SP+6 per IM:I
                // I-299 (factored into apply_behind_parameter).
                self.apply_behind_parameter(bus, window_ptr, behind);
                self.activate_frontmost_created_window_if_needed(
                    bus, window_ptr, visible, behind, old_front,
                );
                self.paint_new_window_content(bus, cpu, window_ptr, visible);
                if !visible || behind == 0 {
                    self.set_current_port_state(
                        bus,
                        cpu,
                        old_current_port,
                        Some(old_current_gdevice),
                    );
                }

                let param_size = 26u32;
                bus.write_long(sp + param_size, window_ptr);
                cpu.write_reg(Register::A7, sp + param_size);
                self.arm_window_def_on_create(cpu, bus, window_ptr, proc_id, true, visible);
                Ok(())
            }

            // GetNewWindow ($A9BD)
            //
            // Honors wStorage at SP+4 per IM:I I-300. Stack layout:
            // SP+0 behind(4), SP+4 wStorage(4), SP+8 windowID(2),
            // SP+10 result(4). NIL wStorage → allocate; non-NIL → use
            // the caller's WindowRecord.
            // GetNewWindow ($A9BD): Reads WIND resource, creates window with procID-based chrome, queues updateEvt
            (true, 0x1BD) => {
                let sp = cpu.read_reg(Register::A7);
                let window_id = bus.read_word(sp + 8) as i16;
                let Some((_, wind_ptr)) = self.find_or_load_resource_any(bus, *b"WIND", window_id)
                else {
                    // MTE 1992 p. 4-78: return NIL when the WIND template
                    // (or its defproc) cannot be read.
                    eprintln!(
                        "[WINDOW] GetNewWindow: WIND {} not found, returning NIL",
                        window_id
                    );
                    bus.write_long(sp + 10, 0);
                    cpu.write_reg(Register::A7, sp + 10);
                    return Some(Ok(()));
                };
                let wind_len = bus.get_alloc_size(wind_ptr).unwrap_or(0);
                let Some(template) = Self::parse_wind_template(bus, wind_ptr, wind_len) else {
                    eprintln!(
                        "[WINDOW] GetNewWindow: WIND {} is malformed, returning NIL",
                        window_id
                    );
                    bus.write_long(sp + 10, 0);
                    cpu.write_reg(Register::A7, sp + 10);
                    return Some(Ok(()));
                };
                let (top, left, bottom, right) = self.positioned_window_bounds(
                    bus,
                    template.bounds,
                    template.position,
                    template.proc_id,
                );
                eprintln!(
                    "[WINDOW] GetNewWindow: WIND {} bounds=({},{},{},{}) procID={} position=${:04X} title=\"{}\"",
                    window_id, top, left, bottom, right, template.proc_id, template.position, template.title
                );

                let storage_ptr = bus.read_long(sp + 4);
                let window_ptr = if storage_ptr != 0 {
                    storage_ptr
                } else {
                    bus.alloc(256)
                };
                let screen_base: u32 = bus.read_long(0x0824);
                bus.write_word(window_ptr, 0);

                let old_front = self.front_window;
                self.init_graf_window(
                    bus,
                    cpu,
                    window_ptr,
                    screen_base,
                    top,
                    left,
                    bottom,
                    right,
                    &template.title,
                    template.proc_id,
                    template.visible,
                    true,
                    template.go_away,
                    template.ref_con,
                );
                self.port_draw_states
                    .insert(window_ptr, PortDrawState::default());

                eprintln!(
                    "[WINDOW] GetNewWindow: window=${:08X} screen_base=${:08X}",
                    window_ptr, screen_base
                );

                let palette = self.copy_palette_resource(bus, window_id);
                if palette != 0 {
                    self.set_window_palette_association(window_ptr, palette, -0x2000);
                    self.activate_palette_for_window(bus, window_ptr);
                }

                // GetNewWindow stack is SP+0: behind(4), SP+4: wStorage(4),
                // SP+8: windowID(2). Honor `behind` post-init.
                let behind = bus.read_long(sp);
                self.apply_behind_parameter(bus, window_ptr, behind);
                self.activate_frontmost_created_window_if_needed(
                    bus,
                    window_ptr,
                    template.visible,
                    behind,
                    old_front,
                );
                self.paint_new_window_content(bus, cpu, window_ptr, template.visible);

                bus.write_long(sp + 10, window_ptr);
                cpu.write_reg(Register::A7, sp + 10);
                self.arm_window_def_on_create(
                    cpu,
                    bus,
                    window_ptr,
                    template.proc_id,
                    true,
                    template.visible,
                );
                Ok(())
            }

            // NewCWindow ($AA45)
            // FUNCTION NewCWindow(wStorage: Ptr; boundsRect: Rect; title: Str255;
            //   visible: BOOLEAN; procID: INTEGER; behind: WindowPtr;
            //   goAwayFlag: BOOLEAN; refCon: LONGINT): CWindowPtr;
            // Inside Macintosh Volume V, V-216
            // NewCWindow ($AA45): Creates CGrafPort with PixMap from explicit bounds/procID/title params (Inside Mac V, V-216)
            (true, 0x245) => {
                let sp = cpu.read_reg(Register::A7);
                // 68K Pascal stack (BOOLEAN = 2 bytes on A7):
                //   SP+0: refCon(4) SP+4: goAwayFlag(2) SP+6: behind(4)
                //   SP+10: procID(2) SP+12: visible(2)  SP+14: title(4)
                //   SP+18: boundsRect(4) SP+22: wStorage(4) SP+26: result(4)
                let bounds_rect_ptr = bus.read_long(sp + 18);
                let wind_top = bus.read_word(bounds_rect_ptr) as i16;
                let wind_left = bus.read_word(bounds_rect_ptr + 2) as i16;
                let wind_bottom = bus.read_word(bounds_rect_ptr + 4) as i16;
                let wind_right = bus.read_word(bounds_rect_ptr + 6) as i16;
                let title_ptr = bus.read_long(sp + 14);
                let wind_proc_id = bus.read_word(sp + 10) as i16;
                // Read Pascal BOOLEAN as the HIGH byte of its 2-byte
                // stack slot (MPW C convention).
                let visible = bus.read_byte(sp + 12) != 0;
                let go_away_flag = bus.read_byte(sp + 4) != 0;
                let ref_con = bus.read_long(sp);
                let storage_ptr = bus.read_long(sp + 22);

                let wind_title = if title_ptr != 0 {
                    String::from_utf8_lossy(&bus.read_pstring(title_ptr)).into_owned()
                } else {
                    String::new()
                };

                eprintln!(
                    "[WINDOW] NewCWindow: bounds=({},{},{},{}) procID={} visible={} title=\"{}\"",
                    wind_top, wind_left, wind_bottom, wind_right, wind_proc_id, visible, wind_title
                );

                // Honor wStorage same as NewWindow above.
                let window_ptr = if storage_ptr != 0 {
                    storage_ptr
                } else {
                    bus.alloc(256)
                };
                let screen_base: u32 = bus.read_long(0x0824);
                let old_front = self.front_window;
                let old_current_port = self.current_port;
                let old_current_gdevice = self.current_gdevice;
                self.init_cgraf_window(
                    bus,
                    cpu,
                    window_ptr,
                    screen_base,
                    wind_top,
                    wind_left,
                    wind_bottom,
                    wind_right,
                    &wind_title,
                    wind_proc_id,
                    visible,
                    true,
                    go_away_flag,
                    ref_con,
                );

                // Honor `behind` at SP+6 (same Pascal layout as NewWindow
                // since NewCWindow has identical signature).
                let behind = bus.read_long(sp + 6);
                self.apply_behind_parameter(bus, window_ptr, behind);
                self.activate_frontmost_created_window_if_needed(
                    bus, window_ptr, visible, behind, old_front,
                );
                self.paint_new_window_content(bus, cpu, window_ptr, visible);
                if !visible || behind == 0 {
                    self.set_current_port_state(
                        bus,
                        cpu,
                        old_current_port,
                        Some(old_current_gdevice),
                    );
                }

                let param_size = 26;
                bus.write_long(sp + param_size, window_ptr);
                cpu.write_reg(Register::A7, sp + param_size);
                self.arm_window_def_on_create(cpu, bus, window_ptr, wind_proc_id, true, visible);
                Ok(())
            }

            // GetNewCWindow ($AA46)
            // FUNCTION GetNewCWindow(windowID: INTEGER; wStorage: Ptr;
            //   behind: CWindowPtr): CWindowPtr;
            // Inside Macintosh Volume V, V-207
            // GetNewCWindow ($AA46): Creates CGrafPort from WIND resource template (Inside Mac V, V-207)
            (true, 0x246) => {
                let sp = cpu.read_reg(Register::A7);

                let window_id = bus.read_word(sp + 8) as i16;
                let Some((_, wind_ptr)) = self.find_or_load_resource_any(bus, *b"WIND", window_id)
                else {
                    // MTE 1992 p. 4-77: return NIL when the WIND template
                    // (or its defproc) cannot be read.
                    eprintln!(
                        "[WINDOW] GetNewCWindow: WIND {} not found, returning NIL",
                        window_id
                    );
                    bus.write_long(sp + 10, 0);
                    cpu.write_reg(Register::A7, sp + 10);
                    return Some(Ok(()));
                };
                let wind_len = bus.get_alloc_size(wind_ptr).unwrap_or(0);
                let Some(template) = Self::parse_wind_template(bus, wind_ptr, wind_len) else {
                    eprintln!(
                        "[WINDOW] GetNewCWindow: WIND {} is malformed, returning NIL",
                        window_id
                    );
                    bus.write_long(sp + 10, 0);
                    cpu.write_reg(Register::A7, sp + 10);
                    return Some(Ok(()));
                };
                let (top, left, bottom, right) = self.positioned_window_bounds(
                    bus,
                    template.bounds,
                    template.position,
                    template.proc_id,
                );
                eprintln!(
                    "[WINDOW] GetNewCWindow: WIND {} bounds=({},{},{},{}) procID={} position=${:04X} title=\"{}\"",
                    window_id, top, left, bottom, right, template.proc_id, template.position, template.title
                );

                // Honor wStorage at SP+4 (same layout as GetNewWindow).
                let storage_ptr = bus.read_long(sp + 4);
                let window_ptr = if storage_ptr != 0 {
                    storage_ptr
                } else {
                    bus.alloc(256)
                };
                let screen_base: u32 = bus.read_long(0x0824);
                let old_front = self.front_window;
                self.init_cgraf_window(
                    bus,
                    cpu,
                    window_ptr,
                    screen_base,
                    top,
                    left,
                    bottom,
                    right,
                    &template.title,
                    template.proc_id,
                    template.visible,
                    true,
                    template.go_away,
                    template.ref_con,
                );
                let wctab = self.copy_window_color_table_resource(bus, window_id);
                if wctab != 0 {
                    self.ensure_window_aux_record(bus, window_ptr, wctab);
                    self.apply_window_color_table(bus, window_ptr, wctab);
                }
                let palette = self.copy_palette_resource(bus, window_id);
                if palette != 0 {
                    self.set_window_palette_association(window_ptr, palette, -0x2000);
                    self.activate_palette_for_window(bus, window_ptr);
                }

                // Same `behind` stack slot as GetNewWindow.
                let behind = bus.read_long(sp);
                self.apply_behind_parameter(bus, window_ptr, behind);
                self.activate_frontmost_created_window_if_needed(
                    bus,
                    window_ptr,
                    template.visible,
                    behind,
                    old_front,
                );
                self.paint_new_window_content(bus, cpu, window_ptr, template.visible);

                bus.write_long(sp + 10, window_ptr);
                cpu.write_reg(Register::A7, sp + 10);
                self.arm_window_def_on_create(
                    cpu,
                    bus,
                    window_ptr,
                    template.proc_id,
                    true,
                    template.visible,
                );
                Ok(())
            }

            // CloseWindow ($A92D)
            // CloseWindow ($A92D): Removes the window from window list/screen.
            // If the window was frontmost and another exists behind it, the
            // latter window is highlighted and receives an activate event.
            // (Inside Macintosh Volume I, I-283)
            (true, 0x12D) => {
                let sp = cpu.read_reg(Register::A7);
                let the_window = bus.read_long(sp);
                let was_front = self.front_window == the_window;
                let exposed_rect = self.window_structure_rect(bus, the_window);
                let windows_behind = self.visible_windows_behind(bus, the_window);
                self.erase_window_for_removal(bus, the_window);
                self.untrack_window(bus, the_window);
                self.invalidate_exposed_windows(bus, &windows_behind, exposed_rect);
                self.apply_closewindow_front_promotion_side_effects(bus, the_window, was_front);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // DisposeWindow ($A914)
            // DisposeWindow ($A914): Calls CloseWindow semantics, then releases
            // the window record. HLE currently applies the CloseWindow-visible
            // effects here (remove/promote/highlight/activate).
            // (Inside Macintosh Volume I, I-283 to I-284)
            (true, 0x114) => {
                let sp = cpu.read_reg(Register::A7);
                let the_window = bus.read_long(sp);
                let was_front = self.front_window == the_window;
                let exposed_rect = self.window_structure_rect(bus, the_window);
                let windows_behind = self.visible_windows_behind(bus, the_window);
                self.erase_window_for_removal(bus, the_window);
                self.untrack_window(bus, the_window);
                self.invalidate_exposed_windows(bus, &windows_behind, exposed_rect);
                self.apply_closewindow_front_promotion_side_effects(bus, the_window, was_front);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SelectWindow ($A91F)
            // Brings the specified window to the front and highlights it.
            // PROCEDURE SelectWindow (theWindow: WindowPtr);
            // Inside Macintosh Volume I, I-286
            //
            // Per IM:I I-286 SelectWindow must unhilite the old front,
            // bring theWindow to the front, hilite it, and queue
            // deactivate+activate events. Shared with MoveWindow(front=TRUE).
            // SelectWindow ($A91F): Brings window to front; queues deactivate event for old front and activate event for new front per IM:I I-284
            (true, 0x11F) => {
                let sp = cpu.read_reg(Register::A7);
                let the_window = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);
                self.activate_as_front_window(bus, the_window);
                Ok(())
            }

            // ShowWindow ($A915)
            // Makes the specified window visible.
            // PROCEDURE ShowWindow (theWindow: WindowPtr);
            // Inside Macintosh Volume I, I-286
            // ShowWindow ($A915): Sets visible flag, updates visRgn/clipRgn, queues updateEvt, and redraws chrome per IM:I I-284/I-286
            (true, 0x115) => {
                let sp = cpu.read_reg(Register::A7);
                let requested_window = bus.read_long(sp);
                let mut resolved_user_item_proc = false;
                // userItem item fields are ProcPtrs, not WindowPtrs
                // (IM:I I-405/I-421). In the documented hidden-dialog
                // setup flow, SetDItem installs those procs before
                // ShowWindow reveals the dialog. If the current front
                // dialog owns the supplied ProcPtr, reveal that dialog
                // and queue its userItem draw procs instead of writing
                // through procedure memory as though it were a WindowRecord.
                let the_window = if let Some(dialog_ptr) =
                    self.front_dialog_for_user_item_proc(requested_window)
                {
                    resolved_user_item_proc = true;
                    if std::env::var_os("SYSTEMLESS_TRACE_INVAL").is_some() {
                        eprintln!(
                                "[INVAL] ShowWindow resolved userItem proc ${:08X} to front dialog ${:08X}",
                                requested_window, dialog_ptr
                            );
                    }
                    dialog_ptr
                } else {
                    requested_window
                };
                if std::env::var_os("SYSTEMLESS_TRACE_INVAL").is_some() {
                    eprintln!(
                        "[INVAL] ShowWindow pc=${:08X} requested=${:08X} window=${:08X} front=${:08X} tick={}",
                        cpu.read_reg(Register::PC).wrapping_sub(2),
                        requested_window,
                        the_window,
                        self.front_window,
                        self.tick_count
                    );
                }
                // A userItem ProcPtr is not a WindowPtr.
                if self.front_dialog_has_user_item_proc(the_window) {
                    if std::env::var_os("SYSTEMLESS_TRACE_INVAL").is_some() {
                        eprintln!(
                            "[INVAL] ShowWindow skipped userItem proc ${:08X} on front dialog ${:08X}",
                            the_window,
                            self.front_window
                        );
                    }
                    cpu.write_reg(Register::A7, sp + 4);
                    return Some(Ok(()));
                }
                let mut arm_custom_wdef_draw = false;
                if the_window != 0 {
                    let was_visible = self.window_visible(bus, the_window);
                    let was_front = self.frontmost_tracked_window(bus) == the_window;
                    bus.write_byte(the_window + Self::WINDOW_VISIBLE_OFFSET, 0xFF);
                    self.set_window_vis_from_content(bus, the_window, true);
                    if !was_visible {
                        self.save_window_under_pixels(bus, the_window);
                    }
                    if !was_visible {
                        if was_front {
                            // Inside Macintosh Volume I, I-285: ShowWindow
                            // of an invisible frontmost window highlights it
                            // and generates an activate event.
                            self.activate_shown_front_window(bus, the_window);
                        }
                        if let Some(content_rect) = self.window_content_global_rect(bus, the_window)
                        {
                            Self::write_region_handle_rect(
                                bus,
                                bus.read_long(the_window + Self::WINDOW_UPDATE_RGN_OFFSET),
                                Some(content_rect),
                            );
                            self.queue_window_update_event(the_window);
                        }
                    }
                    // Redraw the now-visible window's chrome inline so
                    // captures that don't run a composite_frame pass still
                    // show the title bar. Uses the window's HILITED byte
                    // for active/inactive state.
                    if self.dialog_items.contains_key(&the_window) && !was_visible {
                        self.redraw_dialog_window_contents(bus, the_window);
                        if resolved_user_item_proc {
                            self.queue_modeless_dialog_draw_procs(bus, the_window);
                        }
                        if !self.modeless_dialog_cdef_draw_queue.contains(&the_window) {
                            self.modeless_dialog_cdef_draw_queue.push_back(the_window);
                        }
                    } else {
                        let hilited = bus.read_byte(the_window + Self::WINDOW_HILITED_OFFSET) != 0;
                        if self.window_uses_custom_def_proc(bus, the_window) {
                            arm_custom_wdef_draw = !was_visible;
                        } else {
                            let proc_id =
                                self.window_proc_ids.get(&the_window).copied().unwrap_or(0);
                            if !was_visible && matches!(proc_id, 1..=3) {
                                if let Some((top, left, bottom, right)) =
                                    self.window_content_global_rect(bus, the_window)
                                {
                                    let (
                                        screen_base,
                                        row_bytes,
                                        screen_width,
                                        screen_height,
                                        pixel_size,
                                    ) = self.get_screen_params();
                                    Self::fb_fill_rect(
                                        bus,
                                        screen_base,
                                        row_bytes,
                                        pixel_size,
                                        screen_width,
                                        screen_height,
                                        top,
                                        left,
                                        bottom,
                                        right,
                                        false,
                                    );
                                }
                            }
                            self.draw_single_window_chrome_inline(bus, the_window, hilited);
                        }
                    }
                    self.capture_gui_frame(bus, &format!("show_window_{:08X}", the_window));
                }
                cpu.write_reg(Register::A7, sp + 4);
                if arm_custom_wdef_draw {
                    self.arm_window_def_draw(cpu, bus, the_window);
                }
                Ok(())
            }
            // HideWindow ($A916)
            // Makes the specified window invisible.
            // PROCEDURE HideWindow (theWindow: WindowPtr);
            // Inside Macintosh Volume I, I-286
            //
            // If the hidden window was the front window, the first
            // visible window behind it becomes the new front per IM:I
            // I-286 ("If the window is the front window, the window
            // behind it becomes the front window and receives an
            // activate event"). ShowWindow does NOT touch stacking
            // order (correct per IM).
            // HideWindow ($A916): Clears visible flag, erases chrome, promotes next visible window to front, updates hilited bytes; missing activate event for promoted window (IM:I I-285)
            (true, 0x116) => {
                let sp = cpu.read_reg(Register::A7);
                let the_window = bus.read_long(sp);
                if std::env::var_os("SYSTEMLESS_TRACE_INVAL").is_some() {
                    eprintln!(
                        "[INVAL] HideWindow pc=${:08X} window=${:08X} front=${:08X} tick={}",
                        cpu.read_reg(Register::PC).wrapping_sub(2),
                        the_window,
                        self.front_window,
                        self.tick_count
                    );
                }
                // A userItem ProcPtr is not a WindowPtr. Treat ProcPtrs
                // installed on the current front dialog as invalid window
                // arguments here rather than writing through code memory.
                if self.front_dialog_has_user_item_proc(the_window) {
                    if std::env::var_os("SYSTEMLESS_TRACE_INVAL").is_some() {
                        eprintln!(
                            "[INVAL] HideWindow skipped userItem proc ${:08X} on front dialog ${:08X}",
                            the_window,
                            self.front_window
                        );
                    }
                    cpu.write_reg(Register::A7, sp + 4);
                    return Some(Ok(()));
                }
                if the_window != 0 {
                    if self.dialog_items.contains_key(&the_window) {
                        self.dialog_visible_snapshots.remove(&the_window);
                        if self
                            .retained_modal_dialog_click
                            .as_ref()
                            .is_some_and(|click| click.dialog_ptr == the_window)
                        {
                            self.retained_modal_dialog_click = None;
                        }
                    }
                    let was_visible = self.window_visible(bus, the_window);
                    if !was_visible {
                        bus.write_byte(the_window + Self::WINDOW_VISIBLE_OFFSET, 0x00);
                        self.set_window_vis_from_content(bus, the_window, false);
                        self.clear_queued_update_events(the_window);
                        if self.current_port == the_window {
                            self.current_port = self.front_window;
                        }
                        cpu.write_reg(Register::A7, sp + 4);
                        return Some(Ok(()));
                    }
                    let exposed_rect = self.window_structure_rect(bus, the_window);
                    let windows_behind = self.visible_windows_behind(bus, the_window);
                    // Erase the window's chrome area BEFORE clearing
                    // visible so the screen doesn't retain stale chrome.
                    let (wind_top, wind_left, wind_bottom, wind_right) = {
                        let port_version = bus.read_word(the_window + 6);
                        let (pmap_top, pmap_left) = if (port_version & 0xC000) == 0xC000 {
                            let pm_handle = bus.read_long(the_window + 2);
                            let pm_ptr = bus.read_long(pm_handle);
                            (
                                bus.read_word(pm_ptr + 6) as i16,
                                bus.read_word(pm_ptr + 8) as i16,
                            )
                        } else {
                            (
                                bus.read_word(the_window + 8) as i16,
                                bus.read_word(the_window + 10) as i16,
                            )
                        };
                        // wrapping ops match 68k Mac OS i16 wrap-
                        // around (avoids debug-build panic on
                        // ports with bounds.topLeft = i16::MIN or
                        // span exceeding i16 range).
                        let wt = pmap_top.wrapping_neg();
                        let wl = pmap_left.wrapping_neg();
                        let pb = bus.read_word(the_window + 20) as i16;
                        let pr = bus.read_word(the_window + 22) as i16;
                        (wt, wl, wt.wrapping_add(pb), wl.wrapping_add(pr))
                    };
                    bus.write_byte(the_window + Self::WINDOW_VISIBLE_OFFSET, 0x00);
                    self.set_window_vis_from_content(bus, the_window, false);
                    self.clear_queued_update_events(the_window);
                    if !self.restore_window_under_pixels(bus, the_window) {
                        // Restore the exposed structure area from the desktop
                        // background. A full Window Manager would also repaint
                        // uncovered windows behind; this keeps desktop exposure
                        // mode-correct when no save-under snapshot exists.
                        let (_, _, screen_width, screen_height, _) = self.get_screen_params();
                        self.erase_exposed_desktop_rect(
                            bus,
                            (wind_top - 19).max(0),
                            (wind_left - 1).max(0),
                            (wind_bottom + 2).min(screen_height),
                            (wind_right + 2).min(screen_width),
                        );
                    }
                    if self.front_window == the_window {
                        // Find first visible window in the list (other
                        // than the one we just hid). window_list is
                        // ordered front-to-back so the first visible
                        // entry after the hidden one becomes the new
                        // front.
                        let list = self.window_list.clone();
                        let new_front = list
                            .into_iter()
                            .find(|&w| w != the_window && self.window_visible(bus, w))
                            .unwrap_or(0);
                        // Per IM:I I-285/I-286, hiding the front window
                        // promotes the next visible window and generates
                        // the same activate/deactivate side effects as
                        // bringing that window frontmost directly.
                        bus.write_byte(the_window + Self::WINDOW_HILITED_OFFSET, 0x00);
                        if new_front != 0 {
                            self.activate_as_front_window(bus, new_front);
                        } else {
                            self.front_window = 0;
                        }
                        if self.current_port == the_window {
                            self.current_port = new_front;
                        }
                    }
                    self.invalidate_exposed_windows(bus, &windows_behind, exposed_rect);
                    self.capture_gui_frame(bus, &format!("hide_window_{:08X}", the_window));
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SetWTitle ($A91A)
            // Sets the window's title to the given string.
            // PROCEDURE SetWTitle(theWindow: WindowPtr; title: Str255);
            // Inside Macintosh Volume I, I-284
            //
            // SetWTitle ($A91A): Allocates StringHandle and writes Pascal string to window+134 per IM:I I-284
            (true, 0x11A) => {
                let sp = cpu.read_reg(Register::A7);
                let title_ptr = bus.read_long(sp);
                let the_window = bus.read_long(sp + 4);
                if the_window != 0 && title_ptr != 0 {
                    let bytes = bus.read_pstring(title_ptr);
                    Self::set_title_handle(bus, the_window, &bytes);
                    if the_window == self.front_window {
                        self.window_title = String::from_utf8_lossy(&bytes).into_owned();
                    }
                    if self.window_visible(bus, the_window) {
                        let hilited = bus.read_byte(the_window + Self::WINDOW_HILITED_OFFSET) != 0;
                        self.draw_single_window_chrome_inline(bus, the_window, hilited);
                    }
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // GetWTitle ($A919)
            // Returns the window's title as the value of the title parameter.
            // PROCEDURE GetWTitle(theWindow: WindowPtr; VAR title: Str255);
            // Inside Macintosh Volume I, I-284
            //
            // GetWTitle ($A919): Reads title from titleHandle StringHandle at window+134 per IM:I I-284
            (true, 0x119) => {
                let sp = cpu.read_reg(Register::A7);
                let title_out = bus.read_long(sp);
                let the_window = bus.read_long(sp + 4);
                if title_out != 0 {
                    let title_handle = bus.read_long(the_window + Self::WINDOW_TITLE_HANDLE_OFFSET);
                    if the_window != 0 && title_handle != 0 {
                        let str_ptr = bus.read_long(title_handle);
                        if str_ptr != 0 {
                            let bytes = bus.read_pstring(str_ptr);
                            bus.write_pstring(title_out, &bytes);
                        } else {
                            bus.write_byte(title_out, 0);
                        }
                    } else {
                        bus.write_byte(title_out, 0);
                    }
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // FrontWindow ($A924)
            // FUNCTION FrontWindow: WindowPtr;
            // Inside Macintosh Volume I, I-274
            //
            // Returns the first visible window in the Window Manager list.
            // If there are no visible windows, it returns NIL. The low-memory
            // GhostWindow is skipped when present.
            // Inside Macintosh Volume I, I-274; Macintosh Toolbox Essentials
            // 1992, p. 4-93.
            (true, 0x124) => {
                let sp = cpu.read_reg(Register::A7);
                let result = self.front_window_for_trap(bus);
                bus.write_long(sp, result);
                Ok(())
            }

            // FindWindow ($A92C)
            // FUNCTION FindWindow(thePt: Point; VAR whichWindow: WindowPtr): INTEGER;
            // Pascal stack (last param at top): SP+0=whichWindow(4), SP+4=thePt(4), SP+8=result(2)
            // FindWindow ($A92C): Returns inContent (3) if point is inside any window bounds
            (true, 0x12C) => {
                let sp = cpu.read_reg(Register::A7);
                let wnd_ptr_ptr = bus.read_long(sp); // VAR whichWindow
                let pt_v = bus.read_word(sp + 4) as i16; // thePt.v
                let pt_h = bus.read_word(sp + 6) as i16; // thePt.h

                // Determine which part of the screen was clicked.
                // Mac FindWindow part codes:
                //   0 = inDesk, 1 = inMenuBar, 3 = inContent, 4 = inDrag, 5 = inGrow
                // When games hide the menu bar they set MBarHeight=0; clicks at the
                // top of the screen then fall through to window hit-testing instead.
                // Inside Macintosh Volume I, I-287; Inside Macintosh Volume V, V-245
                let mbar_h = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
                let native_menu_click = self.pending_native_menu_selection.is_some();
                let (part, window_ptr) = if native_menu_click || (mbar_h > 0 && pt_v < mbar_h) {
                    (1, 0) // inMenuBar
                } else {
                    self.find_window_at_point(bus, pt_v, pt_h, mbar_h)
                };

                // MTE 1992 p. 4-91: `theWindow` must be NIL when the point
                // is not in a window (for example inDesk or inMenuBar).
                if wnd_ptr_ptr != 0 {
                    bus.write_long(wnd_ptr_ptr, window_ptr);
                }

                if super::dispatch::trace_input_enabled() {
                    eprintln!(
                        "[INPUT] FindWindow point=({}, {}) -> part={} window=${:08X}",
                        pt_v, pt_h, part, window_ptr
                    );
                }
                if trace_dragwindow_enabled() {
                    eprintln!(
                        "[DRAGWINDOW] FindWindow point=({}, {}) -> part={} window=${:08X}",
                        pt_v, pt_h, part, window_ptr
                    );
                }

                bus.write_word(sp + 8, part as u16);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // BeginUpdate ($A922)
            // BeginUpdate ($A922): Saves visRgn, clips to updateRgn intersection, clears update events per IM:I I-291
            (true, 0x122) => {
                let sp = cpu.read_reg(Register::A7);
                let window = bus.read_long(sp);
                self.begin_update_window(bus, window);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // EndUpdate ($A923)
            // EndUpdate ($A923): Restores visRgn saved by BeginUpdate per IM:I I-291
            (true, 0x123) => {
                let sp = cpu.read_reg(Register::A7);
                let window = bus.read_long(sp);
                self.end_update_window(bus, window);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SetWRefCon ($A918)
            // Sets the reference constant for the specified window.
            // PROCEDURE SetWRefCon (theWindow: WindowPtr; data: LONGINT);
            // Inside Macintosh Volume I, I-293
            // SetWRefCon ($A918): Writes refCon at window+152
            (true, 0x118) => {
                let sp = cpu.read_reg(Register::A7);
                let data = bus.read_long(sp);
                let the_window = bus.read_long(sp + 4);
                if the_window != 0 {
                    // refCon is at offset 152 in WindowRecord
                    // (108-byte GrafPort + window manager fields)
                    bus.write_long(the_window + Self::WINDOW_REFCON_OFFSET, data);
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // GetWRefCon ($A917)
            // Returns the reference constant for the specified window.
            // FUNCTION GetWRefCon (theWindow: WindowPtr): LONGINT;
            // Inside Macintosh Volume I, I-293
            // GetWRefCon ($A917): Reads refCon from window+152
            (true, 0x117) => {
                let sp = cpu.read_reg(Register::A7);
                let the_window = bus.read_long(sp);
                let refcon = if the_window != 0 {
                    bus.read_long(the_window + Self::WINDOW_REFCON_OFFSET)
                } else {
                    0
                };
                bus.write_long(sp + 4, refcon);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // MoveWindow ($A91B)
            // PROCEDURE MoveWindow(theWindow: WindowPtr; hGlobal, vGlobal: INTEGER; front: BOOLEAN);
            // Stack (auto-pop): SP+0=front(2), SP+2=vGlobal(2), SP+4=hGlobal(2), SP+6=theWindow(4)
            //
            // Honor the `front` parameter per IM:I I-287. "If front is
            // TRUE, the window is made the active window ... equivalent
            // to calling SelectWindow."
            // MoveWindow ($A91B): Updates portRect, visRgn, clipRgn, and FindWindow hit-test bounds
            (true, 0x11B) => {
                let sp = cpu.read_reg(Register::A7);
                // Pascal BOOLEAN in high byte of 2-byte stack slot
                // (MPW C convention).
                let front_flag = bus.read_byte(sp) != 0;
                let v_global = bus.read_word(sp + 2) as i16;
                let h_global = bus.read_word(sp + 4) as i16;
                let the_window = bus.read_long(sp + 6);

                self.move_window_to_global(bus, the_window, h_global, v_global, front_flag);

                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // SizeWindow ($A91D)
            // PROCEDURE SizeWindow(theWindow: WindowPtr; w, h: INTEGER; fUpdate: BOOLEAN);
            // Stack (auto-pop): SP+0=fUpdate(2), SP+2=h(2), SP+4=w(2), SP+6=theWindow(4)
            //
            // Honor fUpdate per IM:I I-287. "If fUpdate is TRUE,
            // SizeWindow calls InvalRect on the window for any part that
            // is newly uncovered." Conservative implementation:
            // invalidate the full new content rect when fUpdate=TRUE.
            // That's a superset of the strictly newly-uncovered area
            // but bbox-approx region storage can't represent the precise
            // diff anyway.
            // SizeWindow ($A91D): Updates portRect, visRgn, clipRgn, and FindWindow hit-test bounds
            (true, 0x11D) => {
                let sp = cpu.read_reg(Register::A7);
                // Pascal BOOLEAN in high byte (MPW C convention).
                let f_update = bus.read_byte(sp) != 0;
                let h = bus.read_word(sp + 2) as i16;
                let w = bus.read_word(sp + 4) as i16;
                let the_window = bus.read_long(sp + 6);

                if the_window != 0 {
                    // Capture the old content rect before we resize so
                    // the fUpdate branch can invalidate the diff.
                    let old_content_rect = self.window_content_rect(bus, the_window);

                    // portRect in local coords: (0, 0, h, w)
                    bus.write_word(the_window + 16, 0u16);
                    bus.write_word(the_window + 18, 0u16);
                    bus.write_word(the_window + 20, h as u16);
                    bus.write_word(the_window + 22, w as u16);
                    let content_top = old_content_rect.map(|rect| rect.0).unwrap_or(0);
                    let content_rect = (content_top, 0, h, w);
                    let global_content =
                        self.window_local_rect_to_global(bus, the_window, content_rect);
                    let global_structure = self.window_structure_global_rect_for_window(
                        bus,
                        the_window,
                        global_content,
                    );
                    Self::write_region_handle_rect(
                        bus,
                        bus.read_long(the_window + Self::WINDOW_CONT_RGN_OFFSET),
                        Some(global_content),
                    );
                    Self::write_region_handle_rect(
                        bus,
                        bus.read_long(the_window + Self::WINDOW_STRUC_RGN_OFFSET),
                        Some(global_structure),
                    );

                    // Update visRgn in local coords
                    let vis_rgn_handle = bus.read_long(the_window + 24);
                    if vis_rgn_handle != 0 {
                        let vis_rgn = bus.read_long(vis_rgn_handle);
                        if vis_rgn != 0 {
                            // Keep existing top (may clip menu bar)
                            bus.write_word(vis_rgn + 4, 0u16);
                            bus.write_word(vis_rgn + 6, h as u16);
                            bus.write_word(vis_rgn + 8, w as u16);
                        }
                    }

                    // Update clipRgn in local coords
                    let clip_rgn_handle = bus.read_long(the_window + 28);
                    if clip_rgn_handle != 0 {
                        let clip_rgn = bus.read_long(clip_rgn_handle);
                        if clip_rgn != 0 {
                            bus.write_word(clip_rgn + 4, 0u16);
                            bus.write_word(clip_rgn + 6, h as u16);
                            bus.write_word(clip_rgn + 8, w as u16);
                        }
                    }

                    // Resizing changes the content and structure regions that
                    // determine occlusion for this window and every window
                    // behind it. Rebuild visRgn data instead of editing only
                    // its bounding words, which leaves stale complex runs.
                    // Inside Macintosh Volume I, I-287 and I-297.
                    self.recalculate_window_vis_regions(bus);

                    // Derive screen origin from pixmap bounds to update hit-test bounds.
                    if the_window == self.front_window {
                        let port_version = bus.read_word(the_window + 6);
                        let is_cgraf = (port_version & 0xC000) == 0xC000;
                        let (screen_top, screen_left) = if is_cgraf {
                            let pm_h = bus.read_long(the_window + 2);
                            let pm = bus.read_long(pm_h);
                            let bt = bus.read_word(pm + 6) as i16;
                            let bl = bus.read_word(pm + 8) as i16;
                            (-bt, -bl)
                        } else {
                            let bt = bus.read_word(the_window + 8) as i16;
                            let bl = bus.read_word(the_window + 10) as i16;
                            (-bt, -bl)
                        };
                        self.window_bounds =
                            (screen_top, screen_left, screen_top + h, screen_left + w);
                    }

                    // fUpdate=TRUE invalidates the newly-exposed area.
                    // Only invalidate if the new content grew past the old
                    // (shrinking doesn't expose new area).
                    if f_update {
                        let old_h = old_content_rect.map(|(t, _, b, _)| b - t).unwrap_or(0);
                        let old_w = old_content_rect.map(|(_, l, _, r)| r - l).unwrap_or(0);
                        if h > old_h || w > old_w {
                            self.invalidate_window_rect(bus, the_window, content_rect);
                        }
                    }
                }

                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // ============================================================
            // Window Manager interactive-tracking family
            // ($A91E TrackGoAway / $A925 DragWindow / $A92B GrowWindow /
            //  $A926 DragTheRgn — and out-of-arm $A905 DragGrayRgn at
            //  src/trap/toolbox.rs:2794)
            //
            // Per IM:I I-289..I-302 + IM:V V-201 + Macintosh Toolbox
            // Essentials 1992 4-83..4-95 each of these traps drives a
            // `WaitMouseUp`-driven mouse-poll loop that:
            //   - hit-tests against window chrome (close box / drag region
            //     / grow region) on each iteration of the loop
            //   - XOR-paints a feedback indicator (close-box hilite for
            //     TrackGoAway; gray outline of structure region for
            //     DragWindow; gray outline of size box for GrowWindow;
            //     dotted outline of an arbitrary region for DragTheRgn /
            //     DragGrayRgn) tracking cursor position
            //   - calls the DragHook ProcPtr at low-mem global $09F6 once
            //     per loop iteration if non-NIL (per IM:I I-289..I-302
            //     "DragHook" assembly-language note)
            //   - on mouse-up, returns the final cursor location packed
            //     into a LONGINT (high word=v, low word=h) for the
            //     FUNCTION-typed traps; PROCEDUREs (DragWindow) just call
            //     MoveWindow at the final location
            //
            // HLE compromise: frontends feed high-level input events,
            // not a continuous mouse-position-per-tick stream. There is no
            // WaitMouseUp loop and no DragHook dispatch infrastructure
            // (calling a guest-fn ProcPtr would need a synthesised JSR
            // frame + RTS-back-into-trap-handler infrastructure that
            // Systemless does not have — same compromise documented for
            // ModalDialog filterProc + Alert filterProc + Pack1 LSearch
            // searchProc).
            //
            // So these traps model the caller-observable terminal state
            // and skip only the outline/DragHook tracking loop:
            //   - TrackGoAway → FALSE (user didn't release inside the
            //     close box — equivalent to "user pressed close box but
            //     dragged out before releasing"; correct semantic since
            //     no click has actually happened yet from Systemless's
            //     event model)
            //   - DragWindow  → use the frontend's current mouse position
            //     as the release point; if it is inside boundsRect, move
            //     by the release-start delta via MoveWindow semantics
            //   - GrowWindow  → 0 (LONGINT zero == "no drag" sentinel
            //     per IM:I I-298: "GrowWindow returns 0 if the user
            //     releases the mouse button without moving it")
            //   - DragTheRgn / DragGrayRgn → use the frontend's current
            //     mouse position as the release point; return the bounded
            //     offset if inside slopRect, or the $80008000 no-drag
            //     sentinel if outside it.
            //
            // Pascal frame discipline (Rect args BY POINTER per IM:I-91
            // PEA-sizeRect example: "a Rect is an 8-byte record, so push
            // a pointer to it"):
            //   $A91E TrackGoAway theWindow(4) + thePt(4) = 8 args + 2
            //                     BOOLEAN result slot @ sp+8, pop 8
            //   $A925 DragWindow  theWindow(4) + startPt(4) + boundsRect
            //                     ptr(4) = 12, pop 12 (PROCEDURE no
            //                     result slot)
            //   $A92B GrowWindow  theWindow(4) + startPt(4) + sizeRect
            //                     ptr(4) = 12 args + 4 LONGINT result
            //                     slot @ sp+12, pop 12
            //   $A926 DragTheRgn  theRgn(4) + startPt(4) + limitRect
            //                     ptr(4) + slopRect ptr(4) + axis(2) +
            //                     actionProc(4) = 22 args + 4 LONGINT
            //                     result slot @ sp+22, pop 22
            //   $A905 DragGrayRgn same as DragTheRgn — they are macro
            //                     aliases per IM:I I-93 ("DragGrayRgn |
            //                     _DragGrayRgn or, after setting the
            //                     global variable DragPattern,
            //                     _DragTheRgn"). Identical Pascal sig,
            //                     identical pop count of 22.
            //
            // Tests pin: (a) pop discipline matches IM Pascal frame, (b)
            // DragWindow's terminal move/no-move cases, (c) result-slot
            // sentinel value for FUNCTION traps, (d) other registers
            // (A0/A1/D1) not mutated, (e) caller stack ABOVE the pop
            // window not mutated (defensive against future "update
            // DragHook ptr" half-fixes that touch caller-owned bytes).
            // ============================================================

            // DragWindow ($A925)
            // PROCEDURE DragWindow(theWindow: WindowPtr; startPt: Point; boundsRect: Rect);
            // Inside Macintosh Volume I, I-296
            // DragWindow ($A925): Pops 12 bytes (theWindow 4 + startPt 4 + boundsRect ptr 4) per IM:I I-91 PEA convention; uses the current hardware mouse position as the IM:I I-296 release point, moves by release-start delta through MoveWindow semantics when the release is inside global boundsRect, and still skips only the dotted-outline/DragHook loop that HLE cannot drive continuously.
            (true, 0x125) => {
                let sp = cpu.read_reg(Register::A7);
                let bounds_rect_ptr = bus.read_long(sp);
                let start_v = bus.read_word(sp + 4) as i16;
                let start_h = bus.read_word(sp + 6) as i16;
                let the_window = bus.read_long(sp + 8);

                if the_window != 0 && bounds_rect_ptr != 0 {
                    let bounds_rect = (
                        bus.read_word(bounds_rect_ptr) as i16,
                        bus.read_word(bounds_rect_ptr + 2) as i16,
                        bus.read_word(bounds_rect_ptr + 4) as i16,
                        bus.read_word(bounds_rect_ptr + 6) as i16,
                    );
                    let (release_v, release_h) = self.mouse_pos;
                    if trace_dragwindow_enabled() {
                        eprintln!(
                            "[DRAGWINDOW] DragWindow window=${:08X} start=({}, {}) release=({}, {}) bounds=({},{},{},{})",
                            the_window,
                            start_v,
                            start_h,
                            release_v,
                            release_h,
                            bounds_rect.0,
                            bounds_rect.1,
                            bounds_rect.2,
                            bounds_rect.3
                        );
                    }
                    if Self::point_in_rect(release_v, release_h, bounds_rect)
                        && (release_v != start_v || release_h != start_h)
                    {
                        let (top, left, _, _) = self.window_global_port_rect(bus, the_window);
                        let new_v = top.wrapping_add(release_v.wrapping_sub(start_v));
                        let new_h = left.wrapping_add(release_h.wrapping_sub(start_h));
                        let front_flag = !self.key_is_down(0x37);
                        self.move_window_to_global(bus, the_window, new_h, new_v, front_flag);
                        if trace_dragwindow_enabled() {
                            eprintln!(
                                "[DRAGWINDOW] moved window=${:08X} from=({}, {}) to=({}, {}) front={}",
                                the_window, top, left, new_v, new_h, front_flag
                            );
                        }
                    } else if trace_dragwindow_enabled() {
                        eprintln!("[DRAGWINDOW] no move");
                    }
                }
                cpu.write_reg(Register::A7, sp + 12);
                Ok(())
            }

            // TrackGoAway ($A91E)
            // FUNCTION TrackGoAway(theWindow: WindowPtr; thePt: Point): BOOLEAN;
            // Inside Macintosh Volume I, I-294
            // TrackGoAway ($A91E): Pops 8 args bytes + writes FALSE to 2-byte BOOLEAN result slot @ sp+8 per IM:I I-294 "TrackGoAway returns TRUE if the mouse is inside the go-away region when the mouse button is released, and FALSE otherwise" — HLE has no go-away-region hit-test / WaitMouseUp loop so FALSE matches "user pressed close box but didn't release inside it"
            (true, 0x11E) => {
                let sp = cpu.read_reg(Register::A7);
                bus.write_word(sp + 8, 0);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // GrowWindow ($A92B)
            // FUNCTION GrowWindow(theWindow: WindowPtr; startPt: Point; sizeRect: Rect): LongInt;
            // Inside Macintosh Volume I, I-298 (assembly summary IM:I I-91 with PEA sizeRect)
            // GrowWindow ($A92B): Pops 12 args bytes (theWindow 4 + startPt 4 + sizeRect ptr 4) + writes 0 to 4-byte LONGINT result slot @ sp+12 per IM:I I-298 "GrowWindow returns 0 if the user releases the mouse button without moving it"; HLE has no grow-region hit-test / WaitMouseUp loop so 0 matches "user released without growing"
            (true, 0x12B) => {
                let sp = cpu.read_reg(Register::A7);
                bus.write_long(sp + 12, 0);
                cpu.write_reg(Register::A7, sp + 12);
                Ok(())
            }

            // InvalRect ($A928)
            // InvalRect ($A928): Adds rect to window's update region
            (true, 0x128) => {
                let sp = cpu.read_reg(Register::A7);
                let rect_ptr = bus.read_long(sp);
                let target_window = self.current_window_port();
                if trace_inval_enabled() {
                    eprintln!(
                        "[INVAL] InvalRect tick={} port=${:08X} window=${:08X} front=${:08X}",
                        self.tick_count, self.current_port, target_window, self.front_window
                    );
                }
                if target_window != 0 && rect_ptr != 0 {
                    let rect = (
                        bus.read_word(rect_ptr) as i16,
                        bus.read_word(rect_ptr + 2) as i16,
                        bus.read_word(rect_ptr + 4) as i16,
                        bus.read_word(rect_ptr + 6) as i16,
                    );
                    self.invalidate_window_rect(bus, target_window, rect);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // ValidRect ($A92A)
            // ValidRect ($A92A): Removes rect from window's update region
            (true, 0x12A) => {
                let sp = cpu.read_reg(Register::A7);
                let rect_ptr = bus.read_long(sp);
                let target_window = self.current_window_port();
                if target_window != 0 && rect_ptr != 0 {
                    let rect = (
                        bus.read_word(rect_ptr) as i16,
                        bus.read_word(rect_ptr + 2) as i16,
                        bus.read_word(rect_ptr + 4) as i16,
                        bus.read_word(rect_ptr + 6) as i16,
                    );
                    self.validate_window_rect(bus, target_window, rect);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // ShowHide ($A908)
            // PROCEDURE ShowHide(theWindow: WindowPtr; showFlag: BOOLEAN);
            // ShowHide ($A908): Sets window visible byte; rebuilds visRgn/clipRgn from content rect; on show, queues update event for content; on hide, drops queued updates
            (true, 0x108) => {
                let sp = cpu.read_reg(Register::A7);
                // Pascal BOOLEAN in high byte (MPW C convention).
                let show_flag = bus.read_byte(sp) != 0;
                let the_window = bus.read_long(sp + 2);
                if the_window != 0 {
                    if !show_flag {
                        self.dialog_visible_snapshots.remove(&the_window);
                    }
                    let exposed_rect = (!show_flag && self.window_visible(bus, the_window))
                        .then(|| self.window_structure_rect(bus, the_window))
                        .flatten();
                    let windows_behind = if exposed_rect.is_some() {
                        self.visible_windows_behind(bus, the_window)
                    } else {
                        Vec::new()
                    };
                    bus.write_byte(
                        the_window + Self::WINDOW_VISIBLE_OFFSET,
                        if show_flag { 0xFF } else { 0x00 },
                    );
                    self.set_window_vis_from_content(bus, the_window, show_flag);
                    if show_flag {
                        if let Some(content_rect) = self.window_content_global_rect(bus, the_window)
                        {
                            Self::write_region_handle_rect(
                                bus,
                                bus.read_long(the_window + Self::WINDOW_UPDATE_RGN_OFFSET),
                                Some(content_rect),
                            );
                            self.queue_window_update_event(the_window);
                        }
                    } else {
                        self.clear_queued_update_events(the_window);
                        self.invalidate_exposed_windows(bus, &windows_behind, exposed_rect);
                    }
                }
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // CalcVis ($A909)
            // Recalculates the visible region (visRgn) of the given window.
            // PROCEDURE CalcVis(theWindow: WindowPeek);
            // Inside Macintosh Volume I, I-296
            // BasiliskII leaves CalcVis itself as a stack-only no-op;
            // CalcVisBehind handles the recompute path for windows
            // behind a clobbered region.
            (true, 0x109) => {
                let sp = cpu.read_reg(Register::A7);
                let _the_window = bus.read_long(sp);

                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // CalcVisBehind ($A90A)
            // PROCEDURE CalcVisBehind(startWindow: WindowPeek; clobberedRgn: RgnHandle);
            // Inside Macintosh Volume I, I-297
            // CalcVisBehind ($A90A): Recomputes vis/clip/struc/cont regions for startWindow and windows behind it that intersect clobberedRgn per IM:I I-297
            (true, 0x10A) => {
                let sp = cpu.read_reg(Register::A7);
                let clobbered_rgn = bus.read_long(sp);
                let start_window = bus.read_long(sp + 4);

                if clobbered_rgn != 0 {
                    let Some(clobbered_rect) = Self::region_handle_rect(bus, clobbered_rgn) else {
                        cpu.write_reg(Register::A7, sp + 8);
                        return Some(Ok(()));
                    };

                    let mut window_ptr = if start_window == 0 {
                        self.window_list.first().copied().unwrap_or(0)
                    } else {
                        start_window
                    };

                    while window_ptr != 0 {
                        let content_rect = self.window_content_rect(bus, window_ptr);
                        let clobbered_local =
                            self.global_rect_to_window_local(bus, window_ptr, clobbered_rect);
                        let intersects = content_rect
                            .and_then(|content| Self::rect_intersection(content, clobbered_local));
                        if let (Some(content_rect), Some(_)) = (content_rect, intersects) {
                            self.recalculate_window_regions_from_rect(
                                bus,
                                window_ptr,
                                content_rect,
                            );
                        }
                        window_ptr = bus.read_long(window_ptr + Self::WINDOW_NEXT_WINDOW_OFFSET);
                    }
                }

                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // ClipAbove ($A90B)
            // PROCEDURE ClipAbove(startWindow: WindowPeek);
            // Inside Macintosh Volume I, I-296
            //
            // Approximate the Window Manager port clip by subtracting the
            // structure rects of visible windows in front of startWindow.
            // The clip handle itself is preserved; only its region contents
            // are rewritten.
            (true, 0x10B) => {
                let sp = cpu.read_reg(Register::A7);
                let start_window = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);

                if start_window == 0 {
                    return Some(Ok(()));
                }

                let wmgr_port = self.ensure_window_manager_port(bus);
                let clip_handle = bus.read_long(wmgr_port + 28);
                let Some(mut clip_rect) = Self::region_handle_rect(bus, clip_handle) else {
                    return Some(Ok(()));
                };

                if let Some(start_idx) = self.window_list.iter().position(|&w| w == start_window) {
                    for &front_window in self.window_list.iter().take(start_idx) {
                        if !self.window_visible(bus, front_window) {
                            continue;
                        }
                        let front_rect = self.window_port_rect(bus, front_window);
                        clip_rect = Self::rect_difference_bbox(clip_rect, front_rect)
                            .unwrap_or((0, 0, 0, 0));
                        if Self::rect_is_empty(clip_rect) {
                            break;
                        }
                    }
                }

                Self::write_region_handle_rect(bus, clip_handle, Some(clip_rect));
                Ok(())
            }

            // PaintOne ($A90C)
            // Paints the portion of theWindow that lies within
            // clobberedRgn. NIL clobberedRgn = paint the whole window.
            // PROCEDURE PaintOne(theWindow: WindowPeek; clobberedRgn: RgnHandle);
            // Inside Macintosh Volume I, I-296
            //
            // Paints the frame, erases exposed content with the background
            // pattern, and adds that content to the update region. NIL means
            // the full window; an empty region currently follows the same
            // full-portRect path observed on BasiliskII.
            (true, 0x10C) => {
                let sp = cpu.read_reg(Register::A7);
                let rgn_handle = bus.read_long(sp);
                let the_window = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);
                if the_window == 0 {
                    return Some(Ok(()));
                }
                let rect = if rgn_handle != 0 {
                    Self::region_handle_rect(bus, rgn_handle)
                        .map(|rect| self.global_rect_to_window_local(bus, the_window, rect))
                } else {
                    None
                };
                let rect = rect.unwrap_or_else(|| self.window_port_rect(bus, the_window));
                self.erase_window_content_rect(bus, the_window, rect);
                self.invalidate_window_rect(bus, the_window, rect);
                Ok(())
            }

            // PaintBehind ($A90D)
            // Paints every visible area of startWindow and the windows
            // behind it that lies within clobberedRgn.
            // PROCEDURE PaintBehind(startWindow: WindowPeek; clobberedRgn: RgnHandle);
            // Inside Macintosh Volume I, I-293
            //
            // Iterates from startWindow backward through window_list and
            // InvalRects the clobbered bbox on each — the next BeginUpdate
            // / EndUpdate cycle repaints the areas. If startWindow is NIL,
            // the whole list is invalidated.
            // PaintBehind ($A90D): Inval-rects clobberedRgn bbox on startWindow and visible windows behind it per IM:I I-293
            (true, 0x10D) => {
                let sp = cpu.read_reg(Register::A7);
                let rgn_handle = bus.read_long(sp);
                let start_window = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);
                if rgn_handle == 0 {
                    return Some(Ok(()));
                }
                let rect = match Self::region_handle_rect(bus, rgn_handle) {
                    Some(r) => r,
                    None => return Some(Ok(())),
                };
                let windows = self.window_list.clone();
                let start_idx = if start_window == 0 {
                    0
                } else {
                    windows
                        .iter()
                        .position(|&w| w == start_window)
                        .unwrap_or(windows.len())
                };
                let skip_count = if start_window == 0 { 0 } else { start_idx };
                for &w in windows.iter().skip(skip_count) {
                    if self.window_visible(bus, w) {
                        self.invalidate_window_global_rect(bus, w, rect);
                    }
                }
                Ok(())
            }

            // SaveOld ($A90E)
            // PROCEDURE SaveOld(theWindow: WindowPeek);
            // SaveOld ($A90E): Saves theWindow's current structure/content
            // regions for the next DrawNew per IM:I I-296.
            (true, 0x10E) => {
                let sp = cpu.read_reg(Register::A7);
                let the_window = bus.read_long(sp);
                if the_window != 0 {
                    self.saved_draw_old_regions.insert(
                        the_window,
                        DrawOldState {
                            structure: Self::region_handle_rect(
                                bus,
                                bus.read_long(the_window + Self::WINDOW_STRUC_RGN_OFFSET),
                            ),
                            content: self.window_content_global_rect(bus, the_window),
                        },
                    );
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // DrawNew ($A90F)
            // PROCEDURE DrawNew(theWindow: WindowPeek; fUpdate: BOOLEAN);
            // Inside Macintosh Volume I, I-296
            //
            // Per IM:I I-296: SaveOld/DrawNew use (OldStructure XOR NewStructure)
            // UNION (OldContent XOR NewContent). Systemless approximates the TRUE
            // path by merging the saved old/new structure and content bboxes into
            // the window update region. The FALSE path leaves any existing update
            // region alone so a previously pending update remains pending.
            //
            // Stack: SP+0 fUpdate(2), SP+2 theWindow(4). Pop 6.
            // DrawNew ($A90F): TRUE merges the saved/current region bbox; FALSE no-op per IM:I I-296
            (true, 0x10F) => {
                let sp = cpu.read_reg(Register::A7);
                // Pascal BOOLEAN in high byte (MPW C convention).
                let f_update = bus.read_byte(sp) != 0;
                let the_window = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);
                if the_window != 0 {
                    let saved_old = self.saved_draw_old_regions.remove(&the_window);
                    if f_update {
                        let current_structure = Self::region_handle_rect(
                            bus,
                            bus.read_long(the_window + Self::WINDOW_STRUC_RGN_OFFSET),
                        );
                        let current_content = self.window_content_global_rect(bus, the_window);
                        let draw_rect = if let Some(saved_old) = saved_old {
                            Self::rect_union_all([
                                saved_old.structure,
                                current_structure,
                                saved_old.content,
                                current_content,
                            ])
                        } else {
                            current_content
                        };
                        if let Some(draw_rect) = draw_rect {
                            self.merge_window_update_region(bus, the_window, draw_rect);
                            self.queue_window_update_event(the_window);
                        }
                    }
                }
                Ok(())
            }

            // GetWMgrPort ($A910)
            // PROCEDURE GetWMgrPort(VAR wPort: GrafPtr);
            // Inside Macintosh Volume I (1985), p. I-282:
            // writes the Window Manager port pointer into wPort.
            (true, 0x110) => {
                let sp = cpu.read_reg(Register::A7);
                let port_ptr_ptr = bus.read_long(sp);
                if port_ptr_ptr != 0 {
                    let wmgr_port = self.ensure_window_manager_port(bus);
                    bus.write_long(port_ptr_ptr, wmgr_port);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // CheckUpdate ($A911)
            // FUNCTION CheckUpdate(VAR theEvent: EventRecord): BOOLEAN;
            // Macintosh Toolbox Essentials (1992), p. 4-116
            // CheckUpdate ($A911): Redraws dirty windowPic windows, then dequeues an updateEvt for the next ordinary dirty window
            (true, 0x111) => {
                let sp = cpu.read_reg(Register::A7);
                let event_ptr = bus.read_long(sp);
                let next_window = self.service_window_picture_updates(cpu, bus);
                let event = next_window
                    .and_then(|window| {
                        self.event_queue
                            .iter()
                            .position(|event| event.what == 6 && event.message == window)
                            .and_then(|idx| self.event_queue.remove(idx))
                            .or(Some(QueuedEvent {
                                what: 6,
                                message: window,
                                where_v: 0,
                                where_h: 0,
                                modifiers: 0,
                            }))
                    })
                    .or_else(|| {
                        self.event_queue
                            .iter()
                            .position(|event| event.what == 6)
                            .and_then(|idx| self.event_queue.remove(idx))
                    })
                    .or_else(|| self.pending_update_event(bus, 1u16 << 6));
                if let Some(ev) = event {
                    if event_ptr != 0 {
                        self.write_event_record(
                            bus,
                            event_ptr,
                            ev.what,
                            ev.message,
                            ev.where_v,
                            ev.where_h,
                            ev.modifiers,
                        );
                    }
                    bus.write_word(sp + 4, 0xFFFF);
                } else {
                    bus.write_word(sp + 4, 0);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // HiliteWindow ($A91C)
            // Highlights or unhighlights a window.
            // PROCEDURE HiliteWindow(theWindow: WindowPtr; fHilite: BOOLEAN);
            // Inside Macintosh Volume I, I-286
            //
            // HiliteWindow ($A91C): Sets/clears hilited byte at window+111 per IM:I I-286
            (true, 0x11C) => {
                let sp = cpu.read_reg(Register::A7);
                // Pascal BOOLEAN in high byte of 2-byte slot (MPW C
                // convention). read_word would pick up the garbage low
                // byte and mis-hilite when it happened to be non-zero.
                let f_hilite = bus.read_byte(sp) != 0;
                let the_window = bus.read_long(sp + 2);
                if the_window != 0 {
                    bus.write_byte(
                        the_window + Self::WINDOW_HILITED_OFFSET,
                        if f_hilite { 0xFF } else { 0x00 },
                    );
                    // Redraw the affected window's chrome inline so the
                    // on-screen active/inactive state reflects the HILITED
                    // byte change — matches real-Mac behavior where
                    // HiliteWindow is documented as "makes the window
                    // active or inactive" (a visible state change).
                    self.draw_single_window_chrome_inline(bus, the_window, f_hilite);
                }
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // BringToFront ($A920)
            // PROCEDURE BringToFront(theWindow: WindowPtr);
            // BringToFront ($A920): Re-orders window_list to put theWindow first, updates front_window per IM:I I-282
            (true, 0x120) => {
                let sp = cpu.read_reg(Register::A7);
                let the_window = bus.read_long(sp);
                if the_window != 0 {
                    self.track_window_front(bus, the_window);
                    self.front_window = the_window;
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SendBehind ($A921)
            // PROCEDURE SendBehind(theWindow: WindowPtr; behindWindow: WindowPtr);
            // Inside Macintosh Volume I, I-283
            //
            // Updates window_list z-order, re-links windowNext pointers,
            // re-derives front_window, and inval-rects the old visible
            // area so the exposed window behind redraws.
            // SendBehind ($A921): Reorders window_list relative to behindWindow, syncs windowNext links, inval-rects exposed area per IM:I I-283
            (true, 0x121) => {
                let sp = cpu.read_reg(Register::A7);
                let behind = bus.read_long(sp);
                let the_window = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);
                if the_window != 0 && self.window_list.contains(&the_window) {
                    // Read bounds (portRect, port+16..22) for follow-on
                    // inval before the move reshuffles indices.
                    let bounds = if the_window >= 0x100 {
                        Some(self.window_global_port_rect(bus, the_window))
                    } else {
                        None
                    };
                    self.window_list.retain(|&w| w != the_window);
                    if behind == 0 {
                        // Move to back
                        self.window_list.push(the_window);
                    } else if let Some(behind_idx) =
                        self.window_list.iter().position(|&w| w == behind)
                    {
                        // Insert just after behindWindow so theWindow
                        // is immediately behind it.
                        self.window_list.insert(behind_idx + 1, the_window);
                    } else {
                        // behindWindow not tracked — treat as move-to-back.
                        self.window_list.push(the_window);
                    }
                    self.sync_window_list_links(bus);
                    // Re-derive front_window to the first VISIBLE entry.
                    let list = self.window_list.clone();
                    self.front_window = list
                        .into_iter()
                        .find(|&w| self.window_visible(bus, w))
                        .unwrap_or_else(|| self.window_list.first().copied().unwrap_or(0));
                    if let Some(rect) = bounds {
                        // Any window that was behind and is now exposed
                        // should redraw. Conservatively inval each
                        // surviving window's bounds intersected with the
                        // moved window's old rect.
                        let windows = self.window_list.clone();
                        for &w in &windows {
                            if w == the_window {
                                continue;
                            }
                            self.invalidate_window_global_rect(bus, w, rect);
                        }
                    }
                }
                Ok(())
            }

            // DragTheRgn ($A926)
            // FUNCTION DragTheRgn(theRgn: RgnHandle; startPt: Point;
            //                     limitRect, slopRect: Rect;
            //                     axis: INTEGER;
            //                     actionProc: ProcPtr): LongInt;
            // Inside Macintosh Volume I, I-302
            //
            // Macro-aliased to $A905 DragGrayRgn per IM:I I-93 ("DragGrayRgn |
            // _DragGrayRgn or, after setting the global variable DragPattern,
            // _DragTheRgn"). Same Pascal sig, same pop count of 22.
            // See family-level rationale block above $A925 DragWindow arm.
            // DragTheRgn ($A926): Pops 22 args bytes (theRgn 4 + startPt 4 + limitRect ptr 4 + slopRect ptr 4 + axis 2 + actionProc 4) per IM:I-91 PEA convention + writes the bounded offset or $80008000 no-drag sentinel to the 4-byte LONGINT result slot @ sp+22 per IM:I I-302.
            (true, 0x126) => {
                let sp = cpu.read_reg(Register::A7);
                let result = self.drag_region_result(bus, sp);
                Self::finish_drag_result(cpu, bus, sp, result);
                Ok(())
            }

            // InvalRgn ($A927)
            // PROCEDURE InvalRgn(badRgn: RgnHandle);
            // Forwards the region's bbox into the window's update region
            // per the bbox-approx semantics also used by SectRgn/UnionRgn.
            // Malformed region headers (rgnSize < 10) are ignored defensively.
            // InvalRgn ($A927): Forwards rgn bbox into current window's update region (bbox approximation, not full region semantics) per IM:I I-291
            (true, 0x127) => {
                let sp = cpu.read_reg(Register::A7);
                let rgn_handle = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);
                let target_window = self.current_window_port();
                if target_window != 0 && rgn_handle != 0 {
                    if let Some(rect) = Self::region_handle_rect_with_min_size(bus, rgn_handle, 10)
                    {
                        self.invalidate_window_rect(bus, target_window, rect);
                    }
                }
                Ok(())
            }

            // ValidRgn ($A929)
            // PROCEDURE ValidRgn(goodRgn: RgnHandle);
            // Removes the region's bbox from the update region, mirroring
            // ValidRect.
            // Malformed region headers (rgnSize < 10) are ignored defensively.
            // ValidRgn ($A929): Removes rgn bbox from current window's update region (bbox approximation) per IM:I I-291
            (true, 0x129) => {
                let sp = cpu.read_reg(Register::A7);
                let rgn_handle = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);
                let target_window = self.current_window_port();
                if target_window != 0 && rgn_handle != 0 {
                    if let Some(rect) = Self::region_handle_rect_with_min_size(bus, rgn_handle, 10)
                    {
                        self.validate_window_rect(bus, target_window, rect);
                    }
                }
                Ok(())
            }

            // SetWindowPic ($A92E)
            // Stores a picture handle in the window record for automatic redrawing.
            // PROCEDURE SetWindowPic(theWindow: WindowPtr; pic: PicHandle);
            // Inside Macintosh Volume I, I-293
            //
            // SetWindowPic ($A92E): Stores PicHandle at window+148 per IM:I I-293
            (true, 0x12E) => {
                let sp = cpu.read_reg(Register::A7);
                let pic = bus.read_long(sp);
                let the_window = bus.read_long(sp + 4);
                if the_window != 0 {
                    bus.write_long(the_window + Self::WINDOW_PIC_OFFSET, pic);
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // GetWindowPic ($A92F)
            // Returns the picture handle stored in the window record.
            // FUNCTION GetWindowPic(theWindow: WindowPtr): PicHandle;
            // Inside Macintosh Volume I, I-293
            //
            // GetWindowPic ($A92F): Reads PicHandle from window+148 per IM:I I-293
            (true, 0x12F) => {
                let sp = cpu.read_reg(Register::A7);
                let the_window = bus.read_long(sp);
                let pic = if the_window != 0 {
                    bus.read_long(the_window + Self::WINDOW_PIC_OFFSET)
                } else {
                    0
                };
                bus.write_long(sp + 4, pic);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // GetWVariant ($A80A)
            // Returns the variation code of a window.
            // FUNCTION GetWVariant(theWindow: WindowPtr): INTEGER;
            // Inside Macintosh Volume V, V-208 (Macintosh Plus, SE, II);
            // Macintosh Toolbox Essentials 1992, p. 4-112.
            //
            // The window definition ID is `16 * resourceID + variation_code`
            // (Inside Macintosh Volume I 1985, p. I-282 "Defining Your Own
            // Windows"): the upper 12 bits select the WDEF resource, the
            // low 4 bits select the variant. NewWindow / GetNewWindow store
            // both into the WindowRecord — the resolved WDEF handle into
            // windowDefProc (offset 126) and historically the variation
            // code into the high-order byte of that field. Per Macintosh
            // Toolbox Essentials 1992 p. 4-66 (windowDefProc field): "In
            // Macintosh models that use only 24-bit addressing, this field
            // contains both a handle to the window's definition function
            // and the window's variation code. If you need to know the
            // variation code, regardless of the addressing mode, call the
            // GetWVariant function." Guests must NOT read the high byte of
            // windowDefProc directly under 32-bit-clean addressing.
            //
            // Systemless's NewWindow / NewCWindow / GetNewWindow / GetNewCWindow
            // (window.rs:762) record the original procID in the side-table
            // `window_proc_ids: HashMap<window_ptr, i16>`, indexed by the
            // WindowPtr directly (windows are pointers, not handles, so no
            // dereference is needed). GetWVariant recovers the variation
            // code by reading procID from that side table and masking the
            // low 4 bits — the canonical IM:I I-282 definition. Same
            // formula independent of WDEF resource ID:
            //   documentProc=0      → variant 0
            //   dBoxProc=1          → variant 1
            //   plainDBoxProc=2     → variant 2
            //   altDBoxProc=3       → variant 3
            //   noGrowDocProc=4     → variant 4
            //   movableDBoxProc=5   → variant 5
            //   zoomDocProc=8       → variant 8
            //   zoomNoGrow=12       → variant 12
            //   rDocProc=16         → variant 0 (WDEF resID 1, variant 0)
            //
            // Stack layout (Pascal FUNCTION): caller pre-allocates 2-byte
            // INTEGER result slot at sp+4, pushes 4-byte WindowPtr arg at
            // sp+0. Trap reads arg, advances A7 by 4, writes INTEGER result
            // at the former sp+4. NIL theWindow returns 0.
            //
            // MPW Universal Headers `MacWindows.h`:
            //   EXTERN_API(short) GetWVariant(WindowRef window) ONEWORDINLINE(0xA80A);
            (true, 0x00A) => {
                let sp = cpu.read_reg(Register::A7);
                let window_ptr = bus.read_long(sp);
                let variant: i16 = if window_ptr != 0 {
                    let proc_id = self.window_proc_ids.get(&window_ptr).copied().unwrap_or(0);
                    proc_id & 0xF
                } else {
                    0
                };
                cpu.write_reg(Register::A7, sp + 4);
                bus.write_word(sp + 4, variant as u16);
                Ok(())
            }

            // ZoomWindow ($A83A)
            // Moves a window between user state and standard state using
            // the WStateData record stored in the window's dataHandle.
            // PROCEDURE ZoomWindow(theWindow: WindowPtr; partCode: INTEGER; front: BOOLEAN);
            // Stack: SP+0=front(2), SP+2=partCode(2), SP+4=theWindow(4). Pop 8.
            // Inside Macintosh Volume IV, IV-66
            // ZoomWindow ($A83A): Reads WStateData from dataHandle, updates portRect/pixmap/regions for inZoomIn(7)/inZoomOut(8) per IM:IV IV-66
            (true, 0x03A) => {
                let sp = cpu.read_reg(Register::A7);
                let front_flag = bus.read_byte(sp) != 0;
                let part_code = bus.read_word(sp + 2) as i16;
                let the_window = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);

                if the_window != 0 {
                    let data_handle = bus.read_long(the_window + Self::WINDOW_DATA_HANDLE_OFFSET);
                    if data_handle != 0 {
                        let data_ptr = bus.read_long(data_handle);
                        if data_ptr != 0 {
                            // WStateData: userState at +0 (8 bytes), stdState at +8 (8 bytes)
                            // inZoomIn=7 → userState; inZoomOut=8 → stdState
                            // Rect field order in memory: top(+0), left(+2), bottom(+4), right(+6)
                            let rect_off: u32 = if part_code == 7 { 0 } else { 8 };
                            let t = bus.read_word(data_ptr + rect_off) as i16;
                            let l = bus.read_word(data_ptr + rect_off + 2) as i16;
                            let b = bus.read_word(data_ptr + rect_off + 4) as i16;
                            let r = bus.read_word(data_ptr + rect_off + 6) as i16;
                            let new_h = b - t;
                            let new_w = r - l;
                            let v_global = t;
                            let h_global = l;

                            // portRect in local coords: (0, 0, new_h, new_w)
                            bus.write_word(the_window + 16, 0u16);
                            bus.write_word(the_window + 18, 0u16);
                            bus.write_word(the_window + 20, new_h as u16);
                            bus.write_word(the_window + 22, new_w as u16);

                            // Update pixmap bounds (CGrafPort path) so local (0,0)
                            // maps to the new global position.
                            let (_, _, screen_w, screen_h, _) = self.screen_mode;
                            let port_version = bus.read_word(the_window + 6);
                            if (port_version & 0xC000) == 0xC000 {
                                let pm_h = bus.read_long(the_window + 2);
                                let pm = bus.read_long(pm_h);
                                bus.write_word(pm + 6, (-v_global) as u16);
                                bus.write_word(pm + 8, (-h_global) as u16);
                                bus.write_word(pm + 10, (screen_h as i16 - v_global) as u16);
                                bus.write_word(pm + 12, (screen_w as i16 - h_global) as u16);
                            } else {
                                bus.write_word(the_window + 8, (-v_global) as u16);
                                bus.write_word(the_window + 10, (-h_global) as u16);
                                bus.write_word(
                                    the_window + 12,
                                    (screen_h as i16 - v_global) as u16,
                                );
                                bus.write_word(
                                    the_window + 14,
                                    (screen_w as i16 - h_global) as u16,
                                );
                            }

                            // WindowRecord manager regions are in global coords.
                            let global_content =
                                (v_global, h_global, v_global + new_h, h_global + new_w);
                            let global_structure = self.window_structure_global_rect_for_window(
                                bus,
                                the_window,
                                global_content,
                            );
                            Self::write_region_handle_rect(
                                bus,
                                bus.read_long(the_window + Self::WINDOW_CONT_RGN_OFFSET),
                                Some(global_content),
                            );
                            Self::write_region_handle_rect(
                                bus,
                                bus.read_long(the_window + Self::WINDOW_STRUC_RGN_OFFSET),
                                Some(global_structure),
                            );

                            // visRgn and clipRgn in local coords
                            let local_rect = Some((0i16, 0i16, new_h, new_w));
                            Self::write_region_handle_rect(
                                bus,
                                bus.read_long(the_window + 24),
                                local_rect,
                            );
                            Self::write_region_handle_rect(
                                bus,
                                bus.read_long(the_window + 28),
                                local_rect,
                            );

                            // Keep FindWindow hit-test bounds in sync
                            if the_window == self.front_window {
                                self.window_bounds =
                                    (v_global, h_global, v_global + new_h, h_global + new_w);
                            }

                            // Queue redraw and optionally bring to front
                            self.queue_window_update_event(the_window);
                            if front_flag {
                                self.activate_as_front_window(bus, the_window);
                            }
                        }
                    }
                }
                Ok(())
            }

            // TrackBox ($A83B)
            // FUNCTION TrackBox(theWindow: WindowPtr; thePt: Point; partCode: INTEGER): BOOLEAN;
            // Inside Macintosh Volume IV, IV-66
            // Stack: SP+0: partCode(2), SP+2: thePt(4), SP+6: theWindow(4). Result at SP+10. Pop 10.
            // TrackBox ($A83B): Returns FALSE (no zoom-box click tracking) per IM:IV IV-66
            (true, 0x03B) => {
                let sp = cpu.read_reg(Register::A7);
                bus.write_word(sp + 10, 0); // FALSE
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // SetWinColor ($AA41)
            // PROCEDURE SetWinColor(theWindow: WindowPtr; newColorTable: WCTabHandle);
            // Macintosh Toolbox Essentials (1992), pp. 4-114..4-115.
            // SetWinColor ($AA41): Updates the window's existing AuxWin
            // record in place, applies the WCTab via apply_window_color_table,
            // and queues an update event so the window is redrawn in its new
            // colors. BasiliskII/System 7.5.3 already gives fresh windows an
            // AuxWin record, so SetWinColor rewrites `awCTable` instead of
            // allocating the first aux record lazily.
            (true, 0x241) => {
                let sp = cpu.read_reg(Register::A7);
                // Pascal stack order in this trap surface: 2nd parameter at
                // SP, 1st parameter at SP+4.
                let color_table = bus.read_long(sp);
                let the_window = bus.read_long(sp + 4);
                if the_window != 0 && color_table != 0 {
                    self.ensure_window_aux_record(bus, the_window, color_table);
                    self.apply_window_color_table(bus, the_window, color_table);
                    self.queue_window_update_event(the_window);
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // GetAuxWin ($AA42)
            // FUNCTION GetAuxWin(theWindow: WindowPtr; VAR awCTable: AuxWinHandle): BOOLEAN;
            // Macintosh Toolbox Essentials (1992), p. 4-115.
            // GetAuxWin ($AA42): Returns the tracked AuxWinHandle for fresh
            // windows created through NewWindow/NewCWindow/GetNewWindow/
            // GetNewCWindow. BasiliskII/System 7.5.3 returns TRUE with a
            // non-NIL AuxWin record for freshly created windows, so Systemless
            // mirrors that caller-observable contract for tracked windows.
            (true, 0x242) => {
                let sp = cpu.read_reg(Register::A7);
                let the_window = bus.read_long(sp + 4);
                let aux_ptr = bus.read_long(sp);
                let aux_handle = self
                    .window_aux_records
                    .get(&the_window)
                    .copied()
                    .unwrap_or(0);
                if aux_ptr != 0 {
                    bus.write_long(aux_ptr, aux_handle);
                }
                bus.write_word(sp + 8, if aux_handle != 0 { 0x0100 } else { 0 });
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::dispatch::{
        DialogItem, LoadedResources, PersistentDialogSnapshot, QueuedEvent, ResourceFileMap,
    };
    use super::super::test_helpers::{setup, TEST_SP};
    use crate::cpu::{CpuOps, Register};
    use crate::memory::MemoryBus;
    use std::collections::HashMap;

    // Helper: invoke dispatch_window as a toolbox trap.
    fn dispatch(
        disp: &mut super::super::TrapDispatcher,
        trap_num: u16,
        cpu: &mut super::super::test_helpers::MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
    ) -> Option<crate::Result<()>> {
        disp.dispatch_window(true, trap_num, cpu, bus)
    }

    #[test]
    fn layer_dispatch_is_layer_returns_false_and_consumes_its_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 2);
        bus.write_long(sp, 0x003F_119E);
        bus.write_word(sp + 4, 0xFFFF);

        let result = dispatch(&mut disp, 0x029, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(sp + 4), 0);
    }

    fn make_window(bus: &mut crate::memory::MacMemoryBus, visible: bool, dirty: bool) -> u32 {
        let window_ptr = bus.alloc(160);
        bus.write_byte(
            window_ptr + super::super::TrapDispatcher::WINDOW_VISIBLE_OFFSET,
            if visible { 0xFF } else { 0 },
        );
        let region_ptr = bus.alloc(10);
        bus.write_word(region_ptr, 10);
        let rect = if dirty {
            (0i16, 0i16, 10i16, 10i16)
        } else {
            (0, 0, 0, 0)
        };
        bus.write_word(region_ptr + 2, rect.0 as u16);
        bus.write_word(region_ptr + 4, rect.1 as u16);
        bus.write_word(region_ptr + 6, rect.2 as u16);
        bus.write_word(region_ptr + 8, rect.3 as u16);
        let handle = bus.alloc(4);
        bus.write_long(handle, region_ptr);
        bus.write_long(
            window_ptr + super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET,
            handle,
        );
        window_ptr
    }

    #[test]
    fn pending_update_event_selects_front_to_back_and_falls_back_when_list_is_empty() {
        let (mut disp, _cpu, mut bus) = setup();
        // Front-to-back selection: the front window is clean, the middle is
        // the first dirty one, the back is also dirty; the middle must win.
        let front = make_window(&mut bus, true, false);
        let middle = make_window(&mut bus, true, true);
        let back = make_window(&mut bus, true, true);
        disp.window_list = vec![front, middle, back];
        disp.front_window = front;
        let event = disp
            .pending_update_event(&bus, 0xFFFF)
            .expect("update event");
        assert_eq!(event.what, 6, "updateEvt");
        assert_eq!(event.message, middle, "first dirty window front-to-back");

        // An invisible dirty window ahead of a visible dirty one is skipped.
        let hidden = make_window(&mut bus, false, true);
        disp.window_list = vec![hidden, back];
        let event = disp
            .pending_update_event(&bus, 0xFFFF)
            .expect("update event");
        assert_eq!(event.message, back, "visibility still gates selection");

        // Empty list: the front window serves as the fallback...
        let lone = make_window(&mut bus, true, true);
        disp.window_list = Vec::new();
        disp.front_window = lone;
        let event = disp
            .pending_update_event(&bus, 0xFFFF)
            .expect("fallback event");
        assert_eq!(event.message, lone, "front-window fallback on empty list");

        // ...and no fallback exists when it is unset, or when the update
        // bit is masked out.
        disp.front_window = 0;
        assert!(disp.pending_update_event(&bus, 0xFFFF).is_none());
        disp.front_window = lone;
        assert!(
            disp.pending_update_event(&bus, 0xFFBF).is_none(),
            "mask gates"
        );
    }

    fn install_wind_resource(
        disp: &mut super::super::TrapDispatcher,
        bus: &mut crate::memory::MacMemoryBus,
        window_id: i16,
        bounds: (i16, i16, i16, i16),
        proc_id: i16,
        visible: bool,
        go_away: bool,
        ref_con: u32,
        title: &[u8],
    ) {
        install_positioned_wind_resource(
            disp, bus, window_id, bounds, proc_id, visible, go_away, ref_con, title, None,
        );
    }

    fn install_positioned_wind_resource(
        disp: &mut super::super::TrapDispatcher,
        bus: &mut crate::memory::MacMemoryBus,
        window_id: i16,
        bounds: (i16, i16, i16, i16),
        proc_id: i16,
        visible: bool,
        go_away: bool,
        ref_con: u32,
        title: &[u8],
        position: Option<u16>,
    ) {
        let title_len = title.len().min(255) as u8;
        let mut wind_data = vec![0; 18 + 1 + title_len as usize];
        wind_data[0..2].copy_from_slice(&bounds.0.to_be_bytes());
        wind_data[2..4].copy_from_slice(&bounds.1.to_be_bytes());
        wind_data[4..6].copy_from_slice(&bounds.2.to_be_bytes());
        wind_data[6..8].copy_from_slice(&bounds.3.to_be_bytes());
        wind_data[8..10].copy_from_slice(&proc_id.to_be_bytes());
        wind_data[10] = if visible { 0xFF } else { 0x00 };
        wind_data[12] = if go_away { 0xFF } else { 0x00 };
        wind_data[14..18].copy_from_slice(&ref_con.to_be_bytes());
        wind_data[18] = title_len;
        wind_data[19..].copy_from_slice(&title[..title_len as usize]);
        if let Some(position) = position {
            if wind_data.len() % 2 != 0 {
                wind_data.push(0);
            }
            wind_data.extend_from_slice(&position.to_be_bytes());
        }

        let wind_ptr = bus.alloc(wind_data.len() as u32);
        bus.write_bytes(wind_ptr, &wind_data);

        let mut loaded = HashMap::new();
        loaded.insert((*b"WIND", window_id), wind_ptr);
        let file = ResourceFileMap {
            loaded,
            named: HashMap::new(),
            names_by_id: HashMap::new(),
            attrs: HashMap::new(),
            map_attrs: 0,
        };
        disp.resources = Some(LoadedResources {
            files: HashMap::from([(0, file)]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        disp.remember_resource_backing_data(0, *b"WIND", window_id, wind_data);
    }

    fn install_wdef_resource(
        disp: &mut super::super::TrapDispatcher,
        bus: &mut crate::memory::MacMemoryBus,
        wdef_id: i16,
    ) -> u32 {
        let proc_addr = bus.alloc(2);
        bus.write_word(proc_addr, 0x4E56); // plausible 68K LINK.W proc entry
        let resources = disp.resources.get_or_insert_with(|| LoadedResources {
            files: HashMap::new(),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        let file = resources.files.entry(0).or_default();
        file.loaded.insert((*b"WDEF", wdef_id), proc_addr);
        if !resources.search_order.contains(&0) {
            resources.search_order.push(0);
        }
        proc_addr
    }

    fn setup_region_window() -> (
        super::super::TrapDispatcher,
        super::super::test_helpers::MockCpu,
        crate::memory::MacMemoryBus,
        u32,
    ) {
        let (mut disp, mut cpu, mut bus) = setup();
        let bounds_rect_ptr = 0x300000u32;
        bus.write_word(bounds_rect_ptr, 40);
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 200);
        bus.write_word(bounds_rect_ptr + 6, 300);

        let sp = TEST_SP - 26;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 18, bounds_rect_ptr);

        let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
        assert!(result.is_some(), "NewWindow should be handled");
        assert!(result.unwrap().is_ok(), "NewWindow should return");

        let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
        disp.window_list = vec![window_ptr];
        disp.front_window = window_ptr;
        disp.current_port = window_ptr;
        disp.validate_window_rect(&mut bus, window_ptr, (0, 0, 160, 260));

        (disp, cpu, bus, window_ptr)
    }

    fn make_region_handle(
        bus: &mut crate::memory::MacMemoryBus,
        handle_addr: u32,
        data_addr: u32,
        size: u16,
        bbox: (i16, i16, i16, i16),
    ) -> u32 {
        bus.write_long(handle_addr, data_addr);
        bus.write_word(data_addr, size);
        bus.write_word(data_addr + 2, bbox.0 as u16);
        bus.write_word(data_addr + 4, bbox.1 as u16);
        bus.write_word(data_addr + 6, bbox.2 as u16);
        bus.write_word(data_addr + 8, bbox.3 as u16);
        handle_addr
    }

    fn read_window_region_rect(
        bus: &crate::memory::MacMemoryBus,
        window_ptr: u32,
        offset: u32,
    ) -> (i16, i16, i16, i16) {
        let handle = bus.read_long(window_ptr + offset);
        let ptr = bus.read_long(handle);
        (
            bus.read_word(ptr + 2) as i16,
            bus.read_word(ptr + 4) as i16,
            bus.read_word(ptr + 6) as i16,
            bus.read_word(ptr + 8) as i16,
        )
    }

    fn make_v1_paintrect_picture(
        bus: &mut crate::memory::MacMemoryBus,
        frame: (i16, i16, i16, i16),
    ) -> u32 {
        let picture_data = bus.alloc(32);
        let mut cursor = picture_data + 10;
        bus.write_byte(cursor, 0x11); // versionOp
        cursor += 1;
        bus.write_byte(cursor, 0x01); // version 1
        cursor += 1;
        bus.write_byte(cursor, 0x31); // paintRect
        cursor += 1;
        for coordinate in [frame.0, frame.1, frame.2, frame.3] {
            bus.write_word(cursor, coordinate as u16);
            cursor += 2;
        }
        bus.write_byte(cursor, 0xFF); // EndOfPicture
        cursor += 1;
        bus.write_word(picture_data, (cursor - picture_data) as u16);
        bus.write_word(picture_data + 2, frame.0 as u16);
        bus.write_word(picture_data + 4, frame.1 as u16);
        bus.write_word(picture_data + 6, frame.2 as u16);
        bus.write_word(picture_data + 8, frame.3 as u16);

        let picture = bus.alloc(4);
        bus.write_long(picture, picture_data);
        picture
    }

    #[test]
    fn init_cgraf_window_starts_with_large_clip_region() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);
        disp.menu_bar_hidden = false;

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            21,
            1,
            599,
            799,
            "",
            2,
            true,
            true,
            false,
            0,
        );

        assert_eq!(
            read_window_region_rect(&bus, window_addr, 24),
            (0, 0, 578, 798),
            "visible window region should cover the content rect"
        );
        assert_eq!(
            read_window_region_rect(&bus, window_addr, 28),
            (-32767, -32767, 32767, 32767),
            "new windows should inherit QuickDraw's arbitrarily large default clipRgn"
        );
    }

    #[test]
    fn init_cgraf_window_expands_near_fullscreen_plain_window_when_host_hides_menu_bar() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);
        disp.menu_bar_hidden = true;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            21,
            1,
            599,
            799,
            "",
            2,
            true,
            true,
            false,
            0,
        );

        assert_eq!(
            read_window_region_rect(&bus, window_addr, 24),
            (-21, -1, 579, 799),
            "host-hidden menu bar should expose the full screen-backed PixMap"
        );
        assert_eq!(
            read_window_region_rect(
                &bus,
                window_addr,
                super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET
            ),
            (0, 0, 600, 800),
            "initial update region should be the expanded visible region in global coordinates"
        );
    }

    #[test]
    fn init_cgraf_window_stores_windowrecord_manager_regions_in_global_coordinates() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            100,
            200,
            300,
            500,
            "",
            2,
            true,
            false,
            false,
            0,
        );

        assert_eq!(
            read_window_region_rect(&bus, window_addr, 24),
            (0, 0, 200, 300),
            "visRgn remains in window-local coordinates"
        );
        assert_eq!(
            read_window_region_rect(
                &bus,
                window_addr,
                super::super::TrapDispatcher::WINDOW_CONT_RGN_OFFSET
            ),
            (100, 200, 300, 500),
            "contRgn is stored in global coordinates"
        );
        assert_eq!(
            read_window_region_rect(
                &bus,
                window_addr,
                super::super::TrapDispatcher::WINDOW_STRUC_RGN_OFFSET
            ),
            (99, 199, 301, 501),
            "plainDBox strucRgn uses its one-pixel WDEF frame in global coordinates"
        );
        assert_eq!(
            read_window_region_rect(
                &bus,
                window_addr,
                super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET
            ),
            (100, 200, 300, 500),
            "updateRgn is stored in global coordinates"
        );
    }

    #[test]
    fn init_cgraf_window_seeds_dbox_structure_region_with_drawn_frame_margin() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            93,
            236,
            225,
            564,
            "",
            1,
            true,
            false,
            false,
            0,
        );

        assert_eq!(
            read_window_region_rect(
                &bus,
                window_addr,
                super::super::TrapDispatcher::WINDOW_STRUC_RGN_OFFSET
            ),
            (85, 228, 233, 572),
            "dBoxProc strucRgn must match the eight-pixel frame drawn by DrawDialog"
        );
    }

    #[test]
    fn init_cgraf_window_sets_user_window_kind_independent_of_wdef_proc_id() {
        // Application WindowRecords use userKind=8 in windowKind; the WDEF
        // procID that drives chrome/variation behavior is stored separately.
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            100,
            200,
            300,
            500,
            "",
            2,
            true,
            false,
            false,
            0,
        );

        assert_eq!(
            bus.read_word(window_addr + super::super::TrapDispatcher::WINDOW_KIND_OFFSET),
            super::super::TrapDispatcher::USER_WINDOW_KIND
        );
        assert_eq!(disp.window_proc_ids.get(&window_addr), Some(&2));
    }

    #[test]
    fn movewindow_offsets_windowrecord_manager_regions_globally() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            100,
            200,
            300,
            500,
            "",
            2,
            true,
            false,
            false,
            0,
        );

        disp.move_window_to_global(&mut bus, window_addr, 1800, 1600, false);

        assert_eq!(
            disp.window_global_port_rect(&bus, window_addr),
            (1600, 1800, 1800, 2100),
            "window-local origin should map to the new global MoveWindow point"
        );
        assert_eq!(
            read_window_region_rect(&bus, window_addr, 24),
            (0, 0, 200, 300),
            "MoveWindow should not rewrite visRgn out of local coordinates"
        );
        assert_eq!(
            read_window_region_rect(
                &bus,
                window_addr,
                super::super::TrapDispatcher::WINDOW_CONT_RGN_OFFSET
            ),
            (1600, 1800, 1800, 2100),
            "contRgn should move with the window in global coordinates"
        );
        assert_eq!(
            read_window_region_rect(
                &bus,
                window_addr,
                super::super::TrapDispatcher::WINDOW_STRUC_RGN_OFFSET
            ),
            (1599, 1799, 1801, 2101),
            "plainDBox strucRgn should move with the window in global coordinates"
        );
        assert_eq!(
            read_window_region_rect(
                &bus,
                window_addr,
                super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET
            ),
            (1600, 1800, 1800, 2100),
            "pending updateRgn should move with the window in global coordinates"
        );
    }

    #[test]
    fn showwindow_setorigin_preserves_expanded_near_fullscreen_clip_region() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);
        disp.menu_bar_hidden = true;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            21,
            1,
            599,
            799,
            "",
            2,
            true,
            true,
            false,
            0,
        );

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_addr);
        let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, (-86i16) as u16);
        bus.write_word(sp + 2, (-143i16) as u16);
        let result = disp.dispatch_quickdraw(true, 0x078, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            read_window_region_rect(&bus, window_addr, 28),
            (-32767, -32767, 32767, 32767),
            "SetOrigin must not collapse a window's clipRgn"
        );
    }

    #[test]
    fn hidden_window_setorigin_rebases_global_regions_before_showwindow() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            46,
            42,
            388,
            554,
            "",
            3,
            false,
            false,
            false,
            0,
        );

        disp.move_window_to_global(&mut bus, window_addr, 0, 0, false);
        let current_gdevice = disp.current_gdevice;
        disp.set_current_port_state(&mut bus, &mut cpu, window_addr, Some(current_gdevice));

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, (-139i16) as u16);
        bus.write_word(sp + 2, (-143i16) as u16);
        let result = disp.dispatch_quickdraw(true, 0x078, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            read_window_region_rect(
                &bus,
                window_addr,
                super::super::TrapDispatcher::WINDOW_CONT_RGN_OFFSET
            ),
            (139, 143, 481, 655),
            "hidden SetOrigin should re-express the existing local content rect in global coords"
        );

        let show_sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, show_sp);
        bus.write_long(show_sp, window_addr);
        let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            read_window_region_rect(&bus, window_addr, 24),
            (0, 0, 342, 512),
            "ShowWindow should restore a full local visRgn after shifted hidden setup"
        );
        assert_eq!(
            read_window_region_rect(
                &bus,
                window_addr,
                super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET
            ),
            (139, 143, 481, 655),
            "ShowWindow should queue the centered global update region"
        );

        let begin_sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, begin_sp);
        bus.write_long(begin_sp, window_addr);
        let result = dispatch(&mut disp, 0x122, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            read_window_region_rect(&bus, window_addr, 24),
            (0, 0, 342, 512),
            "BeginUpdate should convert the centered update region back to the full local room"
        );
    }

    #[test]
    fn init_cgraf_window_custom_wdef_installs_window_def_proc_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let wdef_proc = install_wdef_resource(&mut disp, &mut bus, 200);
        let window_addr = bus.alloc(256);
        let proc_id = (200i16 << 4) | 3;

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            34,
            2,
            114,
            473,
            "",
            proc_id,
            false,
            true,
            false,
            0,
        );

        let def_handle =
            bus.read_long(window_addr + super::super::TrapDispatcher::WINDOW_DEF_PROC_OFFSET);
        assert_ne!(def_handle, 0, "custom WDEF should be loaded into a handle");
        assert_eq!(
            bus.read_long(def_handle),
            wdef_proc,
            "windowDefProc should point at the loaded WDEF resource"
        );
        assert!(
            disp.window_uses_custom_def_proc(&bus, window_addr),
            "custom WDEF window should be classified from procID + windowDefProc"
        );
    }

    #[test]
    fn init_cgraf_window_standard_wdef_installs_window_def_proc_handle() {
        // The Window Manager stores the resolved WDEF handle in the public
        // WindowRecord even for standard procIDs. Macintosh Toolbox
        // Essentials (1992), pp. 4-66 and 4-145.
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            34,
            2,
            114,
            473,
            "",
            12,
            false,
            true,
            false,
            0,
        );

        let def_handle =
            bus.read_long(window_addr + super::super::TrapDispatcher::WINDOW_DEF_PROC_OFFSET);
        assert_ne!(
            def_handle, 0,
            "standard WDEF should remain guest-visible through windowDefProc"
        );
        assert_ne!(
            bus.read_long(def_handle),
            0,
            "standard WDEF handle should be loaded"
        );
        assert_eq!(
            disp.resource_handle_files.get(&def_handle).copied(),
            Some(0),
            "standard WDEF should belong to the system resource file"
        );
    }

    #[test]
    fn getnewcwindow_visible_custom_wdef_arms_wnew_wcalcrgns_then_wdraw_trampoline() {
        let (mut disp, mut cpu, mut bus) = setup();
        let proc_id = (200i16 << 4) | 3;
        install_wind_resource(
            &mut disp,
            &mut bus,
            600,
            (34, 2, 114, 473),
            proc_id,
            true,
            false,
            0,
            b"",
        );
        let wdef_proc = install_wdef_resource(&mut disp, &mut bus, 200);

        let sp = TEST_SP - 10;
        let return_pc = 0x1234_5678;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        bus.write_long(sp, 0); // behind
        bus.write_long(sp + 4, 0); // wStorage
        bus.write_word(sp + 8, 600); // windowID
        bus.write_long(sp + 10, 0);

        let result = dispatch(&mut disp, 0x246, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let window_ptr = bus.read_long(sp + 10);
        let tramp = disp.window_def_trampoline;
        assert_ne!(window_ptr, 0);
        assert_ne!(tramp, 0, "custom WDEF should allocate a trampoline");
        assert_eq!(cpu.read_reg(Register::PC), tramp);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(
            bus.read_long(sp + 6),
            return_pc,
            "callback RTS should resume at the original trap return PC"
        );
        let def_handle =
            bus.read_long(window_ptr + super::super::TrapDispatcher::WINDOW_DEF_PROC_OFFSET);
        assert_ne!(def_handle, 0);
        assert_eq!(bus.read_long(def_handle), wdef_proc);
        let pixmap_handle = bus.read_long(window_ptr + 2);
        let pixmap = bus.read_long(pixmap_handle);
        assert_eq!(
            bus.read_long(window_ptr + 8),
            bus.read_long(pixmap + 6),
            "legacy WDEF callback should temporarily see portPixMap bounds"
        );
        assert_eq!(bus.read_long(window_ptr + 12), bus.read_long(pixmap + 10));
        assert_eq!(
            bus.read_word(tramp + 12),
            3,
            "variant must be low procID bits"
        );
        assert_eq!(bus.read_long(tramp + 16), window_ptr);
        assert_eq!(
            bus.read_word(tramp + 22),
            super::super::TrapDispatcher::WDEF_WNEW_MSG as u16
        );
        assert_eq!(bus.read_long(tramp + 26), 0);
        assert_eq!(bus.read_long(tramp + 32), wdef_proc);
        assert_eq!(bus.read_long(tramp + 38), (sp + 6).wrapping_sub(32));
        assert_eq!(
            bus.read_word(tramp + 46),
            0x4EF9,
            "first WDEF call should chain"
        );

        let calc_tramp = bus.read_long(tramp + 48);
        assert_ne!(calc_tramp, 0);
        assert_eq!(bus.read_word(calc_tramp + 12), 3);
        assert_eq!(bus.read_long(calc_tramp + 16), window_ptr);
        assert_eq!(
            bus.read_word(calc_tramp + 22),
            super::super::TrapDispatcher::WDEF_WCALC_RGNS_MSG as u16
        );
        assert_eq!(bus.read_long(calc_tramp + 26), 0);
        assert_eq!(bus.read_long(calc_tramp + 32), wdef_proc);
        assert_eq!(bus.read_long(calc_tramp + 38), (sp + 6).wrapping_sub(32));
        assert_eq!(
            bus.read_word(calc_tramp + 46),
            0x4EF9,
            "wCalcRgns should chain to wDraw"
        );

        let draw_tramp = bus.read_long(calc_tramp + 48);
        assert_ne!(draw_tramp, 0);
        assert_eq!(bus.read_word(draw_tramp + 12), 3);
        assert_eq!(bus.read_long(draw_tramp + 16), window_ptr);
        assert_eq!(
            bus.read_word(draw_tramp + 22),
            super::super::TrapDispatcher::WDEF_WDRAW_MSG as u16
        );
        assert_eq!(bus.read_long(draw_tramp + 26), 0);
        assert_eq!(bus.read_long(draw_tramp + 32), wdef_proc);
        assert_eq!(bus.read_long(draw_tramp + 38), (sp + 6).wrapping_sub(32));
        assert_eq!(bus.read_word(draw_tramp + 46), 0x23FC);
        assert_eq!(
            bus.read_long(draw_tramp + 48),
            0,
            "final callback should restore grafVars"
        );
        assert_eq!(bus.read_long(draw_tramp + 52), window_ptr + 8);
        assert_eq!(bus.read_word(draw_tramp + 56), 0x23FC);
        assert_eq!(
            bus.read_long(draw_tramp + 58),
            0,
            "final callback should restore chExtra and pnLocHFrac"
        );
        assert_eq!(bus.read_long(draw_tramp + 62), window_ptr + 12);
        assert_eq!(bus.read_word(draw_tramp + 66), 0x4E75);
        assert_eq!(
            disp.current_port, disp.window_manager_cport,
            "wDraw should run in the color Window Manager port"
        );
    }

    // ---------------------------------------------------------------
    // 1. InitWindows (0x112) -- no-op, returns Ok
    // ---------------------------------------------------------------
    #[test]
    fn initwindows_procedure_call_preserves_stack_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let result = dispatch(&mut disp, 0x112, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        // SP unchanged (no stack parameters)
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn initwindows_initializes_lowmem_grayrgn_to_desktop_region() {
        // Macintosh Toolbox Essentials 1992, pp. 4-113..4-114:
        // GetGrayRgn returns the current desktop region from low-memory
        // GrayRgn, and Universal Headers define it at $09EE.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let result = dispatch(&mut disp, 0x112, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let gray_rgn = bus.read_long(0x09EE);
        assert_ne!(gray_rgn, 0, "InitWindows should publish GrayRgn");
        let gray_ptr = bus.read_long(gray_rgn);
        assert_ne!(gray_ptr, 0, "GrayRgn should be a valid region handle");
        assert_eq!(bus.read_word(gray_ptr), 10);

        let (_, _, width, height, _) = disp.screen_mode;
        assert_eq!(
            super::super::TrapDispatcher::region_handle_rect(&bus, gray_rgn),
            Some((20, 0, height as i16, width as i16))
        );
    }

    #[test]
    fn getwmgrport_writes_window_manager_port_pointer_to_output_argument() {
        // Inside Macintosh Volume I (1985), p. I-282:
        // GetWMgrPort returns a pointer to the Window Manager port in wPort.
        let (mut disp, mut cpu, mut bus) = setup();

        let init = dispatch(&mut disp, 0x112, &mut cpu, &mut bus);
        assert!(init.is_some());
        assert!(init.unwrap().is_ok());

        disp.front_window = 0x00DE_AD00;
        let out_ptr = 0x300000u32;
        bus.write_long(out_ptr, 0xFFFF_FFFF);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, out_ptr);

        let result = dispatch(&mut disp, 0x110, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let wmgr_port = bus.read_long(out_ptr);
        assert_ne!(wmgr_port, 0, "GetWMgrPort should write a non-NIL GrafPtr");
        assert_eq!(
            wmgr_port,
            bus.read_long(0x09DE),
            "GetWMgrPort should agree with low-memory WMgrPort"
        );
        assert_eq!(
            bus.read_word(wmgr_port + 6) & 0xC000,
            0,
            "GetWMgrPort must expose a basic GrafPort rowBytes field"
        );
        assert_ne!(
            wmgr_port, disp.front_window,
            "Window Manager port pointer should not alias the front window pointer"
        );

        let (screen_base, row_bytes, width, height, _) = disp.screen_mode;
        assert_eq!(
            bus.read_long(wmgr_port + 2),
            screen_base,
            "GrafPort.portBits.baseAddr should address the main screen"
        );
        assert_eq!(
            u32::from(bus.read_word(wmgr_port + 6)),
            row_bytes,
            "GrafPort.portBits.rowBytes should describe the main screen"
        );
        assert_eq!(bus.read_word(wmgr_port + 8) as i16, 0);
        assert_eq!(bus.read_word(wmgr_port + 10) as i16, 0);
        assert_eq!(
            bus.read_word(wmgr_port + 12) as i16,
            height as i16,
            "GrafPort.portBits.bounds.bottom should match screen height"
        );
        assert_eq!(
            bus.read_word(wmgr_port + 14) as i16,
            width as i16,
            "GrafPort.portBits.bounds.right should match screen width"
        );
        assert_eq!(bus.read_word(wmgr_port + 16) as i16, 0);
        assert_eq!(bus.read_word(wmgr_port + 18) as i16, 0);
        assert_eq!(
            bus.read_word(wmgr_port + 20) as i16,
            height as i16,
            "Window Manager portRect.bottom should match screen height"
        );
        assert_eq!(
            bus.read_word(wmgr_port + 22) as i16,
            width as i16,
            "Window Manager portRect.right should match screen width"
        );
    }

    #[test]
    fn getwmgrport_consumes_output_pointer_argument() {
        // Inside Macintosh Volume I (1985), p. I-282:
        // PROCEDURE GetWMgrPort(VAR wPort: GrafPtr) consumes one pointer arg.
        let (mut disp, mut cpu, mut bus) = setup();
        let out_ptr = 0x300100u32;
        bus.write_long(out_ptr, 0xA5A5_A5A5);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, out_ptr);

        let result = dispatch(&mut disp, 0x110, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    // ---------------------------------------------------------------
    // 2. NewWindow (0x113) -- 26 bytes params, result at SP+26
    // ---------------------------------------------------------------
    #[test]
    fn test_new_window() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Set up a bounds rect at 0x300000: top=40, left=0, bottom=342, right=512
        let bounds_rect_ptr: u32 = 0x300000;
        bus.write_word(bounds_rect_ptr, 40); // top
        bus.write_word(bounds_rect_ptr + 2, 0); // left
        bus.write_word(bounds_rect_ptr + 4, 342); // bottom
        bus.write_word(bounds_rect_ptr + 6, 512); // right

        // Push 26 bytes of params + 4 result onto stack. SP+18 = bounds_rect_ptr.
        let sp = TEST_SP - 26;
        cpu.write_reg(Register::A7, sp);
        // Zero-fill
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        // SP+18 = bounds_rect_ptr (4 bytes, big-endian)
        bus.write_long(sp + 18, bounds_rect_ptr);

        let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // SP should be TEST_SP - 26 + 26 = TEST_SP, and result is at old_sp+26
        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP);

        // Result (window_ptr) is written at old sp + 26
        // Since the handler did bus.write_long(sp + param_size, window_ptr) before
        // advancing SP, the window_ptr lives at the current SP position.
        let window_ptr = bus.read_long(new_sp);
        assert_ne!(
            window_ptr, 0,
            "NewWindow should return a non-zero window pointer"
        );

        // front_window should be updated
        assert_eq!(disp.front_window, window_ptr);
    }

    #[test]
    fn full_screen_new_window_preserves_initial_update_event() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);
        bus.write_long(crate::memory::globals::addr::SCRN_BASE, screen_base);

        let bounds_rect_ptr = 0x2F0000;
        bus.write_word(bounds_rect_ptr, 0);
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 600);
        bus.write_word(bounds_rect_ptr + 6, 800);

        let sp = TEST_SP - 26;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 6, 0xFFFF_FFFF); // frontmost
        bus.write_word(sp + 10, 0); // documentProc
        bus.write_byte(sp + 12, 1); // visible
        bus.write_long(sp + 18, bounds_rect_ptr);

        let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
        assert!(disp
            .event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == window_ptr));
        assert!(disp.pending_update_event(&bus, 1u16 << 6).is_some());
    }

    fn new_window_content_probe(visible: bool) -> u8 {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = 0x300000;
        let row_bytes = 512;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 8);
        bus.write_long(crate::memory::globals::addr::SCRN_BASE, screen_base);

        // Pick a pixel well inside the content region, away from the frame.
        // A newly exposed visible window must erase it to the default white
        // background before NewWindow returns. A hidden window must leave it
        // untouched.
        let probe = screen_base + 100 * row_bytes + 150;
        bus.write_byte(probe, 0x42);

        let bounds_rect_ptr = 0x2F0000;
        bus.write_word(bounds_rect_ptr, 80);
        bus.write_word(bounds_rect_ptr + 2, 100);
        bus.write_word(bounds_rect_ptr + 4, 136);
        bus.write_word(bounds_rect_ptr + 6, 396);

        let sp = TEST_SP - 26;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 6, 0xFFFF_FFFF); // frontmost
        bus.write_word(sp + 10, 1); // dBoxProc
        bus.write_byte(sp + 12, u8::from(visible));
        bus.write_long(sp + 18, bounds_rect_ptr);

        let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
        assert!(result.is_some(), "NewWindow should be handled");
        assert!(result.unwrap().is_ok(), "NewWindow should return");
        bus.read_byte(probe)
    }

    #[test]
    fn visible_new_window_erases_exposed_content_before_returning() {
        assert_eq!(
            new_window_content_probe(true),
            0,
            "visible NewWindow content must be erased to the default white background"
        );
    }

    #[test]
    fn hidden_new_window_does_not_erase_screen_content() {
        assert_eq!(
            new_window_content_probe(false),
            0x42,
            "hidden NewWindow must not alter the framebuffer"
        );
    }

    // NewWindow must honor the `visible` parameter at SP+12 per IM:I I-299.
    fn run_new_window_with_visible(visible_arg: u16) -> (u32, u8) {
        let (mut disp, mut cpu, mut bus) = setup();

        let bounds_rect_ptr: u32 = 0x300000;
        bus.write_word(bounds_rect_ptr, 40);
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 342);
        bus.write_word(bounds_rect_ptr + 6, 512);

        let sp = TEST_SP - 26;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 18, bounds_rect_ptr);
        // MPW C pushes Pascal BOOLEAN with the value at the even offset
        // of its 2-byte stack slot (high byte of word) — write_byte at
        // sp + 12, NOT write_word(sp + 12, 1) which would land in the
        // ignored low byte.
        bus.write_byte(sp + 12, visible_arg as u8);

        let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let new_sp = cpu.read_reg(Register::A7);
        let window_ptr = bus.read_long(new_sp);
        let visible_byte = bus.read_byte(window_ptr + 110u32);
        (window_ptr, visible_byte)
    }

    #[test]
    fn new_window_with_visible_true_sets_visible_byte_to_ff() {
        let (window_ptr, visible_byte) = run_new_window_with_visible(1);
        assert_ne!(window_ptr, 0);
        assert_eq!(
            visible_byte, 0xFF,
            "NewWindow(visible=true) must mark window visible"
        );
    }

    #[test]
    fn new_window_with_visible_false_sets_visible_byte_to_zero() {
        let (window_ptr, visible_byte) = run_new_window_with_visible(0);
        assert_ne!(window_ptr, 0);
        assert_eq!(
            visible_byte, 0x00,
            "NewWindow(visible=false) must mark window invisible per IM:I I-299"
        );
    }

    #[test]
    fn hidden_new_window_preserves_current_port() {
        let (mut disp, mut cpu, mut bus) = setup();
        let bounds_rect_ptr: u32 = 0x300000;
        bus.write_word(bounds_rect_ptr, 40);
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 342);
        bus.write_word(bounds_rect_ptr + 6, 512);

        let create_window = |disp: &mut super::super::TrapDispatcher,
                             cpu: &mut super::super::test_helpers::MockCpu,
                             bus: &mut crate::memory::MacMemoryBus,
                             visible: bool| {
            let sp = TEST_SP - 26;
            cpu.write_reg(Register::A7, sp);
            for i in 0..30u32 {
                bus.write_byte(sp + i, 0);
            }
            bus.write_long(sp + 18, bounds_rect_ptr);
            bus.write_byte(sp + 12, if visible { 1 } else { 0 });
            bus.write_long(sp + 6, 0xFFFF_FFFF);

            let result = dispatch(disp, 0x113, cpu, bus);
            assert!(result.is_some(), "NewWindow should be handled");
            assert!(result.unwrap().is_ok(), "NewWindow should return");
            bus.read_long(cpu.read_reg(Register::A7))
        };

        let base = create_window(&mut disp, &mut cpu, &mut bus, true);
        assert_eq!(disp.current_port, base);

        let hidden = create_window(&mut disp, &mut cpu, &mut bus, false);
        assert_ne!(hidden, 0);
        assert_eq!(
            disp.current_port, base,
            "hidden NewWindow must preserve the caller's current port"
        );
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::THE_PORT),
            base,
            "hidden NewWindow must preserve low-memory thePort"
        );
        let qd_globals = bus.read_long(cpu.read_reg(Register::A5));
        assert_eq!(
            bus.read_long(qd_globals),
            base,
            "hidden NewWindow must preserve qd.thePort"
        );
    }

    #[test]
    fn backmost_visible_new_window_preserves_current_port() {
        let (mut disp, mut cpu, mut bus) = setup();
        let bounds_rect_ptr: u32 = 0x300000;
        bus.write_word(bounds_rect_ptr, 40);
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 342);
        bus.write_word(bounds_rect_ptr + 6, 512);

        let create_window = |disp: &mut super::super::TrapDispatcher,
                             cpu: &mut super::super::test_helpers::MockCpu,
                             bus: &mut crate::memory::MacMemoryBus,
                             behind: u32| {
            let sp = TEST_SP - 26;
            cpu.write_reg(Register::A7, sp);
            for i in 0..30u32 {
                bus.write_byte(sp + i, 0);
            }
            bus.write_long(sp + 18, bounds_rect_ptr);
            bus.write_byte(sp + 12, 1);
            bus.write_long(sp + 6, behind);

            let result = dispatch(disp, 0x113, cpu, bus);
            assert!(result.is_some(), "NewWindow should be handled");
            assert!(result.unwrap().is_ok(), "NewWindow should return");
            bus.read_long(cpu.read_reg(Register::A7))
        };

        let base = create_window(&mut disp, &mut cpu, &mut bus, 0xFFFF_FFFF);
        assert_eq!(disp.current_port, base);

        let back = create_window(&mut disp, &mut cpu, &mut bus, 0);
        assert_ne!(back, 0);
        assert_eq!(
            disp.current_port, base,
            "visible NewWindow with behind=NIL must preserve the caller's current port"
        );
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::THE_PORT),
            base,
            "backmost NewWindow must preserve low-memory thePort"
        );
        let qd_globals = bus.read_long(cpu.read_reg(Register::A5));
        assert_eq!(
            bus.read_long(qd_globals),
            base,
            "backmost NewWindow must preserve qd.thePort"
        );
    }

    // NewWindow must honor the `behind` parameter at SP+6 per IM:I I-299:
    //   behind == -1  → frontmost (default)
    //   behind == NIL → backmost
    //   behind == X   → immediately behind X
    fn run_new_window_with_behind(behind: u32) -> (u32, Vec<u32>, u32) {
        let (mut disp, mut cpu, mut bus) = setup();

        // Pre-seed an existing window so `behind` has a meaningful
        // target for the middle-insert case.
        let existing = 0x200040u32;
        disp.window_list = vec![existing];
        disp.front_window = existing;
        bus.write_byte(existing + 110u32, 0xFF); // visible

        let bounds_rect_ptr: u32 = 0x300000;
        bus.write_word(bounds_rect_ptr, 40);
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 342);
        bus.write_word(bounds_rect_ptr + 6, 512);

        let sp = TEST_SP - 26;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 18, bounds_rect_ptr);
        bus.write_word(sp + 12, 1); // visible = TRUE
        bus.write_long(sp + 6, behind);

        let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let new_sp = cpu.read_reg(Register::A7);
        let window_ptr = bus.read_long(new_sp);
        (window_ptr, disp.window_list.clone(), disp.front_window)
    }

    #[test]
    fn new_window_behind_minus_one_places_new_window_at_front() {
        let (window_ptr, list, front) = run_new_window_with_behind(0xFFFFFFFF);
        assert_eq!(
            list[0], window_ptr,
            "behind=-1 must put the new window at the front"
        );
        assert_eq!(front, window_ptr);
    }

    #[test]
    fn new_window_behind_nil_places_new_window_at_back() {
        let (window_ptr, list, front) = run_new_window_with_behind(0);
        let existing = 0x200040u32;
        assert_eq!(
            list,
            vec![existing, window_ptr],
            "behind=NIL must put the new window at the back"
        );
        assert_eq!(
            front, existing,
            "front_window must stay on the pre-existing visible window"
        );
    }

    // NewWindow / NewCWindow / GetNewWindow / GetNewCWindow must honor
    // the Pascal `wStorage` parameter per IM:I I-299: "If wStorage is
    // NIL, NewWindow allocates the necessary storage itself; otherwise
    // it uses the storage pointed to by wStorage."
    #[test]
    fn new_window_uses_caller_supplied_storage() {
        let (mut disp, mut cpu, mut bus) = setup();

        let storage = bus.alloc(512);
        let bounds_rect_ptr: u32 = 0x300000;
        bus.write_word(bounds_rect_ptr, 40);
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 342);
        bus.write_word(bounds_rect_ptr + 6, 512);

        let sp = TEST_SP - 26;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 18, bounds_rect_ptr);
        bus.write_long(sp + 22, storage); // wStorage
        bus.write_word(sp + 12, 1); // visible
        bus.write_long(sp + 6, 0xFFFFFFFF); // behind = -1 (frontmost)

        let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
        assert_eq!(
            window_ptr, storage,
            "NewWindow with non-NIL wStorage must return the caller's pointer"
        );
    }

    #[test]
    fn new_window_nil_storage_still_allocates() {
        let (mut disp, mut cpu, mut bus) = setup();

        let bounds_rect_ptr: u32 = 0x300000;
        bus.write_word(bounds_rect_ptr, 40);
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 342);
        bus.write_word(bounds_rect_ptr + 6, 512);

        let sp = TEST_SP - 26;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 18, bounds_rect_ptr);
        bus.write_long(sp + 22, 0); // wStorage = NIL
        bus.write_word(sp + 12, 1);
        bus.write_long(sp + 6, 0xFFFFFFFF);

        let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
        assert_ne!(
            window_ptr, 0,
            "NewWindow with NIL wStorage must fall back to bus.alloc"
        );
    }

    #[test]
    fn new_window_publishes_windowlist_lowmem_global() {
        // Inside Macintosh Volume I, I-299/I-301: Window Manager calls
        // insert a new visible window into the window list. Assembly
        // callers can read the front pointer through low-memory WindowList.
        let (mut disp, mut cpu, mut bus) = setup();

        let bounds_rect_ptr: u32 = 0x300000;
        bus.write_word(bounds_rect_ptr, 40);
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 342);
        bus.write_word(bounds_rect_ptr + 6, 512);

        let sp = TEST_SP - 26;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 18, bounds_rect_ptr);
        bus.write_byte(sp + 12, 1); // visible
        bus.write_long(sp + 6, 0xFFFF_FFFF); // behind = frontmost

        let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let window_ptr = bus.read_long(cpu.read_reg(Register::A7));

        assert_eq!(
            bus.read_long(0x09D6),
            window_ptr,
            "low-memory WindowList must point at the front window"
        );
        assert_eq!(
            bus.read_long(window_ptr + 144),
            0,
            "single-window list should have a NIL nextWindow link"
        );
    }

    #[test]
    fn closewindow_clears_windowlist_lowmem_when_last_window_removed() {
        // IM:I I-282/I-283 exposes the window list through low memory;
        // closing the final tracked window must publish NIL.
        let (mut disp, mut cpu, mut bus) = setup();
        let window_ptr = 0x200040u32;
        disp.window_list = vec![window_ptr];
        disp.front_window = window_ptr;
        bus.write_byte(window_ptr + 110u32, 0xFF);
        bus.write_long(0x09D6, window_ptr);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_ptr);

        let result = dispatch(&mut disp, 0x12D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            bus.read_long(0x09D6),
            0,
            "low-memory WindowList must be NIL after removing the last window"
        );
    }

    #[test]
    fn get_new_window_uses_caller_supplied_storage() {
        let (mut disp, mut cpu, mut bus) = setup();
        install_wind_resource(
            &mut disp,
            &mut bus,
            128,
            (40, 0, 342, 512),
            0,
            true,
            false,
            0,
            b"Doc",
        );

        let storage = bus.alloc(512);
        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        for i in 0..10u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 8, 128); // windowID
        bus.write_long(sp + 4, storage); // wStorage
        bus.write_long(sp, 0xFFFFFFFF); // behind = -1

        let result = dispatch(&mut disp, 0x1BD, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
        assert_eq!(
            window_ptr, storage,
            "GetNewWindow with non-NIL wStorage must return the caller's pointer"
        );
    }

    #[test]
    fn new_cwindow_uses_caller_supplied_storage() {
        let (mut disp, mut cpu, mut bus) = setup();

        let storage = bus.alloc(512);
        let bounds_rect_ptr = 0x301200u32;
        bus.write_word(bounds_rect_ptr, 0);
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 600);
        bus.write_word(bounds_rect_ptr + 6, 800);

        let sp = TEST_SP - 30;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 10, 2); // procID
        bus.write_byte(sp + 12, 1); // visible
        bus.write_long(sp + 18, bounds_rect_ptr);
        bus.write_long(sp + 22, storage); // wStorage
        bus.write_long(sp + 6, 0xFFFFFFFF); // behind = frontmost

        let result = dispatch(&mut disp, 0x245, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
        assert_eq!(
            window_ptr, storage,
            "NewCWindow with non-NIL wStorage must return the caller's pointer"
        );
    }

    #[test]
    fn get_new_cwindow_uses_caller_supplied_storage() {
        let (mut disp, mut cpu, mut bus) = setup();
        install_wind_resource(
            &mut disp,
            &mut bus,
            128,
            (0, 0, 600, 800),
            2,
            true,
            false,
            0,
            b"CWin",
        );

        let storage = bus.alloc(512);
        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        for i in 0..10u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 8, 128);
        bus.write_long(sp + 4, storage); // wStorage
        bus.write_long(sp, 0xFFFFFFFF); // behind = frontmost

        let result = dispatch(&mut disp, 0x246, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
        assert_eq!(
            window_ptr, storage,
            "GetNewCWindow with non-NIL wStorage must return the caller's pointer"
        );
    }

    #[test]
    fn new_window_behind_specific_window_inserts_just_after_target() {
        let existing = 0x200040u32;
        let (window_ptr, list, front) = run_new_window_with_behind(existing);
        assert_eq!(
            list,
            vec![existing, window_ptr],
            "behind=existing must insert the new window immediately behind it"
        );
        assert_eq!(front, existing);
    }

    // NewCWindow ($AA45), GetNewWindow ($A9BD), and GetNewCWindow ($AA46)
    // must also honor the Pascal `behind` parameter. NewCWindow has the
    // same stack layout as NewWindow (behind at SP+6). GetNewWindow /
    // GetNewCWindow share a different 10-byte stack with behind at SP+0.
    #[test]
    fn new_cwindow_behind_nil_places_new_window_at_back() {
        let (mut disp, mut cpu, mut bus) = setup();
        let existing = 0x200040u32;
        disp.window_list = vec![existing];
        disp.front_window = existing;
        bus.write_byte(existing + 110u32, 0xFF);

        let bounds_rect_ptr = 0x301200u32;
        bus.write_word(bounds_rect_ptr, 0);
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 600);
        bus.write_word(bounds_rect_ptr + 6, 800);

        let sp = TEST_SP - 30;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 10, 0); // procID
        bus.write_word(sp + 12, 1); // visible
        bus.write_long(sp + 18, bounds_rect_ptr);
        bus.write_long(sp + 6, 0); // behind = NIL (backmost)

        let result = dispatch(&mut disp, 0x245, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let new_sp = cpu.read_reg(Register::A7);
        let window_ptr = bus.read_long(new_sp);
        assert_eq!(
            disp.window_list,
            vec![existing, window_ptr],
            "NewCWindow(behind=NIL) must insert at the back"
        );
        assert_eq!(disp.front_window, existing);
    }

    #[test]
    fn get_new_window_reads_behind_at_sp_plus_0() {
        let (mut disp, mut cpu, mut bus) = setup();
        install_wind_resource(
            &mut disp,
            &mut bus,
            128,
            (40, 0, 342, 512),
            0,
            true,
            false,
            0,
            b"Doc",
        );
        let existing = 0x200040u32;
        disp.window_list = vec![existing];
        disp.front_window = existing;
        bus.write_byte(existing + 110u32, 0xFF);

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        for i in 0..10u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 8, 128); // windowID
        bus.write_long(sp, 0); // behind = NIL

        let result = dispatch(&mut disp, 0x1BD, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let new_sp = cpu.read_reg(Register::A7);
        let window_ptr = bus.read_long(new_sp);
        assert_eq!(
            disp.window_list,
            vec![existing, window_ptr],
            "GetNewWindow(behind=NIL) must insert at the back"
        );
    }

    #[test]
    fn get_new_cwindow_reads_behind_at_sp_plus_0() {
        let (mut disp, mut cpu, mut bus) = setup();
        install_wind_resource(
            &mut disp,
            &mut bus,
            128,
            (0, 0, 600, 800),
            2,
            true,
            false,
            0,
            b"CWin",
        );
        let existing = 0x200040u32;
        disp.window_list = vec![existing];
        disp.front_window = existing;
        bus.write_byte(existing + 110u32, 0xFF);

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        for i in 0..10u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 8, 128); // windowID
        bus.write_long(sp, existing); // behind = existing → insert after

        let result = dispatch(&mut disp, 0x246, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let new_sp = cpu.read_reg(Register::A7);
        let window_ptr = bus.read_long(new_sp);
        assert_eq!(
            disp.window_list,
            vec![existing, window_ptr],
            "GetNewCWindow(behind=existing) must insert immediately behind it"
        );
    }

    #[test]
    fn apply_behind_parameter_refreshes_cached_front_window_state() {
        let (mut disp, _cpu, mut bus) = setup();
        let front = bus.alloc(200);
        let back = bus.alloc(200);

        for &(window, rect) in &[(front, (10, 20, 110, 220)), (back, (0, 0, 600, 800))] {
            bus.write_byte(
                window + super::super::TrapDispatcher::WINDOW_VISIBLE_OFFSET,
                0xFF,
            );
            bus.write_word(window + 8, 0);
            bus.write_word(window + 10, 0);
            bus.write_word(window + 16, rect.0 as u16);
            bus.write_word(window + 18, rect.1 as u16);
            bus.write_word(window + 20, rect.2 as u16);
            bus.write_word(window + 22, rect.3 as u16);
        }

        disp.window_list = vec![front, back];
        disp.front_window = back;
        disp.window_bounds = (0, 0, 600, 800);

        disp.apply_behind_parameter(&mut bus, back, 0);

        assert_eq!(disp.front_window, front);
        assert_eq!(
            disp.window_bounds,
            (10, 20, 110, 220),
            "cached front-window geometry must follow the visible front after behind=NIL reorders"
        );
    }

    #[test]
    fn apply_behind_parameter_invalidates_windows_clobbered_during_creation() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

        let existing = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            existing,
            screen_base,
            20,
            20,
            300,
            400,
            "Existing",
            4,
            true,
            false,
            false,
            0,
        );
        disp.validate_window_rect(&mut bus, existing, (0, 0, 280, 380));

        let created = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            created,
            screen_base,
            60,
            80,
            180,
            300,
            "Created",
            1,
            true,
            false,
            true,
            0,
        );
        disp.validate_window_rect(&mut bus, created, (0, 0, 120, 220));

        disp.apply_behind_parameter(&mut bus, created, existing);

        assert_eq!(disp.window_list, vec![existing, created]);
        let update = super::super::TrapDispatcher::region_handle_rect(
            &bus,
            bus.read_long(existing + super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET),
        );
        assert!(
            update.is_some(),
            "the window left in front must redraw pixels clobbered by creation"
        );
        assert!(disp
            .event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == existing));
    }

    #[test]
    fn get_new_cwindow_frontmost_visible_queues_activate_events() {
        let (mut disp, mut cpu, mut bus) = setup();
        install_wind_resource(
            &mut disp,
            &mut bus,
            128,
            (0, 0, 600, 800),
            2,
            true,
            false,
            0,
            b"CWin",
        );
        let existing = 0x200040u32;
        disp.window_list = vec![existing];
        disp.front_window = existing;
        bus.write_byte(
            existing + super::super::TrapDispatcher::WINDOW_VISIBLE_OFFSET,
            0xFF,
        );
        bus.write_byte(
            existing + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
            0xFF,
        );

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        for i in 0..10u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 8, 128); // windowID
        bus.write_long(sp, 0xFFFF_FFFF); // behind = -1/frontmost

        let result = dispatch(&mut disp, 0x246, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let new_sp = cpu.read_reg(Register::A7);
        let window_ptr = bus.read_long(new_sp);

        assert_eq!(
            disp.window_list.first().copied(),
            Some(window_ptr),
            "GetNewCWindow(behind=-1) must keep the new visible window frontmost"
        );
        assert_eq!(disp.front_window, window_ptr);
        assert_eq!(
            bus.read_byte(existing + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET),
            0x00,
            "the previous front window should be unhilited"
        );
        assert_eq!(
            bus.read_byte(window_ptr + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET),
            0xFF,
            "the created front window should be hilited"
        );

        let activate_events: Vec<_> = disp
            .event_queue
            .iter()
            .filter(|event| event.what == 8)
            .collect();
        assert_eq!(activate_events.len(), 2);
        assert_eq!(activate_events[0].message, existing);
        assert_eq!(activate_events[0].modifiers & 1, 0);
        assert_eq!(activate_events[1].message, window_ptr);
        assert_eq!(activate_events[1].modifiers & 1, 1);
    }

    // ---------------------------------------------------------------
    // 3. GetNewWindow (0x1BD) -- 10 bytes params, result at SP+10
    // ---------------------------------------------------------------
    #[test]
    fn test_get_new_window() {
        let (mut disp, mut cpu, mut bus) = setup();
        install_wind_resource(
            &mut disp,
            &mut bus,
            128,
            (40, 0, 342, 512),
            0,
            true,
            false,
            0,
            b"Doc",
        );

        // Push 10 bytes of params: SP+0..SP+7 = behind(4)+wStorage(4), SP+8 = window_id(2)
        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        for i in 0..10u32 {
            bus.write_byte(sp + i, 0);
        }
        // window_id at SP+8
        bus.write_word(sp + 8, 128);

        let result = dispatch(&mut disp, 0x1BD, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP);

        let window_ptr = bus.read_long(new_sp);
        assert_ne!(
            window_ptr, 0,
            "GetNewWindow should return a non-zero window pointer"
        );
        assert_eq!(disp.front_window, window_ptr);
    }

    #[test]
    fn classic_wind_template_does_not_read_a_position_word_past_its_length() {
        let (_disp, _cpu, mut bus) = setup();
        let wind_ptr = bus.alloc(22);
        bus.write_word(wind_ptr + 4, 100);
        bus.write_word(wind_ptr + 6, 200);
        bus.write_byte(wind_ptr + 18, 0);
        bus.write_word(wind_ptr + 20, 0x280A);

        let template = super::super::TrapDispatcher::parse_wind_template(&bus, wind_ptr, 19)
            .expect("classic WIND template");
        assert_eq!(template.bounds, (0, 0, 100, 200));
        assert_eq!(template.title, "");
        assert_eq!(
            template.position, 0,
            "bytes beyond the classic resource length must not be parsed"
        );
    }

    #[test]
    fn get_new_window_constructors_apply_center_main_screen_positioning() {
        for trap_num in [0x1BD, 0x246] {
            let (mut disp, mut cpu, mut bus) = setup();
            let screen_base = bus.alloc((800 * 600) as u32);
            bus.write_long(0x0824, screen_base);
            bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
            disp.screen_mode = (screen_base, 800, 800, 600, 8);
            install_positioned_wind_resource(
                &mut disp,
                &mut bus,
                128,
                (42, 7, 436, 502),
                5,
                true,
                false,
                0,
                b"",
                Some(0x280A),
            );

            let sp = TEST_SP - 10;
            cpu.write_reg(Register::A7, sp);
            for i in 0..10u32 {
                bus.write_byte(sp + i, 0);
            }
            bus.write_word(sp + 8, 128);

            let result = dispatch(&mut disp, trap_num, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            let window_ptr = bus.read_long(TEST_SP);
            assert_ne!(window_ptr, 0);
            assert_eq!(
                disp.window_global_port_rect(&bus, window_ptr),
                (121, 152, 515, 647),
                "trap ${trap_num:03X} should center the complete WIND structure below the menu bar"
            );
        }
    }

    /// GetNewWindow creates an old-style GrafPort even when Color QuickDraw
    /// and an 8bpp screen are available. Macintosh Toolbox Essentials (1992),
    /// pp. 4-78..4-79, distinguishes it from GetNewCWindow on exactly this
    /// contract.
    #[test]
    fn get_new_window_exposes_embedded_grafport_bitmap_on_color_screen() {
        let (mut disp, mut cpu, mut bus) = setup();
        install_wind_resource(
            &mut disp,
            &mut bus,
            128,
            (0, 0, 600, 800),
            2,
            true,
            false,
            0,
            b"CWin",
        );

        // Set up an 8bpp 800×600 screen mirroring the play-runner.
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        // Standard GetNewWindow stack: SP+0..7 = behind+wStorage,
        // SP+8 = window_id.
        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        for i in 0..10u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 8, 128);

        let result = dispatch(&mut disp, 0x1BD, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let new_sp = cpu.read_reg(Register::A7);
        let window_ptr = bus.read_long(new_sp);
        assert_ne!(window_ptr, 0);

        assert_eq!(bus.read_long(window_ptr + 2), screen_base);
        assert_eq!(bus.read_word(window_ptr + 6), 800);
        assert_eq!(bus.read_word(window_ptr + 8), 0);
        assert_eq!(bus.read_word(window_ptr + 10), 0);
        assert_eq!(bus.read_word(window_ptr + 12), 600);
        assert_eq!(bus.read_word(window_ptr + 14), 800);

        let private_pm_handle = disp.window_original_pixmaps[&window_ptr];
        let private_pm = bus.read_long(private_pm_handle);
        assert_eq!(bus.read_word(private_pm + 32), 8);
    }

    // ---------------------------------------------------------------
    // 4. NewCWindow (0x245) -- 26 bytes of params + 4 result
    // 68K Pascal stack (BOOLEAN = 2 bytes on A7):
    //   SP+0: refCon(4) SP+4: goAwayFlag(2) SP+6: behind(4)
    //   SP+10: procID(2) SP+12: visible(2)  SP+14: title(4)
    //   SP+18: boundsRect(4) SP+22: wStorage(4) SP+26: result(4)
    // ---------------------------------------------------------------
    #[test]
    fn test_new_cwindow_0x245() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_menu_bar_policy(crate::runner::MenuBarPolicy::ForceHidden);
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        // Pre-fill framebuffer with 0x42 so we can verify it gets erased
        bus.write_byte(screen_base, 0x42);
        bus.write_byte(screen_base + 1, 0x42);

        // Set up a bounds rect in memory
        let bounds_rect_ptr = 0x301200u32;
        bus.write_word(bounds_rect_ptr, 0); // top
        bus.write_word(bounds_rect_ptr + 2, 0); // left
        bus.write_word(bounds_rect_ptr + 4, 600); // bottom
        bus.write_word(bounds_rect_ptr + 6, 800); // right

        let sp = TEST_SP - 30; // 26 params + 4 result
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 10, 2); // procID = plainDBox
                                    // Pascal BOOLEAN at SP+12 — value goes in HIGH byte (MPW C convention).
        bus.write_byte(sp + 12, 1); // visible
        bus.write_long(sp + 18, bounds_rect_ptr); // boundsRect at SP+18

        let result = dispatch(&mut disp, 0x245, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // Handler writes result at sp+26, sets SP = sp+26
        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, sp + 26);

        let window_ptr = bus.read_long(new_sp);
        assert_ne!(
            window_ptr, 0,
            "NewCWindow should return a non-zero window pointer"
        );
        assert_eq!(disp.front_window, window_ptr);

        // Verify it creates a CGrafPort: portVersion at offset +6 should have 0xC000
        let port_version = bus.read_word(window_ptr + 6);
        assert_eq!(port_version, 0xC000, "NewCWindow should set CGrafPort flag");
        // In explicit kiosk mode the Mac desktop is hidden, so fullscreen
        // windows erase exposed framebuffer areas to the black host stage.
        assert_eq!(
            bus.read_byte(screen_base),
            255,
            "fullscreen NewCWindow should erase framebuffer to the black kiosk stage"
        );
    }

    #[test]
    fn kiosk_new_cwindow_suppresses_initial_document_chrome_erase() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_menu_bar_policy(crate::runner::MenuBarPolicy::ForceHidden);
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let probe = screen_base + 10 * 800 + 10;
        bus.write_byte(probe, 0x42);

        let bounds_rect_ptr = 0x301200u32;
        bus.write_word(bounds_rect_ptr, 20); // below classic menu bar
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 600);
        bus.write_word(bounds_rect_ptr + 6, 800);

        let sp = TEST_SP - 30;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 10, 4); // noGrowDocProc
        bus.write_byte(sp + 12, 1); // visible
        bus.write_long(sp + 18, bounds_rect_ptr);

        let result = dispatch(&mut disp, 0x245, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_byte(probe),
            0x42,
            "kiosk document-window creation must not white-erase the hidden desktop"
        );
    }

    // ---------------------------------------------------------------
    // 5. GetNewCWindow (0x246) -- 10 bytes params (windowID, wStorage, behind)
    // ---------------------------------------------------------------
    #[test]
    fn test_get_new_cwindow_0x246() {
        let (mut disp, mut cpu, mut bus) = setup();
        install_wind_resource(
            &mut disp,
            &mut bus,
            128,
            (0, 0, 600, 800),
            2,
            true,
            false,
            0,
            b"CWin",
        );

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        for i in 0..10u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 8, 128);

        let result = dispatch(&mut disp, 0x246, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP);

        let window_ptr = bus.read_long(new_sp);
        assert_ne!(
            window_ptr, 0,
            "GetNewCWindow 0x246 should return a non-zero window pointer"
        );
        assert_eq!(disp.front_window, window_ptr);

        let port_version = bus.read_word(window_ptr + 6);
        assert_eq!(port_version, 0xC000);
    }

    fn assert_get_new_window_reloads_released_wind(trap_num: u16) {
        let (mut disp, mut cpu, mut bus) = setup();
        install_wind_resource(
            &mut disp,
            &mut bus,
            128,
            (20, 30, 220, 330),
            0,
            true,
            false,
            0,
            b"Reloaded",
        );

        let released_ptr = disp
            .resources
            .as_mut()
            .and_then(|resources| resources.files.get_mut(&0))
            .and_then(|file| file.loaded.insert((*b"WIND", 128), 0))
            .expect("installed WIND pointer");
        bus.free(released_ptr);

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        for i in 0..10u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 8, 128);

        let result = dispatch(&mut disp, trap_num, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_ne!(
            bus.read_long(TEST_SP),
            0,
            "window constructor must rematerialize a released WIND template"
        );
    }

    #[test]
    fn get_new_window_reloads_released_wind_resource() {
        assert_get_new_window_reloads_released_wind(0x1BD);
    }

    #[test]
    fn get_new_cwindow_reloads_released_wind_resource() {
        assert_get_new_window_reloads_released_wind(0x246);
    }

    // MTE 1992 pp. 4-77..4-78: GetNewCWindow/GetNewWindow return NIL
    // when the WIND template (or defproc) cannot be read.
    #[test]
    fn get_new_window_missing_wind_returns_nil() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        for i in 0..10u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 8, 999); // missing WIND id

        let result = dispatch(&mut disp, 0x1BD, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_long(TEST_SP), 0, "missing WIND must return NIL");
    }

    #[test]
    fn get_new_cwindow_missing_wind_returns_nil() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        for i in 0..10u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 8, 999); // missing WIND id

        let result = dispatch(&mut disp, 0x246, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_long(TEST_SP), 0, "missing WIND must return NIL");
    }

    // ---------------------------------------------------------------
    // 6. CloseWindow (0x12D) -- pops 4 bytes
    // ---------------------------------------------------------------
    #[test]
    fn test_close_window() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xDEAD0000); // window ptr

        let result = dispatch(&mut disp, 0x12D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn closewindow_front_promotion_highlights_next_visible_and_queues_activate_event() {
        // IM:I I-283: closing the front window promotes the window behind,
        // highlights it, and generates an activate event.
        let (mut disp, mut cpu, mut bus) = setup();
        let front = 0x200040u32;
        let next = 0x200140u32;
        disp.window_list = vec![front, next];
        disp.front_window = front;
        disp.current_port = front;
        disp.window_bounds = (240, 450, 480, 650);
        disp.window_proc_id = 4;
        disp.window_title = "Player".to_string();
        disp.go_away_flag = true;
        for &base in &[front, next] {
            bus.write_word(base + 16, 10);
            bus.write_word(base + 18, 10);
            bus.write_word(base + 20, 50);
            bus.write_word(base + 22, 100);
            bus.write_byte(base + 110u32, 0xFF);
        }
        bus.write_byte(front + 111u32, 0xFF);
        bus.write_byte(next + 111u32, 0x00);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, front);

        let result = dispatch(&mut disp, 0x12D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(disp.front_window, next);
        assert_eq!(disp.current_port, next);
        assert_eq!(
            disp.window_bounds,
            (10, 10, 50, 100),
            "CloseWindow front promotion must refresh cached render bounds \
             before later NewWindow calls redraw the previous front inactive"
        );
        assert_eq!(disp.window_title, "");
        assert!(!disp.go_away_flag);
        assert_eq!(bus.read_byte(front + 111u32), 0x00);
        assert_eq!(bus.read_byte(next + 111u32), 0xFF);
        assert_eq!(disp.window_list, vec![next]);
        assert!(
            disp.event_queue.iter().any(|event| event.what == 8
                && event.message == next
                && (event.modifiers & 1) == 1),
            "CloseWindow front promotion must queue activate event for new front window"
        );
    }

    #[test]
    fn closewindow_invalidates_document_exposed_behind_floating_window() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

        let document = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            document,
            screen_base,
            40,
            40,
            300,
            500,
            "Board",
            0,
            true,
            true,
            true,
            0,
        );
        let utility = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            utility,
            screen_base,
            80,
            100,
            220,
            420,
            "Scores",
            1,
            true,
            true,
            false,
            0,
        );
        disp.window_list = vec![utility, document];
        disp.sync_window_list_links(&mut bus);
        // Floating utilities may remain above an active document without
        // becoming the Window Manager's active front window.
        disp.front_window = document;
        disp.current_port = document;
        disp.dialog_visible_snapshots.insert(
            utility,
            PersistentDialogSnapshot {
                bounds: (80, 100, 220, 420),
                pixels: vec![0xEE; 140 * 320],
            },
        );
        let update_handle = bus.read_long(document + 122);
        super::super::TrapDispatcher::write_region_handle_rect(&mut bus, update_handle, None);
        disp.clear_queued_update_events(document);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, utility);
        let result = dispatch(&mut disp, 0x12D, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        let update = disp
            .window_update_rect(&bus, document)
            .expect("closing the overlapping utility should expose document content");
        assert!(update.0 <= 80 && update.1 <= 100);
        assert!(update.2 >= 220 && update.3 >= 420);
        assert!(disp
            .event_queue
            .iter()
            .any(|event| { event.what == 6 && event.message == document }));
        assert!(!disp.dialog_visible_snapshots.contains_key(&utility));
    }

    // ---------------------------------------------------------------
    // 7. DisposeWindow (0x114) -- pops 4 bytes
    // ---------------------------------------------------------------
    #[test]
    fn test_dispose_window() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xDEAD0000);

        let result = dispatch(&mut disp, 0x114, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn disposewindow_front_promotion_highlights_next_visible_and_queues_activate_event() {
        // IM:I I-284: DisposeWindow calls CloseWindow; IM:I I-283 promotion
        // side effects still apply when disposing a front window.
        let (mut disp, mut cpu, mut bus) = setup();
        let front = 0x200040u32;
        let next = 0x200140u32;
        disp.window_list = vec![front, next];
        disp.front_window = front;
        disp.current_port = front;
        disp.window_bounds = (240, 450, 480, 650);
        disp.window_proc_id = 4;
        disp.window_title = "Player".to_string();
        disp.go_away_flag = true;
        for &base in &[front, next] {
            bus.write_word(base + 16, 10);
            bus.write_word(base + 18, 10);
            bus.write_word(base + 20, 50);
            bus.write_word(base + 22, 100);
            bus.write_byte(base + 110u32, 0xFF);
        }
        bus.write_byte(front + 111u32, 0xFF);
        bus.write_byte(next + 111u32, 0x00);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, front);

        let result = dispatch(&mut disp, 0x114, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(disp.front_window, next);
        assert_eq!(disp.current_port, next);
        assert_eq!(
            disp.window_bounds,
            (10, 10, 50, 100),
            "DisposeWindow front promotion must refresh cached render bounds \
             before later NewWindow calls redraw the previous front inactive"
        );
        assert_eq!(disp.window_title, "");
        assert!(!disp.go_away_flag);
        assert_eq!(bus.read_byte(front + 111u32), 0x00);
        assert_eq!(bus.read_byte(next + 111u32), 0xFF);
        assert_eq!(disp.window_list, vec![next]);
        assert!(
            disp.event_queue.iter().any(|event| event.what == 8
                && event.message == next
                && (event.modifiers & 1) == 1),
            "DisposeWindow front promotion must queue activate event for new front window"
        );
    }

    #[test]
    fn disposewindow_erases_visible_window_from_screen() {
        // IM:I I-284: DisposeWindow calls CloseWindow; IM:I I-283 says
        // CloseWindow removes the window from the screen and window list.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        disp.menu_bar_hidden = false;
        super::super::TrapDispatcher::fb_fill_pattern_rect(
            &mut bus,
            screen_base,
            800,
            8,
            800,
            600,
            0,
            0,
            600,
            800,
            [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
        );

        let content_probe = screen_base + 250 * 800 + 460;
        let right_frame_probe = screen_base + 300 * 800 + 651;
        let title_frame_probe = screen_base + 222 * 800 + 627;
        let desktop_content = bus.read_byte(content_probe);
        let desktop_right_frame = bus.read_byte(right_frame_probe);
        let desktop_title_frame = bus.read_byte(title_frame_probe);
        let window_addr = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            screen_base,
            240,
            450,
            480,
            650,
            "Player",
            4,
            true,
            true,
            true,
            0,
        );
        disp.window_list = vec![window_addr];
        disp.front_window = window_addr;
        disp.current_port = window_addr;
        assert_ne!(
            bus.read_byte(content_probe),
            desktop_content,
            "precondition: visible window content covers the desktop pixel"
        );
        assert_ne!(
            bus.read_byte(right_frame_probe),
            desktop_right_frame,
            "precondition: visible window frame covers the right-edge desktop pixel"
        );
        assert_ne!(
            bus.read_byte(title_frame_probe),
            desktop_title_frame,
            "precondition: visible window frame covers the title-bar desktop pixel"
        );

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_addr);

        let result = dispatch(&mut disp, 0x114, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(
            bus.read_byte(content_probe),
            desktop_content,
            "DisposeWindow must remove the visible window pixels from the screen"
        );
        assert_eq!(
            bus.read_byte(right_frame_probe),
            desktop_right_frame,
            "DisposeWindow must erase the visible right frame from the screen"
        );
        assert_eq!(
            bus.read_byte(title_frame_probe),
            desktop_title_frame,
            "DisposeWindow must erase the visible title frame from the screen"
        );
        assert_eq!(
            bus.read_byte(window_addr + 110u32),
            0x00,
            "disposed window should no longer be marked visible"
        );
        assert!(!disp.window_list.contains(&window_addr));
    }

    // ---------------------------------------------------------------
    // 8. SelectWindow (0x11F) -- pops 4 bytes
    // ---------------------------------------------------------------
    #[test]
    fn test_select_window() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Seed two windows so SelectWindow has a target to move to
        // front; a garbage pointer would short-circuit out of the
        // event-generating path.
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        disp.window_list = vec![win_a, win_b];
        disp.front_window = win_a;
        bus.write_byte(win_a + 110u32, 0xFF);
        bus.write_byte(win_b + 110u32, 0xFF);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, win_b);

        let result = dispatch(&mut disp, 0x11F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(disp.front_window, win_b);
    }

    // SelectWindow must hilite the new front, unhilite the old front,
    // and queue deactivate+activate events per IM:I I-286.
    #[test]
    fn select_window_hilites_and_queues_activate_events() {
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        disp.window_list = vec![win_a, win_b];
        disp.front_window = win_a;
        bus.write_byte(win_a + 110u32, 0xFF);
        bus.write_byte(win_b + 110u32, 0xFF);
        // Start with A hilited (it's front).
        bus.write_byte(win_a + 111u32, 0xFF);
        bus.write_byte(win_b + 111u32, 0x00);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, win_b);

        let queue_len_before = disp.event_queue.len();
        let result = dispatch(&mut disp, 0x11F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(disp.front_window, win_b, "B must become front");
        assert_eq!(
            bus.read_byte(win_a + 111u32),
            0x00,
            "old front A must be unhilited"
        );
        assert_eq!(
            bus.read_byte(win_b + 111u32),
            0xFF,
            "new front B must be hilited"
        );
        assert_eq!(
            disp.event_queue.len() - queue_len_before,
            2,
            "exactly two events (deactivate A + activate B) must be queued"
        );
        // Find both events in the queue.
        let events: Vec<_> = disp
            .event_queue
            .iter()
            .skip(queue_len_before)
            .cloned()
            .collect();
        assert_eq!(events[0].what, 8, "first must be activate-event class");
        assert_eq!(events[0].message, win_a);
        assert_eq!(events[0].modifiers & 1, 0, "A's event is deactivate");
        assert_eq!(events[1].what, 8);
        assert_eq!(events[1].message, win_b);
        assert_eq!(events[1].modifiers & 1, 1, "B's event is activate");
    }

    #[test]
    fn pending_activation_slots_replace_superseded_window_transitions() {
        // Activate events go directly to the Event Manager rather than the
        // Operating System event queue, and the Window Manager exposes only
        // CurActivate and CurDeactive slots. Rapid front-window changes before
        // the next event poll must therefore replace, not accumulate, each
        // class of pending event.
        // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 2-51;
        // A/UX Toolbox: Macintosh ROM Interface (1990), Appendix D.
        let (mut disp, _cpu, mut bus) = setup();
        let first = 0x200040u32;
        let middle = 0x200140u32;
        let last = 0x200240u32;

        disp.queue_window_activation_event(&mut bus, first, false);
        disp.queue_window_activation_event(&mut bus, middle, true);
        disp.queue_window_activation_event(&mut bus, middle, false);
        disp.queue_window_activation_event(&mut bus, last, true);

        let events: Vec<_> = disp
            .event_queue
            .iter()
            .filter(|event| event.what == 8)
            .cloned()
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message, middle);
        assert_eq!(events[0].modifiers & 1, 0);
        assert_eq!(events[1].message, last);
        assert_eq!(events[1].modifiers & 1, 1);
        assert_eq!(
            bus.read_long(super::super::TrapDispatcher::LOWMEM_CUR_DEACTIVATE),
            middle
        );
        assert_eq!(
            bus.read_long(super::super::TrapDispatcher::LOWMEM_CUR_ACTIVATE),
            last
        );

        disp.acknowledge_window_activation_event(&mut bus, &events[0]);
        assert_eq!(
            bus.read_long(super::super::TrapDispatcher::LOWMEM_CUR_DEACTIVATE),
            0
        );
        assert_eq!(
            bus.read_long(super::super::TrapDispatcher::LOWMEM_CUR_ACTIVATE),
            last
        );
    }

    #[test]
    fn invisible_frontmost_new_window_replaces_pending_activation_pair() {
        // A window created at the front is active even when initially
        // invisible; ShowWindow controls only when it becomes visible.
        // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 4-84.
        let (mut disp, _cpu, mut bus) = setup();
        let old_front = 0x200040u32;
        let new_front = 0x200140u32;
        disp.window_list = vec![new_front, old_front];
        disp.front_window = old_front;
        bus.write_byte(old_front + 110, 0xFF);
        bus.write_byte(old_front + 111, 0xFF);
        bus.write_byte(new_front + 110, 0x00);
        bus.write_byte(new_front + 111, 0x00);

        disp.activate_frontmost_created_window_if_needed(
            &mut bus,
            new_front,
            false,
            0xFFFF_FFFF,
            old_front,
        );

        assert_eq!(disp.front_window, new_front);
        assert_eq!(bus.read_byte(old_front + 111), 0x00);
        assert_eq!(bus.read_byte(new_front + 111), 0xFF);
        let events: Vec<_> = disp
            .event_queue
            .iter()
            .filter(|event| event.what == 8)
            .cloned()
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message, old_front);
        assert_eq!(events[0].modifiers & 1, 0);
        assert_eq!(events[1].message, new_front);
        assert_eq!(events[1].modifiers & 1, 1);
    }

    #[test]
    fn select_window_already_front_is_idempotent() {
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        disp.window_list = vec![win_a];
        disp.front_window = win_a;
        bus.write_byte(win_a + 110u32, 0xFF);
        bus.write_byte(win_a + 111u32, 0xFF);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, win_a);

        let queue_len_before = disp.event_queue.len();
        let result = dispatch(&mut disp, 0x11F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            disp.event_queue.len(),
            queue_len_before,
            "SelectWindow on already-front window must not queue any events per IM:I I-286"
        );
    }

    #[test]
    fn select_window_nil_is_safe() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);

        let result = dispatch(&mut disp, 0x11F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    // ---------------------------------------------------------------
    // 9. ShowWindow (0x115) -- pops 4 bytes
    // ---------------------------------------------------------------
    #[test]
    fn test_show_window() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xDEAD0000);

        let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn showwindow_already_visible_does_not_requeue_full_update() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window = 0x200040u32;
        let (_cont_rgn, update_rgn) =
            setup_full_window_with_regions(&mut bus, window, 20, 0, 424, 627);
        bus.write_byte(window + 110, 0xFF);
        bus.write_byte(window + 111, 0xFF);
        disp.window_list = vec![window];
        disp.front_window = window;
        disp.current_port = window;

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window);

        let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        assert!(
            !disp
                .event_queue
                .iter()
                .any(|e| e.what == 6 && e.message == window),
            "ShowWindow on an already visible window must not queue a new updateEvt"
        );
        assert_eq!(
            (
                bus.read_word(update_rgn + 2) as i16,
                bus.read_word(update_rgn + 4) as i16,
                bus.read_word(update_rgn + 6) as i16,
                bus.read_word(update_rgn + 8) as i16,
            ),
            (0, 0, 0, 0),
            "ShowWindow must not expand an already visible window's updateRgn"
        );
    }

    #[test]
    fn showwindow_hidden_window_sets_global_update_region() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window = bus.alloc(256);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window,
            disp.screen_mode.0,
            100,
            200,
            300,
            500,
            "",
            2,
            false,
            false,
            false,
            0,
        );

        assert!(disp.window_update_rect(&bus, window).is_none());

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window);
        let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            disp.window_update_rect(&bus, window),
            Some((100, 200, 300, 500)),
            "ShowWindow should invalidate the revealed content in global coordinates"
        );
    }

    #[test]
    fn showwindow_hidden_frontmost_window_queues_activate_event() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window = bus.alloc(256);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window,
            disp.screen_mode.0,
            100,
            200,
            300,
            500,
            "",
            2,
            false,
            false,
            false,
            0,
        );
        disp.event_queue.clear();
        bus.write_byte(
            window + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
            0x00,
        );
        assert_eq!(disp.front_window, window);
        assert_eq!(
            bus.read_byte(window + super::super::TrapDispatcher::WINDOW_VISIBLE_OFFSET),
            0x00
        );

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window);
        let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            bus.read_byte(window + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET),
            0xFF,
            "ShowWindow should hilite an invisible frontmost window"
        );
        assert!(
            disp.event_queue.iter().any(|event| event.what == 8
                && event.message == window
                && (event.modifiers & 1) == 1),
            "ShowWindow must queue an activate event for an invisible frontmost window"
        );
    }

    #[test]
    fn showwindow_hidden_first_window_becomes_frontwindow_even_if_cached_front_was_behind() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let document = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            document,
            disp.screen_mode.0,
            120,
            120,
            320,
            520,
            "Document",
            8,
            true,
            false,
            true,
            0,
        );

        let dialog = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            dialog,
            disp.screen_mode.0,
            90,
            180,
            260,
            460,
            "Dialog",
            3,
            false,
            false,
            true,
            0,
        );
        disp.front_window = document;
        bus.write_byte(
            document + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
            0xFF,
        );
        bus.write_byte(
            dialog + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
            0x00,
        );
        disp.event_queue.clear();

        assert_eq!(
            disp.window_list.first().copied(),
            Some(dialog),
            "the hidden dialog must be first in Window Manager order"
        );
        assert_eq!(
            disp.front_window_for_trap(&bus),
            document,
            "FrontWindow skips the hidden first window before ShowWindow"
        );

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, dialog);
        let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        cpu.write_reg(Register::A7, TEST_SP);
        let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            bus.read_long(TEST_SP),
            dialog,
            "FrontWindow must return the first visible window in the window list"
        );
        assert_eq!(
            disp.front_window, dialog,
            "ShowWindow should activate a newly visible window that is frontmost in list order"
        );
        assert!(
            disp.event_queue.iter().any(|event| event.what == 8
                && event.message == dialog
                && (event.modifiers & 1) == 1),
            "ShowWindow must queue an activate event for the newly visible frontmost window"
        );
    }

    // ---------------------------------------------------------------
    // 10. HideWindow (0x116) -- pops 4 bytes
    // ---------------------------------------------------------------
    #[test]
    fn test_hide_window() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xDEAD0000);

        let result = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    // HideWindow promotes the next visible window to front when the
    // hidden window was the current front window, per IM:I I-286.
    #[test]
    fn hide_window_promotes_next_visible_to_front() {
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        disp.window_list = vec![win_b, win_a]; // b is front, a is behind
        disp.front_window = win_b;
        disp.current_port = win_b;
        for &base in &[win_a, win_b] {
            bus.write_word(base + 16, 10);
            bus.write_word(base + 18, 10);
            bus.write_word(base + 20, 50);
            bus.write_word(base + 22, 100);
            bus.write_byte(base + 110u32, 0xFF);
        }

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, win_b); // hide the front

        let result = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(disp.front_window, win_a, "next visible must become front");
        assert_eq!(
            disp.current_port, win_a,
            "current_port must follow new front when it was the hidden window"
        );
        assert!(
            disp.event_queue.iter().any(|event| event.what == 8
                && event.message == win_b
                && (event.modifiers & 1) == 0),
            "HideWindow must queue a deactivate event for the hidden front window"
        );
        assert!(
            disp.event_queue.iter().any(|event| event.what == 8
                && event.message == win_a
                && (event.modifiers & 1) == 1),
            "HideWindow must queue an activate event for the promoted front window"
        );
        assert_eq!(
            bus.read_byte(win_b + 110u32),
            0x00,
            "hidden window must be marked invisible"
        );
    }

    #[test]
    fn hide_window_invalidates_visible_content_behind_it() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

        let back = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            back,
            screen_base,
            40,
            40,
            300,
            500,
            "Back",
            0,
            true,
            true,
            true,
            0,
        );
        let front = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            front,
            screen_base,
            80,
            100,
            220,
            420,
            "Front",
            1,
            true,
            true,
            false,
            0,
        );
        disp.window_list = vec![front, back];
        disp.sync_window_list_links(&mut bus);
        disp.front_window = front;
        disp.current_port = front;
        let update_handle = bus.read_long(back + 122);
        super::super::TrapDispatcher::write_region_handle_rect(&mut bus, update_handle, None);
        disp.clear_queued_update_events(back);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, front);
        let result = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert!(disp.window_update_rect(&bus, back).is_some());
        assert!(disp
            .event_queue
            .iter()
            .any(|event| { event.what == 6 && event.message == back }));
    }

    #[test]
    fn hide_window_skips_invisible_candidates() {
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        let win_c = 0x200240u32;
        disp.window_list = vec![win_c, win_b, win_a];
        disp.front_window = win_c;
        disp.current_port = win_c;
        for &base in &[win_a, win_b, win_c] {
            bus.write_word(base + 16, 10);
            bus.write_word(base + 18, 10);
            bus.write_word(base + 20, 50);
            bus.write_word(base + 22, 100);
        }
        // Only c and a are visible; b is already hidden.
        bus.write_byte(win_a + 110u32, 0xFF);
        bus.write_byte(win_b + 110u32, 0x00);
        bus.write_byte(win_c + 110u32, 0xFF);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, win_c); // hide c
        let result = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            disp.front_window, win_a,
            "must skip already-hidden b and pick a"
        );
    }

    #[test]
    fn hide_window_already_hidden_window_does_not_erase_screen_pixels() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

        let hidden_window = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            hidden_window,
            screen_base,
            42,
            2,
            334,
            502,
            "Hidden",
            8,
            false,
            false,
            false,
            0,
        );
        bus.write_byte(hidden_window + 110u32, 0x00);
        disp.front_window = 0;

        let probe = screen_base + 100 * 800 + 100;
        bus.write_byte(probe, 0x42);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, hidden_window);
        let result = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            bus.read_byte(probe),
            0x42,
            "HideWindow on an already hidden window must not erase exposed pixels"
        );
    }

    #[test]
    fn hide_window_restores_saved_under_pixels_for_non_document_window() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

        for y in 40..120u32 {
            for x in 40..180u32 {
                bus.write_byte(screen_base + y * 800 + x, 0xCC);
            }
        }

        let bounds = bus.alloc(8);
        bus.write_word(bounds, 50);
        bus.write_word(bounds + 2, 60);
        bus.write_word(bounds + 4, 100);
        bus.write_word(bounds + 6, 160);

        let sp = TEST_SP - 26;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 18, bounds);
        bus.write_byte(sp + 12, 0xFF); // visible = TRUE
        bus.write_word(sp + 10, 1); // non-document window proc
        bus.write_long(sp + 6, 0xFFFF_FFFF); // front

        let created = dispatch(&mut disp, 0x245, &mut cpu, &mut bus);
        assert!(created.unwrap().is_ok());
        let window = bus.read_long(TEST_SP);
        assert_ne!(window, 0, "NewCWindow should return a window");

        let probe = screen_base + 70 * 800 + 80;
        bus.write_byte(probe, 0x11);

        let hide_sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, hide_sp);
        bus.write_long(hide_sp, window);
        let hidden = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
        assert!(hidden.unwrap().is_ok());

        assert_eq!(
            bus.read_byte(probe),
            0xCC,
            "HideWindow should restore the pixels saved under non-document windows"
        );
    }

    #[test]
    fn hide_window_clears_visible_dialog_snapshot_before_chrome_redraw() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

        let dialog = bus.alloc(256);
        bus.write_word(dialog + 8, (-100i16) as u16);
        bus.write_word(dialog + 10, (-100i16) as u16);
        bus.write_word(dialog + 20, 180);
        bus.write_word(dialog + 22, 260);
        bus.write_byte(
            dialog + super::super::TrapDispatcher::WINDOW_VISIBLE_OFFSET,
            0xFF,
        );
        disp.window_list = vec![dialog];
        disp.front_window = dialog;
        disp.current_port = dialog;
        disp.dialog_items
            .insert(dialog, vec![DialogItem::default()]);
        disp.dialog_visible_snapshots.insert(
            dialog,
            PersistentDialogSnapshot {
                bounds: (120, 120, 140, 180),
                pixels: vec![0xEE; 20 * 60],
            },
        );

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, dialog);
        let hidden = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
        assert!(hidden.unwrap().is_ok());
        assert!(
            !disp.dialog_visible_snapshots.contains_key(&dialog),
            "HideWindow must stop compositing retained visible dialog pixels"
        );

        let probe = screen_base + 125 * 800 + 125;
        bus.write_byte(probe, 0x11);
        disp.redraw_chrome(&mut bus);
        assert_eq!(
            bus.read_byte(probe),
            0x11,
            "redraw_chrome must not repaint a hidden dialog snapshot"
        );
    }

    // DisposeWindow / CloseWindow route through untrack_window. The
    // promotion logic must skip hidden windows, mirroring HideWindow's
    // visible-only walk.
    #[test]
    fn dispose_window_skips_hidden_candidate_when_promoting_front() {
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        let win_c = 0x200240u32;
        // List: c front, b behind (hidden), a back-most (visible).
        disp.window_list = vec![win_c, win_b, win_a];
        disp.front_window = win_c;
        disp.current_port = win_c;
        for &base in &[win_a, win_b, win_c] {
            bus.write_word(base + 16, 10);
            bus.write_word(base + 18, 10);
            bus.write_word(base + 20, 50);
            bus.write_word(base + 22, 100);
        }
        bus.write_byte(win_a + 110u32, 0xFF);
        bus.write_byte(win_b + 110u32, 0x00); // b hidden
        bus.write_byte(win_c + 110u32, 0xFF);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, win_c);

        // DisposeWindow ($A914, trap 0x114)
        let result = dispatch(&mut disp, 0x114, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            disp.front_window, win_a,
            "must skip hidden b and promote visible a to front"
        );
        assert_eq!(disp.current_port, win_a);
    }

    #[test]
    fn close_window_falls_back_to_first_entry_when_all_remaining_are_hidden() {
        // Defensive case: if every remaining window is hidden, fall
        // back to window_list.first() rather than 0 so the guest
        // doesn't see a bogus nil front_window mid-cleanup.
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        disp.window_list = vec![win_b, win_a];
        disp.front_window = win_b;
        disp.current_port = win_b;
        for &base in &[win_a, win_b] {
            bus.write_word(base + 16, 10);
            bus.write_word(base + 18, 10);
            bus.write_word(base + 20, 50);
            bus.write_word(base + 22, 100);
        }
        bus.write_byte(win_a + 110u32, 0x00); // a hidden
        bus.write_byte(win_b + 110u32, 0xFF);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, win_b);

        // CloseWindow ($A92D, trap 0x12D)
        let result = dispatch(&mut disp, 0x12D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            disp.front_window, win_a,
            "fallback must still promote to the only remaining entry (a)"
        );
    }

    #[test]
    fn hide_window_non_front_leaves_front_untouched() {
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        disp.window_list = vec![win_b, win_a];
        disp.front_window = win_b;
        disp.current_port = win_b;
        for &base in &[win_a, win_b] {
            bus.write_word(base + 16, 10);
            bus.write_word(base + 18, 10);
            bus.write_word(base + 20, 50);
            bus.write_word(base + 22, 100);
            bus.write_byte(base + 110u32, 0xFF);
        }

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, win_a); // hide the NON-front window
        let result = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.front_window, win_b, "front must not change");
        assert_eq!(disp.current_port, win_b, "current_port must not change");
    }

    // IM:I I-285: ShowHide(TRUE) makes the target window visible but does
    // not reorder windows or generate activate events.
    #[test]
    fn showhide_true_makes_target_visible_without_front_reorder_or_activate_events() {
        let (mut disp, mut cpu, mut bus) = setup();
        let front_window = 0x200040u32;
        let target_window = 0x200140u32;
        let (_cont_rgn, update_rgn) =
            setup_full_window_with_regions(&mut bus, target_window, 12, 34, 80, 140);
        bus.write_byte(front_window + 110, 0xFF);
        bus.write_byte(front_window + 111, 0xFF);
        bus.write_byte(target_window + 110, 0x00);
        bus.write_byte(target_window + 111, 0x00);

        disp.window_list = vec![front_window, target_window];
        disp.front_window = front_window;
        disp.current_port = front_window;

        let activate_before = disp.event_queue.iter().filter(|e| e.what == 8).count();

        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_byte(sp, 1); // showFlag = TRUE in Pascal BOOLEAN high byte.
        bus.write_long(sp + 2, target_window);

        let result = dispatch(&mut disp, 0x108, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        assert_eq!(
            bus.read_byte(target_window + 110),
            0xFF,
            "ShowHide(TRUE) must set visible byte"
        );
        assert_eq!(
            disp.front_window, front_window,
            "ShowHide must not change front-to-back ordering"
        );
        assert!(
            disp.event_queue
                .iter()
                .any(|e| e.what == 6 && e.message == target_window),
            "ShowHide(TRUE) should queue an update event for the shown window"
        );
        let activate_after = disp.event_queue.iter().filter(|e| e.what == 8).count();
        assert_eq!(
            activate_after, activate_before,
            "ShowHide must not generate activate/deactivate events"
        );
        assert_eq!(bus.read_word(update_rgn + 2) as i16, 12, "updateRgn.top");
        assert_eq!(bus.read_word(update_rgn + 4) as i16, 34, "updateRgn.left");
        assert_eq!(bus.read_word(update_rgn + 6) as i16, 80, "updateRgn.bottom");
        assert_eq!(bus.read_word(update_rgn + 8) as i16, 140, "updateRgn.right");
    }

    #[test]
    fn showhide_true_sets_global_update_region_for_revealed_window() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window = bus.alloc(256);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window,
            disp.screen_mode.0,
            100,
            200,
            300,
            500,
            "",
            2,
            false,
            false,
            false,
            0,
        );

        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_byte(sp, 1);
        bus.write_long(sp + 2, window);

        let result = dispatch(&mut disp, 0x108, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            disp.window_update_rect(&bus, window),
            Some((100, 200, 300, 500)),
            "ShowHide(TRUE) should invalidate revealed content in global coordinates"
        );
    }

    // IM:I I-285: ShowHide(FALSE) makes the target window invisible but does
    // not reorder windows or generate activate events.
    #[test]
    fn showhide_false_makes_target_invisible_without_front_reorder_or_activate_events() {
        let (mut disp, mut cpu, mut bus) = setup();
        let front_window = 0x200040u32;
        let target_window = 0x200140u32;
        setup_full_window_with_regions(&mut bus, target_window, 20, 30, 90, 180);
        bus.write_byte(front_window + 110, 0xFF);
        bus.write_byte(front_window + 111, 0xFF);
        bus.write_byte(target_window + 110, 0xFF);
        bus.write_byte(target_window + 111, 0x00);

        disp.window_list = vec![front_window, target_window];
        disp.front_window = front_window;
        disp.current_port = front_window;
        disp.queue_window_update_event(target_window);
        assert!(
            disp.event_queue
                .iter()
                .any(|e| e.what == 6 && e.message == target_window),
            "test precondition: target has queued update event"
        );
        let activate_before = disp.event_queue.iter().filter(|e| e.what == 8).count();

        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_byte(sp, 0); // showFlag = FALSE.
        bus.write_long(sp + 2, target_window);

        let result = dispatch(&mut disp, 0x108, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        assert_eq!(
            bus.read_byte(target_window + 110),
            0x00,
            "ShowHide(FALSE) must clear visible byte"
        );
        assert_eq!(
            disp.front_window, front_window,
            "ShowHide must not change front-to-back ordering"
        );
        assert!(
            !disp
                .event_queue
                .iter()
                .any(|e| e.what == 6 && e.message == target_window),
            "ShowHide(FALSE) should clear queued update events for hidden window"
        );
        let activate_after = disp.event_queue.iter().filter(|e| e.what == 8).count();
        assert_eq!(
            activate_after, activate_before,
            "ShowHide must not generate activate/deactivate events"
        );
    }

    #[test]
    fn showhide_false_invalidates_visible_content_behind_target() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

        let back = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            back,
            screen_base,
            40,
            40,
            300,
            500,
            "Back",
            0,
            true,
            true,
            true,
            0,
        );
        let target = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            target,
            screen_base,
            80,
            100,
            220,
            420,
            "Target",
            1,
            true,
            true,
            false,
            0,
        );
        disp.window_list = vec![target, back];
        disp.sync_window_list_links(&mut bus);
        disp.front_window = back;
        disp.current_port = back;
        disp.dialog_visible_snapshots.insert(
            target,
            PersistentDialogSnapshot {
                bounds: (80, 100, 220, 420),
                pixels: vec![0xEE; 140 * 320],
            },
        );
        let update_handle = bus.read_long(back + 122);
        super::super::TrapDispatcher::write_region_handle_rect(&mut bus, update_handle, None);
        disp.clear_queued_update_events(back);

        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_byte(sp, 0);
        bus.write_long(sp + 2, target);
        let result = dispatch(&mut disp, 0x108, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert!(disp.window_update_rect(&bus, back).is_some());
        assert!(disp
            .event_queue
            .iter()
            .any(|event| { event.what == 6 && event.message == back }));
        assert!(!disp.dialog_visible_snapshots.contains_key(&target));
    }

    // ---------------------------------------------------------------
    // 11. SetWTitle (0x11A) -- pops 8 bytes
    // ---------------------------------------------------------------
    #[test]
    fn test_set_wtitle() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        // SP+0: title_ptr(4), SP+4: window(4)
        bus.write_long(sp, 0x300000);
        bus.write_long(sp + 4, 0xDEAD0000);

        let result = dispatch(&mut disp, 0x11A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn setwtitle_redraws_front_window_title_bar() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            60,
            30,
            220,
            370,
            "Old",
            0,
            true,
            true,
            false,
            0,
        );

        let (screen_base, row_bytes, _, _, _) = disp.screen_mode;
        let title_region = |bus: &crate::memory::MacMemoryBus| -> Vec<u8> {
            let mut bytes = Vec::new();
            for y in 41..59u32 {
                for x in 60..340u32 {
                    bytes.push(bus.read_byte(screen_base + y * row_bytes + x));
                }
            }
            bytes
        };
        let before = title_region(&bus);

        let new_title = bus.alloc(32);
        bus.write_pstring(new_title, b"Player 1 | Opponent 0");
        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, new_title);
        bus.write_long(sp + 4, window_addr);

        let result = dispatch(&mut disp, 0x11A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_ne!(
            title_region(&bus),
            before,
            "SetWTitle should repaint the visible front-window title bar"
        );
    }

    #[test]
    fn redraw_chrome_reads_front_window_title_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.menu_bar_hidden = false;

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            60,
            30,
            220,
            370,
            "Old",
            0,
            true,
            true,
            false,
            0,
        );

        let (screen_base, row_bytes, _, _, _) = disp.screen_mode;
        let title_region = |bus: &crate::memory::MacMemoryBus| -> Vec<u8> {
            let mut bytes = Vec::new();
            for y in 41..59u32 {
                for x in 60..340u32 {
                    bytes.push(bus.read_byte(screen_base + y * row_bytes + x));
                }
            }
            bytes
        };
        let before = title_region(&bus);

        let title_handle =
            bus.read_long(window_addr + super::super::TrapDispatcher::WINDOW_TITLE_HANDLE_OFFSET);
        let new_title = bus.alloc(32);
        bus.write_pstring(new_title, b"Player 1 | Opponent 0");
        bus.write_long(title_handle, new_title);

        disp.redraw_chrome(&mut bus);

        assert_ne!(
            title_region(&bus),
            before,
            "front-window chrome redraw should use the live WindowRecord titleHandle"
        );
    }

    #[test]
    fn redraw_chrome_does_not_paint_back_window_chrome_through_front_content() {
        // The Window Manager clips a back window's frame to windows above it.
        // Redrawing raw framebuffer chrome without that clip can leave stale
        // back-window borders inside the front window's content area.
        // Inside Macintosh Volume I (1985), pp. I-296..I-297.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(crate::memory::globals::addr::SCREEN_BITS, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.menu_bar_hidden = false;

        let back = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            back,
            screen_base,
            50,
            50,
            150,
            300,
            "Back",
            4,
            true,
            false,
            false,
            0,
        );
        let front = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            front,
            screen_base,
            50,
            30,
            570,
            370,
            "Front",
            4,
            true,
            false,
            false,
            0,
        );

        let protected = screen_base + 60 * 800 + 49;
        bus.write_byte(protected, 0x7B);

        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_byte(protected),
            0x7B,
            "back-window chrome must be clipped by the front window's content"
        );
    }

    #[test]
    fn inactive_document_window_chrome_still_draws_title_text() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.menu_bar_hidden = false;

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            60,
            30,
            220,
            370,
            "Inactive Title",
            0,
            true,
            true,
            false,
            0,
        );
        bus.write_byte(
            window_addr + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
            0,
        );
        disp.draw_single_window_chrome_inline(&mut bus, window_addr, false);

        let (screen_base, row_bytes, _, _, _) = disp.screen_mode;
        let has_title_pixels = (44..57u32)
            .any(|y| (145..255u32).any(|x| bus.read_byte(screen_base + y * row_bytes + x) != 0));
        assert!(
            has_title_pixels,
            "inactive document windows should retain visible title text"
        );
    }

    // ---------------------------------------------------------------
    // 12. GetWTitle (0x119) -- pops 8 bytes, writes empty string
    // ---------------------------------------------------------------
    #[test]
    fn test_get_wtitle() {
        let (mut disp, mut cpu, mut bus) = setup();

        let title_storage: u32 = 0x300000;
        // Write a non-zero byte so we can verify it gets overwritten
        bus.write_byte(title_storage, 0xFF);

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        // SP+0: title_ptr(4), SP+4: window(4)
        bus.write_long(sp, title_storage);
        bus.write_long(sp + 4, 0xDEAD0000);

        let result = dispatch(&mut disp, 0x119, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        // Verify empty Pascal string written at title_storage
        assert_eq!(
            bus.read_byte(title_storage),
            0,
            "GetWTitle should write empty string (length 0)"
        );
    }

    // ---------------------------------------------------------------
    // 13. FrontWindow (0x124) -- writes front_window at SP, SP unchanged
    // ---------------------------------------------------------------
    #[test]
    fn test_front_window() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Real Mac semantics per IM:I I-274: FrontWindow returns the
        // frontmost VISIBLE window. Seed a window_list entry with its
        // visible byte set so the visible-only walk finds it.
        let win = 0x200040u32;
        disp.window_list = vec![win];
        disp.front_window = win;
        bus.write_byte(win + 110u32, 0xFF);

        let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        let returned = bus.read_long(TEST_SP);
        assert_eq!(
            returned, win,
            "FrontWindow should return the current front visible window"
        );
    }

    // FrontWindow must return the frontmost VISIBLE window per IM:I
    // I-274, not just self.front_window — BringToFront on a hidden
    // window can leave front_window pointing at it.
    #[test]
    fn front_window_skips_hidden_and_returns_first_visible() {
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        disp.window_list = vec![win_b, win_a]; // b first, a behind
        disp.front_window = win_b;
        // b hidden, a visible.
        bus.write_byte(win_a + 110u32, 0xFF);
        bus.write_byte(win_b + 110u32, 0x00);

        let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let returned = bus.read_long(TEST_SP);
        assert_eq!(returned, win_a, "must skip hidden b and return a");
    }

    #[test]
    fn front_window_skips_lowmem_ghost_window() {
        let (mut disp, mut cpu, mut bus) = setup();
        let ghost = 0x200040u32;
        let doc = 0x200140u32;
        disp.window_list = vec![ghost, doc];
        disp.front_window = ghost;
        bus.write_byte(ghost + 110u32, 0xFF);
        bus.write_byte(doc + 110u32, 0xFF);
        bus.write_long(crate::memory::globals::addr::GHOST_WINDOW, ghost);

        let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let returned = bus.read_long(TEST_SP);
        assert_eq!(
            returned, doc,
            "FrontWindow must ignore low-memory GhostWindow"
        );
    }

    #[test]
    fn front_window_returns_nil_when_all_hidden() {
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        disp.window_list = vec![win_b, win_a];
        disp.front_window = win_b;
        // Both hidden.
        bus.write_byte(win_a + 110u32, 0x00);
        bus.write_byte(win_b + 110u32, 0x00);

        let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let returned = bus.read_long(TEST_SP);
        assert_eq!(
            returned, 0,
            "FrontWindow must return NIL when all windows are invisible"
        );
    }

    #[test]
    fn front_window_returns_active_document_behind_custom_utility_layer() {
        let (mut disp, mut cpu, mut bus) = setup();
        install_wdef_resource(&mut disp, &mut bus, 200);

        let utility = bus.alloc(256);
        let document = bus.alloc(256);
        let utility_proc_id = (200i16 << 4) | 3;

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            utility,
            disp.screen_mode.0,
            34,
            2,
            114,
            82,
            "",
            utility_proc_id,
            true,
            false,
            false,
            0,
        );
        bus.write_byte(
            utility + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
            0xFF,
        );

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            document,
            disp.screen_mode.0,
            40,
            86,
            473,
            622,
            "Document",
            8,
            false,
            false,
            true,
            0,
        );
        disp.apply_behind_parameter(&mut bus, document, utility);

        assert_eq!(
            disp.window_list,
            vec![utility, document],
            "custom utility window must remain visually in front"
        );
        assert_eq!(
            disp.front_window, document,
            "standard document behind a custom utility layer remains active"
        );
        assert_eq!(
            bus.read_byte(utility + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET),
            0x00,
            "floating utility should not keep active-window hilite"
        );
        assert_eq!(
            bus.read_byte(document + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET),
            0xFF,
            "document should be the active/hilited window"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_long(TEST_SP),
            utility,
            "FrontWindow should skip the active document while it is still hidden"
        );

        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_byte(sp, 1);
        bus.write_long(sp + 2, document);

        let result = dispatch(&mut disp, 0x108, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            disp.front_window, document,
            "ShowHide(TRUE) should not disturb the pending active document"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_long(TEST_SP),
            utility,
            "FrontWindow should return the first visible window in Window Manager order"
        );

        bus.write_long(crate::memory::globals::addr::GHOST_WINDOW, utility);
        cpu.write_reg(Register::A7, TEST_SP);
        let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_long(TEST_SP),
            document,
            "GhostWindow should exclude the floating utility layer from FrontWindow"
        );

        let wnd_ptr_ptr = bus.alloc(4);
        let find_sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, find_sp);
        bus.write_long(find_sp, wnd_ptr_ptr);
        bus.write_word(find_sp + 4, 40);
        bus.write_word(find_sp + 6, 10);
        bus.write_word(find_sp + 8, 0);

        let result = dispatch(&mut disp, 0x12C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_word(TEST_SP - 2),
            3,
            "FindWindow should still report content in the visual utility layer"
        );
        assert_eq!(
            bus.read_long(wnd_ptr_ptr),
            utility,
            "FindWindow should hit-test against visual layer order"
        );
    }

    // ---------------------------------------------------------------
    // 14a. FindWindow (0x12C) -- menu bar click
    // ---------------------------------------------------------------
    #[test]
    fn test_find_window_menu_bar() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        // Keep a non-NIL front window so this test can verify MTE 1992
        // p. 4-91 semantics: clicks not in a window must set theWindow=NIL.
        disp.front_window = 0xDEAD0000;

        let wnd_ptr_ptr: u32 = 0x300000;

        // SP+0: wnd_ptr_ptr(4), SP+4: pt(4), SP+8: result(2)
        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, wnd_ptr_ptr); // VAR whichWindow
        bus.write_word(sp + 4, 10); // pt.v = 10 (in menu bar, < 20)
        bus.write_word(sp + 6, 100); // pt.h = 100
        bus.write_word(sp + 8, 0); // result placeholder

        let result = dispatch(&mut disp, 0x12C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // SP should advance to old_sp + 8 (pops wnd_ptr_ptr + pt, leaves result)
        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP - 2);

        // part = 1 (inMenuBar) written at sp + 8 = TEST_SP - 10 + 8 = TEST_SP - 2
        let part = bus.read_word(new_sp);
        assert_eq!(
            part, 1,
            "FindWindow with pt.v=10 should return inMenuBar (1)"
        );

        let which_window = bus.read_long(wnd_ptr_ptr);
        assert_eq!(
            which_window, 0,
            "FindWindow inMenuBar must write NIL to whichWindow (MTE 1992 p. 4-91)"
        );
    }

    // ---------------------------------------------------------------
    // 14b. FindWindow (0x12C) -- content click
    // ---------------------------------------------------------------
    #[test]
    fn test_find_window_content() {
        let (mut disp, mut cpu, mut bus) = setup();

        let window_addr: u32 = 0x310000;
        setup_full_window_with_regions(&mut bus, window_addr, 40, 0, 342, 512);
        bus.write_byte(window_addr + 110, 0xFF);
        disp.window_list = vec![window_addr];
        disp.front_window = window_addr;

        let wnd_ptr_ptr: u32 = 0x300000;

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, wnd_ptr_ptr);
        bus.write_word(sp + 4, 100); // pt.v = 100 (not in menu bar)
        bus.write_word(sp + 6, 200); // pt.h = 200
        bus.write_word(sp + 8, 0);

        let result = dispatch(&mut disp, 0x12C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP - 2);

        let part = bus.read_word(new_sp);
        assert_eq!(
            part, 3,
            "FindWindow with a visible window under the point should return inContent (3)"
        );

        // Verify whichWindow was written
        let which_window = bus.read_long(wnd_ptr_ptr);
        assert_eq!(which_window, window_addr);
    }

    #[test]
    fn find_window_walks_front_to_back_and_uses_port_bounds_for_global_hits() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

        let main_window = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            main_window,
            screen_base,
            0,
            0,
            600,
            800,
            "",
            0,
            true,
            false,
            false,
            0,
        );

        let dialog_window = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            dialog_window,
            screen_base,
            110,
            155,
            380,
            645,
            "",
            1,
            true,
            false,
            false,
            0,
        );

        assert_eq!(disp.window_list, vec![dialog_window, main_window]);
        // Regression shape: cached front-window fields can be restored by the
        // application while the Window Manager list still has a visible dialog
        // in front. FindWindow must use the list, not these stale caches.
        disp.front_window = main_window;
        disp.window_bounds = (0, 0, 600, 800);
        disp.window_proc_id = 0;

        let wnd_ptr_ptr: u32 = bus.alloc(4);
        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, wnd_ptr_ptr);
        bus.write_word(sp + 4, 362); // global v inside dialog content
        bus.write_word(sp + 6, 491); // global h inside dialog content
        bus.write_word(sp + 8, 0);

        let result = dispatch(&mut disp, 0x12C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP - 2);
        assert_eq!(bus.read_word(new_sp), 3);
        assert_eq!(
            bus.read_long(wnd_ptr_ptr),
            dialog_window,
            "FindWindow must return the frontmost visible window under the global point"
        );
    }

    // FindWindow must return inDesk + whichWindow=NIL when the click is not
    // in the menu bar and not in any application window (MTE 1992 p. 4-91).
    #[test]
    fn test_find_window_in_desk_returns_nil_window() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.menu_bar_hidden = false;
        let window_addr: u32 = 0x310000;
        setup_full_window_with_regions(&mut bus, window_addr, 40, 0, 342, 512);
        bus.write_byte(window_addr + 110, 0xFF);
        disp.window_list = vec![window_addr];
        disp.front_window = window_addr;

        let wnd_ptr_ptr: u32 = 0x300000;
        bus.write_long(wnd_ptr_ptr, 0xFFFFFFFF);

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, wnd_ptr_ptr);
        bus.write_word(sp + 4, 380); // pt.v outside window + below menu bar
        bus.write_word(sp + 6, 520); // pt.h outside window
        bus.write_word(sp + 8, 0);

        let result = dispatch(&mut disp, 0x12C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP - 2);

        let part = bus.read_word(new_sp);
        assert_eq!(
            part, 0,
            "FindWindow outside window/menu bar should return inDesk (0)"
        );
        let which_window = bus.read_long(wnd_ptr_ptr);
        assert_eq!(
            which_window, 0,
            "FindWindow inDesk must write NIL to whichWindow (MTE 1992 p. 4-91)"
        );
    }

    // ---------------------------------------------------------------
    // 15. BeginUpdate (0x122) -- pops 4 bytes
    // ---------------------------------------------------------------
    #[test]
    fn test_begin_update() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xDEAD0000);

        let result = dispatch(&mut disp, 0x122, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    // BeginUpdate must clip visRgn to visRgn∩updateRgn and clear updateRgn.
    // MTE 1992 p. 4-106.
    #[test]
    fn beginupdate_intersects_visrgn_and_clears_updatergn() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr: u32 = 0x300000;
        let (vis_rgn_data, _clip_rgn_data) =
            setup_window_with_regions(&mut bus, window_addr, 0, 0, 110, 210);
        let (_cont_rgn_data, update_rgn_data) =
            setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 110, 210);

        // updateRgn = (40, 50, 140, 120), visRgn = (0, 0, 110, 210)
        bus.write_word(update_rgn_data + 2, 40);
        bus.write_word(update_rgn_data + 4, 50);
        bus.write_word(update_rgn_data + 6, 140);
        bus.write_word(update_rgn_data + 8, 120);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_addr);

        let result = dispatch(&mut disp, 0x122, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        // Intersection should be (40,50,110,120).
        assert_eq!(bus.read_word(vis_rgn_data + 2) as i16, 40, "visRgn.top");
        assert_eq!(bus.read_word(vis_rgn_data + 4) as i16, 50, "visRgn.left");
        assert_eq!(bus.read_word(vis_rgn_data + 6) as i16, 110, "visRgn.bottom");
        assert_eq!(bus.read_word(vis_rgn_data + 8) as i16, 120, "visRgn.right");

        // updateRgn should be empty.
        assert_eq!(bus.read_word(update_rgn_data + 2), 0, "updateRgn.top");
        assert_eq!(bus.read_word(update_rgn_data + 4), 0, "updateRgn.left");
        assert_eq!(bus.read_word(update_rgn_data + 6), 0, "updateRgn.bottom");
        assert_eq!(bus.read_word(update_rgn_data + 8), 0, "updateRgn.right");
    }

    #[test]
    fn beginupdate_without_pending_update_preserves_visrgn() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr: u32 = 0x300000;
        let (vis_rgn_data, _clip_rgn_data) =
            setup_window_with_regions(&mut bus, window_addr, 0, 0, 110, 210);
        let (_cont_rgn_data, update_rgn_data) =
            setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 110, 210);

        assert_eq!(
            (
                bus.read_word(update_rgn_data + 2),
                bus.read_word(update_rgn_data + 6)
            ),
            (0, 0),
            "test fixture should start with an empty updateRgn"
        );

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_addr);

        let result = dispatch(&mut disp, 0x122, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        assert_eq!(bus.read_word(vis_rgn_data + 2) as i16, 0);
        assert_eq!(bus.read_word(vis_rgn_data + 4) as i16, 0);
        assert_eq!(bus.read_word(vis_rgn_data + 6) as i16, 110);
        assert_eq!(bus.read_word(vis_rgn_data + 8) as i16, 210);
        assert!(
            !disp.saved_vis_regions.contains_key(&window_addr),
            "no-update BeginUpdate should not create a restore obligation"
        );
    }

    // Systems Twilight shifts the port origin before servicing the
    // window update. BeginUpdate must intersect updateRgn in that shifted
    // coordinate space so the stale pixels outside local (0,0) repaint.
    #[test]
    fn beginupdate_intersects_shifted_port_visrgn_with_shifted_updatergn() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr: u32 = 0x300000;
        let (_cont_rgn_data, update_rgn_data) =
            setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 578, 798);
        let vis_rgn_data = 0x301000;

        bus.write_word(window_addr + 16, (-86i16) as u16);
        bus.write_word(window_addr + 18, (-143i16) as u16);
        bus.write_word(window_addr + 20, 492);
        bus.write_word(window_addr + 22, 655);
        bus.write_word(window_addr + 8, (-86i16) as u16);
        bus.write_word(window_addr + 10, (-143i16) as u16);
        bus.write_word(window_addr + 12, 492);
        bus.write_word(window_addr + 14, 655);
        bus.write_word(vis_rgn_data + 2, (-86i16) as u16);
        bus.write_word(vis_rgn_data + 4, (-143i16) as u16);
        bus.write_word(vis_rgn_data + 6, 492);
        bus.write_word(vis_rgn_data + 8, 655);
        bus.write_word(update_rgn_data + 2, 0);
        bus.write_word(update_rgn_data + 4, 0);
        bus.write_word(update_rgn_data + 6, 578);
        bus.write_word(update_rgn_data + 8, 798);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_addr);

        let result = dispatch(&mut disp, 0x122, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        assert_eq!(bus.read_word(vis_rgn_data + 2) as i16, -86);
        assert_eq!(bus.read_word(vis_rgn_data + 4) as i16, -143);
        assert_eq!(bus.read_word(vis_rgn_data + 6) as i16, 492);
        assert_eq!(bus.read_word(vis_rgn_data + 8) as i16, 655);

        assert_eq!(bus.read_word(update_rgn_data + 2), 0, "updateRgn.top");
        assert_eq!(bus.read_word(update_rgn_data + 4), 0, "updateRgn.left");
        assert_eq!(bus.read_word(update_rgn_data + 6), 0, "updateRgn.bottom");
        assert_eq!(bus.read_word(update_rgn_data + 8), 0, "updateRgn.right");
    }

    // ---------------------------------------------------------------
    // 16. EndUpdate (0x123) -- pops 4 bytes
    // ---------------------------------------------------------------
    #[test]
    fn test_end_update() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xDEAD0000);

        let result = dispatch(&mut disp, 0x123, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    // EndUpdate restores the pre-BeginUpdate visRgn (MTE 1992 p. 4-107).
    #[test]
    fn endupdate_restores_saved_visrgn_after_beginupdate() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr: u32 = 0x300000;
        let (vis_rgn_data, _clip_rgn_data) =
            setup_window_with_regions(&mut bus, window_addr, 0, 0, 110, 210);
        let (_cont_rgn_data, update_rgn_data) =
            setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 110, 210);

        // Narrow visRgn during BeginUpdate via updateRgn intersection.
        bus.write_word(update_rgn_data + 2, 30);
        bus.write_word(update_rgn_data + 4, 40);
        bus.write_word(update_rgn_data + 6, 80);
        bus.write_word(update_rgn_data + 8, 100);

        let begin_sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, begin_sp);
        bus.write_long(begin_sp, window_addr);
        let begin_result = dispatch(&mut disp, 0x122, &mut cpu, &mut bus);
        assert!(begin_result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_word(vis_rgn_data + 2) as i16, 30);
        assert_eq!(bus.read_word(vis_rgn_data + 4) as i16, 40);
        assert_eq!(bus.read_word(vis_rgn_data + 6) as i16, 80);
        assert_eq!(bus.read_word(vis_rgn_data + 8) as i16, 100);

        let end_sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, end_sp);
        bus.write_long(end_sp, window_addr);
        let end_result = dispatch(&mut disp, 0x123, &mut cpu, &mut bus);
        assert!(end_result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        // Restored original visRgn from before BeginUpdate.
        assert_eq!(bus.read_word(vis_rgn_data + 2) as i16, 0);
        assert_eq!(bus.read_word(vis_rgn_data + 4) as i16, 0);
        assert_eq!(bus.read_word(vis_rgn_data + 6) as i16, 110);
        assert_eq!(bus.read_word(vis_rgn_data + 8) as i16, 210);
    }

    #[test]
    fn calcvis_consumes_windowpeek_argument() {
        // CalcVis takes one WindowPeek argument.
        // Inside Macintosh Volume I (1985), p. I-297;
        // Macintosh Toolbox Essentials (1992), p. 4-119.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);

        let result = dispatch(&mut disp, 0x109, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn calcvis_preserves_front_window_region_boxes_and_balances_stack() {
        // CalcVis leaves the frontmost window's region boxes unchanged.
        // Inside Macintosh Volume I (1985), p. I-297;
        // Macintosh Toolbox Essentials (1992), p. 4-119.
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr: u32 = 0x300000;
        let (vis_rgn_data, clip_rgn_data) =
            setup_window_with_regions(&mut bus, window_addr, 10, 20, 110, 210);
        let (cont_rgn_data, _update_rgn_data) =
            setup_full_window_with_regions(&mut bus, window_addr, 10, 20, 110, 210);

        let struc_rgn_data: u32 = 0x308000;
        let struc_rgn_handle: u32 = 0x308100;
        bus.write_word(struc_rgn_data, 10);
        bus.write_word(struc_rgn_data + 2, 10);
        bus.write_word(struc_rgn_data + 4, 20);
        bus.write_word(struc_rgn_data + 6, 110);
        bus.write_word(struc_rgn_data + 8, 210);
        bus.write_long(struc_rgn_handle, struc_rgn_data);
        bus.write_long(window_addr + 114, struc_rgn_handle);

        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 18);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_addr);

        let result = dispatch(&mut disp, 0x109, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        for rgn_data in [cont_rgn_data, struc_rgn_data, vis_rgn_data, clip_rgn_data] {
            assert_eq!(bus.read_word(rgn_data + 2) as i16, 10, "rgn.top");
            assert_eq!(bus.read_word(rgn_data + 4) as i16, 20, "rgn.left");
            assert_eq!(bus.read_word(rgn_data + 6) as i16, 110, "rgn.bottom");
            assert_eq!(bus.read_word(rgn_data + 8) as i16, 210, "rgn.right");
        }
    }

    #[test]
    fn calcvisbehind_consumes_startwindow_and_clobberedrgn_arguments() {
        // CalcVisBehind takes two pointer arguments and returns no result.
        // Inside Macintosh Volume I (1985), p. I-297;
        // Macintosh Toolbox Essentials (1992), p. 4-119.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, 0);

        let result = dispatch(&mut disp, 0x10A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn calcvisbehind_recomputes_regions_for_startwindow_and_windows_behind_it() {
        // CalcVisBehind recalculates the visible-region state for
        // startWindow and the windows behind it that intersect clobberedRgn.
        // Inside Macintosh Volume I (1985), p. I-297;
        // Macintosh Toolbox Essentials (1992), p. 4-119.
        let (mut disp, mut cpu, mut bus) = setup();
        let front: u32 = 0x300000;
        let middle: u32 = 0x301000;
        let back: u32 = 0x302000;

        fn setup_window_regions(
            bus: &mut crate::memory::MacMemoryBus,
            window_addr: u32,
            top: i16,
            left: i16,
            bottom: i16,
            right: i16,
            base: u32,
        ) -> (u32, u32, u32, u32) {
            bus.write_word(window_addr + 16, top as u16);
            bus.write_word(window_addr + 18, left as u16);
            bus.write_word(window_addr + 20, bottom as u16);
            bus.write_word(window_addr + 22, right as u16);

            let vis_data = base;
            let vis_handle = base + 0x100;
            bus.write_word(vis_data, 10);
            bus.write_word(vis_data + 2, top as u16);
            bus.write_word(vis_data + 4, left as u16);
            bus.write_word(vis_data + 6, bottom as u16);
            bus.write_word(vis_data + 8, right as u16);
            bus.write_long(vis_handle, vis_data);
            bus.write_long(window_addr + 24, vis_handle);

            let clip_data = base + 0x200;
            let clip_handle = base + 0x300;
            bus.write_word(clip_data, 10);
            bus.write_word(clip_data + 2, top as u16);
            bus.write_word(clip_data + 4, left as u16);
            bus.write_word(clip_data + 6, bottom as u16);
            bus.write_word(clip_data + 8, right as u16);
            bus.write_long(clip_handle, clip_data);
            bus.write_long(window_addr + 28, clip_handle);

            let struc_data = base + 0x400;
            let struc_handle = base + 0x500;
            bus.write_word(struc_data, 10);
            bus.write_word(struc_data + 2, top as u16);
            bus.write_word(struc_data + 4, left as u16);
            bus.write_word(struc_data + 6, bottom as u16);
            bus.write_word(struc_data + 8, right as u16);
            bus.write_long(struc_handle, struc_data);
            bus.write_long(window_addr + 114, struc_handle);

            let cont_data = base + 0x600;
            let cont_handle = base + 0x700;
            bus.write_word(cont_data, 10);
            bus.write_word(cont_data + 2, top as u16);
            bus.write_word(cont_data + 4, left as u16);
            bus.write_word(cont_data + 6, bottom as u16);
            bus.write_word(cont_data + 8, right as u16);
            bus.write_long(cont_handle, cont_data);
            bus.write_long(window_addr + 118, cont_handle);

            (vis_data, clip_data, struc_data, cont_data)
        }

        let (front_vis, front_clip, front_struc, front_cont) =
            setup_window_regions(&mut bus, front, 10, 20, 110, 210, 0x310000);
        let (middle_vis, middle_clip, middle_struc, middle_cont) =
            setup_window_regions(&mut bus, middle, 10, 20, 110, 210, 0x320000);
        let (back_vis, back_clip, back_struc, back_cont) =
            setup_window_regions(&mut bus, back, 10, 20, 110, 210, 0x330000);

        disp.window_list = vec![front, middle, back];
        disp.sync_window_list_links(&mut bus);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 18);

        let clobbered_rgn_data: u32 = 0x303000;
        let clobbered_rgn_handle: u32 = 0x303100;
        bus.write_word(clobbered_rgn_data, 10);
        bus.write_word(clobbered_rgn_data + 2, 0);
        bus.write_word(clobbered_rgn_data + 4, 0);
        bus.write_word(clobbered_rgn_data + 6, 200);
        bus.write_word(clobbered_rgn_data + 8, 200);
        bus.write_long(clobbered_rgn_handle, clobbered_rgn_data);

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, clobbered_rgn_handle);
        bus.write_long(sp + 4, middle);

        let result = dispatch(&mut disp, 0x10A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        for (label, rgn_data) in [
            ("middle_cont", middle_cont),
            ("middle_vis", middle_vis),
            ("middle_clip", middle_clip),
        ] {
            assert_eq!(bus.read_word(rgn_data + 2) as i16, 18, "{}.top", label);
            assert_eq!(bus.read_word(rgn_data + 4) as i16, 20, "{}.left", label);
            assert_eq!(bus.read_word(rgn_data + 6) as i16, 110, "{}.bottom", label);
            assert_eq!(bus.read_word(rgn_data + 8) as i16, 210, "{}.right", label);
        }
        assert_eq!(
            bus.read_word(middle_struc + 2) as i16,
            18,
            "middle_struc.top"
        );
        assert_eq!(
            bus.read_word(middle_struc + 4) as i16,
            19,
            "middle_struc.left"
        );
        assert_eq!(
            bus.read_word(middle_struc + 6) as i16,
            112,
            "middle_struc.bottom"
        );
        assert_eq!(
            bus.read_word(middle_struc + 8) as i16,
            212,
            "middle_struc.right"
        );

        for (label, rgn_data) in [
            ("back_cont", back_cont),
            ("back_vis", back_vis),
            ("back_clip", back_clip),
        ] {
            assert_eq!(bus.read_word(rgn_data + 2) as i16, 18, "{}.top", label);
            assert_eq!(bus.read_word(rgn_data + 4) as i16, 20, "{}.left", label);
            assert_eq!(bus.read_word(rgn_data + 6) as i16, 110, "{}.bottom", label);
            assert_eq!(bus.read_word(rgn_data + 8) as i16, 210, "{}.right", label);
        }
        assert_eq!(bus.read_word(back_struc + 2) as i16, 18, "back_struc.top");
        assert_eq!(bus.read_word(back_struc + 4) as i16, 19, "back_struc.left");
        assert_eq!(
            bus.read_word(back_struc + 6) as i16,
            112,
            "back_struc.bottom"
        );
        assert_eq!(
            bus.read_word(back_struc + 8) as i16,
            212,
            "back_struc.right"
        );

        for (label, rgn_data) in [
            ("front_cont", front_cont),
            ("front_struc", front_struc),
            ("front_vis", front_vis),
            ("front_clip", front_clip),
        ] {
            assert_eq!(bus.read_word(rgn_data + 2) as i16, 10, "{}.top", label);
            assert_eq!(bus.read_word(rgn_data + 4) as i16, 20, "{}.left", label);
            assert_eq!(bus.read_word(rgn_data + 6) as i16, 110, "{}.bottom", label);
            assert_eq!(bus.read_word(rgn_data + 8) as i16, 210, "{}.right", label);
        }
    }

    #[test]
    fn calc_vis_subtracts_front_window_structure_from_window_behind() {
        // CalcVis builds a window's visRgn from its content region minus the
        // structure regions of the windows in front of it, so drawing in the
        // back window cannot paint over the front one.
        // Inside Macintosh Volume I (1985), p. I-297.
        let (mut disp, mut cpu, mut bus) = setup();
        let back = bus.alloc(256);
        let front = bus.alloc(256);
        disp.menu_bar_hidden = true;

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            back,
            disp.screen_mode.0,
            0,
            0,
            200,
            300,
            "",
            0,
            true,
            false,
            false,
            0,
        );
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            front,
            disp.screen_mode.0,
            50,
            60,
            90,
            160,
            "",
            2,
            true,
            false,
            false,
            0,
        );

        assert_eq!(
            disp.window_list.first().copied(),
            Some(front),
            "the newly created window should be frontmost"
        );

        let back_vis = bus.read_long(back + 24);
        assert!(
            super::super::TrapDispatcher::region_is_complex(&bus, back_vis),
            "the back window's visRgn should have a hole where the front window sits"
        );
        assert!(
            !super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 70, 100),
            "a point under the front window must be outside the back window's visRgn"
        );
        assert!(
            super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 150, 250),
            "a point clear of the front window must stay inside the back window's visRgn"
        );
    }

    #[test]
    fn begin_update_keeps_front_window_hole_out_of_the_update_clip() {
        // BeginUpdate intersects visRgn with updateRgn. The intersection has to
        // be a real region operation: visRgn already excludes windows in front,
        // and a bounding-box intersection would hand those pixels back and let
        // the window underneath repaint over a modal dialog.
        // Inside Macintosh Volume I (1985), p. I-292.
        let (mut disp, mut cpu, mut bus) = setup();
        let back = bus.alloc(256);
        let front = bus.alloc(256);
        disp.menu_bar_hidden = true;

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            back,
            disp.screen_mode.0,
            0,
            0,
            200,
            300,
            "",
            0,
            true,
            false,
            false,
            0,
        );
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            front,
            disp.screen_mode.0,
            50,
            60,
            90,
            160,
            "",
            2,
            true,
            false,
            false,
            0,
        );

        let back_update =
            bus.read_long(back + super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET);
        super::super::TrapDispatcher::write_region_handle_rect(
            &mut bus,
            back_update,
            Some((0, 0, 200, 300)),
        );
        disp.begin_update_window(&mut bus, back);

        let back_vis = bus.read_long(back + 24);
        assert!(
            !super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 70, 100),
            "BeginUpdate must not restore pixels the front window covers"
        );
        assert!(
            super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 150, 250),
            "BeginUpdate should keep the rest of the update area drawable"
        );

        disp.end_update_window(&mut bus, back);
        assert!(
            !super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 70, 100),
            "EndUpdate should recompute visRgn via CalcVis, hole included"
        );
        assert!(
            super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 150, 250),
            "EndUpdate should restore the rest of the content region"
        );
    }

    #[test]
    fn calcvisbehind_menu_clamp_uses_window_local_coordinates() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            100,
            200,
            300,
            500,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        disp.window_list = vec![window_addr];
        disp.sync_window_list_links(&mut bus);

        let clobbered_rgn = super::super::TrapDispatcher::alloc_rect_region_handle(
            &mut bus,
            Some((100, 200, 300, 500)),
        );
        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, clobbered_rgn);
        bus.write_long(sp + 4, window_addr);

        let result = dispatch(&mut disp, 0x10A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            read_window_region_rect(&bus, window_addr, 24),
            (0, 0, 200, 300),
            "a window below the menu bar should not be clipped by global MBarHeight in local coords"
        );
        assert_eq!(
            read_window_region_rect(
                &bus,
                window_addr,
                super::super::TrapDispatcher::WINDOW_CONT_RGN_OFFSET
            ),
            (100, 200, 300, 500),
            "CalcVisBehind should preserve global content coordinates for nonzero window origins"
        );
    }

    #[test]
    fn checkupdate_returns_true_and_writes_eventrecord_for_pending_update() {
        // CheckUpdate returns TRUE and writes an update EventRecord when a
        // visible window needs updating.
        // Inside Macintosh Volume I (1985), p. I-296;
        // Macintosh Toolbox Essentials (1992), p. 4-116.
        let (mut disp, mut cpu, mut bus) = setup();
        let event_ptr = bus.alloc(16);
        for i in 0..16 {
            bus.write_byte(event_ptr + i, 0xAA);
        }

        let update_window = 0x00A0_4000;
        disp.event_queue.push_back(QueuedEvent {
            what: 6,
            message: update_window,
            where_v: 77,
            where_h: 123,
            modifiers: 0x4400,
        });

        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0);

        let result = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 2);
        assert_eq!(bus.read_word(TEST_SP - 2), 0xFFFF, "result should be TRUE");
        assert_eq!(
            bus.read_word(event_ptr),
            6,
            "event.what should be updateEvt"
        );
        assert_eq!(
            bus.read_long(event_ptr + 2),
            update_window,
            "event.message should carry WindowPtr"
        );
        assert_eq!(bus.read_word(event_ptr + 10) as i16, 77, "event.where.v");
        assert_eq!(bus.read_word(event_ptr + 12) as i16, 123, "event.where.h");
        assert_eq!(bus.read_word(event_ptr + 14), 0x4400, "event.modifiers");
        assert!(
            disp.event_queue.is_empty(),
            "CheckUpdate should dequeue the consumed update event"
        );
    }

    #[test]
    fn checkupdate_returns_false_when_no_update_is_pending() {
        // CheckUpdate returns FALSE when no visible window requires updating.
        // Inside Macintosh Volume I (1985), p. I-296;
        // Macintosh Toolbox Essentials (1992), p. 4-116.
        let (mut disp, mut cpu, mut bus) = setup();
        let event_ptr = bus.alloc(16);
        for i in 0..16 {
            bus.write_byte(event_ptr + i, 0xCC);
        }

        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0xFFFF);

        let result = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 2);
        assert_eq!(bus.read_word(TEST_SP - 2), 0, "result should be FALSE");
        assert_eq!(bus.read_word(event_ptr), 0xCCCC, "event record untouched");
    }

    #[test]
    fn checkupdate_redraws_window_picture_and_continues_to_next_update() {
        // CheckUpdate redraws dirty windows with a non-NIL windowPic itself
        // and continues scanning instead of returning their update events.
        // Macintosh Toolbox Essentials (1992), p. 4-116.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.menu_bar_hidden = true;
        let picture_window = bus.alloc(256);
        let screen_base = disp.screen_mode.0;
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            picture_window,
            screen_base,
            0,
            0,
            8,
            8,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        let picture = make_v1_paintrect_picture(&mut bus, (0, 0, 8, 8));
        bus.write_long(
            picture_window + super::super::TrapDispatcher::WINDOW_PIC_OFFSET,
            picture,
        );

        let ordinary_window = 0x00A0_4000;
        disp.event_queue.push_back(QueuedEvent {
            what: 6,
            message: ordinary_window,
            where_v: 77,
            where_h: 123,
            modifiers: 0x4400,
        });

        let event_ptr = bus.alloc(16);
        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0);

        let result = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 0xFFFF);
        assert_eq!(bus.read_word(event_ptr), 6);
        assert_eq!(
            bus.read_long(event_ptr + 2),
            ordinary_window,
            "CheckUpdate should continue to the next ordinary dirty window"
        );
        assert!(
            !disp.window_has_pending_update(&bus, picture_window),
            "the automatic picture redraw should clear its update region"
        );
        assert_eq!(
            bus.read_byte(screen_base),
            0xFF,
            "the installed picture should be rendered into the window"
        );
        assert!(
            !disp
                .event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == picture_window),
            "the pictured window must not retain an application-visible update"
        );
    }

    #[test]
    fn toolbox_events_deliver_front_update_before_redrawing_back_window_picture() {
        // Event Manager update scans use the same front-to-back CheckUpdate
        // ordering: stop at the first ordinary dirty window, then redraw a
        // pictured window behind it on the following scan.
        // Macintosh Toolbox Essentials (1992), p. 4-116.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.menu_bar_hidden = true;
        let screen_base = disp.screen_mode.0;
        let pictured_back = bus.alloc(256);
        let ordinary_front = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            pictured_back,
            screen_base,
            0,
            0,
            8,
            8,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            ordinary_front,
            screen_base,
            20,
            20,
            28,
            28,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        let picture = make_v1_paintrect_picture(&mut bus, (0, 0, 8, 8));
        bus.write_long(
            pictured_back + super::super::TrapDispatcher::WINDOW_PIC_OFFSET,
            picture,
        );

        let (what, message, _, _, _, has_event) =
            disp.dequeue_toolbox_event(&mut cpu, &mut bus, 1u16 << 6);
        assert!(has_event);
        assert_eq!(what, 6);
        assert_eq!(
            message, ordinary_front,
            "the front ordinary window should receive the first update"
        );
        assert!(
            disp.window_has_pending_update(&bus, pictured_back),
            "CheckUpdate must stop before redrawing a pictured window behind an ordinary update"
        );

        disp.begin_update_window(&mut bus, ordinary_front);
        disp.end_update_window(&mut bus, ordinary_front);
        let (what, _, _, _, _, has_event) =
            disp.dequeue_toolbox_event(&mut cpu, &mut bus, 1u16 << 6);
        assert!(!has_event);
        assert_eq!(what, 0);
        assert!(
            !disp.window_has_pending_update(&bus, pictured_back),
            "the following scan should redraw and clear the pictured window"
        );
        assert_eq!(
            bus.read_byte(screen_base),
            0xFF,
            "the back window picture should be rendered automatically"
        );
    }

    #[test]
    fn checkupdate_nil_output_pointer_consumes_pending_update() {
        // A nil output pointer is defensive no-op territory in Systemless:
        // the pending update still gets consumed, but nothing is written
        // through the pointer.
        let (mut disp, mut cpu, mut bus) = setup();

        disp.event_queue.push_back(QueuedEvent {
            what: 6,
            message: 0x00A0_4000,
            where_v: 12,
            where_h: 34,
            modifiers: 0x4400,
        });

        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, 0xFFFF);

        let result = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 2);
        assert_eq!(bus.read_word(TEST_SP - 2), 0xFFFF, "result should be TRUE");
        assert!(
            disp.event_queue.is_empty(),
            "CheckUpdate(nil) should still consume the pending update"
        );
    }

    #[test]
    fn checkupdate_clears_false_after_validrect_drops_the_pending_update() {
        // CheckUpdate reports TRUE for an invalidated visible window and
        // returns FALSE once ValidRect clears that window's update region.
        let (mut disp, mut cpu, mut bus) = setup();
        let event_ptr = bus.alloc(16);
        let bounds_rect_ptr = bus.alloc(8);
        let sp = TEST_SP - 26;
        let wnd;

        bus.write_word(bounds_rect_ptr, 60);
        bus.write_word(bounds_rect_ptr + 2, 80);
        bus.write_word(bounds_rect_ptr + 4, 180);
        bus.write_word(bounds_rect_ptr + 6, 260);

        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 18, bounds_rect_ptr);
        bus.write_byte(sp + 12, 1);
        bus.write_long(sp + 6, 0xFFFFFFFF);

        let new_window = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
        assert!(new_window.is_some(), "NewWindow should be handled");
        assert!(new_window.unwrap().is_ok(), "NewWindow should return");
        wnd = bus.read_long(cpu.read_reg(Register::A7));
        disp.validate_window_rect(&mut bus, wnd, (0, 0, 120, 180));

        bus.write_byte(event_ptr, 0xAA);
        bus.write_byte(event_ptr + 1, 0xAA);
        bus.write_word(event_ptr + 2, 0xAAAA);
        bus.write_long(event_ptr + 4, 0xAAAAAAAA);
        bus.write_word(event_ptr + 8, 0xAAAA);
        bus.write_word(event_ptr + 10, 0xAAAA);
        bus.write_word(event_ptr + 12, 0xAAAA);
        bus.write_word(event_ptr + 14, 0xAAAA);

        disp.invalidate_window_rect(&mut bus, wnd, (30, 20, 120, 90));

        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0xFFFF);

        let result_true = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
        assert!(result_true.is_some(), "CheckUpdate should be handled");
        assert!(result_true.unwrap().is_ok(), "CheckUpdate should return");
        assert_eq!(bus.read_word(TEST_SP - 2), 0xFFFF, "result should be TRUE");
        assert_eq!(
            bus.read_word(event_ptr),
            6,
            "CheckUpdate should write updateEvt when the window is invalidated",
        );

        disp.validate_window_rect(&mut bus, wnd, (30, 20, 120, 90));

        bus.write_byte(event_ptr, 0xAA);
        bus.write_byte(event_ptr + 1, 0xAA);
        bus.write_word(event_ptr + 2, 0xAAAA);
        bus.write_long(event_ptr + 4, 0xAAAAAAAA);
        bus.write_word(event_ptr + 8, 0xAAAA);
        bus.write_word(event_ptr + 10, 0xAAAA);
        bus.write_word(event_ptr + 12, 0xAAAA);
        bus.write_word(event_ptr + 14, 0xAAAA);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0);

        let result_false = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
        assert!(result_false.is_some(), "CheckUpdate should be handled");
        assert!(result_false.unwrap().is_ok(), "CheckUpdate should return");
        assert_eq!(bus.read_word(TEST_SP - 2), 0, "result should be FALSE");
        assert_eq!(
            bus.read_word(event_ptr),
            0xAAAA,
            "CheckUpdate should leave the caller's EventRecord untouched after ValidRect",
        );
    }

    // ---------------------------------------------------------------
    // 17. SetWRefCon (0x118) -- pops 8 bytes
    // ---------------------------------------------------------------
    #[test]
    fn test_set_wrefcon() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x12345678); // refCon
        bus.write_long(sp + 4, 0xDEAD0000); // window

        let result = dispatch(&mut disp, 0x118, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    // ---------------------------------------------------------------
    // 18. GetWRefCon (0x117) -- pops 4, writes 0 at SP+4
    // ---------------------------------------------------------------
    #[test]
    fn test_get_wrefcon() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xDEAD0000); // window

        // Write a non-zero value where the result will go to verify it gets zeroed
        bus.write_long(sp + 4, 0xFFFFFFFF);

        let result = dispatch(&mut disp, 0x117, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        // Result (0) written at old sp + 4 = TEST_SP
        let refcon = bus.read_long(TEST_SP);
        assert_eq!(refcon, 0, "GetWRefCon should return 0");
    }

    #[test]
    fn setwincolor_consumes_arguments_sets_content_color_and_queues_update() {
        // Macintosh Toolbox Essentials (1992), pp. 4-114..4-115:
        // SetWinColor applies a window color table and redraws the window in
        // the new colors.
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            40,
            40,
            200,
            200,
            "",
            0,
            true,
            true,
            false,
            0,
        );
        let aux_handle_before = disp
            .window_aux_records
            .get(&window_addr)
            .copied()
            .expect("fresh CGraf window should have an AuxWin record");
        let aux_ptr_before = bus.read_long(aux_handle_before);
        let default_ctab =
            bus.read_long(aux_ptr_before + super::super::TrapDispatcher::AUX_WIN_CTABLE_OFFSET);
        disp.event_queue.clear();

        // One-entry WinCTab: entry value 0 (wContentColor) -> RGB.
        let wctab_ptr = bus.alloc(16);
        let wctab_handle = bus.alloc(4);
        bus.write_long(wctab_handle, wctab_ptr);
        bus.write_word(wctab_ptr + 6, 0); // ctSize = 0 (one entry)
        bus.write_word(wctab_ptr + 8, 0); // part id = wContentColor
        bus.write_word(wctab_ptr + 10, 0x1234);
        bus.write_word(wctab_ptr + 12, 0x5678);
        bus.write_word(wctab_ptr + 14, 0x9ABC);

        // Pascal stack order in this trap surface: second parameter at SP,
        // first parameter at SP+4.
        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, wctab_handle); // newColorTable
        bus.write_long(sp + 4, window_addr); // theWindow

        let result = dispatch(&mut disp, 0x241, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetWinColor should be handled");
        assert!(result.unwrap().is_ok(), "SetWinColor should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP,
            "SetWinColor should consume two pointer arguments"
        );
        assert_eq!(
            bus.read_word(window_addr + 42),
            0x1234,
            "SetWinColor should write content red channel"
        );
        assert_eq!(
            bus.read_word(window_addr + 44),
            0x5678,
            "SetWinColor should write content green channel"
        );
        assert_eq!(
            bus.read_word(window_addr + 46),
            0x9ABC,
            "SetWinColor should write content blue channel"
        );
        assert!(
            disp.event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == window_addr),
            "SetWinColor should queue an update event for the target window"
        );
        assert_eq!(
            disp.window_aux_records.get(&window_addr).copied(),
            Some(aux_handle_before),
            "SetWinColor should update the existing AuxWin handle in place"
        );
        assert_eq!(
            bus.read_long(aux_ptr_before + super::super::TrapDispatcher::AUX_WIN_CTABLE_OFFSET),
            wctab_handle,
            "SetWinColor should rewrite awCTable to the supplied WCTabHandle"
        );
        assert_ne!(
            default_ctab, wctab_handle,
            "fixture should replace the default AuxWin color table with a new handle"
        );
    }

    #[test]
    fn getauxwin_returns_true_writes_aux_handle_and_pops_arguments_for_tracked_window() {
        // Fresh NewWindow/NewCWindow objects on BasiliskII already expose a
        // non-NIL AuxWin record; HLE tracks the same caller-observable state.
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            disp.screen_mode.0,
            40,
            40,
            200,
            200,
            "",
            0,
            true,
            true,
            false,
            0,
        );
        let expected_aux = disp
            .window_aux_records
            .get(&window_addr)
            .copied()
            .expect("fresh CGraf window should have an AuxWin record");
        let expected_aux_ptr = bus.read_long(expected_aux);

        let aw_out = bus.alloc(4);
        bus.write_long(aw_out, 0xDEAD_BEEF);

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, aw_out); // VAR awHndl
        bus.write_long(sp + 4, window_addr); // theWindow
        bus.write_word(sp + 8, 0xFFFF); // result sentinel

        let result = dispatch(&mut disp, 0x242, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetAuxWin should be handled");
        assert!(result.unwrap().is_ok(), "GetAuxWin should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP - 2,
            "GetAuxWin should consume two pointer arguments and leave result slot"
        );
        assert_eq!(
            bus.read_long(aw_out),
            expected_aux,
            "GetAuxWin should write the tracked AuxWinHandle"
        );
        assert_eq!(
            bus.read_word(TEST_SP - 2),
            0x0100,
            "GetAuxWin should return TRUE for tracked windows"
        );
        assert_eq!(
            bus.read_long(expected_aux_ptr + super::super::TrapDispatcher::AUX_WIN_OWNER_OFFSET),
            window_addr,
            "AuxWin record should point back to the tracked window"
        );
        assert_ne!(
            bus.read_long(expected_aux_ptr + super::super::TrapDispatcher::AUX_WIN_CTABLE_OFFSET),
            0,
            "AuxWin record should carry a non-NIL color table handle"
        );
    }

    #[test]
    fn getauxwin_returns_false_writes_nil_and_pops_arguments_for_untracked_pointer() {
        // Macintosh Toolbox Essentials (1992), p. 4-115:
        // GetAuxWin reports FALSE when the queried window has no tracked
        // auxiliary window record.
        let (mut disp, mut cpu, mut bus) = setup();

        let aw_out = bus.alloc(4);
        bus.write_long(aw_out, 0xDEAD_BEEF);

        // FUNCTION result slot (Boolean) lives at SP+8 after two pointer args.
        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, aw_out); // VAR awHndl
        bus.write_long(sp + 4, 0x00C0_FFEE); // theWindow
        bus.write_word(sp + 8, 0xFFFF); // result sentinel

        let result = dispatch(&mut disp, 0x242, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetAuxWin should be handled");
        assert!(result.unwrap().is_ok(), "GetAuxWin should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP - 2,
            "GetAuxWin should consume two pointer arguments and leave result slot"
        );
        assert_eq!(
            bus.read_long(aw_out),
            0,
            "GetAuxWin should write NIL when no aux-window record exists"
        );
        assert_eq!(
            bus.read_word(TEST_SP - 2),
            0,
            "GetAuxWin should return FALSE when no aux-window record exists"
        );
    }

    // ---------------------------------------------------------------
    // Helper: set up a window structure at a given address with valid
    // portRect and region handles for MoveWindow/SizeWindow tests.
    // ---------------------------------------------------------------
    // Full window setup including contRgn + updateRgn so tests can
    // exercise the fUpdate=TRUE invalidation path. Returns the contRgn
    // and updateRgn data pointers for inspection.
    fn setup_full_window_with_regions(
        bus: &mut crate::memory::MacMemoryBus,
        window_addr: u32,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) -> (u32, u32) {
        setup_window_with_regions(bus, window_addr, top, left, bottom, right);
        // contRgn at +118
        let cont_rgn_data: u32 = 0x303000;
        let cont_rgn_handle: u32 = 0x303100;
        bus.write_word(cont_rgn_data, 10);
        bus.write_word(cont_rgn_data + 2, top as u16);
        bus.write_word(cont_rgn_data + 4, left as u16);
        bus.write_word(cont_rgn_data + 6, bottom as u16);
        bus.write_word(cont_rgn_data + 8, right as u16);
        bus.write_long(cont_rgn_handle, cont_rgn_data);
        bus.write_long(window_addr + 118, cont_rgn_handle);
        // updateRgn at +122 — starts empty
        let update_rgn_data: u32 = 0x304000;
        let update_rgn_handle: u32 = 0x304100;
        bus.write_word(update_rgn_data, 10);
        bus.write_word(update_rgn_data + 2, 0);
        bus.write_word(update_rgn_data + 4, 0);
        bus.write_word(update_rgn_data + 6, 0);
        bus.write_word(update_rgn_data + 8, 0);
        bus.write_long(update_rgn_handle, update_rgn_data);
        bus.write_long(window_addr + 122, update_rgn_handle);
        (cont_rgn_data, update_rgn_data)
    }

    fn setup_window_with_regions(
        bus: &mut crate::memory::MacMemoryBus,
        window_addr: u32,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) -> (u32, u32) {
        // portRect at window + 16..22
        bus.write_word(window_addr + 16, top as u16);
        bus.write_word(window_addr + 18, left as u16);
        bus.write_word(window_addr + 20, bottom as u16);
        bus.write_word(window_addr + 22, right as u16);

        // visRgn: data at 0x301000, handle at 0x301100
        let vis_rgn_data: u32 = 0x301000;
        let vis_rgn_handle: u32 = 0x301100;
        bus.write_word(vis_rgn_data, 10); // rgnSize
        bus.write_word(vis_rgn_data + 2, top as u16);
        bus.write_word(vis_rgn_data + 4, left as u16);
        bus.write_word(vis_rgn_data + 6, bottom as u16);
        bus.write_word(vis_rgn_data + 8, right as u16);
        bus.write_long(vis_rgn_handle, vis_rgn_data);
        bus.write_long(window_addr + 24, vis_rgn_handle);

        // clipRgn: data at 0x302000, handle at 0x302100
        let clip_rgn_data: u32 = 0x302000;
        let clip_rgn_handle: u32 = 0x302100;
        bus.write_word(clip_rgn_data, 10);
        bus.write_word(clip_rgn_data + 2, top as u16);
        bus.write_word(clip_rgn_data + 4, left as u16);
        bus.write_word(clip_rgn_data + 6, bottom as u16);
        bus.write_word(clip_rgn_data + 8, right as u16);
        bus.write_long(clip_rgn_handle, clip_rgn_data);
        bus.write_long(window_addr + 28, clip_rgn_handle);

        (vis_rgn_data, clip_rgn_data)
    }

    fn install_wstate_data(
        bus: &mut crate::memory::MacMemoryBus,
        window_addr: u32,
        user_state: (i16, i16, i16, i16),
        std_state: (i16, i16, i16, i16),
    ) {
        let data_ptr: u32 = 0x305000;
        let data_handle: u32 = 0x305100;
        bus.write_long(data_handle, data_ptr);
        bus.write_long(window_addr + 130, data_handle);

        bus.write_word(data_ptr, user_state.0 as u16);
        bus.write_word(data_ptr + 2, user_state.1 as u16);
        bus.write_word(data_ptr + 4, user_state.2 as u16);
        bus.write_word(data_ptr + 6, user_state.3 as u16);

        bus.write_word(data_ptr + 8, std_state.0 as u16);
        bus.write_word(data_ptr + 10, std_state.1 as u16);
        bus.write_word(data_ptr + 12, std_state.2 as u16);
        bus.write_word(data_ptr + 14, std_state.3 as u16);
    }

    // ---------------------------------------------------------------
    // 19. MoveWindow (0x11B) -- moves window, updates portBits.bounds
    //     portRect stays in local coords; portBits.bounds maps local→screen.
    //     Stack: SP+0=front(2), SP+2=vGlobal(2), SP+4=hGlobal(2), SP+6=theWindow(4)
    //     Pops 10 bytes.
    // ---------------------------------------------------------------
    #[test]
    fn test_move_window() {
        let (mut disp, mut cpu, mut bus) = setup();

        let window_addr: u32 = 0x300000;
        let (vis_rgn_data, clip_rgn_data) =
            setup_window_with_regions(&mut bus, window_addr, 40, 0, 342, 512);

        // Push 10 bytes of params
        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // front (boolean)
        bus.write_word(sp + 2, 100); // vGlobal = 100
        bus.write_word(sp + 4, 50); // hGlobal = 50
        bus.write_long(sp + 6, window_addr); // theWindow

        let result = dispatch(&mut disp, 0x11B, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        // portRect stays in local coords — unchanged
        assert_eq!(
            bus.read_word(window_addr + 16) as i16,
            40,
            "portRect.top unchanged"
        );
        assert_eq!(
            bus.read_word(window_addr + 18) as i16,
            0,
            "portRect.left unchanged"
        );
        assert_eq!(
            bus.read_word(window_addr + 20) as i16,
            342,
            "portRect.bottom unchanged"
        );
        assert_eq!(
            bus.read_word(window_addr + 22) as i16,
            512,
            "portRect.right unchanged"
        );

        // portBits.bounds updated: top=-vGlobal, left=-hGlobal
        // GrafPort portBits.bounds at offset 8..16
        assert_eq!(
            bus.read_word(window_addr + 8) as i16,
            -100,
            "portBits.bounds.top"
        );
        assert_eq!(
            bus.read_word(window_addr + 10) as i16,
            -50,
            "portBits.bounds.left"
        );

        // visRgn and clipRgn stay in local coords — unchanged
        assert_eq!(
            bus.read_word(vis_rgn_data + 2) as i16,
            40,
            "visRgn.top unchanged"
        );
        assert_eq!(
            bus.read_word(vis_rgn_data + 4) as i16,
            0,
            "visRgn.left unchanged"
        );
        assert_eq!(
            bus.read_word(clip_rgn_data + 2) as i16,
            40,
            "clipRgn.top unchanged"
        );
        assert_eq!(
            bus.read_word(clip_rgn_data + 4) as i16,
            0,
            "clipRgn.left unchanged"
        );
    }

    #[test]
    fn move_window_restores_exposed_desktop_when_menu_bar_visible() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        disp.menu_bar_hidden = false;
        super::super::TrapDispatcher::fb_fill_pattern_rect(
            &mut bus,
            screen_base,
            800,
            8,
            800,
            600,
            0,
            0,
            600,
            800,
            [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
        );

        let window_addr = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            screen_base,
            180,
            400,
            420,
            600,
            "Player",
            4,
            true,
            true,
            true,
            0,
        );

        let old_content_probe = screen_base + 200 * 800 + 410;
        assert_eq!(
            bus.read_byte(old_content_probe),
            0,
            "precondition: old window content area starts white"
        );

        disp.move_window_to_global(&mut bus, window_addr, 450, 230, true);

        assert_eq!(
            bus.read_byte(old_content_probe),
            255,
            "moving a visible window should restore the exposed old area to the desktop pattern"
        );
        let new_content_probe = screen_base + 250 * 800 + 460;
        assert_eq!(
            bus.read_byte(new_content_probe),
            0,
            "moving a visible window should preserve the window's screen pixels at the new position"
        );
    }

    #[test]
    fn move_window_reveals_and_invalidates_background_content() {
        // MoveWindow/DragWindow must pair the screen copy with CalcVisBehind
        // and PaintBehind semantics. The back window's old occlusion hole is
        // removed and its newly exposed content is queued for application
        // redraw. Inside Macintosh Volume I, I-289, I-293, I-297.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(320 * 240);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 320, 320, 240, 8);
        disp.menu_bar_hidden = true;

        let back = bus.alloc(256);
        let front = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            back,
            screen_base,
            20,
            20,
            220,
            300,
            "Back",
            0,
            true,
            false,
            false,
            0,
        );
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            front,
            screen_base,
            50,
            60,
            130,
            180,
            "Front",
            0,
            true,
            false,
            false,
            0,
        );
        let back_vis = bus.read_long(back + 24);
        assert!(
            !super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 80, 100),
            "precondition: the front structure occludes the back window"
        );
        disp.validate_window_rect(&mut bus, back, (0, 0, 200, 280));
        disp.validate_window_rect(&mut bus, front, (0, 0, 80, 120));

        disp.move_window_to_global(&mut bus, front, 190, 140, true);

        assert!(
            super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 80, 100),
            "the back visRgn must expose the front window's former location"
        );
        let back_update = bus.read_long(
            back + super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET,
        );
        assert!(
            super::super::TrapDispatcher::region_contains_point(&bus, back_update, 80, 100),
            "the newly exposed back content must be invalidated"
        );
        assert!(
            disp.event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == back),
            "the back window must receive an update event"
        );
        assert!(
            disp.event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == front),
            "the moved window must redraw any newly visible content"
        );
    }

    // ---------------------------------------------------------------
    // 20. SizeWindow (0x11D) -- resizes window, updates portRect & regions
    //     Stack: SP+0=fUpdate(2), SP+2=h(2), SP+4=w(2), SP+6=theWindow(4)
    //     Pops 10 bytes.
    // ---------------------------------------------------------------
    #[test]
    fn test_size_window() {
        let (mut disp, mut cpu, mut bus) = setup();

        let window_addr: u32 = 0x300000;
        let (vis_rgn_data, clip_rgn_data) =
            setup_window_with_regions(&mut bus, window_addr, 40, 0, 342, 512);

        // Push 10 bytes of params
        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // fUpdate
        bus.write_word(sp + 2, 480); // h = 480
        bus.write_word(sp + 4, 640); // w = 640
        bus.write_long(sp + 6, window_addr); // theWindow

        let result = dispatch(&mut disp, 0x11D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        // SizeWindow sets portRect to local coords: (0, 0, h, w)
        assert_eq!(bus.read_word(window_addr + 16) as i16, 0, "portRect.top");
        assert_eq!(bus.read_word(window_addr + 18) as i16, 0, "portRect.left");
        assert_eq!(
            bus.read_word(window_addr + 20) as i16,
            480,
            "portRect.bottom"
        );
        assert_eq!(
            bus.read_word(window_addr + 22) as i16,
            640,
            "portRect.right"
        );

        // visRgn updated in local coords (top kept from existing region)
        assert_eq!(bus.read_word(vis_rgn_data + 2) as i16, 40, "visRgn.top");
        assert_eq!(bus.read_word(vis_rgn_data + 4) as i16, 0, "visRgn.left");
        assert_eq!(bus.read_word(vis_rgn_data + 6) as i16, 480, "visRgn.bottom");
        assert_eq!(bus.read_word(vis_rgn_data + 8) as i16, 640, "visRgn.right");

        // clipRgn updated in local coords (top kept from existing region)
        assert_eq!(bus.read_word(clip_rgn_data + 2) as i16, 40, "clipRgn.top");
        assert_eq!(bus.read_word(clip_rgn_data + 4) as i16, 0, "clipRgn.left");
        assert_eq!(
            bus.read_word(clip_rgn_data + 6) as i16,
            480,
            "clipRgn.bottom"
        );
        assert_eq!(
            bus.read_word(clip_rgn_data + 8) as i16,
            640,
            "clipRgn.right"
        );
    }

    #[test]
    fn size_window_recalculates_background_occlusion_region() {
        // IM:I 1985 pp. I-287 and I-297: resizing a background window must
        // recalculate its visRgn against the structure regions in front.
        let (mut disp, mut cpu, mut bus) = setup();
        let back = bus.alloc(256);
        let front = bus.alloc(256);
        disp.menu_bar_hidden = true;

        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            back,
            disp.screen_mode.0,
            0,
            0,
            200,
            300,
            "",
            0,
            true,
            false,
            false,
            0,
        );
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            front,
            disp.screen_mode.0,
            50,
            60,
            90,
            160,
            "",
            2,
            true,
            false,
            false,
            0,
        );

        let back_vis = bus.read_long(back + 24);
        assert!(
            !super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 70, 100),
            "the initial background visRgn must exclude the front window"
        );

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0);
        bus.write_word(sp + 2, 250);
        bus.write_word(sp + 4, 350);
        bus.write_long(sp + 6, back);
        let result = dispatch(&mut disp, 0x11D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert!(
            !super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 70, 100),
            "the resized background must still exclude the front window"
        );
        assert!(
            super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 225, 325),
            "newly enlarged unobscured background pixels must become visible"
        );
    }

    // IM:I p.I-296 (with Rect-by-pointer calling convention on p.I-91):
    // DragWindow moves the window by the release-point delta when the
    // mouse-up location is inside the global boundsRect.
    #[test]
    fn dragwindow_moves_window_to_release_delta_inside_boundsrect() {
        let (mut disp, mut cpu, mut bus) = setup();

        let window_addr: u32 = 0x300000;
        setup_window_with_regions(&mut bus, window_addr, 0, 0, 100, 200);
        bus.write_word(window_addr + 6, 0); // GrafPort
        bus.write_word(window_addr + 8, (-40i16) as u16);
        bus.write_word(window_addr + 10, (-20i16) as u16);
        bus.write_word(window_addr + 12, 560);
        bus.write_word(window_addr + 14, 780);
        bus.write_byte(window_addr + 110, 0xFF);
        disp.window_list = vec![window_addr];
        disp.front_window = window_addr;
        disp.window_bounds = (40, 20, 140, 220);

        let bounds_rect_ptr = 0x320000;
        bus.write_word(bounds_rect_ptr + 0, 0);
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 400);
        bus.write_word(bounds_rect_ptr + 6, 600);

        disp.push_mouse_down(50, 30);
        disp.set_mouse_position(70, 80);

        let sp = TEST_SP - 12;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, bounds_rect_ptr); // boundsRect pointer
        bus.write_long(sp + 4, 0x0032_001E); // startPt v=50, h=30
        bus.write_long(sp + 8, window_addr); // theWindow

        let result = dispatch(&mut disp, 0x125, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(
            bus.read_word(window_addr + 8) as i16,
            -60,
            "portBits.bounds.top tracks moved global origin"
        );
        assert_eq!(
            bus.read_word(window_addr + 10) as i16,
            -70,
            "portBits.bounds.left tracks moved global origin"
        );
        assert_eq!(
            disp.window_bounds,
            (60, 70, 160, 270),
            "front-window hit-test bounds move with DragWindow"
        );
    }

    // IM:I p.I-296 (with Rect-by-pointer calling convention on p.I-91):
    // DragWindow consumes WindowPtr + Point + RectPtr and leaves the
    // window in place when the release point is outside boundsRect.
    #[test]
    fn dragwindow_release_outside_boundsrect_leaves_window_unchanged() {
        let (mut disp, mut cpu, mut bus) = setup();

        let window_addr: u32 = 0x300000;
        for i in 0..32u32 {
            bus.write_byte(window_addr + i, (i as u8).wrapping_mul(7));
        }
        let before: Vec<u8> = (0..32u32).map(|i| bus.read_byte(window_addr + i)).collect();

        let bounds_rect_ptr = 0x320000;
        bus.write_word(bounds_rect_ptr + 0, 0);
        bus.write_word(bounds_rect_ptr + 2, 0);
        bus.write_word(bounds_rect_ptr + 4, 400);
        bus.write_word(bounds_rect_ptr + 6, 500);

        disp.set_mouse_position(600, 700);

        let sp = TEST_SP - 12;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, bounds_rect_ptr); // boundsRect pointer
        bus.write_long(sp + 4, 0x0010_0020); // startPt (global Point)
        bus.write_long(sp + 8, window_addr); // theWindow

        let result = dispatch(&mut disp, 0x125, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        let after: Vec<u8> = (0..32u32).map(|i| bus.read_byte(window_addr + i)).collect();
        assert_eq!(after, before);
    }

    #[test]
    fn hidden_menu_mode_draws_document_window_variant_frames() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        for proc_id in [0, 4, 8, 12, 16] {
            for offset in 0..800 * 600 {
                bus.write_byte(screen_base + offset, 0xAA);
            }
            disp.menu_bar_hidden = true;
            disp.front_window = 0;
            disp.window_proc_id = 0;
            disp.window_list.clear();
            disp.window_saved_under_pixels.clear();

            let window_addr = bus.alloc(256);
            disp.init_cgraf_window(
                &mut bus,
                &mut cpu,
                window_addr,
                screen_base,
                180,
                400,
                420,
                600,
                "Player",
                proc_id,
                true,
                true,
                true,
                0,
            );

            assert_ne!(
                bus.read_byte(screen_base + 240 * 800 + 450),
                0xAA,
                "document window procID {proc_id} should erase its content even when the host menu bar is hidden"
            );
            assert_ne!(
                bus.read_byte(screen_base + 162 * 800 + 450),
                0xAA,
                "document window procID {proc_id} should still draw title-bar chrome"
            );
        }
    }

    #[test]
    fn port_changed_resync_follows_a_port_rect_the_application_rewrote_itself() {
        // HyperCard resizes its card window by writing portRect directly and
        // calling PortChanged ($AB1D selector 9) rather than SizeWindow, so the
        // regions QuickDraw and the Window Manager clip against have to be
        // re-derived from the port. Myst Preview's card is 544x332 inside a
        // window NewWindow was told was 512x342.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let window_addr = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            screen_base,
            134,
            128,
            476,
            640,
            "Card",
            4,
            true,
            false,
            false,
            0,
        );
        assert_eq!(bus.read_word(window_addr + 20) as i16, 342);
        assert_eq!(bus.read_word(window_addr + 22) as i16, 512);

        // The application rewrites portRect behind our back, then announces it.
        bus.write_word(window_addr + 20, 332);
        bus.write_word(window_addr + 22, 544);
        disp.resync_window_geometry_from_port_rect(&mut bus, window_addr);

        let vis_rgn = bus.read_long(bus.read_long(window_addr + 24));
        assert_eq!(bus.read_word(vis_rgn + 6) as i16, 332, "visRgn.bottom");
        assert_eq!(bus.read_word(vis_rgn + 8) as i16, 544, "visRgn.right");

        let cont_rect = super::super::TrapDispatcher::region_handle_rect(
            &bus,
            bus.read_long(window_addr + super::super::TrapDispatcher::WINDOW_CONT_RGN_OFFSET),
        )
        .expect("content region");
        assert_eq!(
            (cont_rect.2 - cont_rect.0, cont_rect.3 - cont_rect.1),
            (332, 544),
            "content region should take the port's size"
        );
        assert_eq!(
            disp.window_bounds, cont_rect,
            "cached front-window bounds should track the content region"
        );

        // Idempotent: a second announcement with nothing changed is a no-op.
        let before = disp.window_bounds;
        disp.resync_window_geometry_from_port_rect(&mut bus, window_addr);
        assert_eq!(disp.window_bounds, before);
    }

    #[test]
    fn windows_created_wholly_off_screen_get_no_synthesised_chrome() {
        // An application that parks a window off-screen intends to drive its
        // content itself; real hardware draws that window's frame where it was
        // asked to, out of sight. HyperCard does this with its card window (its
        // NewWindow rect is at 16513,16528) and then blits the card straight
        // into the screen bitmap.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let parked = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            parked,
            screen_base,
            16513,
            16528,
            16855,
            17040,
            "Card",
            4,
            true,
            false,
            false,
            0,
        );
        assert!(
            disp.windows_placed_offscreen.contains(&parked),
            "a window created entirely off-screen should be recorded as parked"
        );

        let on_screen = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            on_screen,
            screen_base,
            60,
            40,
            300,
            400,
            "Normal",
            4,
            true,
            false,
            false,
            0,
        );
        assert!(!disp.windows_placed_offscreen.contains(&on_screen));

        disp.untrack_window(&mut bus, parked);
        assert!(!disp.windows_placed_offscreen.contains(&parked));
    }

    #[test]
    fn port_changed_resync_ignores_ports_that_are_not_tracked_windows() {
        let (mut disp, _cpu, mut bus) = setup();
        let port = bus.alloc(256);
        bus.write_word(port + 20, 332);
        bus.write_word(port + 22, 544);
        let before = disp.window_bounds;
        disp.resync_window_geometry_from_port_rect(&mut bus, port);
        assert_eq!(disp.window_bounds, before);
    }

    fn write_drag_region_frame(
        bus: &mut crate::memory::MacMemoryBus,
        sp: u32,
        start: (i16, i16),
        limit_rect_ptr: u32,
        slop_rect_ptr: u32,
        axis: i16,
    ) {
        bus.write_long(sp, 0); // actionProc
        bus.write_word(sp + 4, axis as u16);
        bus.write_long(sp + 6, slop_rect_ptr);
        bus.write_long(sp + 10, limit_rect_ptr);
        bus.write_word(sp + 14, start.0 as u16);
        bus.write_word(sp + 16, start.1 as u16);
        bus.write_long(sp + 18, 0x300000); // theRgn
        bus.write_long(sp + 22, 0xDEAD_BEEF);
    }

    fn write_test_rect(
        bus: &mut crate::memory::MacMemoryBus,
        ptr: u32,
        rect: (i16, i16, i16, i16),
    ) {
        bus.write_word(ptr, rect.0 as u16);
        bus.write_word(ptr + 2, rect.1 as u16);
        bus.write_word(ptr + 4, rect.2 as u16);
        bus.write_word(ptr + 6, rect.3 as u16);
    }

    // IM:I I-302 + IM:I I-91: DragTheRgn is the custom-outline alias of
    // DragGrayRgn, and the outside-slop path returns $80008000.
    #[test]
    fn dragthergn_returns_no_drag_sentinel_outside_sloprect_and_consumes_arguments() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 22;
        cpu.write_reg(Register::A7, sp);
        let limit_rect = 0x240000;
        let slop_rect = 0x240008;
        write_test_rect(&mut bus, limit_rect, (0, 0, 100, 100));
        write_test_rect(&mut bus, slop_rect, (0, 0, 120, 120));
        write_drag_region_frame(&mut bus, sp, (10, 20), limit_rect, slop_rect, 0);
        disp.set_mouse_position(140, 20);

        let result = dispatch(&mut disp, 0x126, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_long(TEST_SP), 0x8000_8000);
    }

    #[test]
    fn dragthergn_returns_current_local_mouse_offset_inside_sloprect() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 22;
        cpu.write_reg(Register::A7, sp);

        let port = 0x210000;
        disp.current_port = port;
        bus.write_word(port + 8, (-100i16) as u16);
        bus.write_word(port + 10, (-200i16) as u16);

        let limit_rect = 0x240000;
        let slop_rect = 0x240008;
        write_test_rect(&mut bus, limit_rect, (0, 0, 100, 100));
        write_test_rect(&mut bus, slop_rect, (0, 0, 120, 120));
        write_drag_region_frame(&mut bus, sp, (10, 20), limit_rect, slop_rect, 0);
        disp.set_mouse_position(130, 250); // local (30, 50)

        let result = dispatch(&mut disp, 0x126, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_long(TEST_SP), 0x0014_001E);
    }

    #[test]
    fn dragthergn_pins_to_limitrect_and_honors_axis_constraint() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 22;
        cpu.write_reg(Register::A7, sp);

        let limit_rect = 0x240000;
        let slop_rect = 0x240008;
        write_test_rect(&mut bus, limit_rect, (0, 0, 30, 70));
        write_test_rect(&mut bus, slop_rect, (0, 0, 100, 100));
        write_drag_region_frame(&mut bus, sp, (10, 10), limit_rect, slop_rect, 1);
        disp.set_mouse_position(40, 90);

        let result = dispatch(&mut disp, 0x126, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(
            bus.read_long(TEST_SP),
            0x0000_003B,
            "hAxisOnly keeps vertical offset zero and clamps h to right-1"
        );
    }

    // IM:I p.I-294 (signature/call-frame summary on p.I-91):
    // TrackGoAway returns TRUE only when mouse-up lands inside the go-away
    // box; no-tracking path returns FALSE and still consumes its arguments.
    #[test]
    fn trackgoaway_returns_false_and_consumes_window_and_point_arguments() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x0012_0034); // thePt
        bus.write_long(sp + 4, 0x300000); // theWindow
        bus.write_word(sp + 8, 0xFFFF); // result sentinel

        let result = dispatch(&mut disp, 0x11E, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_word(TEST_SP), 0);
    }

    // ---------------------------------------------------------------
    // 23. GrowWindow (0x12B) -- pops to SP+12, writes 0 at SP+12
    // ---------------------------------------------------------------
    #[test]
    fn growwindow_returns_zero_when_size_is_unchanged() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 12;
        cpu.write_reg(Register::A7, sp);
        for i in 0..12u32 {
            bus.write_byte(sp + i, 0);
        }
        // Write non-zero at result position
        bus.write_long(sp + 12, 0xFFFFFFFF);

        let result = dispatch(&mut disp, 0x12B, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        let grow_result = bus.read_long(TEST_SP);
        assert_eq!(grow_result, 0, "GrowWindow should return 0");
    }

    // IM:IV IV-50: TrackBox returns FALSE when mouse-up is outside the
    // zoom box.
    #[test]
    fn trackbox_returns_false_when_zoom_tracking_does_not_end_inside_zoom_box() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0xFFFF); // partCode (inZoomOut)
        bus.write_long(sp + 2, 0x0012_0034); // thePt (global Point)
        bus.write_long(sp + 6, 0x200000); // theWindow
        bus.write_word(sp + 10, 0xFFFF); // function result placeholder

        let result = dispatch(&mut disp, 0x03B, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(
            bus.read_word(TEST_SP),
            0,
            "TrackBox should return FALSE in the result slot"
        );
    }

    // IM:IV IV-50 + IM:I I-90..I-91: TrackBox is stack-based and returns a
    // BOOLEAN in the function-result slot after consuming its arguments.
    #[test]
    fn trackbox_consumes_windowptr_point_partcode_and_returns_boolean_on_stack() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 7); // partCode (inZoomIn)
        bus.write_long(sp + 2, 0x0008_0010); // thePt
        bus.write_long(sp + 6, 0x210000); // theWindow
        bus.write_word(sp + 10, 0x1234);

        let result = dispatch(&mut disp, 0x03B, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP,
            "TrackBox should pop 10 bytes of arguments"
        );
        assert_eq!(
            bus.read_word(TEST_SP),
            0,
            "TrackBox should write BOOLEAN FALSE to the result slot"
        );
    }

    // IM:IV IV-50: partCode inZoomIn (7) chooses userState from WStateData.
    #[test]
    fn zoomwindow_inzoomin_uses_userstate_rect_from_wstatedata() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window = 0x300000u32;
        let front_window = 0x300100u32;
        setup_full_window_with_regions(&mut bus, window, 0, 0, 20, 20);
        install_wstate_data(&mut bus, window, (30, 40, 130, 190), (10, 12, 50, 70));
        bus.write_byte(window + 110, 0xFF);
        bus.write_byte(front_window + 110, 0xFF);
        disp.window_list = vec![front_window, window];
        disp.front_window = front_window;

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_byte(sp, 0); // front = FALSE
        bus.write_word(sp + 2, 7); // inZoomIn
        bus.write_long(sp + 4, window);

        let result = dispatch(&mut disp, 0x03A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        assert_eq!(bus.read_word(window + 16) as i16, 0, "portRect.top");
        assert_eq!(bus.read_word(window + 18) as i16, 0, "portRect.left");
        assert_eq!(bus.read_word(window + 20) as i16, 100, "portRect.bottom");
        assert_eq!(bus.read_word(window + 22) as i16, 150, "portRect.right");

        let cont_ptr = bus.read_long(bus.read_long(window + 118));
        assert_eq!(bus.read_word(cont_ptr + 2) as i16, 30, "contRgn.top");
        assert_eq!(bus.read_word(cont_ptr + 4) as i16, 40, "contRgn.left");
        assert_eq!(bus.read_word(cont_ptr + 6) as i16, 130, "contRgn.bottom");
        assert_eq!(bus.read_word(cont_ptr + 8) as i16, 190, "contRgn.right");
        assert_eq!(
            disp.front_window, front_window,
            "front=FALSE must preserve current front window"
        );
    }

    // IM:IV IV-50: partCode inZoomOut (8) chooses stdState from WStateData.
    #[test]
    fn zoomwindow_inzoomout_uses_stdstate_rect_from_wstatedata() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window = 0x300000u32;
        setup_full_window_with_regions(&mut bus, window, 0, 0, 20, 20);
        install_wstate_data(&mut bus, window, (12, 18, 52, 90), (50, 60, 170, 250));
        bus.write_byte(window + 110, 0xFF);
        disp.window_list = vec![window];
        disp.front_window = window;

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_byte(sp, 0); // front = FALSE
        bus.write_word(sp + 2, 8); // inZoomOut
        bus.write_long(sp + 4, window);

        let result = dispatch(&mut disp, 0x03A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        assert_eq!(bus.read_word(window + 16) as i16, 0, "portRect.top");
        assert_eq!(bus.read_word(window + 18) as i16, 0, "portRect.left");
        assert_eq!(bus.read_word(window + 20) as i16, 120, "portRect.bottom");
        assert_eq!(bus.read_word(window + 22) as i16, 190, "portRect.right");

        let cont_ptr = bus.read_long(bus.read_long(window + 118));
        assert_eq!(bus.read_word(cont_ptr + 2) as i16, 50, "contRgn.top");
        assert_eq!(bus.read_word(cont_ptr + 4) as i16, 60, "contRgn.left");
        assert_eq!(bus.read_word(cont_ptr + 6) as i16, 170, "contRgn.bottom");
        assert_eq!(bus.read_word(cont_ptr + 8) as i16, 250, "contRgn.right");
    }

    // IM:IV IV-50: front=TRUE brings the zoomed window to the front.
    #[test]
    fn zoomwindow_front_true_brings_window_to_front() {
        let (mut disp, mut cpu, mut bus) = setup();
        let old_front = 0x200040u32;
        let zoom_target = 0x200140u32;
        setup_full_window_with_regions(&mut bus, zoom_target, 0, 0, 20, 20);
        install_wstate_data(
            &mut bus,
            zoom_target,
            (20, 20, 120, 180),
            (40, 50, 140, 210),
        );
        bus.write_byte(old_front + 110, 0xFF);
        bus.write_byte(zoom_target + 110, 0xFF);
        bus.write_byte(old_front + 111, 0xFF);
        bus.write_byte(zoom_target + 111, 0x00);

        disp.window_list = vec![old_front, zoom_target];
        disp.front_window = old_front;

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_byte(sp, 1); // front = TRUE
        bus.write_word(sp + 2, 8); // inZoomOut
        bus.write_long(sp + 4, zoom_target);

        let result = dispatch(&mut disp, 0x03A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(
            disp.front_window, zoom_target,
            "front=TRUE must promote target"
        );
        assert!(
            disp.event_queue
                .iter()
                .any(|e| e.what == 8 && e.message == old_front && (e.modifiers & 1) == 0),
            "old front must receive deactivate event"
        );
        assert!(
            disp.event_queue
                .iter()
                .any(|e| e.what == 8 && e.message == zoom_target && (e.modifiers & 1) == 1),
            "new front must receive activate event"
        );
    }

    // ---------------------------------------------------------------
    // 24. InvalRect (0x128) -- pops 4 bytes
    // ---------------------------------------------------------------
    #[test]
    fn test_inval_rect() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x300000); // rect ptr

        let result = dispatch(&mut disp, 0x128, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    // ---------------------------------------------------------------
    // 25. ValidRect (0x12A) -- pops 4 bytes
    // ---------------------------------------------------------------
    #[test]
    fn test_valid_rect() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x300000); // rect ptr

        let result = dispatch(&mut disp, 0x12A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    // ---------------------------------------------------------------
    // PaintBehind (0x10D) — inval-rects the clobbered region on every
    // window at/behind startWindow.
    // ---------------------------------------------------------------

    #[test]
    fn paintone_consumes_window_and_clobberedrgn_arguments() {
        // PaintOne takes WindowPeek plus RgnHandle arguments.
        // Inside Macintosh Volume I (1985), p. I-296;
        // Macintosh Toolbox Essentials (1992), p. 4-118.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, 0);

        let result = dispatch(&mut disp, 0x10C, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn paintone_nil_clobberedrgn_invalidates_window_portrect() {
        // With NIL clobberedRgn, PaintOne repaints the whole window.
        // Inside Macintosh Volume I (1985), p. I-296;
        // Macintosh Toolbox Essentials (1992), p. 4-118.
        let (mut disp, mut cpu, mut bus) = setup();
        let win = 0x200040u32;
        let (_cont_rgn, update_rgn) =
            setup_full_window_with_regions(&mut bus, win, 10, 20, 50, 100);
        disp.window_list = vec![win];

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0); // NIL clobberedRgn
        bus.write_long(sp + 4, win); // window

        let result = dispatch(&mut disp, 0x10C, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_word(update_rgn + 2) as i16, 10, "updateRgn.top");
        assert_eq!(bus.read_word(update_rgn + 4) as i16, 20, "updateRgn.left");
        assert_eq!(bus.read_word(update_rgn + 6) as i16, 50, "updateRgn.bottom");
        assert_eq!(bus.read_word(update_rgn + 8) as i16, 100, "updateRgn.right");
        assert!(
            disp.event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == win),
            "PaintOne should queue an update event for the target window"
        );
    }

    #[test]
    fn paintone_erases_exposed_content_before_queuing_update() {
        // PaintOne erases exposed content with the background pattern and
        // adds it to the update region.
        // Inside Macintosh Volume I (1985), p. I-296.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(crate::memory::globals::addr::SCREEN_BITS, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let win = bus.alloc(256);
        let (_cont_rgn, update_rgn) =
            setup_full_window_with_regions(&mut bus, win, 10, 20, 50, 100);
        bus.write_byte(win + 110u32, 0xFF);
        disp.window_list = vec![win];
        disp.front_window = win;

        let probe = screen_base + 30 * 800 + 50;
        bus.write_byte(probe, 0x7B);

        let clobbered_ptr = bus.alloc(10);
        let clobbered_handle = bus.alloc(4);
        bus.write_long(clobbered_handle, clobbered_ptr);
        bus.write_word(clobbered_ptr, 10);
        bus.write_word(clobbered_ptr + 2, 15);
        bus.write_word(clobbered_ptr + 4, 25);
        bus.write_word(clobbered_ptr + 6, 30);
        bus.write_word(clobbered_ptr + 8, 60);

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, clobbered_handle);
        bus.write_long(sp + 4, win);

        let result = dispatch(&mut disp, 0x10C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_ne!(
            bus.read_byte(probe),
            0x7B,
            "PaintOne must erase exposed content before the application redraws it"
        );
        assert_eq!(bus.read_word(update_rgn + 2) as i16, 15, "updateRgn.top");
        assert_eq!(bus.read_word(update_rgn + 4) as i16, 25, "updateRgn.left");
        assert_eq!(bus.read_word(update_rgn + 6) as i16, 30, "updateRgn.bottom");
        assert_eq!(bus.read_word(update_rgn + 8) as i16, 60, "updateRgn.right");
    }

    #[test]
    fn paintone_empty_clobberedrgn_invalidates_window_portrect() {
        // An empty clobberedRgn currently falls back to the window's
        // portRect path, so CheckUpdate returns TRUE and writes the
        // target window into the event record.
        // Inside Macintosh Volume I (1985), p. I-296;
        // Macintosh Toolbox Essentials (1992), p. 4-118.
        let (mut disp, mut cpu, mut bus) = setup();
        let win = 0x200040u32;
        let (_cont_rgn, _update_rgn) =
            setup_full_window_with_regions(&mut bus, win, 10, 20, 50, 100);
        disp.window_list = vec![win];

        let empty_rgn = bus.alloc(10);
        let empty_handle = bus.alloc(4);
        bus.write_word(empty_rgn, 10);
        bus.write_word(empty_rgn + 2, 0);
        bus.write_word(empty_rgn + 4, 0);
        bus.write_word(empty_rgn + 6, 0);
        bus.write_word(empty_rgn + 8, 0);
        bus.write_long(empty_handle, empty_rgn);

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, empty_handle);
        bus.write_long(sp + 4, win);

        let result = dispatch(&mut disp, 0x10C, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        let event_ptr = bus.alloc(16);
        for i in 0..16 {
            bus.write_byte(event_ptr + i, 0xCC);
        }
        let sp2 = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp2);
        bus.write_long(sp2, event_ptr);
        bus.write_word(sp2 + 4, 0xFFFF);
        let check = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
        assert!(check.is_some());
        assert!(check.unwrap().is_ok());
        assert_eq!(bus.read_word(TEST_SP - 2), 0xFFFF, "result should be TRUE");
        assert_eq!(
            bus.read_word(event_ptr),
            6,
            "event.what should be updateEvt"
        );
        assert_eq!(
            bus.read_long(event_ptr + 2),
            win,
            "event.message should carry WindowPtr"
        );
        assert!(
            disp.event_queue.is_empty(),
            "CheckUpdate should dequeue the consumed update event"
        );
    }

    #[test]
    fn paint_one_zero_window_is_noop() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, 0); // NIL theWindow
        let result = dispatch(&mut disp, 0x10C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn paintbehind_consumes_startwindow_and_clobberedrgn_arguments() {
        // Inside Macintosh Volume I (1985), p. I-293:
        // PaintBehind(startWindow, clobberedRgn) consumes two pointer args.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0); // NIL clobbered region
        bus.write_long(sp + 4, 0); // NIL startWindow
        let result = dispatch(&mut disp, 0x10D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn paintbehind_updates_startwindow_and_skips_invisible_windows_behind_it() {
        // Inside Macintosh Volume I (1985), p. I-293; Macintosh Toolbox
        // Essentials (1992), p. 4-118: PaintBehind repaints startWindow and
        // visible windows behind it.
        let (mut disp, mut cpu, mut bus) = setup();
        let front = bus.alloc(256);
        let middle = bus.alloc(256);
        let back = bus.alloc(256);

        fn setup_paintbehind_window(
            bus: &mut crate::memory::MacMemoryBus,
            window: u32,
            rect: (i16, i16, i16, i16),
        ) {
            bus.write_word(window + 16, rect.0 as u16);
            bus.write_word(window + 18, rect.1 as u16);
            bus.write_word(window + 20, rect.2 as u16);
            bus.write_word(window + 22, rect.3 as u16);
            let cont_rgn = super::super::TrapDispatcher::alloc_rect_region_handle(bus, Some(rect));
            let update_rgn = super::super::TrapDispatcher::alloc_rect_region_handle(bus, None);
            bus.write_long(window + 118, cont_rgn);
            bus.write_long(window + 122, update_rgn);
            bus.write_byte(window + 110, 0xFF);
        }

        for window in [front, middle, back] {
            setup_paintbehind_window(&mut bus, window, (0, 0, 200, 200));
        }
        bus.write_byte(back + 110u32, 0x00);
        disp.window_list = vec![front, middle, back];
        disp.front_window = front;

        let clobbered_ptr = bus.alloc(10);
        let clobbered_handle = bus.alloc(4);
        bus.write_long(clobbered_handle, clobbered_ptr);
        bus.write_word(clobbered_ptr, 10);
        bus.write_word(clobbered_ptr + 2, 50);
        bus.write_word(clobbered_ptr + 4, 60);
        bus.write_word(clobbered_ptr + 6, 120);
        bus.write_word(clobbered_ptr + 8, 130);

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, clobbered_handle);
        bus.write_long(sp + 4, middle);

        let result = dispatch(&mut disp, 0x10D, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        assert_eq!(
            super::super::TrapDispatcher::region_handle_rect(&bus, bus.read_long(front + 122)),
            None,
            "front window should remain untouched"
        );
        assert_eq!(
            super::super::TrapDispatcher::region_handle_rect(&bus, bus.read_long(middle + 122)),
            Some((50, 60, 120, 130)),
            "startWindow should be invalidated"
        );
        assert_eq!(
            super::super::TrapDispatcher::region_handle_rect(&bus, bus.read_long(back + 122)),
            None,
            "hidden windows behind startWindow should be skipped"
        );
    }

    #[test]
    fn paintbehind_converts_global_clobbered_rect_to_back_window_local_update() {
        // PaintBehind receives a Window Manager clobbered region in global
        // desktop coordinates. Back-window update regions are local to that
        // window's port; forwarding the global bbox directly over-invalidates
        // document windows whose origin is not (0,0).
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
        let screen_base = bus.alloc(800 * 600);
        bus.write_long(0x0824, screen_base);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

        let main_window = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            main_window,
            screen_base,
            99,
            86,
            538,
            713,
            "Main",
            4,
            true,
            false,
            false,
            0,
        );
        disp.validate_window_rect(&mut bus, main_window, (0, 0, 439, 627));

        let overlay_window = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            overlay_window,
            screen_base,
            271,
            283,
            348,
            517,
            "Overlay",
            1,
            true,
            false,
            true,
            0,
        );
        disp.validate_window_rect(&mut bus, overlay_window, (0, 0, 77, 234));
        assert_eq!(disp.window_list, vec![overlay_window, main_window]);

        let clobbered_handle = super::super::TrapDispatcher::alloc_rect_region_handle(
            &mut bus,
            Some((271, 283, 348, 517)),
        );
        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, clobbered_handle);
        bus.write_long(sp + 4, overlay_window);

        let result = dispatch(&mut disp, 0x10D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            super::super::TrapDispatcher::region_handle_rect(
                &bus,
                bus.read_long(main_window + 122),
            ),
            Some((271, 283, 348, 517)),
            "PaintBehind must store the overlay's invalidated bounds in global updateRgn coordinates"
        );
    }

    #[test]
    fn paint_behind_nil_start_walks_whole_list() {
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        disp.window_list = vec![win_b, win_a];
        // Minimum: a 10-byte rect region at 0x300020.
        let rgn_ptr = 0x300020u32;
        bus.write_word(rgn_ptr, 10);
        bus.write_word(rgn_ptr + 2, 5);
        bus.write_word(rgn_ptr + 4, 5);
        bus.write_word(rgn_ptr + 6, 30);
        bus.write_word(rgn_ptr + 8, 40);
        let rgn_handle = 0x300000u32;
        bus.write_long(rgn_handle, rgn_ptr);

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, rgn_handle);
        bus.write_long(sp + 4, 0); // NIL startWindow
        let result = dispatch(&mut disp, 0x10D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn hilitewindow_consumes_windowptr_and_boolean_arguments() {
        // HiliteWindow takes one WindowPtr and one Boolean argument.
        // Inside Macintosh Volume I (1985), p. I-286;
        // Macintosh Toolbox Essentials (1992), p. 4-90.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_byte(sp, 1);
        bus.write_byte(sp + 1, 0x7F);
        bus.write_long(sp + 2, 0x200040);

        let result = dispatch(&mut disp, 0x11C, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn hilitewindow_sets_or_clears_hilited_state_byte() {
        // HiliteWindow sets or clears a window's highlighted state.
        // Inside Macintosh Volume I (1985), p. I-286;
        // Macintosh Toolbox Essentials (1992), p. 4-90.
        let (mut disp, mut cpu, mut bus) = setup();
        let window = 0x200040u32;
        bus.write_byte(window + 110, 0x00); // hidden => skip chrome drawing
        bus.write_byte(window + 111, 0x11);

        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_byte(sp, 1);
        bus.write_long(sp + 2, window);
        let result = dispatch(&mut disp, 0x11C, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_byte(window + 111),
            0xFF,
            "window should be highlighted"
        );

        let sp2 = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp2);
        bus.write_byte(sp2, 0);
        bus.write_long(sp2 + 2, window);
        let result2 = dispatch(&mut disp, 0x11C, &mut cpu, &mut bus);
        assert!(result2.is_some());
        assert!(result2.unwrap().is_ok());
        assert_eq!(
            bus.read_byte(window + 111),
            0x00,
            "window should be unhighlighted"
        );
    }

    #[test]
    fn bringtofront_consumes_windowptr_argument() {
        // BringToFront takes one WindowPtr argument.
        // Inside Macintosh Volume I (1985), p. I-286;
        // Macintosh Toolbox Essentials (1992), p. 4-90.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x200040);

        let result = dispatch(&mut disp, 0x120, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn bringtofront_moves_window_to_front_of_window_list() {
        // BringToFront moves the target window to the beginning of the window list.
        // Inside Macintosh Volume I (1985), p. I-286;
        // Macintosh Toolbox Essentials (1992), p. 4-90.
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        let win_c = 0x200240u32;
        disp.window_list = vec![win_a, win_b, win_c];
        disp.front_window = win_a;

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, win_c);

        let result = dispatch(&mut disp, 0x120, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.window_list, vec![win_c, win_a, win_b]);
        assert_eq!(disp.front_window, win_c);
    }

    #[test]
    fn setwindowpic_consumes_windowptr_and_pichandle_arguments() {
        // SetWindowPic takes WindowPtr and PicHandle pointer arguments.
        // Inside Macintosh Volume I (1985), p. I-293;
        // Macintosh Toolbox Essentials (1992), p. 4-110.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x200000);
        bus.write_long(sp + 4, 0x200040);

        let result = dispatch(&mut disp, 0x12E, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn setwindowpic_stores_pic_handle_in_window_record() {
        // SetWindowPic stores a picture handle for later window-content drawing.
        // Inside Macintosh Volume I (1985), p. I-293;
        // Macintosh Toolbox Essentials (1992), p. 4-110.
        let (mut disp, mut cpu, mut bus) = setup();
        let window = 0x200040u32;
        let pic = 0x300040u32;

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, pic);
        bus.write_long(sp + 4, window);

        let result = dispatch(&mut disp, 0x12E, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(window + 148), pic);
    }

    #[test]
    fn getwindowpic_consumes_windowptr_argument_and_writes_function_result_slot() {
        // GetWindowPic takes one WindowPtr argument and returns a PicHandle.
        // Inside Macintosh Volume I (1985), p. I-293;
        // Macintosh Toolbox Essentials (1992), p. 4-110.
        let (mut disp, mut cpu, mut bus) = setup();
        let window = 0x200040u32;
        let expected_pic = 0x300080u32;
        bus.write_long(window + 148, expected_pic);

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window);
        bus.write_long(sp + 4, 0xFFFF_FFFF);

        let result = dispatch(&mut disp, 0x12F, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_long(TEST_SP), expected_pic);
    }

    #[test]
    fn getwindowpic_returns_picture_handle_previously_set_by_setwindowpic() {
        // GetWindowPic returns the handle previously stored by SetWindowPic.
        // Inside Macintosh Volume I (1985), p. I-293;
        // Macintosh Toolbox Essentials (1992), p. 4-110.
        let (mut disp, mut cpu, mut bus) = setup();
        let window = 0x200040u32;
        let pic = 0x300000u32;

        let sp_set = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp_set);
        bus.write_long(sp_set, pic);
        bus.write_long(sp_set + 4, window);
        let set_result = dispatch(&mut disp, 0x12E, &mut cpu, &mut bus);
        assert!(set_result.is_some());
        assert!(set_result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        let sp_get = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp_get);
        bus.write_long(sp_get, window);
        let get_result = dispatch(&mut disp, 0x12F, &mut cpu, &mut bus);
        assert!(get_result.is_some());
        assert!(get_result.unwrap().is_ok());
        assert_eq!(bus.read_long(TEST_SP), pic);
    }

    // ---------------------------------------------------------------
    // SendBehind (0x121) — reorders the window_list and re-derives
    // front_window.
    // ---------------------------------------------------------------

    #[test]
    fn sendbehind_consumes_windowptr_pair_arguments() {
        // SendBehind takes theWindow and behindWindow pointer arguments.
        // Inside Macintosh Volume I (1985), p. I-286;
        // Macintosh Toolbox Essentials (1992), p. 4-91.
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        disp.window_list = vec![win_b, win_a];
        for base in [win_a, win_b] {
            bus.write_word(base + 16, 10);
            bus.write_word(base + 18, 10);
            bus.write_word(base + 20, 50);
            bus.write_word(base + 22, 100);
        }

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, win_a);
        bus.write_long(sp + 4, win_b);

        let result = dispatch(&mut disp, 0x121, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn sendbehind_nil_behindwindow_moves_window_to_back_and_rederives_front() {
        // SendBehind with behindWindow = NIL moves the window behind all others.
        // Inside Macintosh Volume I (1985), p. I-286;
        // Macintosh Toolbox Essentials (1992), p. 4-91.
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        disp.window_list = vec![win_b, win_a];
        disp.front_window = win_b;
        bus.write_byte(win_a + 110u32, 0xFF);
        bus.write_byte(win_b + 110u32, 0xFF);
        for base in [win_a, win_b] {
            bus.write_word(base + 16, 10);
            bus.write_word(base + 18, 10);
            bus.write_word(base + 20, 50);
            bus.write_word(base + 22, 100);
        }

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, win_b);

        let result = dispatch(&mut disp, 0x121, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.window_list, vec![win_a, win_b]);
        assert_eq!(disp.front_window, win_a);
    }

    #[test]
    fn send_behind_null_moves_window_to_back() {
        let (mut disp, mut cpu, mut bus) = setup();
        // Two fake windows already in the list, newest at front.
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        disp.window_list = vec![win_b, win_a];
        disp.front_window = win_b;
        // Minimum portRect to satisfy the bounds read.
        for base in [win_a, win_b] {
            bus.write_word(base + 16, 10);
            bus.write_word(base + 18, 10);
            bus.write_word(base + 20, 50);
            bus.write_word(base + 22, 100);
        }

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0); // behindWindow = NULL
        bus.write_long(sp + 4, win_b); // theWindow = B (currently front)

        let result = dispatch(&mut disp, 0x121, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        assert_eq!(
            disp.window_list,
            vec![win_a, win_b],
            "B must move to back of window_list"
        );
        assert_eq!(
            disp.front_window, win_a,
            "front_window must re-derive to the new head"
        );
    }

    #[test]
    fn send_behind_specific_window_inserts_just_after_target() {
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        let win_c = 0x200240u32;
        disp.window_list = vec![win_c, win_b, win_a];
        disp.front_window = win_c;
        for base in [win_a, win_b, win_c] {
            bus.write_word(base + 16, 10);
            bus.write_word(base + 18, 10);
            bus.write_word(base + 20, 50);
            bus.write_word(base + 22, 100);
        }

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, win_a); // behindWindow = A
        bus.write_long(sp + 4, win_c); // theWindow = C

        let result = dispatch(&mut disp, 0x121, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // C must now sit immediately behind A, with B still in front.
        assert_eq!(disp.window_list, vec![win_b, win_a, win_c]);
        assert_eq!(disp.front_window, win_b);
    }

    // SendBehind's front re-derivation must skip hidden windows when
    // picking the new front.
    #[test]
    fn send_behind_skips_hidden_candidate_when_promoting_front() {
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        let win_c = 0x200240u32;
        disp.window_list = vec![win_c, win_b, win_a]; // c front
        disp.front_window = win_c;
        for &base in &[win_a, win_b, win_c] {
            bus.write_word(base + 16, 10);
            bus.write_word(base + 18, 10);
            bus.write_word(base + 20, 50);
            bus.write_word(base + 22, 100);
        }
        // Only c and a are visible; b (middle) is hidden.
        bus.write_byte(win_a + 110u32, 0xFF);
        bus.write_byte(win_b + 110u32, 0x00);
        bus.write_byte(win_c + 110u32, 0xFF);

        let sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0); // behindWindow = NIL (move to back)
        bus.write_long(sp + 4, win_c); // theWindow = C

        let result = dispatch(&mut disp, 0x121, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        // List becomes [b, a, c]. b is hidden, so front must
        // promote to the first visible entry = a.
        assert_eq!(disp.window_list, vec![win_b, win_a, win_c]);
        assert_eq!(
            disp.front_window, win_a,
            "must skip hidden b and pick visible a"
        );
    }

    // ---------------------------------------------------------------
    // InvalRgn (0x127) — forwards the region's bbox into
    // invalidate_window_rect, mirroring InvalRect.
    // ---------------------------------------------------------------
    #[test]
    fn inval_rgn_adds_region_bbox_to_update_region() {
        let (mut disp, mut cpu, mut bus, window_ptr) = setup_region_window();
        let rgn_handle = make_region_handle(&mut bus, 0x300000, 0x300020, 10, (20, 10, 110, 70));
        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, rgn_handle);

        assert!(disp.window_update_rect(&bus, window_ptr).is_none());
        let result = dispatch(&mut disp, 0x127, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(
            disp.window_update_rect(&bus, window_ptr),
            Some((60, 10, 150, 70))
        );
    }

    #[test]
    fn inval_rgn_ignores_region_handles_with_short_size_headers() {
        let (mut disp, mut cpu, mut bus, window_ptr) = setup_region_window();
        let rgn_handle = make_region_handle(&mut bus, 0x300000, 0x300020, 8, (20, 10, 110, 70));
        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, rgn_handle);

        assert!(disp.window_update_rect(&bus, window_ptr).is_none());
        let result = dispatch(&mut disp, 0x129, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert!(disp.window_update_rect(&bus, window_ptr).is_none());
    }

    #[test]
    fn valid_rgn_clears_region_bbox_from_update_region() {
        let (mut disp, mut cpu, mut bus, window_ptr) = setup_region_window();
        let rgn_handle = make_region_handle(&mut bus, 0x300000, 0x300020, 10, (20, 10, 110, 70));
        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, rgn_handle);

        let inval = dispatch(&mut disp, 0x127, &mut cpu, &mut bus);
        assert!(inval.unwrap().is_ok());
        assert_eq!(
            disp.window_update_rect(&bus, window_ptr),
            Some((60, 10, 150, 70))
        );

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, rgn_handle);
        let result = dispatch(&mut disp, 0x129, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert!(disp.window_update_rect(&bus, window_ptr).is_none());
    }

    // ---------------------------------------------------------------
    // GetWVariant (0x00A) — returns low 4 bits of window's procID per
    // IM:V V-208 + IM:I I-282 (window definition ID = 16*resourceID +
    // variation_code). NewWindow / NewCWindow / GetNewWindow /
    // GetNewCWindow populate the window_proc_ids sidetable; GetWVariant
    // recovers procID from it and masks the low 4 bits.
    // ---------------------------------------------------------------
    #[test]
    fn getwvariant_returns_variation_code_from_low_four_bits_of_proc_id() {
        let (mut disp, mut cpu, mut bus) = setup();
        let cases: &[(i16, i16)] = &[
            (0, 0),  // documentProc → variant 0
            (1, 1),  // dBoxProc → variant 1
            (4, 4),  // noGrowDocProc → variant 4
            (5, 5),  // movableDBoxProc → variant 5
            (16, 0), // rDocProc (WDEF resID 1, variant 0)
        ];
        for &(proc_id, expected) in cases {
            let window_ptr = 0x0040_0000u32 + (proc_id as u32) * 0x100;
            disp.window_proc_ids.insert(window_ptr, proc_id);
            let sp = TEST_SP - 4;
            cpu.write_reg(Register::A7, sp);
            bus.write_long(sp, window_ptr);
            bus.write_word(sp + 4, 0xBEEF);
            let result = dispatch(&mut disp, 0x00A, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            assert_eq!(
                cpu.read_reg(Register::A7),
                TEST_SP,
                "GetWVariant must advance A7 by 4 (procID={})",
                proc_id
            );
            assert_eq!(
                bus.read_word(TEST_SP) as i16,
                expected,
                "GetWVariant must return low 4 bits of procID; got wrong value for procID={}",
                proc_id
            );
        }
    }

    #[test]
    fn getwvariant_returns_zero_for_nil_window_ptr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, 0xBEEF);
        let result = dispatch(&mut disp, 0x00A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(
            bus.read_word(TEST_SP),
            0,
            "GetWVariant on NIL WindowPtr must defensively return 0 (no crash)"
        );
    }

    #[test]
    fn getwvariant_function_protocol_pops_windowptr_and_writes_integer_result() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_ptr = 0x0042_0000u32;
        disp.window_proc_ids.insert(window_ptr, 8); // zoomDocProc → variant 8
        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_ptr);
        bus.write_word(sp + 4, 0xCAFE);
        bus.write_word(sp + 6, 0xBABE);
        let result = dispatch(&mut disp, 0x00A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP,
            "GetWVariant must advance A7 by exactly 4 (consume WindowPtr arg)"
        );
        assert_eq!(
            bus.read_word(TEST_SP) as i16,
            8,
            "GetWVariant must return the variant"
        );
        assert_eq!(
            bus.read_word(TEST_SP + 2),
            0xBABE,
            "GetWVariant must not write past the 2-byte INTEGER result slot"
        );
    }

    // ---------------------------------------------------------------
    // Unhandled trap returns None
    // ---------------------------------------------------------------
    #[test]
    fn test_unhandled_trap_returns_none() {
        let (mut disp, mut cpu, mut bus) = setup();
        let result = dispatch(&mut disp, 0xFFF, &mut cpu, &mut bus);
        assert!(result.is_none(), "Unhandled trap should return None");
    }

    // MoveWindow with front=TRUE must bring the window to the front and
    // emit the same hilite/activate side effects as SelectWindow (IM:I
    // I-287 says it's equivalent to SelectWindow).
    #[test]
    fn move_window_with_front_true_brings_to_front_and_activates() {
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        disp.window_list = vec![win_a, win_b];
        disp.front_window = win_a;
        bus.write_byte(win_a + 110u32, 0xFF); // visible
        bus.write_byte(win_b + 110u32, 0xFF);
        bus.write_byte(win_a + 111u32, 0xFF); // hilited (front)
        bus.write_byte(win_b + 111u32, 0x00);
        // Minimum CGrafPort / pixmap handle at +2 so MoveWindow's
        // `is_cgraf` path exits cleanly. Use GrafPort (portVersion
        // high bit not set) to avoid the pixmap deref.
        bus.write_word(win_b + 6, 0x0000);
        bus.write_word(win_b + 20, 50); // portRect.bottom
        bus.write_word(win_b + 22, 100); // portRect.right

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        // Pascal BOOLEAN in high byte of 2-byte slot (MPW C
        // convention).
        bus.write_byte(sp, 1); // front = TRUE
        bus.write_word(sp + 2, 60); // vGlobal
        bus.write_word(sp + 4, 40); // hGlobal
        bus.write_long(sp + 6, win_b);

        let queue_len_before = disp.event_queue.len();
        let result = dispatch(&mut disp, 0x11B, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.front_window, win_b, "B must become front");
        assert_eq!(bus.read_byte(win_a + 111u32), 0x00, "A unhilited");
        assert_eq!(bus.read_byte(win_b + 111u32), 0xFF, "B hilited");
        assert_eq!(
            disp.event_queue.len() - queue_len_before,
            2,
            "must queue deactivate A + activate B"
        );
    }

    #[test]
    fn move_window_with_front_false_preserves_z_order() {
        let (mut disp, mut cpu, mut bus) = setup();
        let win_a = 0x200040u32;
        let win_b = 0x200140u32;
        disp.window_list = vec![win_a, win_b];
        disp.front_window = win_a;
        bus.write_byte(win_a + 110u32, 0xFF);
        bus.write_byte(win_b + 110u32, 0xFF);
        bus.write_byte(win_a + 111u32, 0xFF);
        bus.write_byte(win_b + 111u32, 0x00);
        bus.write_word(win_b + 6, 0x0000);
        bus.write_word(win_b + 20, 50);
        bus.write_word(win_b + 22, 100);

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0); // front = FALSE
        bus.write_word(sp + 2, 60);
        bus.write_word(sp + 4, 40);
        bus.write_long(sp + 6, win_b);

        let queue_len_before = disp.event_queue.len();
        let result = dispatch(&mut disp, 0x11B, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            disp.front_window, win_a,
            "front must NOT change when front=FALSE"
        );
        assert_eq!(
            disp.event_queue.len(),
            queue_len_before,
            "no activate events when front=FALSE"
        );
    }

    // SizeWindow(fUpdate=TRUE) must InvalRect the newly-exposed area
    // when the window grows, per IM:I I-287.
    #[test]
    fn size_window_with_fupdate_true_invalidates_new_area() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr: u32 = 0x300000;
        let (_cont_rgn, update_rgn) =
            setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 100, 100);
        // Mark window visible so invalidate_window_rect's clip
        // intersection picks up the content rect.
        bus.write_byte(window_addr + 110u32, 0xFF);
        disp.window_list = vec![window_addr];
        disp.front_window = window_addr;

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        // Pascal BOOLEAN in high byte (MPW C convention).
        bus.write_byte(sp, 1); // fUpdate = TRUE
        bus.write_word(sp + 2, 200); // h = 200 (was 100)
        bus.write_word(sp + 4, 200); // w = 200 (was 100)
        bus.write_long(sp + 6, window_addr);

        let queue_len_before = disp.event_queue.len();
        let result = dispatch(&mut disp, 0x11D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // updateRgn bbox must match the new content rect.
        assert_eq!(bus.read_word(update_rgn + 2) as i16, 0, "updateRgn.top");
        assert_eq!(
            bus.read_word(update_rgn + 6) as i16,
            200,
            "updateRgn.bottom must match new h"
        );
        assert_eq!(
            bus.read_word(update_rgn + 8) as i16,
            200,
            "updateRgn.right must match new w"
        );
        // And an update event was queued.
        assert!(
            disp.event_queue.len() > queue_len_before,
            "fUpdate=TRUE must queue an update event"
        );
    }

    #[test]
    fn size_window_with_fupdate_false_skips_invalidation() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr: u32 = 0x300000;
        let (_cont_rgn, update_rgn) =
            setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 100, 100);
        bus.write_byte(window_addr + 110u32, 0xFF);
        disp.window_list = vec![window_addr];
        disp.front_window = window_addr;

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0); // fUpdate = FALSE
        bus.write_word(sp + 2, 200);
        bus.write_word(sp + 4, 200);
        bus.write_long(sp + 6, window_addr);

        let result = dispatch(&mut disp, 0x11D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // updateRgn must remain empty (the bbox we seeded as 0,0,0,0).
        assert_eq!(bus.read_word(update_rgn + 6) as i16, 0);
        assert_eq!(bus.read_word(update_rgn + 8) as i16, 0);
    }

    #[test]
    fn drawnew_consumes_windowpeek_and_update_arguments() {
        // DrawNew consumes a Boolean update flag and WindowPeek pointer.
        // Inside Macintosh Volume I (1985), p. I-296;
        // Macintosh Toolbox Essentials (1992), p. 4-117.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, 0);

        let result = dispatch(&mut disp, 0x10F, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    // DrawNew update behavior reference:
    // Inside Macintosh Volume I (1985), p. I-296;
    // Macintosh Toolbox Essentials (1992), p. 4-117.
    #[test]
    fn drawnew_update_true_consumes_saveold_and_drawnew_arguments() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr: u32 = 0x300000;
        let _ = setup_full_window_with_regions(&mut bus, window_addr, 10, 20, 60, 120);
        bus.write_byte(window_addr + 110u32, 0xFF);
        disp.window_list = vec![window_addr];
        disp.front_window = window_addr;

        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_addr);
        let save_old = dispatch(&mut disp, 0x10E, &mut cpu, &mut bus);
        assert!(save_old.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 2);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // fUpdate = TRUE
        bus.write_long(sp + 2, window_addr);

        let result = dispatch(&mut disp, 0x10F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn saveold_drawnew_true_uses_saved_regions_after_content_change() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr: u32 = 0x300000;
        let (cont_rgn, update_rgn) =
            setup_full_window_with_regions(&mut bus, window_addr, 10, 20, 60, 120);
        bus.write_byte(window_addr + 110u32, 0xFF);
        disp.window_list = vec![window_addr];
        disp.front_window = window_addr;

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_addr);
        let save_old = dispatch(&mut disp, 0x10E, &mut cpu, &mut bus);
        assert!(save_old.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        // Change the content region before DrawNew. If SaveOld snapshots the
        // old bounds, DrawNew(TRUE) should invalidate the union of the saved
        // and current rectangles instead of only the new one.
        bus.write_word(cont_rgn + 2, 30);
        bus.write_word(cont_rgn + 4, 40);
        bus.write_word(cont_rgn + 6, 80);
        bus.write_word(cont_rgn + 8, 140);
        bus.write_word(update_rgn + 2, 0);
        bus.write_word(update_rgn + 4, 0);
        bus.write_word(update_rgn + 6, 0);
        bus.write_word(update_rgn + 8, 0);

        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_byte(sp, 1);
        bus.write_byte(sp + 1, 0);
        bus.write_long(sp + 2, window_addr);
        let result = dispatch(&mut disp, 0x10F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_word(update_rgn + 2) as i16, 10, "updateRgn.top");
        assert_eq!(bus.read_word(update_rgn + 4) as i16, 20, "updateRgn.left");
        assert_eq!(bus.read_word(update_rgn + 6) as i16, 80, "updateRgn.bottom");
        assert_eq!(bus.read_word(update_rgn + 8) as i16, 140, "updateRgn.right");
    }

    #[test]
    fn drawnew_update_false_preserves_pending_update_and_checkupdate_returns_true() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr: u32 = 0x300000;
        let (_cont_rgn, update_rgn) =
            setup_full_window_with_regions(&mut bus, window_addr, 10, 20, 60, 120);
        bus.write_word(update_rgn + 2, 20);
        bus.write_word(update_rgn + 4, 30);
        bus.write_word(update_rgn + 6, 90);
        bus.write_word(update_rgn + 8, 140);
        bus.write_byte(window_addr + 110u32, 0xFF);
        disp.window_list = vec![window_addr];
        disp.front_window = window_addr;

        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_addr);
        let save_old = dispatch(&mut disp, 0x10E, &mut cpu, &mut bus);
        assert!(save_old.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 2);

        cpu.write_reg(Register::A7, sp);
        bus.write_byte(sp, 0);
        bus.write_byte(sp + 1, 0x7F); // garbage low byte must be ignored
        bus.write_long(sp + 2, window_addr);

        let result = dispatch(&mut disp, 0x10F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        let event_ptr = bus.alloc(16);
        for i in 0..16 {
            bus.write_byte(event_ptr + i, 0xCC);
        }
        let check_sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, check_sp);
        bus.write_long(check_sp, event_ptr);
        bus.write_word(check_sp + 4, 0xFFFF);

        let check_result = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
        assert!(check_result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 2);
        assert_eq!(bus.read_word(TEST_SP - 2), 0xFFFF, "result should be TRUE");
        assert_eq!(
            bus.read_word(event_ptr),
            6,
            "event.what should be updateEvt"
        );
        assert_eq!(
            bus.read_long(event_ptr + 2),
            window_addr,
            "event.message should carry WindowPtr"
        );
    }

    #[test]
    fn draw_new_nil_window_is_safe() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 6;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1);
        bus.write_long(sp + 2, 0); // NIL window
        let result = dispatch(&mut disp, 0x10F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn size_window_shrinking_does_not_invalidate_even_with_fupdate_true() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_addr: u32 = 0x300000;
        let (_cont_rgn, update_rgn) =
            setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 200, 200);
        bus.write_byte(window_addr + 110u32, 0xFF);
        disp.window_list = vec![window_addr];
        disp.front_window = window_addr;

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // fUpdate = TRUE
        bus.write_word(sp + 2, 100); // h = 100 (shrinking from 200)
        bus.write_word(sp + 4, 100); // w = 100 (shrinking from 200)
        bus.write_long(sp + 6, window_addr);

        let result = dispatch(&mut disp, 0x11D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // updateRgn stays empty — shrinking uncovers nothing.
        assert_eq!(bus.read_word(update_rgn + 6) as i16, 0);
        assert_eq!(bus.read_word(update_rgn + 8) as i16, 0);
    }
}
