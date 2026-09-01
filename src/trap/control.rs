//! Control Manager trap handlers.

use super::dispatch::{ControlAuxRecordState, ControlTrackingState};
use super::types::{decode_mac_roman_for_render, Rect, ShapeOp};
use crate::cpu::{CpuOps, Register};
use crate::memory::{globals::addr, MacMemoryBus, MemoryBus};
use crate::menu_manager::standard_popup_menu_layout;
use crate::quickdraw::text::get_font_metrics;
use crate::ui_theme::ControlKind;
use crate::Result;

use std::sync::OnceLock;
static TRACE_CONTROLS: OnceLock<bool> = OnceLock::new();
fn trace_controls_enabled() -> bool {
    *TRACE_CONTROLS.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_CONTROLS").is_some())
}

impl super::TrapDispatcher {
    const CDEF_DRAW_CNTL_MSG: i16 = 0;
    const CDEF_TEST_CNTL_MSG: i16 = 1;
    const CDEF_INIT_CNTL_MSG: i16 = 3;
    const CDEF_TRAMPOLINE_SIZE: u32 = 80;
    const LOWMEM_AUX_CTL_HEAD: u32 = 0x0CD4;
    const AUX_CTL_NEXT_OFFSET: u32 = 0;
    const AUX_CTL_OWNER_OFFSET: u32 = 4;
    const AUX_CTL_CTABLE_OFFSET: u32 = 8;
    const AUX_CTL_FLAGS_OFFSET: u32 = 12;
    const AUX_CTL_RESERVED_OFFSET: u32 = 14;
    const AUX_CTL_REFCON_OFFSET: u32 = 18;
    const AUX_CTL_RECORD_SIZE: u32 = 22;
    /// Resolve the standard CDEF's `grayishTextOr` title ink for an 8-bit
    /// color device. Color QuickDraw blends the foreground and background;
    /// the standard controls use black and white, producing 50% gray before
    /// the live device CLUT maps it to an indexed color.
    ///
    /// Prefer the darker entry when two palette colors are equidistant. A
    /// custom CLUT containing only neutral white/black endpoints must not map
    /// a non-empty inactive title to its white dialog background.
    /// Inside Macintosh: Text 1993, pp. 3-25 to 3-26.
    pub(crate) fn inactive_control_title_index(&self) -> u8 {
        const BLENDED_COMPONENT: i64 = 0x7FFF;
        self.device_clut
            .iter()
            .enumerate()
            .min_by_key(|(_, rgb)| {
                let red = BLENDED_COMPONENT - i64::from(rgb[0]);
                let green = BLENDED_COMPONENT - i64::from(rgb[1]);
                let blue = BLENDED_COMPONENT - i64::from(rgb[2]);
                (
                    red * red + green * green + blue * blue,
                    u32::from(rgb[0]) + u32::from(rgb[1]) + u32::from(rgb[2]),
                )
            })
            .map_or(255, |(index, _)| index as u8)
    }

    // The drawCntl contract defines invisibility as contrlVis == 0; callers
    // that write the packed ControlRecord directly may use another nonzero
    // Boolean representation instead of ShowControl's canonical 255.
    // Inside Macintosh Volume I, I-329
    fn control_vis_is_visible(vis: u8) -> bool {
        vis != 0
    }

    fn control_trace_nonzero(value: u32) -> String {
        if value == 0 {
            "$00000000".to_string()
        } else {
            "$NONZERO".to_string()
        }
    }

    fn control_trace_control_fields(&self, bus: &MacMemoryBus, ctrl_handle: u32) -> String {
        let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
        if ctrl_ptr == 0 {
            return "control_handle=$00000000 control_ptr=$00000000 rect=none visible=false hilite=none proc_id=none".to_string();
        }
        let top = bus.read_word(ctrl_ptr + 8) as i16;
        let left = bus.read_word(ctrl_ptr + 10) as i16;
        let bottom = bus.read_word(ctrl_ptr + 12) as i16;
        let right = bus.read_word(ctrl_ptr + 14) as i16;
        let vis = bus.read_byte(ctrl_ptr + 16);
        let hilite = bus.read_byte(ctrl_ptr + 17);
        let proc_id = self.control_proc_ids.get(&ctrl_ptr).copied().unwrap_or(0);
        format!(
            "control_handle={} control_ptr={} rect=({top},{left},{bottom},{right}) visible={} hilite={} proc_id={proc_id}",
            Self::control_trace_nonzero(ctrl_handle),
            Self::control_trace_nonzero(ctrl_ptr),
            Self::control_vis_is_visible(vis),
            hilite,
        )
    }

    fn record_trackcontrol_input_trace(
        &mut self,
        bus: &MacMemoryBus,
        action: &str,
        ctrl_handle: u32,
        start_pt: Option<(i16, i16)>,
        action_proc: u32,
        part: Option<u16>,
        highlighted_item: Option<i16>,
        outcome: &str,
    ) {
        if !self.input_trace_enabled {
            return;
        }
        let start = start_pt
            .map(|(v, h)| format!("({v},{h})"))
            .unwrap_or_else(|| "none".to_string());
        let part = part
            .map(|value| value.to_string())
            .unwrap_or_else(|| "pending".to_string());
        let highlighted = highlighted_item
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string());
        // IM:I I-323 / MTE 1992 5-89..5-90: TrackControl takes the
        // mouse-down point in the control window's local coordinates and
        // returns the release part code, or 0 when no control part remains hit.
        self.record_input_trace_line(format!(
            "A968 action={} start={} action_proc={} live_mouse=({},{}) {} {} part={} highlighted_item={} outcome={}",
            action,
            start,
            Self::control_trace_nonzero(action_proc),
            self.input_state.mouse_pos.0,
            self.input_state.mouse_pos.1,
            self.input_trace_state_fields(),
            self.control_trace_control_fields(bus, ctrl_handle),
            part,
            highlighted,
            outcome,
        ));
    }

    fn control_tracking_button_down(&self, bus: &MacMemoryBus) -> bool {
        // TrackControl follows the mouse until button release (IM:I I-323).
        // MBState ($0172) is the classic low-memory button mirror (0=down,
        // $80=up), so guest callbacks/VBL tasks can hold or release tracking
        // between trap re-fires. Inside Macintosh Volume II, p. II-371.
        self.input_state.mouse_button || bus.read_byte(addr::MB_STATE) == 0x00
    }

    fn control_tracking_mouse_pos(&self, bus: &MacMemoryBus) -> (i16, i16) {
        // Mouse ($0830) is the current low-memory mouse Point. Prefer it when
        // a guest callback updates classic globals directly during tracking;
        // otherwise keep dispatcher-driven tests and host input authoritative.
        // Inside Macintosh Volume II, p. II-371; Volume III, p. III-446.
        let v = bus.read_word(addr::MOUSE_LOC2) as i16;
        let h = bus.read_word(addr::MOUSE_LOC2 + 2) as i16;
        if v != 0 || h != 0 {
            (v, h)
        } else {
            self.input_state.mouse_pos
        }
    }

    fn control_record_ptr(bus: &MacMemoryBus, ctrl_handle: u32) -> u32 {
        if ctrl_handle == 0 {
            0
        } else {
            bus.read_long(ctrl_handle)
        }
    }

    /// Save portions of a control rectangle that are outside its owner's
    /// current visible region. Standard CDEFs draw through the owner's
    /// GrafPort and are clipped by `visRgn`; several HLE control renderers
    /// write screen pixels directly, so those pixels need the same clipping
    /// guarantee when a front window overlaps a background control.
    /// Inside Macintosh Volume I (1985), pp. I-145 and I-326;
    /// Macintosh Toolbox Essentials (1992), pp. 4-69 to 4-70.
    fn save_control_pixels_outside_owner_visibility(
        &self,
        bus: &MacMemoryBus,
        owner: u32,
        rect: (i16, i16, i16, i16),
    ) -> Vec<(i16, i16, i16, i16, Vec<u8>)> {
        if owner == 0 {
            return Vec::new();
        }
        let vis_rgn = bus.read_long(owner + 24);
        if vis_rgn == 0 {
            return Vec::new();
        }
        let Some((top, left, width, height, pixels)) = self.save_screen_rect_pixels(bus, rect)
        else {
            return Vec::new();
        };
        let (bounds_top, bounds_left) = self.port_bounds_top_left(bus, owner);
        let visible_at = |bus: &MacMemoryBus, y: i16, x: i16| {
            Self::region_contains_point(
                bus,
                vis_rgn,
                y.wrapping_add(bounds_top),
                x.wrapping_add(bounds_left),
            )
        };

        let mut saved_runs = Vec::new();
        for row in 0..height {
            let y = top + row;
            let mut column = 0i16;
            while column < width {
                let x = left + column;
                if visible_at(bus, y, x) {
                    column += 1;
                    continue;
                }
                let run_start = column;
                column += 1;
                while column < width && !visible_at(bus, y, left + column) {
                    column += 1;
                }
                let start = row as usize * width as usize + run_start as usize;
                let end = row as usize * width as usize + column as usize;
                saved_runs.push((
                    y,
                    left + run_start,
                    column - run_start,
                    1,
                    pixels[start..end].to_vec(),
                ));
            }
        }
        saved_runs
    }

    fn read_pascal_string(bus: &MacMemoryBus, str_ptr: u32) -> Vec<u8> {
        if str_ptr == 0 {
            return Vec::new();
        }

        let len = bus.read_byte(str_ptr) as usize;
        (0..len)
            .map(|i| bus.read_byte(str_ptr + 1 + i as u32))
            .collect()
    }

    fn write_pascal_string(bus: &mut MacMemoryBus, str_ptr: u32, bytes: &[u8]) {
        if str_ptr == 0 {
            return;
        }

        let len = bytes.len().min(255);
        bus.write_byte(str_ptr, len as u8);
        for (index, &byte) in bytes.iter().take(len).enumerate() {
            bus.write_byte(str_ptr + 1 + index as u32, byte);
        }
    }

    pub(crate) fn control_title_bytes(bus: &MacMemoryBus, ctrl_ptr: u32) -> Vec<u8> {
        Self::read_pascal_string(bus, ctrl_ptr + 40)
    }

    fn set_control_title_bytes(bus: &mut MacMemoryBus, ctrl_ptr: u32, bytes: &[u8]) {
        Self::write_pascal_string(bus, ctrl_ptr + 40, bytes);
    }

    fn control_def_proc_handle(&mut self, bus: &mut MacMemoryBus, proc_id: i16) -> u32 {
        if proc_id <= 0 {
            return 0;
        }

        let cdef_id = proc_id >> 4;
        self.find_or_load_resource_any(bus, *b"CDEF", cdef_id)
            .map(|(_, ptr)| ptr)
            .map(|ptr| self.get_or_create_resource_handle(bus, *b"CDEF", cdef_id, ptr))
            .unwrap_or(0)
    }

    fn control_def_proc_addr(bus: &MacMemoryBus, ctrl_ptr: u32) -> u32 {
        let def_handle = bus.read_long(ctrl_ptr + 24);
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

    fn control_def_entry_looks_callable(bus: &MacMemoryBus, proc_addr: u32) -> bool {
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

    fn control_uses_application_def_proc(&self, bus: &MacMemoryBus, ctrl_ptr: u32) -> bool {
        let proc_id = self.control_proc_ids.get(&ctrl_ptr).copied().unwrap_or(0);
        let proc_addr = Self::control_def_proc_addr(bus, ctrl_ptr);
        let callable = Self::control_def_entry_looks_callable(bus, proc_addr);
        if trace_controls_enabled() && !matches!(proc_id, 0 | 1 | 2 | 16) {
            eprintln!(
                "[CONTROL] CDEF proc_id={} handle=${:08X} entry=${:08X} first=${:04X} callable={}",
                proc_id,
                bus.read_long(ctrl_ptr + 24),
                proc_addr,
                bus.read_word(proc_addr),
                callable,
            );
        }
        !matches!(proc_id, 0 | 1 | 2 | 16) && !Self::is_popup_menu_proc_id(proc_id) && callable
    }

    fn get_or_create_control_def_trampoline(&mut self, bus: &mut MacMemoryBus) -> u32 {
        if self.control_def_trampoline != 0 {
            return self.control_def_trampoline;
        }
        let tramp = bus.alloc(Self::CDEF_TRAMPOLINE_SIZE);
        self.control_def_trampoline = tramp;
        self.control_def_trampoline_chain.push(tramp);
        tramp
    }

    fn write_control_def_trampoline(
        bus: &mut MacMemoryBus,
        tramp: u32,
        variant: i16,
        ctrl_handle: u32,
        message: i16,
        param: u32,
        proc_addr: u32,
        result_addr: u32,
        saved_regs_sp: u32,
        next_trampoline: Option<u32>,
        restore_port: u32,
        restore_gdevice: u32,
    ) {
        bus.write_word(tramp, 0x48E7); // MOVEM.L D0-D3/A0-A3,-(SP)
        bus.write_word(tramp + 2, 0xF0F0);
        bus.write_word(tramp + 4, 0x2F3C); // MOVE.L #result,-(SP)
        bus.write_long(tramp + 6, 0);
        bus.write_word(tramp + 10, 0x3F3C); // MOVE.W #varCode,-(SP)
        bus.write_word(tramp + 12, variant as u16);
        bus.write_word(tramp + 14, 0x2F3C); // MOVE.L #theControl,-(SP)
        bus.write_long(tramp + 16, ctrl_handle);
        bus.write_word(tramp + 20, 0x3F3C); // MOVE.W #message,-(SP)
        bus.write_word(tramp + 22, message as u16);
        bus.write_word(tramp + 24, 0x2F3C); // MOVE.L #param,-(SP)
        bus.write_long(tramp + 26, param);
        bus.write_word(tramp + 30, 0x4EB9); // JSR abs.L
        bus.write_long(tramp + 32, proc_addr);
        bus.write_word(tramp + 36, 0x2017); // MOVE.L (A7),D0
        bus.write_word(tramp + 38, 0x33C0); // MOVE.W D0,abs.L
        bus.write_long(tramp + 40, result_addr);
        bus.write_word(tramp + 44, 0x2E7C); // MOVEA.L #savedRegsSP,A7
        bus.write_long(tramp + 46, saved_regs_sp);
        bus.write_word(tramp + 50, 0x4CDF); // MOVEM.L (SP)+,D0-D3/A0-A3
        bus.write_word(tramp + 52, 0x0F0F);
        match next_trampoline {
            Some(next) => {
                bus.write_word(tramp + 54, 0x4EF9); // JMP abs.L
                bus.write_long(tramp + 56, next);
            }
            None => {
                // Control Manager calls a CDEF in its owning window's port,
                // but leaves the caller's current GrafPort and GDevice
                // unchanged. Restore both after the final callback.
                // Inside Macintosh Volume I, pp. I-163, I-329 to I-331.
                bus.write_word(tramp + 54, 0x2F3C); // MOVE.L #port,-(SP)
                bus.write_long(tramp + 56, restore_port);
                bus.write_word(tramp + 60, 0xA873); // _SetPort
                bus.write_word(tramp + 62, 0x2F3C); // MOVE.L #gdh,-(SP)
                bus.write_long(tramp + 64, restore_gdevice);
                bus.write_word(tramp + 68, 0xAA31); // _SetGDevice
                bus.write_word(tramp + 70, 0x4E75); // RTS
            }
        }
    }

    fn arm_control_def_call_chain<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        calls: &[(u32, i16, u32, Option<u32>)],
    ) -> bool {
        let callable: Vec<(u32, i16, u32, Option<u32>, i16, u32)> = calls
            .iter()
            .filter_map(|&(ctrl_handle, message, param, result_addr)| {
                let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
                if ctrl_ptr == 0 || !self.control_uses_application_def_proc(bus, ctrl_ptr) {
                    return None;
                }
                let proc_id = self.control_proc_ids.get(&ctrl_ptr).copied().unwrap_or(0);
                Some((
                    ctrl_handle,
                    message,
                    param,
                    result_addr,
                    proc_id & 0xF,
                    Self::control_def_proc_addr(bus, ctrl_ptr),
                ))
            })
            .collect();
        if callable.is_empty() {
            return false;
        }
        let restore_port = *self.current_port;
        let restore_gdevice = *self.current_gdevice;
        let first_ctrl_ptr = Self::control_record_ptr(bus, callable[0].0);
        let owner = bus.read_long(first_ctrl_ptr + 4);
        if owner != 0 && owner != *self.current_port {
            self.set_current_port_state(bus, cpu, owner, None);
        }
        let final_sp = cpu.read_reg(Register::A7);
        let return_pc = cpu.read_reg(Register::PC);
        let return_slot = final_sp.wrapping_sub(4);
        let saved_regs_sp = return_slot.wrapping_sub(32);
        self.get_or_create_control_def_trampoline(bus);
        while self.control_def_trampoline_chain.len() < callable.len() {
            self.control_def_trampoline_chain
                .push(bus.alloc(Self::CDEF_TRAMPOLINE_SIZE));
        }
        let trampolines: Vec<u32> = self
            .control_def_trampoline_chain
            .iter()
            .copied()
            .take(callable.len())
            .collect();

        for (idx, &(ctrl_handle, message, param, result_addr, variant, proc_addr)) in
            callable.iter().enumerate()
        {
            Self::write_control_def_trampoline(
                bus,
                trampolines[idx],
                variant,
                ctrl_handle,
                message,
                param,
                proc_addr,
                result_addr.unwrap_or(trampolines[idx] + 72),
                saved_regs_sp,
                trampolines.get(idx + 1).copied(),
                restore_port,
                restore_gdevice,
            );
        }

        bus.write_long(return_slot, return_pc);
        cpu.write_reg(Register::A7, return_slot);
        cpu.write_reg(Register::PC, trampolines[0]);
        true
    }

    fn arm_control_def_messages<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        ctrl_handle: u32,
        messages: &[(i16, u32, Option<u32>)],
    ) -> bool {
        let calls: Vec<(u32, i16, u32, Option<u32>)> = messages
            .iter()
            .map(|&(message, param, result_addr)| (ctrl_handle, message, param, result_addr))
            .collect();
        self.arm_control_def_call_chain(cpu, bus, &calls)
    }

    fn arm_control_action_proc<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        ctrl_handle: u32,
        part: u16,
        action_proc: u32,
        final_sp: u32,
        callback_return_pc: u32,
    ) -> bool {
        if action_proc == 0 || action_proc > bus.ram_size().saturating_sub(2) {
            return false;
        }

        // ControlActionProcPtr is a Pascal procedure taking the control and
        // part code. TrackControl calls it while the mouse remains over an
        // arrow or page region. One callback corresponds to the initial
        // mouse-down; subsequent host events can enter TrackControl again for
        // auto-repeat. Macintosh Toolbox Essentials 1992, pp. 5-89 to 5-91.
        let trampoline = self.get_or_create_control_def_trampoline(bus);
        let return_slot = final_sp.wrapping_sub(4);
        let saved_regs_sp = return_slot.wrapping_sub(32);
        bus.write_word(trampoline, 0x48E7); // MOVEM.L D0-D3/A0-A3,-(SP)
        bus.write_word(trampoline + 2, 0xF0F0);
        bus.write_word(trampoline + 4, 0x2F3C); // MOVE.L #control,-(SP)
        bus.write_long(trampoline + 6, ctrl_handle);
        bus.write_word(trampoline + 10, 0x3F3C); // MOVE.W #part,-(SP)
        bus.write_word(trampoline + 12, part);
        bus.write_word(trampoline + 14, 0x4EB9); // JSR abs.L
        bus.write_long(trampoline + 16, action_proc);
        bus.write_word(trampoline + 20, 0x2E7C); // MOVEA.L #savedRegsSP,A7
        bus.write_long(trampoline + 22, saved_regs_sp);
        bus.write_word(trampoline + 26, 0x4CDF); // MOVEM.L (SP)+,D0-D3/A0-A3
        bus.write_word(trampoline + 28, 0x0F0F);
        bus.write_word(trampoline + 30, 0x4E75); // RTS

        bus.write_long(return_slot, callback_return_pc);
        cpu.write_reg(Register::A7, return_slot);
        cpu.write_reg(Register::PC, trampoline);
        true
    }

    const SCROLLBAR_ACTION_REPEAT_TICKS: u32 = 3;

    fn scrollbar_tracking_part(&self, bus: &MacMemoryBus) -> u16 {
        let Some(tracking) = self.control_tracking.as_ref() else {
            return 0;
        };
        let ctrl_ptr = tracking.ctrl_ptr;
        if ctrl_ptr == 0 {
            return 0;
        }
        let (screen_top, screen_left, screen_bottom, screen_right) = tracking.simple_screen_rect;
        let (mouse_v, mouse_h) = self.control_tracking_mouse_pos(bus);
        if mouse_v < screen_top
            || mouse_v >= screen_bottom
            || mouse_h < screen_left
            || mouse_h >= screen_right
        {
            return 0;
        }
        let local_top = bus.read_word(ctrl_ptr + 8) as i16;
        let local_left = bus.read_word(ctrl_ptr + 10) as i16;
        let port_top = screen_top.wrapping_sub(local_top);
        let port_left = screen_left.wrapping_sub(local_left);
        self.standard_scrollbar_testcontrol_part_code(
            bus,
            ctrl_ptr,
            mouse_v.wrapping_sub(port_top),
            mouse_h.wrapping_sub(port_left),
        )
    }

    fn finish_scrollbar_control_tracking<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        current_part: u16,
    ) {
        let Some(tracking) = self.control_tracking.take() else {
            return;
        };
        let part = if current_part == tracking.scrollbar_part {
            current_part
        } else {
            0
        };
        self.record_trackcontrol_input_trace(
            bus,
            "tracking_finish",
            tracking.ctrl_handle,
            None,
            tracking.scrollbar_action_proc,
            Some(part),
            None,
            if part == 0 {
                "scrollbar_no_selection"
            } else {
                "scrollbar_part_selected"
            },
        );
        bus.write_word(tracking.stack_ptr + 12, part);
        cpu.write_reg(Register::A7, tracking.stack_ptr + 12);
    }

    pub(crate) fn arm_new_dialog_control_defs<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
    ) -> bool {
        if dialog_ptr == 0 {
            return false;
        }
        let mut handles = Vec::new();
        let mut ctrl_handle = bus.read_long(dialog_ptr + 140);
        while ctrl_handle != 0 {
            let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
            if ctrl_ptr == 0 {
                break;
            }
            handles.push(ctrl_handle);
            ctrl_handle = bus.read_long(ctrl_ptr);
        }

        let mut calls = Vec::new();
        for ctrl_handle in handles.into_iter().rev() {
            let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
            if !self.control_uses_application_def_proc(bus, ctrl_ptr) {
                continue;
            }
            calls.push((ctrl_handle, Self::CDEF_INIT_CNTL_MSG, 0, None));
        }
        if calls.is_empty() {
            return false;
        }
        self.arm_control_def_call_chain(cpu, bus, &calls)
    }

    pub(crate) fn arm_dialog_control_def_draws<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
    ) -> bool {
        if dialog_ptr == 0 {
            return false;
        }
        let vis_rgn = bus.read_long(dialog_ptr + 24);
        if Self::region_handle_rect(bus, vis_rgn).is_none() {
            return false;
        }
        let mut handles = Vec::new();
        let mut ctrl_handle = bus.read_long(dialog_ptr + 140);
        while ctrl_handle != 0 {
            let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
            if ctrl_ptr == 0 {
                break;
            }
            handles.push(ctrl_handle);
            ctrl_handle = bus.read_long(ctrl_ptr);
        }

        let calls: Vec<_> = handles
            .into_iter()
            .rev()
            .filter_map(|ctrl_handle| {
                let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
                (self.control_uses_application_def_proc(bus, ctrl_ptr)
                    && Self::control_vis_is_visible(bus.read_byte(ctrl_ptr + 16)))
                .then_some((ctrl_handle, Self::CDEF_DRAW_CNTL_MSG, 0, None))
            })
            .collect();
        if calls.is_empty() {
            return false;
        }
        // Establish the retained-dialog baseline before guest drawing. Each
        // DrawPicture callback then refreshes this snapshot, so frame
        // composition cannot restore the pre-CDEF shell over the artwork.
        self.refresh_visible_dialog_snapshot_for_port(bus, dialog_ptr);
        let armed = self.arm_control_def_call_chain(cpu, bus, &calls);
        if armed {
            self.dialog_cdefs_initially_drawn.insert(dialog_ptr);
        }
        armed
    }

    fn control_aux_reserved_value(&self, bus: &MacMemoryBus, ctrl_handle: u32) -> u32 {
        let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
        let proc_id = self.control_proc_ids.get(&ctrl_ptr).copied().unwrap_or(0);
        u32::from((proc_id & 0xF) as u16) << 24
    }

    fn standard_testcontrol_part_code(&self, ctrl_ptr: u32) -> u16 {
        match self.control_proc_ids.get(&ctrl_ptr).copied().unwrap_or(0) {
            1 | 2 => 11, // inCheckBox for checkbox and radio-button variants
            _ => 10,     // inButton for push buttons and the current fallback path
        }
    }

    pub(crate) fn write_control_value(
        &mut self,
        bus: &mut MacMemoryBus,
        ctrl_handle: u32,
        value: i16,
    ) {
        let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
        if ctrl_ptr == 0 {
            return;
        }
        if let Some((dlg_ptr, item_no)) = self.dialog_control_handles.get(&ctrl_handle).copied() {
            self.dialog_control_values.insert((dlg_ptr, item_no), value);
        }
        bus.write_word(ctrl_ptr + 18, value as u16);
    }

    fn sync_dialog_item_rect_for_control(&mut self, bus: &MacMemoryBus, ctrl_handle: u32) {
        let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
        if ctrl_ptr == 0 {
            return;
        }
        let Some((dialog_ptr, item_no)) = self.dialog_control_handles.get(&ctrl_handle).copied()
        else {
            return;
        };
        let rect = (
            bus.read_word(ctrl_ptr + 8) as i16,
            bus.read_word(ctrl_ptr + 10) as i16,
            bus.read_word(ctrl_ptr + 12) as i16,
            bus.read_word(ctrl_ptr + 14) as i16,
        );
        let idx = (item_no as usize).wrapping_sub(1);
        if let Some(items) = self.dialog_items.get_mut(&dialog_ptr) {
            if idx < items.len() {
                items[idx].rect = rect;
            }
        }
        if let Some(tracking) = self.dialog_tracking.as_mut() {
            if tracking.dialog_ptr == dialog_ptr && idx < tracking.items.len() {
                tracking.items[idx].rect = rect;
            }
        }
    }

    pub(crate) fn popup_control_dropdown_rect(
        &self,
        bus: &MacMemoryBus,
        ctrl_ptr: u32,
        menu_idx: usize,
    ) -> (i16, i16, i16, i16) {
        let (_, _, screen_width, screen_height, _) = self.get_screen_params();
        let owner = bus.read_long(ctrl_ptr + 4);
        let (owner_top, owner_left, _, _) = if owner != 0 {
            Self::dialog_screen_bounds(bus, owner)
        } else {
            (0, 0, 0, 0)
        };
        let r_top = bus.read_word(ctrl_ptr + 8) as i16;
        let r_left = bus.read_word(ctrl_ptr + 10) as i16;
        let r_right = bus.read_word(ctrl_ptr + 14) as i16;
        let selected_value = bus.read_word(ctrl_ptr + 18) as i16;
        let selected_index = selected_value.max(0) as usize;
        let abs_top = owner_top + r_top;
        let abs_left = owner_left + r_left;

        let mut width = (r_right - r_left).max(80);
        if let Some(menu) = self.menus.get(menu_idx) {
            width = width.max(self.standard_menu_width(bus, &menu.items));
            // The standard popup CDEF opens the live menu with the current
            // value's item aligned to the popup box, matching the Menu
            // Manager's PopUpMenuSelect(top, left, popUpItem) convention.
            let rows = self.menu_rows(bus, &menu.items);
            if let Some(layout) = standard_popup_menu_layout(
                &rows,
                width,
                (screen_width, screen_height),
                (abs_top, abs_left),
                selected_index as i16,
            ) {
                return layout.rect();
            }
        }

        let desired_top = abs_top - 1;
        let height = 2;
        let clamped_top = if screen_height <= 0 {
            desired_top
        } else if height >= screen_height {
            0
        } else {
            desired_top.clamp(0, screen_height - height)
        };
        (
            clamped_top,
            abs_left,
            clamped_top + height,
            (abs_left + width).min(screen_width),
        )
    }

    fn control_tracking_item_at_point(
        &self,
        bus: &MacMemoryBus,
        mouse_x: i16,
        mouse_y: i16,
    ) -> i16 {
        let Some(tracking) = self.control_tracking.as_ref() else {
            return 0;
        };
        let (top, left, bottom, right) = tracking.dropdown_rect;
        if mouse_x < left || mouse_x >= right || mouse_y < top || mouse_y >= bottom {
            return 0;
        }
        let Some(menu) = self.menus.get(tracking.active_menu) else {
            return 0;
        };
        let mut item_top = top;
        for (item_idx, item) in menu.items.iter().enumerate() {
            let item_bottom = item_top + self.menu_item_height(bus, item);
            if mouse_y >= item_top && mouse_y < item_bottom {
                if item.text == "-" || !item.enabled {
                    return 0;
                }
                return item_idx as i16 + 1;
            }
            item_top = item_bottom;
        }
        0
    }

    fn set_control_tracking_highlight(&mut self, bus: &mut MacMemoryBus, item: i16) {
        let Some(old_item) = self
            .control_tracking
            .as_ref()
            .map(|tracking| tracking.highlighted_item)
        else {
            return;
        };
        if old_item == item {
            return;
        }

        let Some((active_menu, dropdown_rect)) = self.control_tracking.as_mut().map(|tracking| {
            tracking.highlighted_item = item;
            (tracking.active_menu, tracking.dropdown_rect)
        }) else {
            return;
        };
        self.draw_menu_dropdown(bus, active_menu, dropdown_rect);
    }

    fn finish_popup_control_tracking<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        selected_item: i16,
    ) {
        let Some(tracking) = self.control_tracking.take() else {
            return;
        };
        self.restore_dropdown_pixels(bus, tracking.dropdown_rect, &tracking.saved_pixels);

        let part = if selected_item > 0 {
            self.write_control_value(bus, tracking.ctrl_handle, selected_item);
            self.draw_control(cpu, bus, tracking.ctrl_ptr);
            10u16
        } else {
            0u16
        };
        self.record_trackcontrol_input_trace(
            bus,
            "tracking_finish",
            tracking.ctrl_handle,
            None,
            0,
            Some(part),
            Some(selected_item),
            if selected_item > 0 {
                "popup_item_selected"
            } else {
                "popup_no_selection"
            },
        );
        bus.write_word(tracking.stack_ptr + 12, part);
        cpu.write_reg(Register::A7, tracking.stack_ptr + 12);
    }

    fn simple_control_tracking_inside(&self, bus: &MacMemoryBus) -> bool {
        let Some(tracking) = self.control_tracking.as_ref() else {
            return false;
        };
        let (top, left, bottom, right) = tracking.simple_screen_rect;
        let (mouse_v, mouse_h) = self.control_tracking_mouse_pos(bus);
        mouse_v >= top && mouse_v < bottom && mouse_h >= left && mouse_h < right
    }

    fn redraw_simple_control_tracking_state<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        highlighted: bool,
    ) {
        let Some((ctrl_ptr, saved_hilite)) = self
            .control_tracking
            .as_ref()
            .map(|tracking| (tracking.ctrl_ptr, tracking.saved_hilite))
        else {
            return;
        };
        bus.write_byte(ctrl_ptr + 17, if highlighted { 1 } else { saved_hilite });
        self.draw_control(cpu, bus, ctrl_ptr);
        if let Some(tracking) = self.control_tracking.as_mut() {
            tracking.simple_highlighted = highlighted;
        }
    }

    fn finish_simple_control_tracking<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        inside: bool,
    ) {
        let Some(tracking) = self.control_tracking.take() else {
            return;
        };
        bus.write_byte(tracking.ctrl_ptr + 17, tracking.saved_hilite);
        self.draw_control(cpu, bus, tracking.ctrl_ptr);

        let part = if inside { tracking.simple_part } else { 0 };
        self.record_trackcontrol_input_trace(
            bus,
            "tracking_finish",
            tracking.ctrl_handle,
            None,
            0,
            Some(part),
            None,
            if inside {
                "simple_part_selected"
            } else {
                "simple_no_selection"
            },
        );
        bus.write_word(tracking.stack_ptr + 12, part);
        cpu.write_reg(Register::A7, tracking.stack_ptr + 12);
    }

    fn standard_scrollbar_testcontrol_part_code(
        &self,
        bus: &MacMemoryBus,
        ctrl_ptr: u32,
        pt_v: i16,
        pt_h: i16,
    ) -> u16 {
        let top = bus.read_word(ctrl_ptr + 8) as i16;
        let left = bus.read_word(ctrl_ptr + 10) as i16;
        let bottom = bus.read_word(ctrl_ptr + 12) as i16;
        let right = bus.read_word(ctrl_ptr + 14) as i16;
        let value = bus.read_word(ctrl_ptr + 18) as i16;
        let min = bus.read_word(ctrl_ptr + 20) as i16;
        let max = bus.read_word(ctrl_ptr + 22) as i16;

        if min >= max {
            return 0;
        }

        let is_vertical = (bottom - top) > (right - left);
        let arrow_len = 16i16.min(if is_vertical {
            bottom - top
        } else {
            right - left
        });
        let range = i32::from(max) - i32::from(min);
        if range <= 0 {
            return 0;
        }
        let value_clamped = (i32::from(value) - i32::from(min)).clamp(0, range);
        let thumb_len = 16i16.min(if is_vertical {
            (bottom - top) - arrow_len * 2
        } else {
            (right - left) - arrow_len * 2
        });
        if thumb_len <= 0 {
            return 0;
        }

        if is_vertical {
            let track_top = top + arrow_len;
            let track_bottom = bottom - arrow_len;
            let track_len = track_bottom - track_top;
            let travel = i32::from(track_len - thumb_len);
            if travel < 0 {
                return 0;
            }
            let thumb_top = track_top + ((value_clamped * travel) / range) as i16;
            let thumb_bottom = thumb_top + thumb_len;
            if pt_v < track_top {
                20
            } else if pt_v >= track_bottom {
                21
            } else if pt_v < thumb_top {
                22
            } else if pt_v < thumb_bottom {
                129
            } else {
                23
            }
        } else {
            let track_left = left + arrow_len;
            let track_right = right - arrow_len;
            let track_len = track_right - track_left;
            let travel = i32::from(track_len - thumb_len);
            if travel < 0 {
                return 0;
            }
            let thumb_left = track_left + ((value_clamped * travel) / range) as i16;
            let thumb_right = thumb_left + thumb_len;
            if pt_h < track_left {
                20
            } else if pt_h >= track_right {
                21
            } else if pt_h < thumb_left {
                22
            } else if pt_h < thumb_right {
                129
            } else {
                23
            }
        }
    }

    fn sync_control_aux_head_lowmem(&self, bus: &mut MacMemoryBus) {
        bus.write_long(Self::LOWMEM_AUX_CTL_HEAD, self.control_aux_head);
    }

    fn default_control_color_table_handle(&mut self, bus: &mut MacMemoryBus) -> u32 {
        let gd_handle = self.ensure_main_gdevice(bus);
        let gd_ptr = bus.read_long(gd_handle);
        let gd_pixmap_handle = bus.read_long(gd_ptr + 22);
        let gd_pixmap_ptr = bus.read_long(gd_pixmap_handle);
        bus.read_long(gd_pixmap_ptr + 42)
    }

    pub(crate) fn ensure_control_aux_record(
        &mut self,
        bus: &mut MacMemoryBus,
        ctrl_handle: u32,
    ) -> u32 {
        if ctrl_handle == 0 {
            return 0;
        }

        let default_ctab = self.default_control_color_table_handle(bus);
        if let Some(state) = self.control_aux_records.get(&ctrl_handle).copied() {
            let aux_ptr = bus.read_long(state.handle);
            if aux_ptr != 0 {
                bus.write_long(aux_ptr + Self::AUX_CTL_OWNER_OFFSET, ctrl_handle);
                if bus.read_long(aux_ptr + Self::AUX_CTL_CTABLE_OFFSET) == 0 {
                    bus.write_long(aux_ptr + Self::AUX_CTL_CTABLE_OFFSET, default_ctab);
                }
                bus.write_word(aux_ptr + Self::AUX_CTL_FLAGS_OFFSET, 0);
                bus.write_long(
                    aux_ptr + Self::AUX_CTL_RESERVED_OFFSET,
                    self.control_aux_reserved_value(bus, ctrl_handle),
                );
            }
            return state.handle;
        }

        let aux_ptr = bus.alloc(Self::AUX_CTL_RECORD_SIZE);
        let aux_handle = bus.alloc(4);
        bus.write_long(aux_handle, aux_ptr);
        bus.write_long(aux_ptr + Self::AUX_CTL_NEXT_OFFSET, self.control_aux_head);
        bus.write_long(aux_ptr + Self::AUX_CTL_OWNER_OFFSET, ctrl_handle);
        bus.write_long(aux_ptr + Self::AUX_CTL_CTABLE_OFFSET, default_ctab);
        bus.write_word(aux_ptr + Self::AUX_CTL_FLAGS_OFFSET, 0);
        bus.write_long(
            aux_ptr + Self::AUX_CTL_RESERVED_OFFSET,
            self.control_aux_reserved_value(bus, ctrl_handle),
        );
        bus.write_long(aux_ptr + Self::AUX_CTL_REFCON_OFFSET, 0);
        self.control_aux_head = aux_handle;
        self.sync_control_aux_head_lowmem(bus);
        self.control_aux_records
            .insert(ctrl_handle, ControlAuxRecordState { handle: aux_handle });
        aux_handle
    }

    pub(crate) fn release_control_aux_record(&mut self, bus: &mut MacMemoryBus, ctrl_handle: u32) {
        let Some(state) = self.control_aux_records.remove(&ctrl_handle) else {
            return;
        };

        let mut prev_handle = 0u32;
        let mut cur_handle = self.control_aux_head;
        while cur_handle != 0 {
            let cur_ptr = bus.read_long(cur_handle);
            if cur_ptr == 0 {
                break;
            }
            let next_handle = bus.read_long(cur_ptr + Self::AUX_CTL_NEXT_OFFSET);
            if cur_handle == state.handle {
                if prev_handle == 0 {
                    self.control_aux_head = next_handle;
                } else {
                    let prev_ptr = bus.read_long(prev_handle);
                    if prev_ptr != 0 {
                        bus.write_long(prev_ptr + Self::AUX_CTL_NEXT_OFFSET, next_handle);
                    }
                }
                break;
            }
            prev_handle = cur_handle;
            cur_handle = next_handle;
        }
        self.sync_control_aux_head_lowmem(bus);

        let aux_ptr = bus.read_long(state.handle);
        if aux_ptr != 0 {
            bus.free(aux_ptr);
        }
        bus.free(state.handle);
    }

    pub(crate) fn control_aux_state(&self, ctrl_handle: u32) -> Option<ControlAuxRecordState> {
        self.control_aux_records.get(&ctrl_handle).copied()
    }

    pub(crate) fn initialize_control_record(
        &mut self,
        bus: &mut MacMemoryBus,
        ctrl_ptr: u32,
        window_ptr: u32,
        bounds: (i16, i16, i16, i16),
        title: &[u8],
        visible: bool,
        value: i16,
        min: i16,
        max: i16,
        proc_id: i16,
        ref_con: u32,
    ) {
        bus.write_long(ctrl_ptr, 0); // nextControl
        bus.write_long(ctrl_ptr + 4, window_ptr); // contrlOwner
        bus.write_word(ctrl_ptr + 8, bounds.0 as u16);
        bus.write_word(ctrl_ptr + 10, bounds.1 as u16);
        bus.write_word(ctrl_ptr + 12, bounds.2 as u16);
        bus.write_word(ctrl_ptr + 14, bounds.3 as u16);
        bus.write_byte(ctrl_ptr + 16, if visible { 255 } else { 0 });
        bus.write_byte(ctrl_ptr + 17, 0); // contrlHilite
        bus.write_word(ctrl_ptr + 18, value as u16);
        bus.write_word(ctrl_ptr + 20, min as u16);
        bus.write_word(ctrl_ptr + 22, max as u16);
        let def_proc_handle = self.control_def_proc_handle(bus, proc_id);
        bus.write_long(ctrl_ptr + 24, def_proc_handle);
        let contrl_data = if Self::is_popup_menu_proc_id(proc_id) {
            // `max` is the title width only while the pop-up is being
            // created. The standard CDEF then publishes the menu item count
            // in contrlMax, so retain the pixel width as private CDEF state.
            // Macintosh Toolbox Essentials 1992, pp. 5-25 to 5-27.
            self.popup_control_title_widths.insert(ctrl_ptr, max);
            let data = self.create_popup_control_private_data(bus, min);
            let item_count = self
                .menus
                .iter()
                .rev()
                .find(|menu| menu.id == min)
                .map_or(0, |menu| menu.items.len().min(i16::MAX as usize) as i16);
            bus.write_word(ctrl_ptr + 20, 1);
            bus.write_word(ctrl_ptr + 22, item_count as u16);
            data
        } else {
            0
        };
        bus.write_long(ctrl_ptr + 28, contrl_data);
        bus.write_long(ctrl_ptr + 32, 0); // contrlAction
        bus.write_long(ctrl_ptr + 36, ref_con);
        Self::set_control_title_bytes(bus, ctrl_ptr, title);
        self.control_proc_ids.insert(ctrl_ptr, proc_id);
    }

    pub(crate) fn create_control_record(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        bounds: (i16, i16, i16, i16),
        title: &[u8],
        visible: bool,
        value: i16,
        min: i16,
        max: i16,
        proc_id: i16,
        ref_con: u32,
    ) -> (u32, u32) {
        let ctrl_ptr = bus.alloc(296);
        let handle = bus.alloc(4);
        if ctrl_ptr == 0 || handle == 0 {
            return (0, 0);
        }
        bus.write_long(handle, ctrl_ptr);
        self.initialize_control_record(
            bus, ctrl_ptr, window_ptr, bounds, title, visible, value, min, max, proc_id, ref_con,
        );
        self.ensure_control_aux_record(bus, handle);
        if window_ptr != 0 {
            let old_head = bus.read_long(window_ptr + 140);
            bus.write_long(ctrl_ptr, old_head);
            bus.write_long(window_ptr + 140, handle);
        }
        (handle, ctrl_ptr)
    }

    pub(crate) fn dispose_control_handle(&mut self, bus: &mut MacMemoryBus, ctrl_handle: u32) {
        if ctrl_handle == 0 {
            return;
        }
        let ctrl_ptr = bus.read_long(ctrl_handle);
        if ctrl_ptr != 0 {
            let owner = bus.read_long(ctrl_ptr + 4);
            if owner != 0 {
                let mut prev_handle = 0u32;
                let mut cur_handle = bus.read_long(owner + 140);
                while cur_handle != 0 {
                    let cur_ptr = bus.read_long(cur_handle);
                    if cur_ptr == 0 {
                        break;
                    }
                    if cur_handle == ctrl_handle {
                        let next = bus.read_long(cur_ptr);
                        if prev_handle == 0 {
                            bus.write_long(owner + 140, next);
                        } else {
                            let prev_ptr = bus.read_long(prev_handle);
                            if prev_ptr != 0 {
                                bus.write_long(prev_ptr, next);
                            }
                        }
                        break;
                    }
                    prev_handle = cur_handle;
                    cur_handle = bus.read_long(cur_ptr);
                }
            }
            self.release_control_aux_record(bus, ctrl_handle);
            self.control_proc_ids.remove(&ctrl_ptr);
            self.popup_control_title_widths.remove(&ctrl_ptr);
            bus.free(ctrl_ptr);
        }
        bus.free(ctrl_handle);
    }

    fn create_popup_control_private_data(&mut self, bus: &mut MacMemoryBus, menu_id: i16) -> u32 {
        let menu_handle = self.create_popup_menu_handle(bus, menu_id);
        if menu_handle == 0 {
            return 0;
        }

        let data_ptr = bus.alloc(8);
        let data_handle = bus.alloc(4);
        if data_ptr == 0 || data_handle == 0 {
            return 0;
        }

        // IM:VI 1991 pp. 3-18..3-19: popup control contrlData is a
        // handle to popupPrivateData { mHandle: MenuHandle, mID: Integer,
        // mPrivate: reserved bytes }.
        bus.write_long(data_handle, data_ptr);
        bus.write_long(data_ptr, menu_handle);
        bus.write_word(data_ptr + 4, menu_id as u16);
        bus.write_word(data_ptr + 6, 0);
        self.track_handle_ptr(data_ptr, data_handle);
        data_handle
    }

    pub(crate) fn popup_control_menu_id(
        &self,
        bus: &MacMemoryBus,
        ctrl_ptr: u32,
        fallback_menu_id: i16,
    ) -> i16 {
        let data_handle = bus.read_long(ctrl_ptr + 28);
        if data_handle != 0 {
            let data_ptr = bus.read_long(data_handle);
            if data_ptr != 0 && bus.get_alloc_size(data_ptr).unwrap_or(0) >= 6 {
                let menu_handle = bus.read_long(data_ptr);
                let menu_id = bus.read_word(data_ptr + 4) as i16;
                if menu_id != 0
                    && self
                        .menus
                        .iter()
                        .any(|menu| menu.id == menu_id && menu.handle == menu_handle)
                {
                    return menu_id;
                }
            }
        }
        fallback_menu_id
    }

    pub(crate) fn popup_control_title_width(&self, ctrl_ptr: u32, fallback: i16) -> i16 {
        self.popup_control_title_widths
            .get(&ctrl_ptr)
            .copied()
            .unwrap_or(fallback)
    }

    /// Draw a single control based on its procID, using QuickDraw primitives
    /// so the output matches real Mac CDEF rendering.
    /// Coordinates in the control record are local to the owning window.
    /// QuickDraw shape methods handle the local→screen conversion via the port.
    pub(crate) fn draw_control<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        ctrl_ptr: u32,
    ) {
        if ctrl_ptr == 0 {
            return;
        }
        let vis = bus.read_byte(ctrl_ptr + 16);
        if !Self::control_vis_is_visible(vis) {
            return;
        }
        let window_ptr = bus.read_long(ctrl_ptr + 4);
        if window_ptr != 0 {
            let vis_rgn = bus.read_long(window_ptr + 24);
            if vis_rgn != 0 && Self::region_handle_rect(bus, vis_rgn).is_none() {
                return;
            }
        }
        let saved_port = *self.current_port;
        let saved_gdevice = *self.current_gdevice;
        if window_ptr != 0 && window_ptr != saved_port {
            self.set_current_port_state(bus, cpu, window_ptr, None);
        }

        let r_top = bus.read_word(ctrl_ptr + 8) as i16;
        let r_left = bus.read_word(ctrl_ptr + 10) as i16;
        let r_bottom = bus.read_word(ctrl_ptr + 12) as i16;
        let r_right = bus.read_word(ctrl_ptr + 14) as i16;
        let hilite = bus.read_byte(ctrl_ptr + 17);
        let value = bus.read_word(ctrl_ptr + 18) as i16;
        let min = bus.read_word(ctrl_ptr + 20) as i16;
        let max = bus.read_word(ctrl_ptr + 22) as i16;
        let title_bytes = Self::control_title_bytes(bus, ctrl_ptr);
        let title = decode_mac_roman_for_render(&title_bytes);

        let proc_id = self.control_proc_ids.get(&ctrl_ptr).copied().unwrap_or(0);

        // Save pen state, set to defaults (PenNormal)
        let saved_pn_size = self.pn_size;
        let saved_pn_mode = self.pn_mode;
        let saved_pn_pat = self.pn_pat;
        self.pn_size = (1, 1);
        self.pn_mode = 8; // patCopy
        self.pn_pat = [0xFF; 8]; // solid black

        // Local rect for QuickDraw primitives
        let r = Rect {
            top: r_top,
            left: r_left,
            bottom: r_bottom,
            right: r_right,
        };

        // Screen coordinates for fb_draw_string (text rendering)
        let (scr_top, scr_left, _, _) = Self::dialog_screen_bounds(bus, window_ptr);
        let abs_top = scr_top + r_top;
        let abs_left = scr_left + r_left;
        let abs_bottom = scr_top + r_bottom;
        let abs_right = scr_left + r_right;
        let occluded_pixels = self.save_control_pixels_outside_owner_visibility(
            bus,
            window_ptr,
            (abs_top, abs_left, abs_bottom, abs_right),
        );

        match proc_id {
            0 => {
                // pushButProc — push button
                self.draw_push_button_control(
                    cpu, bus, &r, abs_top, abs_left, abs_bottom, abs_right, hilite, &title,
                );
            }
            1 => {
                // checkBoxProc — checkbox
                let checked = value != 0;
                let layout = crate::control_manager::standard_checkbox_layout((
                    r_top, r_left, r_bottom, r_right,
                ));
                let (box_top, box_left, box_bottom, box_right) = layout.indicator;
                let box_size = box_bottom.saturating_sub(box_top);
                let box_r = Rect {
                    top: box_top,
                    left: box_left,
                    bottom: box_bottom,
                    right: box_right,
                };

                if !self.draw_theme_control_chrome(
                    bus,
                    ControlKind::Checkbox,
                    scr_top + box_top,
                    scr_left + box_left,
                    scr_top + box_top + box_size,
                    scr_left + box_left + box_size,
                    hilite != 255,
                    hilite == 1,
                    checked,
                    false,
                ) {
                    // Erase checkbox area
                    self.draw_rect(cpu, bus, &box_r, ShapeOp::Erase);
                    // Frame checkbox border
                    self.draw_rect(cpu, bus, &box_r, ShapeOp::Frame);

                    if checked {
                        // Draw X inside the checkbox
                        self.draw_checkbox_x(bus, scr_top + box_top, scr_left + box_left, box_size);
                    }
                }

                // Draw label text to the right of the checkbox
                let font_id = 0i16;
                let font_size = 12i16;
                let metrics = get_font_metrics(font_id, font_size);
                let text_x = scr_left + layout.label_left;
                let text_y = scr_top
                    + crate::control_manager::centered_control_label_origin(
                        (r_top, r_left, r_bottom, r_right),
                        0,
                        metrics.ascent,
                        metrics.descent,
                    )
                    .1;
                self.draw_control_label_text(
                    bus,
                    abs_top,
                    scr_left + box_left + box_size + 1,
                    abs_bottom,
                    abs_right,
                    text_x,
                    text_y,
                    &title,
                    font_id,
                    font_size,
                    hilite == 255,
                );

                if self.ui_theme_id() == crate::ui_theme::UiThemeId::ClassicSystem7 && hilite == 1 {
                    // Pressed: invert the checkbox box area
                    self.draw_rect(cpu, bus, &box_r, ShapeOp::Invert);
                }
            }
            2 => {
                // radioButProc — radio button
                let selected = value != 0;
                let layout = crate::control_manager::standard_radio_button_layout((
                    r_top, r_left, r_bottom, r_right,
                ));
                let (circle_top, circle_left, circle_bottom, circle_right) = layout.indicator;
                let circle_size = circle_bottom.saturating_sub(circle_top);
                let circle_r = Rect {
                    top: circle_top,
                    left: circle_left,
                    bottom: circle_bottom,
                    right: circle_right,
                };

                if !self.draw_theme_control_chrome(
                    bus,
                    ControlKind::RadioButton,
                    scr_top + circle_top,
                    scr_left + circle_left,
                    scr_top + circle_top + circle_size,
                    scr_left + circle_left + circle_size,
                    hilite != 255,
                    hilite == 1,
                    selected,
                    false,
                ) {
                    // Erase circle area
                    self.draw_oval(cpu, bus, &circle_r, ShapeOp::Erase);
                    // Frame circle
                    self.draw_oval(cpu, bus, &circle_r, ShapeOp::Frame);

                    if selected {
                        // Fill inner circle (smaller by 3px on each side)
                        let inner_r = Rect {
                            top: circle_top + 3,
                            left: circle_left + 3,
                            bottom: circle_top + circle_size - 3,
                            right: circle_left + circle_size - 3,
                        };
                        self.draw_oval(cpu, bus, &inner_r, ShapeOp::Paint);
                    }
                }

                // Draw label text to the right of the circle
                let font_id = 0i16;
                let font_size = 12i16;
                let metrics = get_font_metrics(font_id, font_size);
                let text_x = scr_left + layout.label_left;
                let text_y = scr_top
                    + crate::control_manager::centered_control_label_origin(
                        (r_top, r_left, r_bottom, r_right),
                        0,
                        metrics.ascent,
                        metrics.descent,
                    )
                    .1;
                self.draw_control_label_text(
                    bus,
                    abs_top,
                    scr_left + circle_left + circle_size + 1,
                    abs_bottom,
                    abs_right,
                    text_x,
                    text_y,
                    &title,
                    font_id,
                    font_size,
                    hilite == 255,
                );

                if self.ui_theme_id() == crate::ui_theme::UiThemeId::ClassicSystem7 && hilite == 1 {
                    // Pressed: invert the circle area
                    self.draw_oval(cpu, bus, &circle_r, ShapeOp::Invert);
                }
            }
            16 => {
                // scrollBarProc — scroll bar (uses fb_* functions with screen coords)
                self.draw_scroll_bar(
                    bus, abs_top, abs_left, abs_bottom, abs_right, value, min, max, hilite,
                );
            }
            proc_id if Self::is_popup_menu_proc_id(proc_id) => {
                // popupMenuProc — draw the CNTL-backed popup button using
                // the selected MENU resource item.
                let menu_id = self.popup_control_menu_id(bus, ctrl_ptr, min);
                let selected = value.max(1) as usize;
                let item_title = self.popup_menu_item_title(bus, menu_id, selected);
                let title_width = self.popup_control_title_width(ctrl_ptr, max);
                let (draw_top, draw_left, draw_bottom, draw_right) = self.popup_control_box_rect(
                    bus,
                    abs_top,
                    abs_left,
                    abs_bottom,
                    abs_right,
                    menu_id,
                    title_width,
                    proc_id,
                );
                self.draw_popup_control_label(
                    bus,
                    abs_top,
                    abs_left,
                    abs_bottom,
                    draw_left,
                    &title,
                    hilite != 255,
                );
                // HIG 1992 p. 87 describes the closed pop-up menu as the
                // rectangle/triangle control; MTE 1992 p. 6-98 says
                // HiliteControl dims inactive pop-up menus. Keep part-code
                // and menu tracking behavior unchanged while routing that
                // semantic state to non-classic theme chrome.
                self.draw_popup_control_with_state(
                    bus,
                    draw_top,
                    draw_left,
                    draw_bottom,
                    draw_right,
                    &item_title.unwrap_or_default(),
                    hilite != 255,
                    hilite == 1,
                );
            }
            _ => {
                if trace_controls_enabled() {
                    eprintln!(
                        "[CONTROL] Unknown procID {} for control at ${:08X}",
                        proc_id, ctrl_ptr
                    );
                }
                // IM:I I-328..I-331 defines custom controls as CDEF-backed
                // drawCntl/testCntl implementations selected by procID. Systemless
                // cannot execute arbitrary guest CDEFs here, so preserve the
                // ControlRecord title and rectangle as a generic button fallback.
                if !self.draw_picture_title_control(
                    bus,
                    abs_top,
                    abs_left,
                    abs_bottom,
                    abs_right,
                    hilite,
                    value,
                    &title_bytes,
                ) {
                    self.draw_push_button_control(
                        cpu, bus, &r, abs_top, abs_left, abs_bottom, abs_right, hilite, &title,
                    );
                }
            }
        }

        // Restore pen state
        self.pn_size = saved_pn_size;
        self.pn_mode = saved_pn_mode;
        self.pn_pat = saved_pn_pat;
        for (top, left, width, height, pixels) in occluded_pixels {
            self.restore_screen_rect_pixels(bus, top, left, width, height, &pixels);
        }
        if *self.current_port != saved_port || *self.current_gdevice != saved_gdevice {
            self.set_current_port_state(bus, cpu, saved_port, Some(saved_gdevice));
        }
    }

    fn draw_picture_title_control(
        &mut self,
        bus: &mut MacMemoryBus,
        abs_top: i16,
        abs_left: i16,
        abs_bottom: i16,
        abs_right: i16,
        hilite: u8,
        value: i16,
        title_bytes: &[u8],
    ) -> bool {
        let Some((off_id, on_id)) = Self::picture_ids_from_control_title(title_bytes) else {
            return false;
        };
        let Some((_, off_pic_ptr)) = self.find_or_load_resource_any(bus, *b"PICT", off_id) else {
            return false;
        };
        let Some((_, on_pic_ptr)) = self.find_or_load_resource_any(bus, *b"PICT", on_id) else {
            return false;
        };

        // IM:I I-317..I-331 says custom CDEFs interpret ControlRecord
        // fields, and IM:I I-369 says DrawPicture scales a Picture into a
        // destination Rect. If a custom control's title bytes are exactly two
        // valid PICT resource IDs, use those resources as a generic CDEF
        // fallback and leave all other custom controls on the text/button path.
        let pic_ptr = if hilite == 1 || value != 0 {
            on_pic_ptr
        } else {
            off_pic_ptr
        };
        let device_ct_seed =
            Self::ctab_seed(bus, self.current_gdevice_ctab_handle(bus)).unwrap_or(0);
        let (drawn, _) = super::pict::draw_picture(
            bus,
            pic_ptr,
            abs_top,
            abs_left,
            abs_bottom,
            abs_right,
            self.screen_mode,
            &self.color_manager_clut,
            device_ct_seed,
            None,
        );
        if drawn && hilite == 255 {
            self.dim_rect(bus, abs_top, abs_left, abs_bottom, abs_right);
        }
        drawn
    }

    fn picture_ids_from_control_title(title_bytes: &[u8]) -> Option<(i16, i16)> {
        (title_bytes.len() == 4).then(|| {
            (
                i16::from_be_bytes([title_bytes[0], title_bytes[1]]),
                i16::from_be_bytes([title_bytes[2], title_bytes[3]]),
            )
        })
    }

    fn draw_push_button_control<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        r: &Rect,
        abs_top: i16,
        abs_left: i16,
        abs_bottom: i16,
        abs_right: i16,
        hilite: u8,
        title: &str,
    ) {
        if self.draw_theme_push_button_chrome(
            bus,
            abs_top,
            abs_left,
            abs_bottom,
            abs_right,
            hilite != 255,
            hilite == 1,
            false,
        ) {
            self.draw_control_text(bus, abs_top, abs_left, abs_bottom, abs_right, title);
            if hilite == 255 {
                self.dim_rect(bus, abs_top, abs_left, abs_bottom, abs_right);
            }
            return;
        }

        // Standard CDEF uses FrameRoundRect with oval 10x10.
        // Macintosh Revealed Volume One, p. 412.
        let oval = crate::control_manager::STANDARD_BUTTON_OVAL;

        // Erase first so a re-draw (e.g. from SetCTitle) overwrites the old title
        // instead of overlapping the new one. Checkbox / radio CDEFs already do this.
        self.draw_round_rect(cpu, bus, r, oval, oval, ShapeOp::Erase);
        self.draw_round_rect(cpu, bus, r, oval, oval, ShapeOp::Frame);

        self.draw_control_text(bus, abs_top, abs_left, abs_bottom, abs_right, title);

        if hilite == 1 {
            // Pressed: invert the interior.
            let inv = Rect {
                top: r.top + 1,
                left: r.left + 1,
                bottom: r.bottom - 1,
                right: r.right - 1,
            };
            let inv_oval = (oval - 2).max(0);
            self.draw_round_rect(cpu, bus, &inv, inv_oval, inv_oval, ShapeOp::Invert);
        } else if hilite == 255 {
            // Inactive: dim by clearing every other pixel to white.
            self.dim_rect(bus, abs_top, abs_left, abs_bottom, abs_right);
        }
    }

    /// Draw centered text for a push button control using fb_draw_string.
    fn draw_control_text(
        &self,
        bus: &mut MacMemoryBus,
        abs_top: i16,
        abs_left: i16,
        abs_bottom: i16,
        abs_right: i16,
        title: &str,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let font_id = 0i16; // Chicago (system font)
        let font_size = 12i16;
        let metrics = get_font_metrics(font_id, font_size);
        let text_w = Self::fb_measure_string(title, font_id, font_size);
        let (text_x, text_y) = crate::control_manager::centered_control_label_origin(
            (abs_top, abs_left, abs_bottom, abs_right),
            text_w,
            metrics.ascent,
            metrics.descent,
        );
        if pixel_size == 8 {
            // Control drawing writes straight to the screen framebuffer, so
            // the ink has to be resolved against the colour table the screen
            // is actually displayed with. `fb_draw_string` picks its index
            // from the active GDevice table instead, and the two disagree
            // whenever a game installs its own palette: Dark Castle's
            // GDevice table calls index 1 black, while the live device CLUT
            // has index 1 as pure green, so every control title was painted
            // in a colour the button then swallowed.
            //
            // Imaging With QuickDraw 1994, p. 4-82: the Color Manager's
            // inverse table is derived from its own palette, which
            // low-level `cscSetEntries` palette animation does not update.
            // For a screen-backed draw the hardware CLUT is the authority.
            let ink = super::pict::closest_clut_index(0, 0, 0, &self.device_clut);
            Self::fb_draw_string_styled_index(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                text_x,
                text_y,
                title,
                font_id,
                font_size,
                0,
                ink,
            );
            return;
        }

        Self::fb_draw_string(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            text_x,
            text_y,
            title,
            font_id,
            font_size,
        );
    }

    pub(crate) fn draw_control_label_text(
        &self,
        bus: &mut MacMemoryBus,
        label_top: i16,
        label_left: i16,
        label_bottom: i16,
        label_right: i16,
        text_x: i16,
        text_y: i16,
        title: &str,
        font_id: i16,
        font_size: i16,
        inactive: bool,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        if inactive && pixel_size == 8 {
            // Control drawing writes directly to the screen framebuffer, so
            // resolve against the same live device CLUT used by screenshots
            // instead of TheGDevice, which games may leave on an offscreen
            // palette while redrawing dialogs.
            let ink = self.inactive_control_title_index();
            Self::fb_draw_string_styled_index(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                text_x,
                text_y,
                title,
                font_id,
                font_size,
                0,
                ink,
            );
            return;
        }

        Self::fb_draw_string(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            text_x,
            text_y,
            title,
            font_id,
            font_size,
        );
        if inactive {
            // MTE 1992 p. 5-13 describes inactive controls as dimmed; when
            // no indexed-gray device color is available, preserve the older
            // monochrome CDEF fallback by pattern-dimming only the title.
            self.dim_rect(bus, label_top, label_left, label_bottom, label_right);
        }
    }

    /// Draw the X mark inside a checkbox at screen coordinates.
    /// System 7.5.3 checkbox X spans the full interior diagonal (i=1..size-1),
    /// starting one pixel inside the box border on each side.
    /// Inside Macintosh Volume I, I-322 figure 27.
    fn draw_checkbox_x(
        &self,
        bus: &mut MacMemoryBus,
        box_top_screen: i16,
        box_left_screen: i16,
        box_size: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        crate::control_manager::for_each_standard_checkbox_mark_pixel(box_size, |x, y| {
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                box_left_screen + x,
                box_top_screen + y,
                true,
            );
        });
    }

    /// Dim a rectangular area with a 50% gray pattern (checkerboard).
    /// Used for inactive (hilite=255) controls.
    pub(crate) fn dim_rect(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        for y in top..bottom {
            if y < 0 || y >= screen_height {
                continue;
            }
            for x in left..right {
                if x < 0 || x >= screen_width {
                    continue;
                }
                // Checkerboard pattern: clear every other pixel to white
                if (x + y) & 1 == 0 {
                    Self::fb_set_pixel(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        x,
                        y,
                        false,
                    );
                }
            }
        }
    }

    /// Draw a scroll bar control.
    /// Inside Macintosh Volume I, I-332 (scroll bar CDEF)
    pub(crate) fn draw_scroll_bar(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        value: i16,
        min: i16,
        max: i16,
        hilite: u8,
    ) {
        // IM:I I-332 / MTE 1992 5-58..5-61 define scroll-bar CDEF state in
        // terms of orientation, value range, arrows, page regions, and the
        // scroll box. Theme providers can own those pixels without changing
        // ControlRecord fields, hit testing, or guest-visible part codes.
        if self.draw_theme_scrollbar_chrome(bus, top, left, bottom, right, value, min, max, hilite)
        {
            return;
        }

        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let is_vertical = (bottom - top) > (right - left);
        let is_inactive = hilite == 255 || min >= max;

        // Draw outer frame
        self.draw_rect_border(bus, top, left, bottom, right);

        if is_inactive {
            // Inactive: white fill, arrow box dividers, arrow shapes (no shadow/track/thumb)
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top + 1,
                left + 1,
                bottom - 1,
                right - 1,
                false,
            );
            if is_vertical {
                let arrow_h = 16i16.min(bottom - top);
                // Divider lines
                self.draw_hline(bus, top + arrow_h - 1, left, right);
                self.draw_hline(bus, bottom - arrow_h, left, right);
                // Arrow shapes (14x14 content, no inner shadow)
                self.draw_scroll_arrow_inactive(bus, top + 1, left + 1, 0); // up
                self.draw_scroll_arrow_inactive(bus, bottom - arrow_h + 1, left + 1, 1);
            // down
            } else {
                let arrow_w = 16i16.min(right - left);
                // Divider lines
                self.draw_vline(bus, left + arrow_w - 1, top, bottom);
                self.draw_vline(bus, right - arrow_w, top, bottom);
                // Arrow shapes
                self.draw_scroll_arrow_inactive(bus, top + 1, left + 1, 2); // left
                self.draw_scroll_arrow_inactive(bus, top + 1, right - arrow_w + 1, 3);
                // right
            }
            return;
        }

        if is_vertical {
            let arrow_h = 16i16.min(bottom - top);

            // Up arrow box
            self.draw_rect_border(bus, top, left, top + arrow_h, right);
            // White fill inside arrow box
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top + 1,
                left + 1,
                top + arrow_h - 1,
                right - 1,
                false,
            );
            // Inner shadow: right line at right-2, bottom line at top+arrow_h-2
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top + 1,
                right - 2,
                top + arrow_h - 1,
                right - 1,
                true,
            );
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top + arrow_h - 2,
                left + 1,
                top + arrow_h - 1,
                right - 1,
                true,
            );
            // Up arrow glyph
            self.draw_scroll_arrow(bus, top + 1, left + 1, 0); // up

            // Down arrow box
            let da_top = bottom - arrow_h;
            self.draw_rect_border(bus, da_top, left, bottom, right);
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                da_top + 1,
                left + 1,
                bottom - 1,
                right - 1,
                false,
            );
            // Inner shadow
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                da_top + 1,
                right - 2,
                bottom - 1,
                right - 1,
                true,
            );
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                bottom - 2,
                left + 1,
                bottom - 1,
                right - 1,
                true,
            );
            // Down arrow glyph
            self.draw_scroll_arrow(bus, da_top + 1, left + 1, 1); // down

            // Track area between arrows
            let track_top = top + arrow_h;
            let track_bottom = bottom - arrow_h;
            if track_top < track_bottom {
                // Fill track with ltGray pattern
                self.fill_scroll_track(bus, track_top, left + 1, track_bottom, right - 1);

                // Draw thumb (fixed 16px height)
                let track_len = track_bottom - track_top;
                let thumb_h = 16i16.min(track_len);
                let range = (max - min).max(1) as i32;
                let val_clamped = (value - min).max(0).min(max - min) as i32;
                let travel = (track_len - thumb_h) as i32;
                let thumb_top = track_top + ((val_clamped * travel) / range) as i16;
                let thumb_bottom = thumb_top + thumb_h;

                // White fill for thumb
                Self::fb_fill_rect(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    thumb_top,
                    left + 1,
                    thumb_bottom,
                    right - 1,
                    false,
                );
                // Thumb border
                self.draw_rect_border(bus, thumb_top, left, thumb_bottom, right);
                // Thumb inner right shadow
                Self::fb_fill_rect(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    thumb_top + 1,
                    right - 2,
                    thumb_bottom - 1,
                    right - 1,
                    true,
                );
                // Thumb grip lines (4 horizontal lines, 6px wide, centered)
                self.draw_thumb_grip_lines_v(bus, thumb_top, left, right);
            }
        } else {
            // Horizontal scroll bar
            let arrow_w = 16i16.min(right - left);

            // Left arrow box
            self.draw_rect_border(bus, top, left, bottom, left + arrow_w);
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top + 1,
                left + 1,
                bottom - 1,
                left + arrow_w - 1,
                false,
            );
            // Inner shadow
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top + 1,
                left + arrow_w - 2,
                bottom - 1,
                left + arrow_w - 1,
                true,
            );
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                bottom - 2,
                left + 1,
                bottom - 1,
                left + arrow_w - 1,
                true,
            );
            // Left arrow glyph
            self.draw_scroll_arrow(bus, top + 1, left + 1, 2); // left

            // Right arrow box
            let ra_left = right - arrow_w;
            self.draw_rect_border(bus, top, ra_left, bottom, right);
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top + 1,
                ra_left + 1,
                bottom - 1,
                right - 1,
                false,
            );
            // Inner shadow
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top + 1,
                right - 2,
                bottom - 1,
                right - 1,
                true,
            );
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                bottom - 2,
                ra_left + 1,
                bottom - 1,
                right - 1,
                true,
            );
            // Right arrow glyph
            self.draw_scroll_arrow(bus, top + 1, ra_left + 1, 3); // right

            // Track area between arrows
            let track_left = left + arrow_w;
            let track_right = right - arrow_w;
            if track_left < track_right {
                self.fill_scroll_track(bus, top + 1, track_left, bottom - 1, track_right);

                // Draw thumb (fixed 16px width)
                let track_len = track_right - track_left;
                let thumb_w = 16i16.min(track_len);
                let range = (max - min).max(1) as i32;
                let val_clamped = (value - min).max(0).min(max - min) as i32;
                let travel = (track_len - thumb_w) as i32;
                let thumb_left = track_left + ((val_clamped * travel) / range) as i16;
                let thumb_right = thumb_left + thumb_w;

                // White fill for thumb
                Self::fb_fill_rect(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    top + 1,
                    thumb_left,
                    bottom - 1,
                    thumb_right,
                    false,
                );
                // Thumb border
                self.draw_rect_border(bus, top, thumb_left, bottom, thumb_right);
                // Thumb inner bottom shadow only (no right shadow for horizontal)
                Self::fb_fill_rect(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    bottom - 2,
                    thumb_left + 1,
                    bottom - 1,
                    thumb_right - 1,
                    true,
                );
                // Thumb grip lines (4 vertical lines, 6px tall, centered)
                self.draw_thumb_grip_lines_h(bus, thumb_left, top, bottom);
            }
        }
    }

    /// Fill a scroll bar track area with the ltGray pattern ($88/$22).
    fn fill_scroll_track(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) {
        // ltGray pattern: $88 $22 $88 $22 $88 $22 $88 $22
        const LT_GRAY: [u8; 8] = [0x88, 0x22, 0x88, 0x22, 0x88, 0x22, 0x88, 0x22];
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        for y in top..bottom {
            if y < 0 || y >= screen_height {
                continue;
            }
            let pat_row = LT_GRAY[(y as u16 % 8) as usize];
            for x in left..right {
                if x < 0 || x >= screen_width {
                    continue;
                }
                let bit = 7 - (x as u16 % 8);
                let on = (pat_row >> bit) & 1 != 0;
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    x,
                    y,
                    on,
                );
            }
        }
    }

    /// Draw a scroll bar arrow glyph (outlined arrow with shaft).
    /// Direction: 0=up, 1=down, 2=left, 3=right
    /// content_top/content_left: top-left of the 13x13 content area inside the arrow box.
    fn draw_scroll_arrow(
        &self,
        bus: &mut MacMemoryBus,
        content_top: i16,
        content_left: i16,
        direction: u8,
    ) {
        // Up arrow pattern in a 13x13 content area
        // Each entry: (row, &[cols])
        const UP_ARROW: [(i16, &[i16]); 10] = [
            (2, &[6]),    // tip
            (3, &[5, 7]), // outline
            (4, &[4, 8]),
            (5, &[3, 9]),
            (6, &[2, 10]),
            (7, &[1, 2, 3, 4, 8, 9, 10, 11]), // arms
            (8, &[4, 8]),                     // shaft sides
            (9, &[4, 8]),
            (10, &[4, 8]),
            (11, &[4, 5, 6, 7, 8]), // shaft bottom
        ];

        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();

        for &(row, cols) in &UP_ARROW {
            for &col in cols {
                // Transform coordinates based on direction
                let (py, px) = match direction {
                    0 => (row, col),      // up: as-is
                    1 => (13 - row, col), // down: vertical flip
                    2 => (12 - col, row), // left: rotate CW
                    3 => (col, 13 - row), // right: rotate CCW + shift
                    _ => continue,
                };
                let sy = content_top + py;
                let sx = content_left + px;
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    sx,
                    sy,
                    true,
                );
            }
        }
    }

    /// Draw 4 horizontal grip lines on a vertical scroll bar thumb.
    fn draw_thumb_grip_lines_v(
        &self,
        bus: &mut MacMemoryBus,
        thumb_top: i16,
        box_left: i16,
        _box_right: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        // Grip lines at thumb_top + 5, 7, 9, 11
        // Each line: 6px wide, starting at box_left + 5
        let grip_left = box_left + 5;
        let grip_right = box_left + 11; // exclusive
        for &offset in &[5i16, 7, 9, 11] {
            let y = thumb_top + offset;
            for x in grip_left..grip_right {
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    x,
                    y,
                    true,
                );
            }
        }
    }

    /// Draw 4 vertical grip lines on a horizontal scroll bar thumb.
    fn draw_thumb_grip_lines_h(
        &self,
        bus: &mut MacMemoryBus,
        thumb_left: i16,
        box_top: i16,
        _box_bottom: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        // Grip lines at thumb_left + 5, 7, 9, 11
        // Each line: 6px tall, starting at box_top + 5
        let grip_top = box_top + 5;
        let grip_bottom = box_top + 11; // exclusive
        for &offset in &[5i16, 7, 9, 11] {
            let x = thumb_left + offset;
            for y in grip_top..grip_bottom {
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    x,
                    y,
                    true,
                );
            }
        }
    }

    /// Draw a horizontal line at the given y, from left to right-1 (screen coords).
    fn draw_hline(&self, bus: &mut MacMemoryBus, y: i16, left: i16, right: i16) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        for x in left..right {
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                x,
                y,
                true,
            );
        }
    }

    /// Draw a vertical line at the given x, from top to bottom-1 (screen coords).
    fn draw_vline(&self, bus: &mut MacMemoryBus, x: i16, top: i16, bottom: i16) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        for y in top..bottom {
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                x,
                y,
                true,
            );
        }
    }

    /// Draw an inactive scroll bar arrow glyph (14x14 content area, no inner shadow).
    /// Direction: 0=up, 1=down, 2=left, 3=right
    fn draw_scroll_arrow_inactive(
        &self,
        bus: &mut MacMemoryBus,
        content_top: i16,
        content_left: i16,
        direction: u8,
    ) {
        // Up/Down arrow pattern in a 14x14 content area
        const UP_ARROW_14: [(i16, &[i16]); 10] = [
            (2, &[6, 7]), // tip (2px wide)
            (3, &[5, 8]),
            (4, &[4, 9]),
            (5, &[3, 10]),
            (6, &[2, 11]),
            (7, &[1, 2, 3, 4, 9, 10, 11, 12]), // arms
            (8, &[4, 9]),                      // shaft sides
            (9, &[4, 9]),
            (10, &[4, 9]),
            (11, &[4, 5, 6, 7, 8, 9]), // shaft bottom
        ];

        // Left arrow pattern in 14x14 (derived from expected reference)
        const LEFT_ARROW_14: [(i16, &[i16]); 12] = [
            (1, &[6]),
            (2, &[5, 6]),
            (3, &[4, 6]),
            (4, &[3, 6, 7, 8, 9, 10]), // arrowhead + shaft top
            (5, &[2, 10]),
            (6, &[1, 10]), // tip
            (7, &[1, 10]),
            (8, &[2, 10]),
            (9, &[3, 6, 7, 8, 9, 10]), // arrowhead + shaft bottom
            (10, &[4, 6]),
            (11, &[5, 6]),
            (12, &[6]),
        ];

        // Right arrow = left arrow mirrored: col → 13-col
        const RIGHT_ARROW_14: [(i16, &[i16]); 12] = [
            (1, &[7]),
            (2, &[7, 8]),
            (3, &[7, 9]),
            (4, &[3, 4, 5, 6, 7, 10]),
            (5, &[3, 11]),
            (6, &[3, 12]), // tip
            (7, &[3, 12]),
            (8, &[3, 11]),
            (9, &[3, 4, 5, 6, 7, 10]),
            (10, &[7, 9]),
            (11, &[7, 8]),
            (12, &[7]),
        ];

        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();

        match direction {
            0 | 1 => {
                for &(row, cols) in &UP_ARROW_14 {
                    for &col in cols {
                        let (py, px) = if direction == 0 {
                            (row, col)
                        } else {
                            (13 - row, col)
                        };
                        let sy = content_top + py;
                        let sx = content_left + px;
                        Self::fb_set_pixel(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            sx,
                            sy,
                            true,
                        );
                    }
                }
            }
            2 => {
                for &(row, cols) in &LEFT_ARROW_14 {
                    for &col in cols {
                        let sy = content_top + row;
                        let sx = content_left + col;
                        Self::fb_set_pixel(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            sx,
                            sy,
                            true,
                        );
                    }
                }
            }
            3 => {
                for &(row, cols) in &RIGHT_ARROW_14 {
                    for &col in cols {
                        let sy = content_top + row;
                        let sx = content_left + col;
                        Self::fb_set_pixel(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            sx,
                            sy,
                            true,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    /// Read a Pascal `BOOLEAN` parameter from its two-byte stack slot.
    ///
    /// Inside Macintosh Volume I, I-331 declares parameters such as
    /// `NewControl`'s `visible` as `BOOLEAN`, which is one byte, but the 68K
    /// stack keeps word alignment so the caller pushes two. Compilers differ
    /// on which half they write and leave the other undefined: MPW C leaves
    /// the flag in the high byte, while Dark Castle's build writes it in the
    /// low byte and zeroes the high one (`$00FF` for TRUE). Reading a fixed
    /// half therefore misreads one of the two — taking the high byte makes
    /// every Dark Castle control invisible, so its Play/Demo/Quit buttons and
    /// Novice/Beginner/Intermediate/Advanced radios never draw.
    ///
    /// A `BOOLEAN` is false only when it is zero, so treat the slot as true
    /// when either byte is set. That is exactly the set of values a caller
    /// can mean by TRUE, and it leaves the high-byte convention unchanged.
    ///
    /// Regression coverage:
    ///   src/trap/control.rs::tests::newcontrol_accepts_a_visible_flag_in_either_byte
    fn read_pascal_boolean(bus: &MacMemoryBus, addr: u32) -> bool {
        bus.read_word(addr) != 0
    }

    pub(crate) fn dispatch_control<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        Some(match (is_tool, trap_num) {
            // NewControl ($A954)
            // Allocates a new control and adds it to the specified window.
            // FUNCTION NewControl(theWindow: WindowPtr; boundsRect: Rect; title: Str255;
            //   visible: BOOLEAN; value: INTEGER; min, max: INTEGER; procID: INTEGER;
            //   refCon: LONGINT): ControlHandle;
            // Inside Macintosh Volume I, I-331
            // NewControl ($A954): Initializes ControlRecord fields from stack parameters, stores title/refCon/default CDEF handle, prepends handle into window's wControlList per IM:I I-317/I-331
            (true, 0x154) => {
                let sp = cpu.read_reg(Register::A7);
                let window_ptr = bus.read_long(sp + 22);
                let bounds_ptr = bus.read_long(sp + 18);
                let title_ptr = bus.read_long(sp + 14);
                let visible = Self::read_pascal_boolean(bus, sp + 12);
                let value = bus.read_word(sp + 10) as i16;
                let min = bus.read_word(sp + 8) as i16;
                let max = bus.read_word(sp + 6) as i16;
                let proc_id = bus.read_word(sp + 4) as i16;
                let ref_con = bus.read_long(sp);

                let bounds = if bounds_ptr != 0 {
                    (
                        bus.read_word(bounds_ptr) as i16,
                        bus.read_word(bounds_ptr + 2) as i16,
                        bus.read_word(bounds_ptr + 4) as i16,
                        bus.read_word(bounds_ptr + 6) as i16,
                    )
                } else {
                    (0, 0, 0, 0)
                };
                let title = Self::read_pascal_string(bus, title_ptr);

                let (handle, ctrl_ptr) = self.create_control_record(
                    bus, window_ptr, bounds, &title, visible, value, min, max, proc_id, ref_con,
                );

                let is_application_cdef = self.control_uses_application_def_proc(bus, ctrl_ptr);
                // On a real Mac, NewControl initializes a custom CDEF and,
                // when visible, immediately asks it to draw the whole control.
                // Inside Macintosh Volume I, I-329..I-331.
                if visible && !is_application_cdef {
                    self.draw_control(cpu, bus, ctrl_ptr);
                }

                bus.write_long(sp + 26, handle);
                cpu.write_reg(Register::A7, sp + 26);
                if is_application_cdef {
                    let mut messages = vec![(Self::CDEF_INIT_CNTL_MSG, 0, None)];
                    if visible {
                        messages.push((Self::CDEF_DRAW_CNTL_MSG, 0, None));
                    }
                    self.arm_control_def_messages(cpu, bus, handle, &messages);
                }
                Ok(())
            }

            // DisposeControl ($A955)
            // Removes a control from its window's control list and
            // releases the memory it occupies.
            // PROCEDURE DisposeControl(theControl: ControlHandle);
            // Inside Macintosh Volume I, I-332
            //
            // DisposeControl ($A955): Reads contrlOwner at offset 4, unlinks handle from window chain, frees memory per IM:I I-332
            (true, 0x155) => {
                let sp = cpu.read_reg(Register::A7);
                let ctrl_handle = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);
                self.dispose_control_handle(bus, ctrl_handle);
                Ok(())
            }

            // KillControls ($A956)
            // Disposes of all controls belonging to the specified window.
            // Walks the window's control chain and frees each handle.
            // PROCEDURE KillControls(theWindow: WindowPtr);
            // Inside Macintosh Volume I, I-332
            // KillControls ($A956): Walks + frees all controls in window per IM:I I-332
            (true, 0x156) => {
                let sp = cpu.read_reg(Register::A7);
                let window_ptr = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);
                if window_ptr != 0 {
                    // Walk the control list starting at window+140
                    let mut ctrl_handle = bus.read_long(window_ptr + 140);
                    while ctrl_handle != 0 {
                        let ctrl_ptr = bus.read_long(ctrl_handle);
                        if ctrl_ptr == 0 {
                            bus.free(ctrl_handle);
                            break;
                        }
                        let next = bus.read_long(ctrl_ptr);
                        self.release_control_aux_record(bus, ctrl_handle);
                        self.control_proc_ids.remove(&ctrl_ptr);
                        bus.free(ctrl_ptr);
                        bus.free(ctrl_handle);
                        ctrl_handle = next;
                    }
                    // Nil out the window's control list head
                    bus.write_long(window_ptr + 140, 0);
                }
                Ok(())
            }

            // ShowControl ($A957)
            // Makes the given control visible by setting contrlVis to 255.
            // PROCEDURE ShowControl(theControl: ControlHandle);
            // Inside Macintosh Volume I, I-328
            // ShowControl ($A957): Makes an actually hidden control visible
            // and asks its definition procedure to draw the whole control.
            (true, 0x157) => {
                let sp = cpu.read_reg(Register::A7);
                let ctrl_handle = bus.read_long(sp);
                let mut newly_visible_control = None;
                if ctrl_handle != 0 {
                    let ctrl_ptr = bus.read_long(ctrl_handle);
                    if ctrl_ptr != 0 {
                        let was_visible =
                            Self::control_vis_is_visible(bus.read_byte(ctrl_ptr + 16));
                        bus.write_byte(ctrl_ptr + 16, 255); // contrlVis = visible
                        if !was_visible {
                            newly_visible_control = Some(ctrl_ptr);
                        }
                    }
                }
                cpu.write_reg(Register::A7, sp + 4);
                if let Some(ctrl_ptr) = newly_visible_control {
                    if !self.arm_control_def_messages(
                        cpu,
                        bus,
                        ctrl_handle,
                        &[(Self::CDEF_DRAW_CNTL_MSG, 0, None)],
                    ) {
                        self.draw_control(cpu, bus, ctrl_ptr);
                    }
                }
                Ok(())
            }

            // HideControl ($A958)
            // Makes the given control invisible by setting contrlVis to 0.
            // PROCEDURE HideControl(theControl: ControlHandle);
            // Inside Macintosh Volume I, I-328
            // HideControl ($A958): Sets contrlVis byte to 0 (invisible)
            (true, 0x158) => {
                let sp = cpu.read_reg(Register::A7);
                let ctrl_handle = bus.read_long(sp);
                if ctrl_handle != 0 {
                    let ctrl_ptr = bus.read_long(ctrl_handle);
                    if ctrl_ptr != 0 {
                        bus.write_byte(ctrl_ptr + 16, 0); // contrlVis = invisible
                    }
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // MoveControl ($A959)
            // Moves a control to the specified position in local coordinates.
            // The top left corner of contrlRect moves to (h, v); the size
            // is preserved.
            // PROCEDURE MoveControl(theControl: ControlHandle; h, v: INTEGER);
            // Inside Macintosh Volume I, I-329
            //
            // Pascal pushes theControl first, then h, then v — so v is at SP+0,
            // h at SP+2, theControl at SP+4.
            // MoveControl ($A959): Moves contrlRect to (h,v) preserving size per IM:I I-329
            (true, 0x159) => {
                let sp = cpu.read_reg(Register::A7);
                let v = bus.read_word(sp) as i16;
                let h = bus.read_word(sp + 2) as i16;
                let ctrl_handle = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);
                if ctrl_handle != 0 {
                    let ctrl_ptr = bus.read_long(ctrl_handle);
                    if ctrl_ptr != 0 {
                        // contrlRect is at offset 8: top(2), left(2), bottom(2), right(2)
                        let old_top = bus.read_word(ctrl_ptr + 8) as i16;
                        let old_left = bus.read_word(ctrl_ptr + 10) as i16;
                        let old_bottom = bus.read_word(ctrl_ptr + 12) as i16;
                        let old_right = bus.read_word(ctrl_ptr + 14) as i16;
                        let width = old_right - old_left;
                        let height = old_bottom - old_top;

                        // Erase the control's CURRENT screen rect before moving it. Per
                        // IM:I I-329 (and ROM _MoveControl source) this is implemented as
                        // HideControl + update contrlRect + ShowControl; HideControl
                        // invalidates and erases the old location.
                        let owner_window = bus.read_long(ctrl_ptr + 4);
                        if owner_window != 0 {
                            let (scr_top, scr_left, _, _) =
                                Self::dialog_screen_bounds(bus, owner_window);
                            let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
                                self.get_screen_params();
                            Self::fb_fill_rect(
                                bus,
                                screen_base,
                                row_bytes,
                                pixel_size,
                                screen_width,
                                screen_height,
                                scr_top + old_top,
                                scr_left + old_left,
                                scr_top + old_bottom,
                                scr_left + old_right,
                                false, // erase to white
                            );
                        }

                        bus.write_word(ctrl_ptr + 8, v as u16);
                        bus.write_word(ctrl_ptr + 10, h as u16);
                        bus.write_word(ctrl_ptr + 12, (v + height) as u16);
                        bus.write_word(ctrl_ptr + 14, (h + width) as u16);
                        self.sync_dialog_item_rect_for_control(bus, ctrl_handle);
                    }
                }
                Ok(())
            }

            // GetCRefCon ($A95A)
            // Returns the reference constant for the given control.
            // FUNCTION GetCRefCon(theControl: ControlHandle): LONGINT;
            // Inside Macintosh Volume I, I-330
            // GetCRefCon ($A95A): Returns ControlRecord `contrlRfCon`
            (true, 0x15A) => {
                let sp = cpu.read_reg(Register::A7);
                let ctrl_handle = bus.read_long(sp);
                let ref_con = Self::control_record_ptr(bus, ctrl_handle)
                    .checked_add(36)
                    .map(|addr| bus.read_long(addr))
                    .unwrap_or(0);
                bus.write_long(sp + 4, ref_con);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SetCRefCon ($A95B)
            // Sets the reference constant for the given control.
            // PROCEDURE SetCRefCon(theControl: ControlHandle; data: LONGINT);
            // Inside Macintosh Volume I, I-330
            // SetCRefCon ($A95B): Stores ControlRecord `contrlRfCon`
            (true, 0x15B) => {
                let sp = cpu.read_reg(Register::A7);
                let ref_con = bus.read_long(sp);
                let ctrl_handle = bus.read_long(sp + 4);
                let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
                if ctrl_ptr != 0 {
                    bus.write_long(ctrl_ptr + 36, ref_con);
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // SizeControl ($A95C)
            // Changes the size of a control.
            // PROCEDURE SizeControl(theControl: ControlHandle; w, h: INTEGER);
            // Inside Macintosh Volume I, I-329
            // SizeControl ($A95C): Updates control rect width/height in place; does not redraw
            (true, 0x15C) => {
                let sp = cpu.read_reg(Register::A7);
                let height = bus.read_word(sp) as i16;
                let width = bus.read_word(sp + 2) as i16;
                let ctrl_handle = bus.read_long(sp + 4);
                let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
                if ctrl_ptr != 0 {
                    let top = bus.read_word(ctrl_ptr + 8) as i16;
                    let left = bus.read_word(ctrl_ptr + 10) as i16;
                    bus.write_word(ctrl_ptr + 12, top.wrapping_add(height) as u16);
                    bus.write_word(ctrl_ptr + 14, left.wrapping_add(width) as u16);
                    self.sync_dialog_item_rect_for_control(bus, ctrl_handle);
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // HiliteControl ($A95D)
            // Highlights a control as specified by hiliteState.
            // PROCEDURE HiliteControl(theControl: ControlHandle; hiliteState: INTEGER);
            // Macintosh Toolbox Essentials 1992, p. 5-98
            // HiliteControl ($A95D): Updates ControlRecord `contrlHilite`
            // and redraws through the control definition path.
            (true, 0x15D) => {
                let sp = cpu.read_reg(Register::A7);
                let hilite_state = bus.read_word(sp) as u8;
                let ctrl_handle = bus.read_long(sp + 2);
                let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
                cpu.write_reg(Register::A7, sp + 6);
                if ctrl_ptr != 0 {
                    bus.write_byte(ctrl_ptr + 17, hilite_state);
                    if trace_controls_enabled() {
                        let dialog_item = self
                            .dialog_control_handles
                            .get(&ctrl_handle)
                            .map(|(dlg, item)| format!(" dialog=${dlg:08X} item={item}"))
                            .unwrap_or_default();
                        eprintln!(
                            "[CONTROL] HiliteControl state={}{} {}",
                            hilite_state,
                            dialog_item,
                            self.control_trace_control_fields(bus, ctrl_handle),
                        );
                    }
                    if !self.arm_control_def_messages(
                        cpu,
                        bus,
                        ctrl_handle,
                        &[(Self::CDEF_DRAW_CNTL_MSG, 0, None)],
                    ) {
                        self.draw_control(cpu, bus, ctrl_ptr);
                    }
                } else if trace_controls_enabled() {
                    eprintln!(
                        "[CONTROL] HiliteControl state={} {}",
                        hilite_state,
                        self.control_trace_control_fields(bus, ctrl_handle),
                    );
                }
                Ok(())
            }

            // GetCTitle ($A95E)
            // Returns the title of the given control.
            // PROCEDURE GetCTitle(theControl: ControlHandle; VAR title: Str255);
            // Inside Macintosh Volume I, I-327
            // GetCTitle ($A95E): Copies Pascal title from ControlRecord
            (true, 0x15E) => {
                let sp = cpu.read_reg(Register::A7);
                let title_ptr = bus.read_long(sp);
                let ctrl_handle = bus.read_long(sp + 4);
                let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
                let title = if ctrl_ptr != 0 {
                    Self::control_title_bytes(bus, ctrl_ptr)
                } else {
                    Vec::new()
                };
                Self::write_pascal_string(bus, title_ptr, &title);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // SetCTitle ($A95F)
            // Sets the title of the given control and redraws it.
            // PROCEDURE SetCTitle(theControl: ControlHandle; title: Str255);
            // Inside Macintosh Volume I, I-327
            // SetCTitle ($A95F): Stores Pascal title in ControlRecord and redraws control
            (true, 0x15F) => {
                let sp = cpu.read_reg(Register::A7);
                let title_ptr = bus.read_long(sp);
                let ctrl_handle = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);
                let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
                if ctrl_ptr != 0 {
                    let title = Self::read_pascal_string(bus, title_ptr);
                    Self::set_control_title_bytes(bus, ctrl_ptr, &title);
                    if !self.arm_control_def_messages(
                        cpu,
                        bus,
                        ctrl_handle,
                        &[(Self::CDEF_DRAW_CNTL_MSG, 0, None)],
                    ) {
                        self.draw_control(cpu, bus, ctrl_ptr);
                    }
                }
                Ok(())
            }

            // GetCtlValue ($A960)
            // Returns the current value of a control.
            // FUNCTION GetCtlValue(theControl: ControlHandle): INTEGER;
            // Inside Macintosh Volume I, I-327
            // GetCtlValue ($A960): Routes through dialog control handles; falls back to ControlRecord offset 18
            (true, 0x160) => {
                let sp = cpu.read_reg(Register::A7);
                let ctrl_handle = bus.read_long(sp);
                let value = if let Some((dlg_ptr, item_no)) =
                    self.dialog_control_handles.get(&ctrl_handle).copied()
                {
                    self.dialog_control_values
                        .get(&(dlg_ptr, item_no))
                        .copied()
                        .unwrap_or(0)
                } else if ctrl_handle != 0 {
                    let ctrl_ptr = bus.read_long(ctrl_handle);
                    if ctrl_ptr != 0 {
                        bus.read_word(ctrl_ptr + 18) as i16
                    } else {
                        0
                    }
                } else {
                    0
                };
                bus.write_word(sp + 4, value as u16);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SetCtlValue ($A963)
            // Sets the current value of a control, clamped to
            // [contrlMin, contrlMax].
            // PROCEDURE SetCtlValue(theControl: ControlHandle; theValue: INTEGER);
            // Inside Macintosh Volume I, I-328
            // SetCtlValue ($A963): Routes through dialog control handles; writes to ControlRecord offset 18
            (true, 0x163) => {
                let sp = cpu.read_reg(Register::A7);
                let requested = bus.read_word(sp) as i16;
                let ctrl_handle = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);

                if ctrl_handle == 0 {
                    return Some(Ok(()));
                }
                let ctrl_ptr = bus.read_long(ctrl_handle);
                if ctrl_ptr == 0 {
                    return Some(Ok(()));
                }
                let min = bus.read_word(ctrl_ptr + 20) as i16;
                let max = bus.read_word(ctrl_ptr + 22) as i16;
                let proc_id = self.control_proc_ids.get(&ctrl_ptr).copied().unwrap_or(0);
                // If min > max the control is degenerate — IM doesn't
                // define the behaviour. Be conservative: take the
                // requested value unchanged in that case.
                let value = if Self::is_popup_menu_proc_id(proc_id) {
                    requested
                } else if min <= max {
                    requested.clamp(min, max)
                } else {
                    requested
                };
                if let Some((dlg_ptr, item_no)) =
                    self.dialog_control_handles.get(&ctrl_handle).copied()
                {
                    self.dialog_control_values.insert((dlg_ptr, item_no), value);
                }
                bus.write_word(ctrl_ptr + 18, value as u16);
                if trace_controls_enabled() {
                    let dialog_item = self
                        .dialog_control_handles
                        .get(&ctrl_handle)
                        .map(|(dlg, item)| format!(" dialog=${dlg:08X} item={item}"))
                        .unwrap_or_default();
                    eprintln!(
                        "[CONTROL] SetCtlValue requested={} value={}{} {}",
                        requested,
                        value,
                        dialog_item,
                        self.control_trace_control_fields(bus, ctrl_handle),
                    );
                }
                let owner = bus.read_long(ctrl_ptr + 4);
                let drew_initial_dialog_controls = self.dialog_items.contains_key(&owner)
                    && !self.dialog_cdefs_initially_drawn.contains(&owner)
                    && self.arm_dialog_control_def_draws(cpu, bus, owner);
                if !drew_initial_dialog_controls
                    && !self.arm_control_def_messages(
                        cpu,
                        bus,
                        ctrl_handle,
                        &[(Self::CDEF_DRAW_CNTL_MSG, 0, None)],
                    )
                {
                    self.draw_control(cpu, bus, ctrl_ptr);
                }
                Ok(())
            }

            // GetCtlMin ($A961)
            // Returns the minimum setting of a control.
            // FUNCTION GetCtlMin(theControl: ControlHandle): INTEGER;
            // Inside Macintosh Volume I, I-327
            // GetCtlMin ($A961): Returns ControlRecord `contrlMin`
            (true, 0x161) => {
                let sp = cpu.read_reg(Register::A7);
                let ctrl_handle = bus.read_long(sp);
                let min = Self::control_record_ptr(bus, ctrl_handle)
                    .checked_add(20)
                    .map(|addr| bus.read_word(addr))
                    .unwrap_or(0);
                bus.write_word(sp + 4, min);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SetCtlMin ($A964)
            // Sets the minimum setting of a control. If the current
            // value is less than the new minimum, it's bumped up per
            // IM:I I-328 ("SetCtlValue is called to set the current
            // value to the new minimum").
            // PROCEDURE SetCtlMin(theControl: ControlHandle; minValue: INTEGER);
            // Inside Macintosh Volume I, I-328
            // SetCtlMin ($A964): Stores ControlRecord `contrlMin`
            (true, 0x164) => {
                let sp = cpu.read_reg(Register::A7);
                let min = bus.read_word(sp) as i16;
                let ctrl_handle = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);
                let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
                if ctrl_ptr != 0 {
                    bus.write_word(ctrl_ptr + 20, min as u16);
                    let proc_id = self.control_proc_ids.get(&ctrl_ptr).copied().unwrap_or(0);
                    let value = bus.read_word(ctrl_ptr + 18) as i16;
                    if !Self::is_popup_menu_proc_id(proc_id) && value < min {
                        bus.write_word(ctrl_ptr + 18, min as u16);
                        if let Some((dlg_ptr, item_no)) =
                            self.dialog_control_handles.get(&ctrl_handle).copied()
                        {
                            self.dialog_control_values.insert((dlg_ptr, item_no), min);
                        }
                        self.draw_control(cpu, bus, ctrl_ptr);
                    }
                }
                Ok(())
            }

            // GetCtlMax ($A962)
            // Returns the maximum setting of a control.
            // FUNCTION GetCtlMax(theControl: ControlHandle): INTEGER;
            // Inside Macintosh Volume I, I-327
            // GetCtlMax ($A962): Returns ControlRecord `contrlMax`
            (true, 0x162) => {
                let sp = cpu.read_reg(Register::A7);
                let ctrl_handle = bus.read_long(sp);
                let max = Self::control_record_ptr(bus, ctrl_handle)
                    .checked_add(22)
                    .map(|addr| bus.read_word(addr))
                    .unwrap_or(0);
                bus.write_word(sp + 4, max);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SetCtlMax ($A965)
            // Sets the maximum setting of a control. If the current
            // value is greater than the new maximum, it's pulled
            // down per IM:I I-328 (symmetric to SetCtlMin).
            // PROCEDURE SetCtlMax(theControl: ControlHandle; maxValue: INTEGER);
            // Inside Macintosh Volume I, I-328
            // SetCtlMax ($A965): Stores ControlRecord `contrlMax`
            (true, 0x165) => {
                let sp = cpu.read_reg(Register::A7);
                let max = bus.read_word(sp) as i16;
                let ctrl_handle = bus.read_long(sp + 2);
                cpu.write_reg(Register::A7, sp + 6);
                let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
                if ctrl_ptr != 0 {
                    bus.write_word(ctrl_ptr + 22, max as u16);
                    let proc_id = self.control_proc_ids.get(&ctrl_ptr).copied().unwrap_or(0);
                    let value = bus.read_word(ctrl_ptr + 18) as i16;
                    if !Self::is_popup_menu_proc_id(proc_id) && value > max {
                        bus.write_word(ctrl_ptr + 18, max as u16);
                        if let Some((dlg_ptr, item_no)) =
                            self.dialog_control_handles.get(&ctrl_handle).copied()
                        {
                            self.dialog_control_values.insert((dlg_ptr, item_no), max);
                        }
                        self.draw_control(cpu, bus, ctrl_ptr);
                    }
                }
                Ok(())
            }

            // TestControl ($A966)
            // Tests which part of a control the given point is in.
            // FUNCTION TestControl(theControl: ControlHandle; thePt: Point): INTEGER;
            // Inside Macintosh Volume I, I-325; Macintosh Toolbox Essentials
            // (1992), p. 5-93
            // TestControl ($A966): For the standard button-family controls
            // Systemless currently models, visible active push buttons return
            // inButton (10) while checkboxes/radio buttons return inCheckBox
            // (11). Invisible, inactive, or missed controls return 0.
            (true, 0x166) => {
                let sp = cpu.read_reg(Register::A7);
                let pt_v = bus.read_word(sp) as i16;
                let pt_h = bus.read_word(sp + 2) as i16;
                let ctrl_handle = bus.read_long(sp + 4);
                let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);

                let is_application_cdef =
                    ctrl_ptr != 0 && self.control_uses_application_def_proc(bus, ctrl_ptr);
                let mut part: u16 = 0;
                if ctrl_ptr != 0 {
                    let vis = bus.read_byte(ctrl_ptr + 16);
                    let hilite = bus.read_byte(ctrl_ptr + 17);
                    if Self::control_vis_is_visible(vis) && hilite != 255 {
                        let r_top = bus.read_word(ctrl_ptr + 8) as i16;
                        let r_left = bus.read_word(ctrl_ptr + 10) as i16;
                        let r_bottom = bus.read_word(ctrl_ptr + 12) as i16;
                        let r_right = bus.read_word(ctrl_ptr + 14) as i16;
                        if pt_v >= r_top && pt_v < r_bottom && pt_h >= r_left && pt_h < r_right {
                            part = match self.control_proc_ids.get(&ctrl_ptr).copied().unwrap_or(0)
                            {
                                16 => self.standard_scrollbar_testcontrol_part_code(
                                    bus, ctrl_ptr, pt_v, pt_h,
                                ),
                                _ => self.standard_testcontrol_part_code(ctrl_ptr),
                            };
                        }
                    }
                }

                bus.write_word(sp + 8, part);
                cpu.write_reg(Register::A7, sp + 8);
                if is_application_cdef {
                    let point = ((pt_v as u16 as u32) << 16) | u32::from(pt_h as u16);
                    self.arm_control_def_messages(
                        cpu,
                        bus,
                        ctrl_handle,
                        &[(Self::CDEF_TEST_CNTL_MSG, point, Some(sp + 8))],
                    );
                }
                Ok(())
            }

            // TrackControl ($A968)
            // Tracks the mouse and returns the part code where it was released.
            // FUNCTION TrackControl(theControl: ControlHandle; startPt: Point;
            //   actionProc: ProcPtr): INTEGER;
            // Macintosh Toolbox Essentials 1992, pp. 5-89 to 5-90
            // Stack: SP+0=actionProc(4), SP+4=startPt(4), SP+8=theControl(4), SP+12=result(2)
            // TrackControl ($A968): tracks popup controls and held-mouse
            // simple push/checkbox/radio controls across refires; preserves
            // the old immediate hit-test path when the mouse is already up.
            (true, 0x168) => {
                if self
                    .control_tracking
                    .as_ref()
                    .is_some_and(|tracking| tracking.scrollbar_action_proc != 0)
                {
                    if self
                        .control_tracking
                        .as_ref()
                        .is_some_and(|tracking| tracking.scrollbar_callback_pending)
                    {
                        if let Some(tracking) = self.control_tracking.as_mut() {
                            tracking.scrollbar_callback_pending = false;
                        }
                        return Some(Ok(()));
                    }

                    let current_part = self.scrollbar_tracking_part(bus);
                    if !self.control_tracking_button_down(bus) {
                        self.finish_scrollbar_control_tracking(cpu, bus, current_part);
                        return Some(Ok(()));
                    }

                    let tick = self.tick_count;
                    let callback_due = if let Some(tracking) = self.control_tracking.as_mut() {
                        tracking.scrollbar_idle_refires =
                            tracking.scrollbar_idle_refires.saturating_add(1);
                        if self.yield_for_ui {
                            tracking.scrollbar_idle_refires
                                >= Self::SCROLLBAR_ACTION_REPEAT_TICKS as u8
                        } else {
                            tick.wrapping_sub(tracking.scrollbar_last_action_tick)
                                >= Self::SCROLLBAR_ACTION_REPEAT_TICKS
                        }
                    } else {
                        false
                    };
                    let expected_part = self
                        .control_tracking
                        .as_ref()
                        .map(|tracking| tracking.scrollbar_part)
                        .unwrap_or(0);
                    if callback_due && current_part == expected_part {
                        let (ctrl_handle, action_proc, stack_ptr) = self
                            .control_tracking
                            .as_ref()
                            .map(|tracking| {
                                (
                                    tracking.ctrl_handle,
                                    tracking.scrollbar_action_proc,
                                    tracking.stack_ptr,
                                )
                            })
                            .unwrap_or((0, 0, 0));
                        let callback_return_pc = cpu.read_reg(Register::PC).wrapping_sub(2);
                        if self.arm_control_action_proc(
                            cpu,
                            bus,
                            ctrl_handle,
                            expected_part,
                            action_proc,
                            stack_ptr,
                            callback_return_pc,
                        ) {
                            if let Some(tracking) = self.control_tracking.as_mut() {
                                tracking.scrollbar_last_action_tick = tick;
                                tracking.scrollbar_idle_refires = 0;
                                tracking.scrollbar_callback_pending = true;
                            }
                        }
                    }
                    return Some(Ok(()));
                }

                if self.control_tracking.is_some() {
                    let popup_tracking = self
                        .control_tracking
                        .as_ref()
                        .map(|tracking| tracking.popup_tracking)
                        .unwrap_or(false);
                    if popup_tracking {
                        if self.control_tracking_button_down(bus) {
                            let ctrl_handle = self
                                .control_tracking
                                .as_ref()
                                .map(|tracking| tracking.ctrl_handle)
                                .unwrap_or(0);
                            let (mv, mh) = self.control_tracking_mouse_pos(bus);
                            let new_item = self.control_tracking_item_at_point(bus, mh, mv);
                            let old_item = self
                                .control_tracking
                                .as_ref()
                                .map(|tracking| tracking.highlighted_item)
                                .unwrap_or(0);
                            if new_item != old_item {
                                self.set_control_tracking_highlight(bus, new_item);
                            }
                            self.record_trackcontrol_input_trace(
                                bus,
                                "tracking_update",
                                ctrl_handle,
                                None,
                                0,
                                None,
                                Some(new_item),
                                if new_item > 0 {
                                    "popup_item_highlighted"
                                } else {
                                    "popup_no_item"
                                },
                            );
                        } else {
                            let selected_item = self
                                .control_tracking
                                .as_ref()
                                .map(|tracking| tracking.highlighted_item)
                                .unwrap_or(0);
                            self.finish_popup_control_tracking(cpu, bus, selected_item);
                        }
                    } else {
                        let ctrl_handle = self
                            .control_tracking
                            .as_ref()
                            .map(|tracking| tracking.ctrl_handle)
                            .unwrap_or(0);
                        let inside = self.simple_control_tracking_inside(bus);
                        if self.control_tracking_button_down(bus) {
                            let highlighted = self
                                .control_tracking
                                .as_ref()
                                .map(|tracking| tracking.simple_highlighted)
                                .unwrap_or(false);
                            if inside != highlighted {
                                self.redraw_simple_control_tracking_state(cpu, bus, inside);
                            }
                            self.record_trackcontrol_input_trace(
                                bus,
                                "tracking_update",
                                ctrl_handle,
                                None,
                                0,
                                self.control_tracking.as_ref().map(|tracking| {
                                    if inside {
                                        tracking.simple_part
                                    } else {
                                        0
                                    }
                                }),
                                None,
                                if inside {
                                    "simple_part_highlighted"
                                } else {
                                    "simple_no_part"
                                },
                            );
                        } else {
                            self.finish_simple_control_tracking(cpu, bus, inside);
                        }
                    }
                    return Some(Ok(()));
                }

                let sp = cpu.read_reg(Register::A7);
                let action_proc = bus.read_long(sp);
                let ctrl_handle = bus.read_long(sp + 8);
                let pt_v = bus.read_word(sp + 4) as i16;
                let pt_h = bus.read_word(sp + 6) as i16;

                let mut part: u16 = 0;
                let mut outcome = "no_control";
                if ctrl_handle != 0 {
                    let ctrl_ptr = bus.read_long(ctrl_handle);
                    if ctrl_ptr != 0 {
                        let vis = bus.read_byte(ctrl_ptr + 16);
                        let hilite = bus.read_byte(ctrl_ptr + 17);
                        outcome = "inactive_or_invisible";
                        if Self::control_vis_is_visible(vis) && hilite != 255 {
                            let r_top = bus.read_word(ctrl_ptr + 8) as i16;
                            let r_left = bus.read_word(ctrl_ptr + 10) as i16;
                            let r_bottom = bus.read_word(ctrl_ptr + 12) as i16;
                            let r_right = bus.read_word(ctrl_ptr + 14) as i16;
                            outcome = "outside_control";
                            if pt_v >= r_top && pt_v < r_bottom && pt_h >= r_left && pt_h < r_right
                            {
                                let proc_id =
                                    self.control_proc_ids.get(&ctrl_ptr).copied().unwrap_or(0);
                                if proc_id == 16 {
                                    part = self.standard_scrollbar_testcontrol_part_code(
                                        bus, ctrl_ptr, pt_v, pt_h,
                                    );
                                    outcome = if part == 0 {
                                        "scrollbar_no_part"
                                    } else {
                                        "scrollbar_part_hit"
                                    };
                                    self.record_trackcontrol_input_trace(
                                        bus,
                                        "start",
                                        ctrl_handle,
                                        Some((pt_v, pt_h)),
                                        action_proc,
                                        Some(part),
                                        None,
                                        outcome,
                                    );
                                    bus.write_word(sp + 12, part);
                                    if matches!(part, 20..=23)
                                        && action_proc != 0
                                        && action_proc <= bus.ram_size().saturating_sub(2)
                                    {
                                        let window_ptr = bus.read_long(ctrl_ptr + 4);
                                        let (scr_top, scr_left, _, _) =
                                            Self::dialog_screen_bounds(bus, window_ptr);
                                        let screen_rect = (
                                            scr_top + r_top,
                                            scr_left + r_left,
                                            scr_top + r_bottom,
                                            scr_left + r_right,
                                        );
                                        let callback_return_pc =
                                            cpu.read_reg(Register::PC).wrapping_sub(2);
                                        if self.arm_control_action_proc(
                                            cpu,
                                            bus,
                                            ctrl_handle,
                                            part,
                                            action_proc,
                                            sp,
                                            callback_return_pc,
                                        ) {
                                            self.control_tracking = Some(ControlTrackingState {
                                                ctrl_handle,
                                                ctrl_ptr,
                                                popup_tracking: false,
                                                active_menu: 0,
                                                highlighted_item: 0,
                                                saved_pixels: Vec::new(),
                                                dropdown_rect: (0, 0, 0, 0),
                                                simple_part: 0,
                                                simple_screen_rect: screen_rect,
                                                simple_highlighted: false,
                                                saved_hilite: hilite,
                                                stack_ptr: sp,
                                                scrollbar_action_proc: action_proc,
                                                scrollbar_part: part,
                                                scrollbar_last_action_tick: self.tick_count,
                                                scrollbar_idle_refires: 0,
                                                scrollbar_callback_pending: true,
                                            });
                                            return Some(Ok(()));
                                        }
                                    }
                                    cpu.write_reg(Register::A7, sp + 12);
                                    return Some(Ok(()));
                                }
                                if Self::is_popup_menu_proc_id(proc_id) {
                                    let menu_id = self.popup_control_menu_id(
                                        bus,
                                        ctrl_ptr,
                                        bus.read_word(ctrl_ptr + 20) as i16,
                                    );
                                    if let Some(menu_idx) =
                                        self.menus.iter().rposition(|menu| menu.id == menu_id)
                                    {
                                        let dropdown_rect = self
                                            .popup_control_dropdown_rect(bus, ctrl_ptr, menu_idx);
                                        let saved = self.save_dropdown_pixels(bus, dropdown_rect);
                                        self.control_tracking = Some(ControlTrackingState {
                                            ctrl_handle,
                                            ctrl_ptr,
                                            popup_tracking: true,
                                            active_menu: menu_idx,
                                            highlighted_item: 0,
                                            saved_pixels: saved,
                                            dropdown_rect,
                                            simple_part: 0,
                                            simple_screen_rect: (0, 0, 0, 0),
                                            simple_highlighted: false,
                                            saved_hilite: hilite,
                                            stack_ptr: sp,
                                            scrollbar_action_proc: 0,
                                            scrollbar_part: 0,
                                            scrollbar_last_action_tick: 0,
                                            scrollbar_idle_refires: 0,
                                            scrollbar_callback_pending: false,
                                        });
                                        self.draw_menu_dropdown(bus, menu_idx, dropdown_rect);
                                        self.record_trackcontrol_input_trace(
                                            bus,
                                            "start",
                                            ctrl_handle,
                                            Some((pt_v, pt_h)),
                                            action_proc,
                                            None,
                                            Some(0),
                                            "open_popup_tracking",
                                        );
                                        return Some(Ok(()));
                                    }
                                }

                                // IM:I I-323: TrackControl follows mouse
                                // movement, highlights simple controls, and
                                // returns the part code only after mouse-up.
                                // Preserve the old immediate path when the
                                // mouse is already up; scripted callers that
                                // model a real mouse-down take the refire path.
                                if self.input_state.mouse_button
                                    && action_proc == 0
                                    && matches!(proc_id, 0 | 1 | 2)
                                {
                                    let part = self.standard_testcontrol_part_code(ctrl_ptr);
                                    let window_ptr = bus.read_long(ctrl_ptr + 4);
                                    let (scr_top, scr_left, _, _) =
                                        Self::dialog_screen_bounds(bus, window_ptr);
                                    let screen_rect = (
                                        scr_top + r_top,
                                        scr_left + r_left,
                                        scr_top + r_bottom,
                                        scr_left + r_right,
                                    );
                                    self.control_tracking = Some(ControlTrackingState {
                                        ctrl_handle,
                                        ctrl_ptr,
                                        popup_tracking: false,
                                        active_menu: 0,
                                        highlighted_item: 0,
                                        saved_pixels: Vec::new(),
                                        dropdown_rect: (0, 0, 0, 0),
                                        simple_part: part,
                                        simple_screen_rect: screen_rect,
                                        simple_highlighted: false,
                                        saved_hilite: hilite,
                                        stack_ptr: sp,
                                        scrollbar_action_proc: 0,
                                        scrollbar_part: 0,
                                        scrollbar_last_action_tick: 0,
                                        scrollbar_idle_refires: 0,
                                        scrollbar_callback_pending: false,
                                    });
                                    self.redraw_simple_control_tracking_state(cpu, bus, true);
                                    self.record_trackcontrol_input_trace(
                                        bus,
                                        "start",
                                        ctrl_handle,
                                        Some((pt_v, pt_h)),
                                        action_proc,
                                        None,
                                        None,
                                        "simple_tracking_started",
                                    );
                                    return Some(Ok(()));
                                }

                                // Return inButton (10) for simple controls.
                                // Inside Macintosh Volume I, I-316
                                part = 10;
                                outcome = "visible_active_hit";
                            }
                        }
                    }
                }

                self.record_trackcontrol_input_trace(
                    bus,
                    "start",
                    ctrl_handle,
                    Some((pt_v, pt_h)),
                    action_proc,
                    Some(part),
                    None,
                    outcome,
                );
                bus.write_word(sp + 12, part);
                cpu.write_reg(Register::A7, sp + 12);
                Ok(())
            }

            // DrawControls ($A969)
            // Draws all the controls belonging to the specified window.
            // PROCEDURE DrawControls(theWindow: WindowPtr);
            // Inside Macintosh Volume I, I-333
            // DrawControls ($A969): Draws all controls in window's control list per IM:I I-333
            (true, 0x169) => {
                let sp = cpu.read_reg(Register::A7);
                let window_ptr = bus.read_long(sp);
                let mut cdef_draw_calls = Vec::new();

                if window_ptr != 0 {
                    // The shared Control Manager owns traversal and documented
                    // draw order; this adapter only reads live Handle fields
                    // and executes presentation or CDEF callbacks.
                    let controls = crate::control_manager::control_draw_order(
                        bus.read_long(window_ptr + 140),
                        |ctrl_handle| {
                            let ctrl_ptr = bus.read_long(ctrl_handle);
                            (ctrl_ptr != 0).then(|| bus.read_long(ctrl_ptr))
                        },
                    );
                    for ctrl_handle in controls {
                        let ctrl_ptr = bus.read_long(ctrl_handle);
                        if ctrl_ptr == 0 {
                            continue;
                        }
                        if self.control_uses_application_def_proc(bus, ctrl_ptr) {
                            if Self::control_vis_is_visible(bus.read_byte(ctrl_ptr + 16)) {
                                cdef_draw_calls.push((
                                    ctrl_handle,
                                    Self::CDEF_DRAW_CNTL_MSG,
                                    0,
                                    None,
                                ));
                            }
                            continue;
                        }
                        let proc_id = self.control_proc_ids.get(&ctrl_ptr).copied().unwrap_or(0);
                        if Self::is_popup_menu_proc_id(proc_id) {
                            // popupMenuProc needs special MENU resource lookup
                            let vis = bus.read_byte(ctrl_ptr + 16);
                            let hilite = bus.read_byte(ctrl_ptr + 17);
                            if Self::control_vis_is_visible(vis) {
                                let r_top = bus.read_word(ctrl_ptr + 8) as i16;
                                let r_left = bus.read_word(ctrl_ptr + 10) as i16;
                                let r_bottom = bus.read_word(ctrl_ptr + 12) as i16;
                                let r_right = bus.read_word(ctrl_ptr + 14) as i16;
                                let ctrl_value = bus.read_word(ctrl_ptr + 18) as i16;
                                let menu_id = self.popup_control_menu_id(
                                    bus,
                                    ctrl_ptr,
                                    bus.read_word(ctrl_ptr + 20) as i16,
                                );
                                let (scr_top, scr_left, _, _) =
                                    Self::dialog_screen_bounds(bus, window_ptr);
                                let abs_top = scr_top + r_top;
                                let abs_left = scr_left + r_left;
                                let abs_bottom = scr_top + r_bottom;
                                let abs_right = scr_left + r_right;
                                let selected = ctrl_value.max(1) as usize;
                                let item_title = self.popup_menu_item_title(bus, menu_id, selected);
                                self.draw_popup_control_with_state(
                                    bus,
                                    abs_top,
                                    abs_left,
                                    abs_bottom,
                                    abs_right,
                                    &item_title.unwrap_or_default(),
                                    hilite != 255,
                                    hilite == 1,
                                );
                            }
                        } else {
                            self.draw_control(cpu, bus, ctrl_ptr);
                        }
                    }
                }

                cpu.write_reg(Register::A7, sp + 4);
                self.arm_control_def_call_chain(cpu, bus, &cdef_draw_calls);
                Ok(())
            }

            // UpdtControl ($A953)
            // Redraws the window's controls whose rect intersects the
            // given update region.
            // PROCEDURE UpdtControl(theWindow: WindowPtr; updateRgn: RgnHandle);
            // Inside Macintosh Volume V, V-222
            //
            // Uses bbox-approx region semantics. A nil update region is a no-op;
            // a non-empty update region redraws only the intersecting visible controls.
            // UpdtControl ($A953): Walks the window's wControlList at +140, hit-tests each control's contrlRect against the bbox of the update region, invokes draw_control on each survivor; pops 8 bytes (updateRgn + theWindow). Region semantics are bbox-approximated since Systemless does not model arbitrary RgnHandle scanlines.
            (true, 0x153) => {
                let sp = cpu.read_reg(Register::A7);
                let update_rgn = bus.read_long(sp);
                let window_ptr = bus.read_long(sp + 4);
                cpu.write_reg(Register::A7, sp + 8);
                if trace_controls_enabled() {
                    eprintln!(
                        "[CONTROL] UpdtControl window=${:08X} updateRgn=${:08X}",
                        window_ptr, update_rgn
                    );
                }
                let window_limit = bus.ram_size().saturating_sub(140);
                // Treat stale or out-of-range window pointers as inert rather
                // than dereferencing arbitrary guest memory.
                if window_ptr == 0 || window_ptr > window_limit {
                    return Some(Ok(()));
                }
                if update_rgn == 0 {
                    return Some(Ok(()));
                }
                let update_rect = match Self::region_handle_rect(bus, update_rgn) {
                    Some(rect) => rect,
                    None => return Some(Ok(())),
                };
                let mut ctrl_handle = bus.read_long(window_ptr + 140);
                let mut controls: Vec<(u32, u32)> = Vec::new();
                while ctrl_handle != 0 {
                    let ctrl_ptr = bus.read_long(ctrl_handle);
                    if ctrl_ptr == 0 {
                        break;
                    }
                    controls.push((ctrl_handle, ctrl_ptr));
                    ctrl_handle = bus.read_long(ctrl_ptr);
                }
                let (ut, ul, ub, ur) = update_rect;
                let mut cdef_draw_calls = Vec::new();
                for &(ctrl_handle, ctrl_ptr) in controls.iter().rev() {
                    let ct = bus.read_word(ctrl_ptr + 8) as i16;
                    let cl = bus.read_word(ctrl_ptr + 10) as i16;
                    let cb = bus.read_word(ctrl_ptr + 12) as i16;
                    let cr = bus.read_word(ctrl_ptr + 14) as i16;
                    // Skip controls fully outside the update bbox.
                    if cb <= ut || cr <= ul || ct >= ub || cl >= ur {
                        continue;
                    }
                    if self.control_uses_application_def_proc(bus, ctrl_ptr) {
                        if Self::control_vis_is_visible(bus.read_byte(ctrl_ptr + 16)) {
                            cdef_draw_calls.push((ctrl_handle, Self::CDEF_DRAW_CNTL_MSG, 0, None));
                        }
                    } else {
                        self.draw_control(cpu, bus, ctrl_ptr);
                    }
                }
                self.arm_control_def_call_chain(cpu, bus, &cdef_draw_calls);
                Ok(())
            }

            // FindControl ($A96C)
            // Determines which control, if any, the given point is in.
            // FUNCTION FindControl(thePoint: Point; theWindow: WindowPtr;
            //   VAR whichControl: ControlHandle): INTEGER;
            // Inside Macintosh Volume I, I-334
            // Stack: SP+0=whichControl(4), SP+4=theWindow(4), SP+8=thePt(4), SP+12=result(2)
            // FindControl ($A96C): Walks window's control list, checks visibility and hilite, hit-tests point against contrlRect, returns part code and control handle
            (true, 0x16C) => {
                let sp = cpu.read_reg(Register::A7);
                let which_ctrl_ptr = bus.read_long(sp);
                let window_ptr = bus.read_long(sp + 4);
                let pt_v = bus.read_word(sp + 8) as i16;
                let pt_h = bus.read_word(sp + 10) as i16;

                let mut found_handle: u32 = 0;
                let mut found_part: u16 = 0;

                // Walk the control list starting at window+140 (wControlList)
                // Macintosh TB Essentials 1992, 4-67
                if window_ptr != 0 {
                    let mut ctrl_handle = bus.read_long(window_ptr + 140);
                    while ctrl_handle != 0 {
                        let ctrl_ptr = bus.read_long(ctrl_handle);
                        if ctrl_ptr == 0 {
                            break;
                        }

                        // Check visibility (offset 16; zero is invisible)
                        let vis = bus.read_byte(ctrl_ptr + 16);
                        // Check hilite (offset 17, 255 = inactive/disabled)
                        let hilite = bus.read_byte(ctrl_ptr + 17);

                        if Self::control_vis_is_visible(vis) && hilite != 255 {
                            // Check contrlRect (offset 8): top, left, bottom, right
                            let r_top = bus.read_word(ctrl_ptr + 8) as i16;
                            let r_left = bus.read_word(ctrl_ptr + 10) as i16;
                            let r_bottom = bus.read_word(ctrl_ptr + 12) as i16;
                            let r_right = bus.read_word(ctrl_ptr + 14) as i16;

                            if pt_v >= r_top && pt_v < r_bottom && pt_h >= r_left && pt_h < r_right
                            {
                                found_handle = ctrl_handle;
                                // Return part code 10 (kControlButtonPart) for
                                // simple button controls. The proper approach
                                // would call the CDEF's testCntl, but for HLE
                                // we return a generic hit indicator.
                                found_part = 10;
                                break;
                            }
                        }

                        // Follow the linked list: nextControl at offset 0
                        ctrl_handle = bus.read_long(ctrl_ptr);
                    }
                }

                if which_ctrl_ptr != 0 {
                    bus.write_long(which_ctrl_ptr, found_handle);
                }
                bus.write_word(sp + 12, found_part);
                cpu.write_reg(Register::A7, sp + 12);

                Ok(())
            }

            // GetNewControl ($A9BE)
            // Creates a control from a 'CNTL' resource.
            // FUNCTION GetNewControl(controlID: INTEGER; theWindow: WindowPtr): ControlHandle;
            // Inside Macintosh Volume I, I-332
            // Stack: SP+0=theWindow(4), SP+4=controlID(2), SP+6=result(4)
            // GetNewControl ($A9BE): Loads CNTL resource, parses all fields (bounds, value, vis, min, max, procID, refCon, title), allocates ControlRecord, links into window's control list
            (true, 0x1BE) => {
                let sp = cpu.read_reg(Register::A7);
                let window_ptr = bus.read_long(sp);
                let ctrl_id = bus.read_word(sp + 4) as i16;

                // IM:I I-321: if the CNTL template cannot be read, return NIL.
                let cntl_data = self
                    .find_or_load_resource_any(bus, *b"CNTL", ctrl_id)
                    .map(|(_, ptr)| ptr);

                let Some(rsrc_addr) = cntl_data else {
                    bus.write_long(sp + 6, 0);
                    cpu.write_reg(Register::A7, sp + 6);
                    return Some(Ok(()));
                };

                // Allocate ControlRecord (296 bytes: 40 fixed + 256 title)
                let ctrl_ptr = bus.alloc(296);
                let handle = bus.alloc(4);
                bus.write_long(handle, ctrl_ptr);

                // CNTL resource format (Macintosh TB Essentials 1992, 5-118):
                //   0: boundsRect (8 bytes: top, left, bottom, right)
                //   8: value (2 bytes)
                //  10: visibility (1 byte)
                //  11: fill (1 byte)
                //  12: max (2 bytes)
                //  14: min (2 bytes)
                //  16: procID (2 bytes)
                //  18: refCon (4 bytes)
                //  22: title (Pascal string)
                let r_top = bus.read_word(rsrc_addr);
                let r_left = bus.read_word(rsrc_addr + 2);
                let r_bottom = bus.read_word(rsrc_addr + 4);
                let r_right = bus.read_word(rsrc_addr + 6);
                let value = bus.read_word(rsrc_addr + 8);
                // CNTL visibility is stored as a BOOLEAN word (2 bytes),
                // not a single byte. $FFFF or $0100 = visible, $0000 = invisible.
                // Inside Macintosh Volume I, I-333
                let vis_word = bus.read_word(rsrc_addr + 10);
                let visibility: u8 = if vis_word != 0 { 255 } else { 0 };
                let max = bus.read_word(rsrc_addr + 12);
                let min = bus.read_word(rsrc_addr + 14);
                let proc_id = bus.read_word(rsrc_addr + 16) as i16;
                let ref_con = bus.read_long(rsrc_addr + 18);
                let title = Self::read_pascal_string(bus, rsrc_addr + 22);

                self.initialize_control_record(
                    bus,
                    ctrl_ptr,
                    window_ptr,
                    (r_top as i16, r_left as i16, r_bottom as i16, r_right as i16),
                    &title,
                    visibility == 255,
                    value as i16,
                    min as i16,
                    max as i16,
                    proc_id,
                    ref_con,
                );
                self.ensure_control_aux_record(bus, handle);
                if trace_controls_enabled() {
                    eprintln!(
                        "[CONTROL] GetNewControl id={} proc_id={} cdef_id={} title=\"{}\" rect=({},{}..{},{} ) window=${:08X}",
                        ctrl_id,
                        proc_id,
                        proc_id >> 4,
                        decode_mac_roman_for_render(&title),
                        r_top as i16,
                        r_left as i16,
                        r_bottom as i16,
                        r_right as i16,
                        window_ptr
                    );
                }

                // Link into window's control list at offset 140 (wControlList)
                // New controls go at the beginning of the list.
                // Inside Macintosh Volume I, I-331
                if window_ptr != 0 {
                    let old_head = bus.read_long(window_ptr + 140);
                    bus.write_long(ctrl_ptr, old_head); // nextControl = old head
                    bus.write_long(window_ptr + 140, handle); // window's controlList = new handle
                }

                bus.write_long(sp + 6, handle);
                cpu.write_reg(Register::A7, sp + 6);
                let mut messages = vec![(Self::CDEF_INIT_CNTL_MSG, 0, None)];
                if visibility != 0 {
                    messages.push((Self::CDEF_DRAW_CNTL_MSG, 0, None));
                }
                self.arm_control_def_messages(cpu, bus, handle, &messages);
                Ok(())
            }

            // SetCtlAction ($A96B)
            // Sets the action procedure for the given control.
            // PROCEDURE SetCtlAction(theControl: ControlHandle; actionProc: ProcPtr);
            // Inside Macintosh Volume I, I-336
            // SetCtlAction ($A96B): Stores ControlRecord `contrlAction`
            (true, 0x16B) => {
                let sp = cpu.read_reg(Register::A7);
                let action_proc = bus.read_long(sp);
                let ctrl_handle = bus.read_long(sp + 4);
                let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
                if ctrl_ptr != 0 {
                    bus.write_long(ctrl_ptr + 32, action_proc);
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // GetCtlAction ($A96A)
            // Returns the action procedure for the given control.
            // FUNCTION GetCtlAction(theControl: ControlHandle): ProcPtr;
            // Inside Macintosh Volume I, I-336
            // GetCtlAction ($A96A): Returns ControlRecord `contrlAction`
            (true, 0x16A) => {
                let sp = cpu.read_reg(Register::A7);
                let ctrl_handle = bus.read_long(sp);
                let action_proc = Self::control_record_ptr(bus, ctrl_handle)
                    .checked_add(32)
                    .map(|addr| bus.read_long(addr))
                    .unwrap_or(0);
                bus.write_long(sp + 4, action_proc);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // DragControl ($A967)
            // Drags a control, following the mouse until the button is released.
            // PROCEDURE DragControl(theControl: ControlHandle; startPt: Point;
            //   limitRect: Rect; slopRect: Rect; axis: INTEGER);
            // Inside Macintosh Volume I, I-335
            //
            // Pascal frame (Rect args by VALUE — pinned by the existing
            // inline contract test dragcontrol_does_not_mutate_control_record):
            //   sp+0   axis: INTEGER         (2)
            //   sp+2   slopRect: Rect        (8)
            //   sp+10  limitRect: Rect       (8)
            //   sp+18  startPt: Point        (4)
            //   sp+22  theControl: Handle    (4)
            // Total pop = 26 bytes.
            //
            // CONVENTION DIVERGENCE WARNING: IM:I-91
            // explicit PEA-sizeRect example documents Window-Manager Rect
            // args BY POINTER (4 bytes). The Window Manager interactive-
            // tracking family at src/trap/window.rs ($A925 DragWindow,
            // $A92B GrowWindow, $A926 DragTheRgn) AND $A905 DragGrayRgn
            // at src/trap/toolbox.rs all follow that convention (Rect by
            // ptr, 4 bytes per Rect). DragControl here is the ONLY
            // remaining "drag" trap that pops by-VALUE (8 bytes per
            // Rect). The 26-byte pop is pinned by an existing contract
            // test but is not backed by an IM citation — the IM:I I-432
            // "assembly summary" reference is to the Dialog Mgr Summary,
            // not a DragControl-specific assembly block. Cross-checking
            // against MPW Pascal compiler output for a real corpus
            // binary is the right way to resolve this. For now, the
            // 26-byte pop is preserved as the established behavior.
            //
            // HLE compromise: Systemless does not model interactive mouse
            // dragging — there is no `WaitMouseUp` event loop synthesizing
            // mouse-moved events that would feed back into a control-
            // tracking redraw cycle. The trap therefore returns without
            // moving the control or triggering any redraws. Apps that
            // call DragControl during user interaction (typically from
            // a TrackControl callback) will see the control unchanged.
            // The scripted corpus exercises controls via
            // direct CtlValue manipulation rather than drag flows, so
            // this no-op is appropriate.
            //
            // Regression coverage:
            //   tests::dragcontrol_pops_18_bytes_and_leaves_contrlrect_unchanged_on_no_drag_path
            // DragControl ($A967): The public MPW C declaration uses
            // pointer rect parameters, so the C ABI consumes 18 bytes
            // (theControl 4 + startPt 4 + limitRect* 4 + slopRect* 4
            // + axis 2). HLE has no interactive mouse drag loop so the
            // control is left in place.
            (true, 0x167) => {
                let sp = cpu.read_reg(Register::A7);
                cpu.write_reg(Register::A7, sp + 18);
                Ok(())
            }

            // SetCtlColor ($AA43)
            // PROCEDURE SetCtlColor(theControl: ControlHandle; newColorTable: CCTabHandle);
            // Macintosh Toolbox Essentials (1992), pp. 5-101 and 5-107.
            // Stack: SP+0: newColorTable(4), SP+4: theControl(4). Pop 8.
            // SetCtlColor ($AA43): Rewrites the tracked AuxCtlRec's acCTable
            // field in place. Fresh BasiliskII/System 7.5.3 controls already
            // carry AuxCtlRecs, but Systemless still allocates one on demand if a
            // control somehow reaches SetCtlColor without tracked aux state.
            (true, 0x243) => {
                let sp = cpu.read_reg(Register::A7);
                let color_table = bus.read_long(sp);
                let ctrl_handle = bus.read_long(sp + 4);
                if ctrl_handle != 0 {
                    let default_ctab = self.default_control_color_table_handle(bus);
                    let effective_ctab = if color_table != 0 {
                        color_table
                    } else {
                        default_ctab
                    };
                    let aux_handle = self.ensure_control_aux_record(bus, ctrl_handle);
                    let aux_ptr = bus.read_long(aux_handle);
                    if aux_ptr != 0 {
                        bus.write_long(aux_ptr + Self::AUX_CTL_OWNER_OFFSET, ctrl_handle);
                        bus.write_long(aux_ptr + Self::AUX_CTL_CTABLE_OFFSET, effective_ctab);
                        bus.write_word(aux_ptr + Self::AUX_CTL_FLAGS_OFFSET, 0);
                        bus.write_long(
                            aux_ptr + Self::AUX_CTL_RESERVED_OFFSET,
                            self.control_aux_reserved_value(bus, ctrl_handle),
                        );
                    }

                    let ctrl_ptr = Self::control_record_ptr(bus, ctrl_handle);
                    if ctrl_ptr != 0 && bus.read_byte(ctrl_ptr + 16) == 255 {
                        self.draw_control(cpu, bus, ctrl_ptr);
                    }
                }
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // Draw1Control ($A96D)
            // Draws a single control.
            // PROCEDURE Draw1Control(theControl: ControlHandle);
            // Inside Macintosh Volume I, I-326
            // Draw1Control ($A96D): Pops 4-byte handle (theControl), dereferences via *handle to ControlPtr, invokes draw_control on it (same code path DrawControls walks per-element). NIL handle is a graceful no-op. Per IM:I I-326. Popup buttons (procID 1008) use the popup-control renderer instead of falling through as a no-op, and the current GDevice is preserved across the redraw.
            (true, 0x16D) => {
                let sp = cpu.read_reg(Register::A7);
                let ctrl_handle = bus.read_long(sp);
                cpu.write_reg(Register::A7, sp + 4);
                let saved_gdevice = *self.current_gdevice;
                // Treat stale or out-of-range handles as inert rather than
                // dereferencing arbitrary guest memory. The control record's
                // Pascal title begins at offset 40, so we also reject handles
                // whose title byte or declared title payload would run off the
                // end of RAM before handing the record to draw_control.
                let ram_limit = bus.ram_size().saturating_sub(4);
                if ctrl_handle != 0 && ctrl_handle <= ram_limit {
                    let ctrl_ptr = bus.read_long(ctrl_handle);
                    let title_off = ctrl_ptr.saturating_add(40);
                    if ctrl_ptr != 0 && title_off < bus.ram_size() && title_off <= ram_limit {
                        let title_len = bus.read_byte(title_off) as u32;
                        let title_end = title_off.saturating_add(1).saturating_add(title_len);
                        if title_end <= bus.ram_size() {
                            if !self.arm_control_def_messages(
                                cpu,
                                bus,
                                ctrl_handle,
                                &[(Self::CDEF_DRAW_CNTL_MSG, 0, None)],
                            ) {
                                // popupMenuProc (1008) already has a direct
                                // renderer in draw_control, so Draw1Control can
                                // reuse the same control path as DrawControls.
                                self.draw_control(cpu, bus, ctrl_ptr);
                            }
                            debug_assert_eq!(*self.current_gdevice, saved_gdevice);
                        }
                    }
                }
                Ok(())
            }

            // GetCVariant ($A809)
            // Returns the variation code of a control.
            // FUNCTION GetCVariant(theControl: ControlHandle): INTEGER;
            // Inside Macintosh Volume V, V-222 (Macintosh Plus, SE, and II).
            //
            // The control definition ID is `16 * resourceID + variation_code`
            // (Inside Macintosh Volume I 1985, p. I-323 "Defining Your Own
            // Controls"): the upper 12 bits select the CDEF resource, the low
            // 4 bits select the variant. NewControl stores both into the
            // ControlRecord — the resolved CDEF handle into contrlDefProc
            // (offset 24) and historically the variation code into the
            // high-order byte of that field. Per IM:V V-222: "This value was
            // formerly stored in the high four bits of the control defproc
            // handle; for future compatibility, use the GetCVariant routine
            // to access this value."
            //
            // Systemless's NewControl / GetNewControl (toolbox.rs:1144 and
            // :1972) record the original procID in the side-table
            // `control_proc_ids: HashMap<ctrl_ptr, i16>`, indexed by the
            // dereferenced ControlPtr (i.e. *theControl). GetCVariant
            // recovers the variation code by reading procID from that side
            // table and masking the low 4 bits — the canonical IM:I I-323
            // definition. Same formula independent of CDEF resource ID:
            //   pushButProc=0   → variant 0
            //   checkBoxProc=1  → variant 1
            //   radioButProc=2  → variant 2
            //   scrollBarProc=16 → variant 0
            //   popupMenuProc=1008 + popupFixedWidth=1 → variant 1
            //
            // Stack layout (Pascal FUNCTION): caller pre-allocates 2-byte
            // INTEGER result slot at sp+4, pushes 4-byte ControlHandle arg at
            // sp+0. Trap reads arg, advances A7 by 4, writes INTEGER result
            // at the former sp+4. NIL theControl returns 0.
            //
            // MPW Universal Headers `Controls.h`:
            //   EXTERN_API(SInt16) GetControlVariant(ControlRef) ONEWORDINLINE(0xA809);
            //   #define GetCVariant(theControl) GetControlVariant(theControl)
            (true, 0x009) => {
                let sp = cpu.read_reg(Register::A7);
                let ctrl_handle = bus.read_long(sp);
                let variant: i16 = if ctrl_handle != 0 {
                    let ctrl_ptr = bus.read_long(ctrl_handle);
                    let proc_id = self.control_proc_ids.get(&ctrl_ptr).copied().unwrap_or(0);
                    proc_id & 0xF
                } else {
                    0
                };
                cpu.write_reg(Register::A7, sp + 4);
                bus.write_word(sp + 4, variant as u16);
                Ok(())
            }

            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::{setup, setup_with_port};
    use crate::cpu::{CpuOps, Register};
    use crate::memory::{MacMemoryBus, MemoryBus};
    use crate::trap::menu::{Menu, MenuItem};
    use crate::trap::TrapDispatcher;
    use crate::ui_theme::UiThemeId;

    fn build_cntl_resource(
        rect: (i16, i16, i16, i16),
        value: i16,
        visible: bool,
        min: i16,
        max: i16,
        proc_id: i16,
        ref_con: u32,
        title: &[u8],
    ) -> Vec<u8> {
        let mut data = Vec::with_capacity(23 + title.len());
        data.extend_from_slice(&(rect.0 as u16).to_be_bytes());
        data.extend_from_slice(&(rect.1 as u16).to_be_bytes());
        data.extend_from_slice(&(rect.2 as u16).to_be_bytes());
        data.extend_from_slice(&(rect.3 as u16).to_be_bytes());
        data.extend_from_slice(&(value as u16).to_be_bytes());
        data.extend_from_slice(&(if visible { 0xFFFFu16 } else { 0 }).to_be_bytes());
        data.extend_from_slice(&(max as u16).to_be_bytes());
        data.extend_from_slice(&(min as u16).to_be_bytes());
        data.extend_from_slice(&(proc_id as u16).to_be_bytes());
        data.extend_from_slice(&ref_con.to_be_bytes());
        let len = title.len().min(255);
        data.push(len as u8);
        data.extend_from_slice(&title[..len]);
        data
    }

    fn fill_rect_pict_resource(
        frame: (i16, i16, i16, i16),
        fill_rect: (i16, i16, i16, i16),
    ) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&0u16.to_be_bytes()); // picSize placeholder
        for value in [frame.0, frame.1, frame.2, frame.3] {
            data.extend_from_slice(&(value as u16).to_be_bytes());
        }
        data.push(0x11); // versionOp
        data.push(0x01); // PICT v1
        data.push(0x0A); // FillPat
        data.extend_from_slice(&[0xFF; 8]);
        data.push(0x34); // fillRect
        for value in [fill_rect.0, fill_rect.1, fill_rect.2, fill_rect.3] {
            data.extend_from_slice(&(value as u16).to_be_bytes());
        }
        data.push(0xFF); // EndOfPicture
        let size = data.len() as u16;
        data[0..2].copy_from_slice(&size.to_be_bytes());
        data
    }

    fn alloc_control_handle(
        bus: &mut MacMemoryBus,
        rect: (i16, i16, i16, i16),
        vis: u8,
        hilite: u8,
    ) -> (u32, u32) {
        let ctrl_ptr = bus.alloc(40);
        let ctrl_handle = bus.alloc(4);
        bus.write_long(ctrl_handle, ctrl_ptr);
        bus.write_word(ctrl_ptr + 8, rect.0 as u16);
        bus.write_word(ctrl_ptr + 10, rect.1 as u16);
        bus.write_word(ctrl_ptr + 12, rect.2 as u16);
        bus.write_word(ctrl_ptr + 14, rect.3 as u16);
        bus.write_byte(ctrl_ptr + 16, vis);
        bus.write_byte(ctrl_ptr + 17, hilite);
        (ctrl_handle, ctrl_ptr)
    }

    fn alloc_scrollbar_control(
        disp: &mut super::super::TrapDispatcher,
        bus: &mut MacMemoryBus,
        rect: (i16, i16, i16, i16),
        vis: u8,
        hilite: u8,
        value: i16,
        min: i16,
        max: i16,
    ) -> (u32, u32) {
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(bus, rect, vis, hilite);
        bus.write_word(ctrl_ptr + 18, value as u16);
        bus.write_word(ctrl_ptr + 20, min as u16);
        bus.write_word(ctrl_ptr + 22, max as u16);
        disp.control_proc_ids.insert(ctrl_ptr, 16);
        (ctrl_handle, ctrl_ptr)
    }

    fn new_control_handle<C: CpuOps>(
        disp: &mut TrapDispatcher,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        visible: bool,
        proc_id: i16,
    ) -> u32 {
        let sp = 0x300000;
        let bounds_ptr = bus.alloc(8);
        let title_ptr = bus.alloc(16);

        bus.write_word(bounds_ptr, 10);
        bus.write_word(bounds_ptr + 2, 20);
        bus.write_word(bounds_ptr + 4, 30);
        bus.write_word(bounds_ptr + 6, 80);
        bus.write_byte(title_ptr, 4);
        bus.write_bytes(title_ptr + 1, b"Aux!");

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x1234_5678);
        bus.write_word(sp + 4, proc_id as u16);
        bus.write_word(sp + 6, 99);
        bus.write_word(sp + 8, 1);
        bus.write_word(sp + 10, 7);
        bus.write_byte(sp + 12, if visible { 1 } else { 0 });
        bus.write_long(sp + 14, title_ptr);
        bus.write_long(sp + 18, bounds_ptr);
        bus.write_long(sp + 22, window_ptr);

        let result = disp.dispatch_control(true, 0x154, cpu, bus);
        assert!(result.unwrap().is_ok());
        bus.read_long(cpu.read_reg(Register::A7))
    }

    /// IM:I I-331 declares `NewControl`'s `visible` as a `BOOLEAN`, which is
    /// one byte in a word-aligned stack slot. MPW C writes the flag in the
    /// high byte; Dark Castle's build writes it in the low byte and zeroes
    /// the high one, so a fixed-half read makes every one of its controls
    /// invisible and none of them draw.
    #[test]
    fn newcontrol_accepts_a_visible_flag_in_either_byte() {
        for (label, word) in [
            ("high byte (MPW C)", 0xFF00u16),
            ("low byte (Dark Castle)", 0x00FFu16),
            ("both bytes", 0xFFFFu16),
        ] {
            let (mut disp, mut cpu, mut bus) = setup();
            let sp = 0x300000u32;
            let window_ptr = bus.alloc(200);
            let bounds_ptr = bus.alloc(8);
            for (i, v) in [276i16, 20, 292, 87].iter().enumerate() {
                bus.write_word(bounds_ptr + (i as u32) * 2, *v as u16);
            }
            let title_ptr = bus.alloc(16);
            bus.write_byte(title_ptr, 4);
            bus.write_bytes(title_ptr + 1, b"Play");

            cpu.write_reg(Register::A7, sp);
            bus.write_long(sp, 0);
            bus.write_word(sp + 4, 0); // pushButProc
            bus.write_word(sp + 6, 1);
            bus.write_word(sp + 8, 0);
            bus.write_word(sp + 10, 0);
            bus.write_word(sp + 12, word);
            bus.write_long(sp + 14, title_ptr);
            bus.write_long(sp + 18, bounds_ptr);
            bus.write_long(sp + 22, window_ptr);

            disp.dispatch_control(true, 0x154, &mut cpu, &mut bus)
                .expect("NewControl handled")
                .expect("NewControl returns");

            let handle = bus.read_long(sp + 26);
            let ctrl_ptr = bus.read_long(handle);
            assert_eq!(
                bus.read_byte(ctrl_ptr + 16),
                255,
                "contrlVis must be 255 for a TRUE visible flag in the {} slot",
                label
            );
        }

        // A genuinely FALSE flag stays invisible.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let bounds_ptr = bus.alloc(8);
        let title_ptr = bus.alloc(4);
        bus.write_byte(title_ptr, 0);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 1);
        bus.write_word(sp + 8, 0);
        bus.write_word(sp + 10, 0);
        bus.write_word(sp + 12, 0x0000);
        bus.write_long(sp + 14, title_ptr);
        bus.write_long(sp + 18, bounds_ptr);
        bus.write_long(sp + 22, 0);
        disp.dispatch_control(true, 0x154, &mut cpu, &mut bus)
            .expect("NewControl handled")
            .expect("NewControl returns");
        let handle = bus.read_long(sp + 26);
        let ctrl_ptr = bus.read_long(handle);
        assert_eq!(bus.read_byte(ctrl_ptr + 16), 0, "FALSE stays invisible");
    }

    fn make_control_ctab_handle(bus: &mut MacMemoryBus) -> u32 {
        let ctab_ptr = bus.alloc(8 + 4 * 8);
        let ctab_handle = bus.alloc(4);
        bus.write_long(ctab_handle, ctab_ptr);
        bus.write_long(ctab_ptr, 0); // ccSeed
        bus.write_word(ctab_ptr + 4, 0); // ccRider
        bus.write_word(ctab_ptr + 6, 3); // ctSize (4 ColorSpec entries)
        for (index, part_id) in [0u16, 1, 2, 3].iter().enumerate() {
            let entry = ctab_ptr + 8 + index as u32 * 8;
            bus.write_word(entry, *part_id);
            bus.write_word(entry + 2, 0x1111u16.wrapping_mul(index as u16 + 1));
            bus.write_word(entry + 4, 0x0101u16.wrapping_mul(index as u16 + 2));
            bus.write_word(entry + 6, 0x0202u16.wrapping_mul(index as u16 + 3));
        }
        ctab_handle
    }

    #[test]
    fn new_control_initializes_record_and_links_window() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000;
        let window_ptr = bus.alloc(200);
        let bounds_ptr = bus.alloc(8);
        let title_ptr = bus.alloc(16);

        bus.write_word(bounds_ptr, 10);
        bus.write_word(bounds_ptr + 2, 20);
        bus.write_word(bounds_ptr + 4, 30);
        bus.write_word(bounds_ptr + 6, 80);
        bus.write_byte(title_ptr, 4);
        bus.write_bytes(title_ptr + 1, b"Test");

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x1234_5678);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 99);
        bus.write_word(sp + 8, 1);
        bus.write_word(sp + 10, 7);
        // Pascal BOOLEAN at SP+12 — value goes in HIGH byte (MPW C convention).
        bus.write_byte(sp + 12, 1);
        bus.write_long(sp + 14, title_ptr);
        bus.write_long(sp + 18, bounds_ptr);
        bus.write_long(sp + 22, window_ptr);

        let result = disp.dispatch_control(true, 0x154, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let handle = bus.read_long(sp + 26);
        let ctrl_ptr = bus.read_long(handle);
        assert_ne!(handle, 0);
        assert_ne!(ctrl_ptr, 0);
        assert_eq!(bus.read_long(window_ptr + 140), handle);
        assert_eq!(bus.read_long(ctrl_ptr + 4), window_ptr);
        assert_eq!(bus.read_word(ctrl_ptr + 8), 10);
        assert_eq!(bus.read_word(ctrl_ptr + 10), 20);
        assert_eq!(bus.read_word(ctrl_ptr + 12), 30);
        assert_eq!(bus.read_word(ctrl_ptr + 14), 80);
        assert_eq!(bus.read_byte(ctrl_ptr + 16), 255);
        assert_eq!(bus.read_word(ctrl_ptr + 18), 7);
        assert_eq!(bus.read_word(ctrl_ptr + 20), 1);
        assert_eq!(bus.read_word(ctrl_ptr + 22), 99);
        assert_eq!(bus.read_long(ctrl_ptr + 36), 0x1234_5678);
        assert_eq!(bus.read_byte(ctrl_ptr + 40), 4);
        assert_eq!(bus.read_bytes(ctrl_ptr + 41, 4), b"Test");
        assert_eq!(cpu.read_reg(Register::A7), sp + 26);
    }

    #[test]
    fn newcontrol_creates_aux_record_and_getauxctl_returns_true() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_ptr = bus.alloc(200);
        let ctrl_handle = new_control_handle(&mut disp, &mut cpu, &mut bus, window_ptr, false, 0);

        let aux_state = disp
            .control_aux_state(ctrl_handle)
            .expect("fresh control should have an AuxCtlRec on BasiliskII System 7.5.3");
        let aux_ptr = bus.read_long(aux_state.handle);
        assert_ne!(aux_ptr, 0, "AuxCtlHandle must master a live AuxCtlRec");
        assert_eq!(
            bus.read_long(aux_ptr + 4),
            ctrl_handle,
            "acOwner must point back to the control handle"
        );
        assert_ne!(
            bus.read_long(aux_ptr + 8),
            0,
            "fresh controls should already carry a non-NIL acCTable"
        );
        assert_eq!(
            bus.read_long(0x0CD4),
            aux_state.handle,
            "AuxCtlHead low-memory global should point at the fresh control's aux record"
        );

        let aux_out = bus.alloc(4);
        let sp = 0x300100;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, aux_out);
        bus.write_long(sp + 4, ctrl_handle);
        bus.write_word(sp + 8, 0xFFFF);

        let result = disp.dispatch_quickdraw(true, 0x244, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(
            bus.read_long(aux_out),
            aux_state.handle,
            "GetAuxCtl should write the tracked AuxCtlHandle for fresh controls"
        );
        assert_eq!(
            bus.read_word(sp + 8),
            0x0100,
            "GetAuxCtl should return TRUE for fresh tracked controls"
        );
    }

    #[test]
    fn setctlcolor_reuses_aux_record_and_getauxctl_reports_custom_colors() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_ptr = bus.alloc(200);
        let ctrl_handle = new_control_handle(&mut disp, &mut cpu, &mut bus, window_ptr, false, 0);
        let aux_before = disp
            .control_aux_state(ctrl_handle)
            .expect("fresh control should already have an AuxCtlRec");
        let custom_ctab = make_control_ctab_handle(&mut bus);

        let sp = 0x300140;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, custom_ctab);
        bus.write_long(sp + 4, ctrl_handle);

        let result = disp.dispatch_control(true, 0x243, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        let aux_after = disp
            .control_aux_state(ctrl_handle)
            .expect("SetCtlColor should keep the AuxCtlRec alive");
        assert_eq!(
            aux_after.handle, aux_before.handle,
            "SetCtlColor should rewrite the existing AuxCtlRec instead of allocating a new one"
        );
        let aux_ptr = bus.read_long(aux_after.handle);
        assert_eq!(
            bus.read_long(aux_ptr + 8),
            custom_ctab,
            "SetCtlColor should rewrite acCTable to the caller-supplied CCTabHandle"
        );

        let aux_out = bus.alloc(4);
        let get_sp = 0x300180;
        cpu.write_reg(Register::A7, get_sp);
        bus.write_long(get_sp, aux_out);
        bus.write_long(get_sp + 4, ctrl_handle);
        bus.write_word(get_sp + 8, 0);

        let get_result = disp.dispatch_quickdraw(true, 0x244, &mut cpu, &mut bus);
        assert!(get_result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), get_sp + 8);
        assert_eq!(bus.read_long(aux_out), aux_after.handle);
        assert_eq!(
            bus.read_word(get_sp + 8),
            0x0100,
            "GetAuxCtl should return TRUE after SetCtlColor installs custom colors"
        );
    }

    #[test]
    fn disposecontrol_releases_aux_record_and_repairs_auxctl_chain() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_ptr = bus.alloc(200);
        let first = new_control_handle(&mut disp, &mut cpu, &mut bus, window_ptr, false, 0);
        let second = new_control_handle(&mut disp, &mut cpu, &mut bus, window_ptr, false, 1);
        let first_ctab = make_control_ctab_handle(&mut bus);
        let second_ctab = make_control_ctab_handle(&mut bus);

        for (ctrl_handle, ctab_handle) in [(first, first_ctab), (second, second_ctab)] {
            let sp = if ctrl_handle == first {
                0x300180
            } else {
                0x3001A0
            };
            cpu.write_reg(Register::A7, sp);
            bus.write_long(sp, ctab_handle);
            bus.write_long(sp + 4, ctrl_handle);
            let result = disp.dispatch_control(true, 0x243, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
        }

        let first_state = disp
            .control_aux_state(first)
            .expect("first control aux record");
        let second_state = disp
            .control_aux_state(second)
            .expect("second control aux record");
        let first_aux_ptr = bus.read_long(first_state.handle);
        let second_aux_ptr = bus.read_long(second_state.handle);
        assert_eq!(
            bus.read_long(second_aux_ptr),
            first_state.handle,
            "second AuxCtlRec should link to the first via acNext"
        );

        let sp = 0x3001C0;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, first);

        let result = disp.dispatch_control(true, 0x155, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert!(
            disp.control_aux_state(first).is_none(),
            "DisposeControl should drop the released control from the aux-state map"
        );
        assert_eq!(
            bus.read_long(0x0CD4),
            second_state.handle,
            "AuxCtlHead should remain pointed at the surviving control's aux record"
        );
        assert_eq!(
            bus.read_long(second_aux_ptr),
            0,
            "DisposeControl should repair the acNext chain when unlinking a non-head aux record"
        );
        assert_eq!(
            bus.get_alloc_size(first_state.handle),
            None,
            "DisposeControl should free the released AuxCtlHandle"
        );
        assert_eq!(
            bus.get_alloc_size(first_aux_ptr),
            None,
            "DisposeControl should free the released AuxCtlRec"
        );
    }

    // IM:I I-331: NewControl takes nine arguments and returns a ControlHandle.
    #[test]
    fn newcontrol_consumes_arguments_and_writes_result_slot() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let window_ptr = bus.alloc(200);
        let bounds_ptr = bus.alloc(8);
        let title_ptr = bus.alloc(16);

        bus.write_word(bounds_ptr, 12);
        bus.write_word(bounds_ptr + 2, 18);
        bus.write_word(bounds_ptr + 4, 40);
        bus.write_word(bounds_ptr + 6, 90);
        bus.write_byte(title_ptr, 4);
        bus.write_bytes(title_ptr + 1, b"Play");

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x1020_3040);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 99);
        bus.write_word(sp + 8, 0);
        bus.write_word(sp + 10, 1);
        // Pascal BOOLEAN high byte convention for visible = TRUE.
        bus.write_byte(sp + 12, 1);
        bus.write_long(sp + 14, title_ptr);
        bus.write_long(sp + 18, bounds_ptr);
        bus.write_long(sp + 22, window_ptr);

        disp.dispatch_control(true, 0x154, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 26);
        assert_ne!(bus.read_long(sp + 26), 0);
    }

    // IM:I I-331: NewControl adds the allocated control to the window's
    // control list.
    #[test]
    fn newcontrol_prepends_new_handle_into_window_control_list() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let window_ptr = bus.alloc(200);
        let bounds_ptr = bus.alloc(8);
        let title_ptr = bus.alloc(16);
        let (old_head, old_ptr) = alloc_control_handle(&mut bus, (5, 5, 15, 50), 255, 0);
        bus.write_long(old_ptr + 4, window_ptr);
        bus.write_long(old_ptr, 0);
        bus.write_long(window_ptr + 140, old_head);

        bus.write_word(bounds_ptr, 20);
        bus.write_word(bounds_ptr + 2, 24);
        bus.write_word(bounds_ptr + 4, 60);
        bus.write_word(bounds_ptr + 6, 120);
        bus.write_byte(title_ptr, 3);
        bus.write_bytes(title_ptr + 1, b"New");

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xA0B0_C0D0);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 100);
        bus.write_word(sp + 8, 0);
        bus.write_word(sp + 10, 10);
        bus.write_byte(sp + 12, 1);
        bus.write_long(sp + 14, title_ptr);
        bus.write_long(sp + 18, bounds_ptr);
        bus.write_long(sp + 22, window_ptr);

        disp.dispatch_control(true, 0x154, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let new_head = bus.read_long(sp + 26);
        let new_ptr = bus.read_long(new_head);
        assert_eq!(bus.read_long(window_ptr + 140), new_head);
        assert_eq!(bus.read_long(new_ptr), old_head);
        assert_eq!(bus.read_long(new_ptr + 4), window_ptr);
    }

    #[test]
    fn newcontrol_custom_cdef_arms_init_then_draw_pascal_callbacks() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let return_pc = 0x1234_5678;
        let window_ptr = bus.alloc(200);
        let bounds_ptr = bus.alloc(8);
        let title_ptr = bus.alloc(16);
        let proc_id = (160i16 << 4) | 5;
        let cdef_proc = disp.install_test_resource(&mut bus, *b"CDEF", 160, &[0x4E, 0x56, 0, 0]);

        for (offset, value) in [10i16, 20, 40, 100].into_iter().enumerate() {
            bus.write_word(bounds_ptr + offset as u32 * 2, value as u16);
        }
        bus.write_byte(title_ptr, 4);
        bus.write_bytes(title_ptr + 1, b"Play");
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, proc_id as u16);
        bus.write_word(sp + 6, 1);
        bus.write_word(sp + 8, 0);
        bus.write_word(sp + 10, 0);
        bus.write_word(sp + 12, 1);
        bus.write_long(sp + 14, title_ptr);
        bus.write_long(sp + 18, bounds_ptr);
        bus.write_long(sp + 22, window_ptr);

        disp.dispatch_control(true, 0x154, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let ctrl_handle = bus.read_long(sp + 26);
        let tramp = disp.control_def_trampoline;
        assert_ne!(ctrl_handle, 0);
        assert_ne!(tramp, 0);
        assert_eq!(cpu.read_reg(Register::PC), tramp);
        assert_eq!(cpu.read_reg(Register::A7), sp + 22);
        assert_eq!(bus.read_long(sp + 22), return_pc);
        assert_eq!(bus.read_word(tramp + 12), 5);
        assert_eq!(bus.read_long(tramp + 16), ctrl_handle);
        assert_eq!(
            bus.read_word(tramp + 22),
            super::super::TrapDispatcher::CDEF_INIT_CNTL_MSG as u16
        );
        assert_eq!(bus.read_long(tramp + 26), 0);
        assert_eq!(bus.read_long(tramp + 32), cdef_proc);
        assert_eq!(bus.read_word(tramp + 54), 0x4EF9);

        let draw_tramp = bus.read_long(tramp + 56);
        assert_ne!(draw_tramp, 0);
        assert_eq!(bus.read_word(draw_tramp + 12), 5);
        assert_eq!(bus.read_long(draw_tramp + 16), ctrl_handle);
        assert_eq!(
            bus.read_word(draw_tramp + 22),
            super::super::TrapDispatcher::CDEF_DRAW_CNTL_MSG as u16
        );
        assert_eq!(bus.read_long(draw_tramp + 26), 0);
        assert_eq!(bus.read_long(draw_tramp + 32), cdef_proc);
        assert_eq!(bus.read_word(draw_tramp + 54), 0x2F3C);
        assert_eq!(bus.read_word(draw_tramp + 60), 0xA873);
        assert_eq!(bus.read_word(draw_tramp + 68), 0xAA31);
        assert_eq!(bus.read_word(draw_tramp + 70), 0x4E75);
    }

    // IM:I I-332: DisposeControl takes one ControlHandle argument.
    #[test]
    fn disposecontrol_consumes_control_handle_argument() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (ctrl_handle, _) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctrl_handle);
        disp.dispatch_control(true, 0x155, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // IM:I I-332: DisposeControl removes the control from the window's
    // control list.
    #[test]
    fn disposecontrol_unlinks_control_from_owner_window_list() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let window_ptr = bus.alloc(200);
        let (tail_handle, tail_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
        let (head_handle, head_ptr) = alloc_control_handle(&mut bus, (12, 24, 36, 72), 255, 0);
        bus.write_long(tail_ptr + 4, window_ptr);
        bus.write_long(head_ptr + 4, window_ptr);
        bus.write_long(tail_ptr, 0);
        bus.write_long(head_ptr, tail_handle);
        bus.write_long(window_ptr + 140, head_handle);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, head_handle);
        disp.dispatch_control(true, 0x155, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_long(window_ptr + 140), tail_handle);
        assert_eq!(bus.read_long(tail_ptr), 0);
    }

    // IM:I I-332: KillControls takes one WindowPtr argument.
    #[test]
    fn killcontrols_consumes_window_argument() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let window_ptr = bus.alloc(200);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_ptr);
        disp.dispatch_control(true, 0x156, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // IM:I I-332: KillControls disposes every control owned by the window.
    #[test]
    fn killcontrols_clears_window_control_list_head() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let window_ptr = bus.alloc(200);
        let (tail_handle, tail_ptr) = alloc_control_handle(&mut bus, (8, 12, 24, 80), 255, 0);
        let (head_handle, head_ptr) = alloc_control_handle(&mut bus, (16, 20, 36, 100), 255, 0);
        bus.write_long(tail_ptr + 4, window_ptr);
        bus.write_long(head_ptr + 4, window_ptr);
        bus.write_long(tail_ptr, 0);
        bus.write_long(head_ptr, tail_handle);
        bus.write_long(window_ptr + 140, head_handle);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_ptr);
        disp.dispatch_control(true, 0x156, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_long(window_ptr + 140), 0);
    }

    // IM:I I-336: SetCtlAction takes actionProc and control-handle arguments.
    #[test]
    fn setctlaction_consumes_control_and_action_arguments() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (ctrl_handle, _) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x00AA_BBCC);
        bus.write_long(sp + 4, ctrl_handle);
        disp.dispatch_control(true, 0x16B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // IM:I I-336: SetCtlAction stores the control's action procedure pointer.
    #[test]
    fn setctlaction_updates_control_action_procptr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x0012_3456);
        bus.write_long(sp + 4, ctrl_handle);
        disp.dispatch_control(true, 0x16B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_long(ctrl_ptr + 32), 0x0012_3456);
    }

    // IM:I I-336: GetCtlAction takes one ControlHandle argument and returns
    // the action ProcPtr.
    #[test]
    fn getctlaction_consumes_control_argument_and_writes_result_slot() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
        bus.write_long(ctrl_ptr + 32, 0x00AB_CDEF);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctrl_handle);
        disp.dispatch_control(true, 0x16A, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_long(sp + 4), 0x00AB_CDEF);
    }

    // IM:I I-336: GetCtlAction returns the action procedure currently stored
    // in the control record.
    #[test]
    fn getctlaction_returns_control_action_procptr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
        bus.write_long(ctrl_ptr + 32, 0x00FE_DCBA);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctrl_handle);
        disp.dispatch_control(true, 0x16A, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_long(sp + 4), 0x00FE_DCBA);
    }

    #[test]
    fn getnewcontrol_parses_cntl_resource_and_links_at_window_head() {
        // IM:I (1985) pp. I-321 and I-333: GetNewControl copies CNTL fields
        // into a control record and links it into the window's control list.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let window_ptr = bus.alloc(200);
        let old_ctrl_ptr = bus.alloc(40);
        let old_ctrl_handle = bus.alloc(4);
        bus.write_long(old_ctrl_handle, old_ctrl_ptr);
        bus.write_long(window_ptr + 140, old_ctrl_handle);

        let cntl = build_cntl_resource((10, 20, 40, 100), 7, true, 2, 99, 0, 0x1234_5678, b"Play");
        disp.install_test_resource(&mut bus, *b"CNTL", 128, &cntl);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_ptr);
        bus.write_word(sp + 4, 128);
        bus.write_long(sp + 6, 0xDEAD_BEEF);
        disp.dispatch_control(true, 0x1BE, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let handle = bus.read_long(sp + 6);
        let ctrl_ptr = bus.read_long(handle);
        assert_ne!(handle, 0);
        assert_ne!(ctrl_ptr, 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(bus.read_long(window_ptr + 140), handle);
        assert_eq!(bus.read_long(ctrl_ptr), old_ctrl_handle);
        assert_eq!(bus.read_long(ctrl_ptr + 4), window_ptr);
        assert_eq!(bus.read_word(ctrl_ptr + 8) as i16, 10);
        assert_eq!(bus.read_word(ctrl_ptr + 10) as i16, 20);
        assert_eq!(bus.read_word(ctrl_ptr + 12) as i16, 40);
        assert_eq!(bus.read_word(ctrl_ptr + 14) as i16, 100);
        assert_eq!(bus.read_byte(ctrl_ptr + 16), 255);
        assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 7);
        assert_eq!(bus.read_word(ctrl_ptr + 20) as i16, 2);
        assert_eq!(bus.read_word(ctrl_ptr + 22) as i16, 99);
        assert_eq!(bus.read_long(ctrl_ptr + 36), 0x1234_5678);
        assert_eq!(bus.read_byte(ctrl_ptr + 40), 4);
        assert_eq!(bus.read_bytes(ctrl_ptr + 41, 4), b"Play");
    }

    #[test]
    fn getnewcontrol_missing_cntl_returns_nil_and_preserves_window_list() {
        // IM:I (1985) p. I-321: if the control template can't be read,
        // GetNewControl returns NIL.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let window_ptr = bus.alloc(200);
        let old_ctrl_ptr = bus.alloc(40);
        let old_ctrl_handle = bus.alloc(4);
        bus.write_long(old_ctrl_handle, old_ctrl_ptr);
        bus.write_long(window_ptr + 140, old_ctrl_handle);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_ptr);
        bus.write_word(sp + 4, 999);
        bus.write_long(sp + 6, 0xDEAD_BEEF);
        disp.dispatch_control(true, 0x1BE, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(bus.read_long(sp + 6), 0);
        assert_eq!(bus.read_long(window_ptr + 140), old_ctrl_handle);
    }

    #[test]
    fn control_accessors_round_trip_action_refcon_title_and_bounds() {
        let (mut disp, mut cpu, mut bus) = setup();
        let ctrl_ptr = bus.alloc(296);
        let ctrl_handle = bus.alloc(4);
        let title_ptr = bus.alloc(16);
        let out_title_ptr = bus.alloc(16);
        let sp = 0x300000;

        bus.write_long(ctrl_handle, ctrl_ptr);
        bus.write_long(ctrl_ptr + 36, 0xCAFE_BABE);
        bus.write_long(ctrl_ptr + 32, 0x0012_3456);
        bus.write_word(ctrl_ptr + 8, 10);
        bus.write_word(ctrl_ptr + 10, 20);
        bus.write_word(ctrl_ptr + 12, 30);
        bus.write_word(ctrl_ptr + 14, 60);
        bus.write_byte(ctrl_ptr + 16, 255);
        bus.write_word(ctrl_ptr + 20, 3);
        bus.write_word(ctrl_ptr + 22, 11);
        bus.write_byte(title_ptr, 3);
        bus.write_bytes(title_ptr + 1, b"EV!");

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, title_ptr);
        bus.write_long(sp + 4, ctrl_handle);
        let result = disp.dispatch_control(true, 0x15F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_byte(ctrl_ptr + 40), 3);
        assert_eq!(bus.read_bytes(ctrl_ptr + 41, 3), b"EV!");

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, out_title_ptr);
        bus.write_long(sp + 4, ctrl_handle);
        let result = disp.dispatch_control(true, 0x15E, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_byte(out_title_ptr), 3);
        assert_eq!(bus.read_bytes(out_title_ptr + 1, 3), b"EV!");

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctrl_handle);
        let result = disp.dispatch_control(true, 0x15A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0xCAFE_BABE);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctrl_handle);
        let result = disp.dispatch_control(true, 0x16A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0x0012_3456);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 10);
        bus.write_long(sp + 2, ctrl_handle);
        let result = disp.dispatch_control(true, 0x164, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 21);
        bus.write_long(sp + 2, ctrl_handle);
        let result = disp.dispatch_control(true, 0x165, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctrl_handle);
        let result = disp.dispatch_control(true, 0x161, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 10);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctrl_handle);
        let result = disp.dispatch_control(true, 0x162, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 21);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 15);
        bus.write_word(sp + 2, 25);
        bus.write_long(sp + 4, ctrl_handle);
        let result = disp.dispatch_control(true, 0x166, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 8), 10);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0);
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, ctrl_handle);
        let result = disp.dispatch_control(true, 0x166, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 8), 0);
    }

    // SizeControl/GetCTitle/SetCTitle semantics per Inside Macintosh
    // Volume I (1985), pp. I-321 and I-325.
    #[test]
    fn sizecontrol_sets_bottom_right_from_requested_width_height_and_pops_args() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 40, 70), 255, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 18); // h
        bus.write_word(sp + 2, 44); // w
        bus.write_long(sp + 4, ctrl_handle);
        disp.dispatch_control(true, 0x15C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_word(ctrl_ptr + 8) as i16, 10);
        assert_eq!(bus.read_word(ctrl_ptr + 10) as i16, 20);
        assert_eq!(bus.read_word(ctrl_ptr + 12) as i16, 28);
        assert_eq!(bus.read_word(ctrl_ptr + 14) as i16, 64);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn getctitle_copies_control_title_to_output_str255_and_pops_args() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let ctrl_ptr = bus.alloc(296);
        let ctrl_handle = bus.alloc(4);
        bus.write_long(ctrl_handle, ctrl_ptr);
        let out_title = bus.alloc(16);

        bus.write_byte(ctrl_ptr + 40, 5);
        bus.write_bytes(ctrl_ptr + 41, b"Hello");

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, out_title);
        bus.write_long(sp + 4, ctrl_handle);
        disp.dispatch_control(true, 0x15E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_byte(out_title), 5);
        assert_eq!(bus.read_bytes(out_title + 1, 5), b"Hello");
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn setctitle_updates_control_title_from_input_str255_and_pops_args() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let ctrl_ptr = bus.alloc(296);
        let ctrl_handle = bus.alloc(4);
        bus.write_long(ctrl_handle, ctrl_ptr);
        let in_title = bus.alloc(16);

        bus.write_word(ctrl_ptr + 8, 10);
        bus.write_word(ctrl_ptr + 10, 20);
        bus.write_word(ctrl_ptr + 12, 40);
        bus.write_word(ctrl_ptr + 14, 70);
        bus.write_byte(ctrl_ptr + 16, 255);
        bus.write_byte(ctrl_ptr + 40, 3);
        bus.write_bytes(ctrl_ptr + 41, b"Old");
        bus.write_byte(in_title, 7);
        bus.write_bytes(in_title + 1, b"Options");

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, in_title);
        bus.write_long(sp + 4, ctrl_handle);
        disp.dispatch_control(true, 0x15F, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_byte(ctrl_ptr + 40), 7);
        assert_eq!(bus.read_bytes(ctrl_ptr + 41, 7), b"Options");
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // SetCtlValue clamps to [contrlMin, contrlMax] per IM:I I-328.

    #[test]
    fn setctlvalue_clamps_above_max() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        cpu.write_reg(Register::A7, sp);
        let ctrl_ptr = bus.alloc(40);
        bus.write_word(ctrl_ptr + 20, 0);
        bus.write_word(ctrl_ptr + 22, 100);
        let handle = bus.alloc(4);
        bus.write_long(handle, ctrl_ptr);
        bus.write_word(sp, 500);
        bus.write_long(sp + 2, handle);
        disp.dispatch_control(true, 0x163, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 100);
    }

    #[test]
    fn setctlvalue_clamps_below_min() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        cpu.write_reg(Register::A7, sp);
        let ctrl_ptr = bus.alloc(40);
        bus.write_word(ctrl_ptr + 20, 50);
        bus.write_word(ctrl_ptr + 22, 100);
        let handle = bus.alloc(4);
        bus.write_long(handle, ctrl_ptr);
        bus.write_word(sp, 10);
        bus.write_long(sp + 2, handle);
        disp.dispatch_control(true, 0x163, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 50);
    }

    #[test]
    fn setctlvalue_passes_through_inrange() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        cpu.write_reg(Register::A7, sp);
        let ctrl_ptr = bus.alloc(40);
        bus.write_word(ctrl_ptr + 20, 0);
        bus.write_word(ctrl_ptr + 22, 100);
        let handle = bus.alloc(4);
        bus.write_long(handle, ctrl_ptr);
        bus.write_word(sp, 42);
        bus.write_long(sp + 2, handle);
        disp.dispatch_control(true, 0x163, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 42);
    }

    #[test]
    fn setctlvalue_popupmenuproc_variant_does_not_clamp_to_menu_id_min() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        cpu.write_reg(Register::A7, sp);
        let ctrl_ptr = bus.alloc(40);
        bus.write_word(ctrl_ptr + 20, 1300); // popupMenuProc stores MENU id in min
        bus.write_word(ctrl_ptr + 22, 0);
        let handle = bus.alloc(4);
        bus.write_long(handle, ctrl_ptr);
        disp.control_proc_ids.insert(ctrl_ptr, 1009);
        bus.write_word(sp, 3);
        bus.write_long(sp + 2, handle);

        disp.dispatch_control(true, 0x163, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 3);
    }

    // SetCtlMin / SetCtlMax must bump the current value into the new range if it's outside.
    #[test]
    fn setctlmin_bumps_below_value_up() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        cpu.write_reg(Register::A7, sp);
        let ctrl_ptr = bus.alloc(40);
        bus.write_word(ctrl_ptr + 18, 5); // current value
        bus.write_word(ctrl_ptr + 20, 0); // min
        bus.write_word(ctrl_ptr + 22, 100); // max
        let handle = bus.alloc(4);
        bus.write_long(handle, ctrl_ptr);
        bus.write_word(sp, 20); // new min
        bus.write_long(sp + 2, handle);
        disp.dispatch_control(true, 0x164, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(ctrl_ptr + 20) as i16, 20);
        assert_eq!(
            bus.read_word(ctrl_ptr + 18) as i16,
            20,
            "current value must be bumped up to the new minimum"
        );
    }

    #[test]
    fn setctlmax_pulls_above_value_down() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        cpu.write_reg(Register::A7, sp);
        let ctrl_ptr = bus.alloc(40);
        bus.write_word(ctrl_ptr + 18, 80); // current value
        bus.write_word(ctrl_ptr + 20, 0); // min
        bus.write_word(ctrl_ptr + 22, 100); // max
        let handle = bus.alloc(4);
        bus.write_long(handle, ctrl_ptr);
        bus.write_word(sp, 50); // new max
        bus.write_long(sp + 2, handle);
        disp.dispatch_control(true, 0x165, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(ctrl_ptr + 22) as i16, 50);
        assert_eq!(
            bus.read_word(ctrl_ptr + 18) as i16,
            50,
            "current value must be pulled down to the new maximum"
        );
    }

    #[test]
    fn setctlmin_inrange_value_untouched() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        cpu.write_reg(Register::A7, sp);
        let ctrl_ptr = bus.alloc(40);
        bus.write_word(ctrl_ptr + 18, 50); // current value
        bus.write_word(ctrl_ptr + 20, 0);
        bus.write_word(ctrl_ptr + 22, 100);
        let handle = bus.alloc(4);
        bus.write_long(handle, ctrl_ptr);
        bus.write_word(sp, 10); // new min, still below value
        bus.write_long(sp + 2, handle);
        disp.dispatch_control(true, 0x164, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(ctrl_ptr + 20) as i16, 10);
        assert_eq!(
            bus.read_word(ctrl_ptr + 18) as i16,
            50,
            "current value in range must stay unchanged"
        );
    }

    // ShowControl/HideControl/MoveControl semantics per Inside Macintosh
    // Volume I (1985), pp. I-328 to I-329.
    #[test]
    fn showcontrol_draws_standard_control_and_pops_handle_arg() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = 0x300000u32;
        let window = bus.read_long(bus.read_long(cpu.read_reg(Register::A5)));
        let screen_base = bus.read_long(window + 2);
        let row_bytes = u32::from(bus.read_word(window + 6) & 0x3FFF);
        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut bus, screen_base, row_bytes, 342);
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 0, 0);
        bus.write_long(ctrl_ptr + 4, window);
        disp.control_proc_ids.insert(ctrl_ptr, 0);
        let before: Vec<u8> = (0..row_bytes * 342)
            .map(|offset| bus.read_byte(screen_base + offset))
            .collect();

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctrl_handle);
        disp.dispatch_control(true, 0x157, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_byte(ctrl_ptr + 16), 255);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_ne!(
            (0..row_bytes * 342)
                .map(|offset| bus.read_byte(screen_base + offset))
                .collect::<Vec<_>>(),
            before,
            "ShowControl should immediately draw a newly visible standard control"
        );
    }

    #[test]
    fn showcontrol_arms_application_cdef_draw_in_owner_port() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = 0x300000u32;
        let return_pc = 0x1234_5678;
        let owner = bus.read_long(bus.read_long(cpu.read_reg(Register::A5)));
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 0, 0);
        let cdef_proc = bus.alloc(4);
        let cdef_handle = bus.alloc(4);
        bus.write_word(cdef_proc, 0x4E56); // LINK.W A6,#imm
        bus.write_long(cdef_handle, cdef_proc);
        bus.write_long(ctrl_ptr + 4, owner);
        bus.write_long(ctrl_ptr + 24, cdef_handle);
        disp.control_proc_ids.insert(ctrl_ptr, (160 << 4) | 5);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        bus.write_long(sp, ctrl_handle);
        disp.dispatch_control(true, 0x157, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let trampoline = disp.control_def_trampoline;
        assert_eq!(bus.read_byte(ctrl_ptr + 16), 255);
        assert_ne!(trampoline, 0);
        assert_eq!(cpu.read_reg(Register::PC), trampoline);
        assert_eq!(bus.read_word(trampoline + 12), 5);
        assert_eq!(bus.read_long(trampoline + 16), ctrl_handle);
        assert_eq!(
            bus.read_word(trampoline + 22),
            super::super::TrapDispatcher::CDEF_DRAW_CNTL_MSG as u16
        );
        assert_eq!(bus.read_long(trampoline + 26), 0);
        assert_eq!(bus.read_long(trampoline + 32), cdef_proc);
        assert_eq!(*disp.current_port, owner);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        bus.write_long(sp, ctrl_handle);
        disp.dispatch_control(true, 0x157, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(
            cpu.read_reg(Register::PC),
            return_pc,
            "ShowControl should not redraw a control that is already visible"
        );
    }

    #[test]
    fn hidecontrol_clears_contrlvis_and_pops_handle_arg() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctrl_handle);
        disp.dispatch_control(true, 0x158, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_byte(ctrl_ptr + 16), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn movecontrol_repositions_rect_preserves_size_and_pops_args() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 40, 70), 255, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 35); // v
        bus.write_word(sp + 2, 90); // h
        bus.write_long(sp + 4, ctrl_handle);
        disp.dispatch_control(true, 0x159, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_word(ctrl_ptr + 8) as i16, 35);
        assert_eq!(bus.read_word(ctrl_ptr + 10) as i16, 90);
        assert_eq!(bus.read_word(ctrl_ptr + 12) as i16, 65);
        assert_eq!(bus.read_word(ctrl_ptr + 14) as i16, 140);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // HiliteControl semantics per Macintosh Toolbox Essentials 1992, p. 5-98.
    #[test]
    fn hilite_control_writes_contrlhilite_and_pops_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1);
        bus.write_long(sp + 2, ctrl_handle);
        disp.dispatch_control(true, 0x15D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_byte(ctrl_ptr + 17), 1);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    }

    #[test]
    fn hilite_control_255_marks_control_inactive() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 255);
        bus.write_long(sp + 2, ctrl_handle);
        disp.dispatch_control(true, 0x15D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_byte(ctrl_ptr + 17), 255);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    }

    #[test]
    fn inactive_checkbox_and_radio_titles_use_gray_device_index() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(128 * 128);
        let row_bytes = 128u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 128, 128, 8);
        *disp.device_clut = [[0x2020, 0x4040, 0x6060]; 256];
        disp.device_clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
        disp.device_clut[42] = [0x7FFF, 0x7FFF, 0x7FFF];
        disp.device_clut[255] = [0, 0, 0];
        bus.write_long(0x0824, screen_base);
        bus.write_word(0x0828, row_bytes as u16);

        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);
        let gdevice = bus.read_long(gdevice_handle);
        let pixmap_handle = bus.read_long(gdevice + 22);
        let pixmap = bus.read_long(pixmap_handle);
        let ctab_handle = bus.read_long(pixmap + 42);
        let ctab = bus.read_long(ctab_handle);
        for index in 0u32..256 {
            let entry = ctab + 8 + index * 8;
            bus.write_word(entry, index as u16);
            bus.write_word(entry + 2, 0x2020);
            bus.write_word(entry + 4, 0x4040);
            bus.write_word(entry + 6, 0x6060);
        }
        let white_entry = ctab + 8;
        bus.write_word(white_entry, 0);
        bus.write_word(white_entry + 2, 0xFFFF);
        bus.write_word(white_entry + 4, 0xFFFF);
        bus.write_word(white_entry + 6, 0xFFFF);
        // Deliberately put the exact gray at a different GDevice-table index.
        // The inactive title draw path should resolve against device_clut,
        // matching screen export, not this current-GDevice table.
        let gray_entry = ctab + 8 + 17 * 8;
        bus.write_word(gray_entry, 17);
        bus.write_word(gray_entry + 2, 0x7FFF);
        bus.write_word(gray_entry + 4, 0x7FFF);
        bus.write_word(gray_entry + 6, 0x7FFF);
        let black_entry = ctab + 8 + 255 * 8;
        bus.write_word(black_entry, 255);
        bus.write_word(black_entry + 2, 0);
        bus.write_word(black_entry + 4, 0);
        bus.write_word(black_entry + 6, 0);

        for offset in 0..(row_bytes * 128) {
            bus.write_byte(screen_base + offset, 0);
        }
        let window_ptr = *disp.current_port;
        bus.write_word(window_ptr + 8, 0);
        bus.write_word(window_ptr + 10, 0);
        bus.write_word(window_ptr + 16, 0);
        bus.write_word(window_ptr + 18, 0);
        bus.write_word(window_ptr + 20, 128);
        bus.write_word(window_ptr + 22, 128);

        let mut control_ptrs = Vec::new();
        for (proc_id, rect, value) in [(1, (20, 20, 40, 118), 1), (2, (52, 20, 72, 118), 1)] {
            let ctrl_ptr = bus.alloc(296);
            disp.initialize_control_record(
                &mut bus, ctrl_ptr, window_ptr, rect, b"Sound", true, value, 0, 1, proc_id, 0,
            );
            bus.write_byte(ctrl_ptr + 17, 255);
            disp.draw_control(&mut cpu, &mut bus, ctrl_ptr);
            control_ptrs.push(ctrl_ptr);
        }

        let checkbox_gray = count_pixel_index(&bus, screen_base, row_bytes, 20, 35, 40, 118, 42);
        let checkbox_black = count_pixel_index(&bus, screen_base, row_bytes, 20, 35, 40, 118, 255);
        let radio_gray = count_pixel_index(&bus, screen_base, row_bytes, 52, 35, 72, 118, 42);
        let radio_black = count_pixel_index(&bus, screen_base, row_bytes, 52, 35, 72, 118, 255);

        assert!(
            checkbox_gray > 12,
            "inactive checkbox title should draw with the device gray index"
        );
        assert_eq!(
            checkbox_black, 0,
            "inactive checkbox title must not leave black glyph pixels"
        );
        assert!(
            radio_gray > 12,
            "inactive radio title should draw with the device gray index"
        );
        assert_eq!(
            radio_black, 0,
            "inactive radio title must not leave black glyph pixels"
        );

        // A custom palette with only neutral white/black endpoints still
        // maps grayishTextOr's blend to a visible tinted intermediate entry.
        *disp.device_clut = [[0x2020, 0x4040, 0x6060]; 256];
        disp.device_clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
        disp.device_clut[255] = [0, 0, 0];
        for offset in 0..(row_bytes * 128) {
            bus.write_byte(screen_base + offset, 0);
        }
        for ctrl_ptr in control_ptrs {
            disp.draw_control(&mut cpu, &mut bus, ctrl_ptr);
        }
        for (label, top) in [("checkbox", 20), ("radio", 52)] {
            assert!(
                count_pixel_index(&bus, screen_base, row_bytes, top, 35, top + 20, 118, 1) > 12,
                "inactive {label} title should use a visible palette blend without a gray ramp"
            );
        }
    }

    #[test]
    fn inactive_popup_label_and_selected_title_use_live_device_palette() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(256 * 128);
        let row_bytes = 256u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 256, 128, 8);
        *disp.device_clut = [[0x2020, 0x4040, 0x6060]; 256];
        disp.device_clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
        disp.device_clut[255] = [0, 0, 0];
        bus.write_long(0x0824, screen_base);
        bus.write_word(0x0828, row_bytes as u16);
        for offset in 0..(row_bytes * 128) {
            bus.write_byte(screen_base + offset, 0);
        }

        let window_ptr = *disp.current_port;
        bus.write_word(window_ptr + 8, 0);
        bus.write_word(window_ptr + 10, 0);
        bus.write_word(window_ptr + 16, 0);
        bus.write_word(window_ptr + 18, 0);
        bus.write_word(window_ptr + 20, 128);
        bus.write_word(window_ptr + 22, 256);
        let menu_ptr = bus.alloc(256);
        let menu_handle = bus.alloc(4);
        bus.write_long(menu_handle, menu_ptr);
        disp.menus.push(Menu {
            id: 900,
            title: "Files".to_string(),
            items: vec![MenuItem {
                text: "Demo Map".to_string(),
                icon: 0,
                key_equiv: 0,
                mark: 0,
                style: 0,
                enabled: true,
            }],
            enabled: true,
            handle: menu_handle,
            in_menu_bar: false,
            hierarchical: false,
            visible_in_menu_bar: false,
        });

        for (top, hilite) in [(20, 255), (52, 0)] {
            let (_handle, ctrl_ptr) = disp.create_control_record(
                &mut bus,
                window_ptr,
                (top, 20, top + 20, 220),
                b"Map:",
                true,
                1,
                900,
                60,
                1009,
                0,
            );
            assert_eq!(bus.read_word(ctrl_ptr + 20), 1);
            assert_eq!(bus.read_word(ctrl_ptr + 22), 1);
            bus.write_byte(ctrl_ptr + 17, hilite);
            assert_eq!(disp.popup_control_title_width(ctrl_ptr, 1), 60);
            disp.draw_control(&mut cpu, &mut bus, ctrl_ptr);
        }

        assert!(
            count_pixel_index(&bus, screen_base, row_bytes, 20, 20, 40, 80, 1) > 8,
            "inactive popup label should use a visible palette blend"
        );
        assert!(
            count_pixel_index(&bus, screen_base, row_bytes, 20, 90, 40, 170, 1) > 8,
            "inactive popup selected title should use a visible palette blend"
        );
        assert!(
            count_pixel_index(&bus, screen_base, row_bytes, 52, 20, 72, 80, 255) > 8,
            "enabled popup label should retain active black ink"
        );
        assert!(
            count_pixel_index(&bus, screen_base, row_bytes, 52, 90, 72, 170, 255) > 8,
            "enabled popup selected title should retain active black ink"
        );
    }

    /// An active push-button title must resolve its ink against the colour
    /// table the screen is displayed with, not the current GDevice table.
    ///
    /// Games that install their own palette leave the two disagreeing. Dark
    /// Castle's GDevice table calls index 1 black while the live device CLUT
    /// has index 1 as pure green, so every control title was painted green.
    /// The inactive-title path already resolved against `device_clut`; this
    /// pins the active path to the same rule.
    #[test]
    fn active_button_title_uses_the_device_clut_black_index() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(128 * 128);
        let row_bytes = 128u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 128, 128, 8);

        // Live device CLUT: black lives at 255, and index 1 is bright green.
        *disp.device_clut = [[0x8080, 0x8080, 0x8080]; 256];
        disp.device_clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
        disp.device_clut[1] = [0, 0xFFFF, 0];
        disp.device_clut[255] = [0, 0, 0];
        bus.write_long(0x0824, screen_base);
        bus.write_word(0x0828, row_bytes as u16);

        // GDevice table disagrees: it claims index 1 is the black one, which
        // is what the old ink lookup would have latched onto.
        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);
        let gdevice = bus.read_long(gdevice_handle);
        let pixmap_handle = bus.read_long(gdevice + 22);
        let pixmap = bus.read_long(pixmap_handle);
        let ctab_handle = bus.read_long(pixmap + 42);
        let ctab = bus.read_long(ctab_handle);
        for index in 0u32..256 {
            let entry = ctab + 8 + index * 8;
            bus.write_word(entry, index as u16);
            bus.write_word(entry + 2, 0x8080);
            bus.write_word(entry + 4, 0x8080);
            bus.write_word(entry + 6, 0x8080);
        }
        let black_claim = ctab + 8 + 8;
        bus.write_word(black_claim, 1);
        bus.write_word(black_claim + 2, 0);
        bus.write_word(black_claim + 4, 0);
        bus.write_word(black_claim + 6, 0);

        for offset in 0..(row_bytes * 128) {
            bus.write_byte(screen_base + offset, 0);
        }
        let window_ptr = *disp.current_port;
        bus.write_word(window_ptr + 8, 0);
        bus.write_word(window_ptr + 10, 0);
        bus.write_word(window_ptr + 16, 0);
        bus.write_word(window_ptr + 18, 0);
        bus.write_word(window_ptr + 20, 128);
        bus.write_word(window_ptr + 22, 128);

        let rect = (20i16, 10i16, 44i16, 118i16);
        let ctrl_ptr = bus.alloc(296);
        disp.initialize_control_record(
            &mut bus, ctrl_ptr, window_ptr, rect, b"Play", true, 0, 0, 1, 0, 0,
        );
        disp.draw_control(&mut cpu, &mut bus, ctrl_ptr);

        let black = count_pixel_index(&bus, screen_base, row_bytes, 20, 10, 44, 118, 255);
        let green = count_pixel_index(&bus, screen_base, row_bytes, 20, 10, 44, 118, 1);
        assert!(
            black > 12,
            "title glyphs should use the device CLUT's black index, got {black} px"
        );
        assert_eq!(
            green, 0,
            "title must not be painted in the GDevice table's mistaken black"
        );
    }

    // TrackControl semantics per Macintosh Toolbox Essentials 1992, pp. 5-89 to 5-90.
    #[test]
    fn track_control_visible_active_button_hit_returns_inbutton() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.enable_input_trace_capture();
        let sp = 0x300000u32;
        let (ctrl_handle, _) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, 15);
        bus.write_word(sp + 6, 25);
        bus.write_long(sp + 8, ctrl_handle);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_word(sp + 12), 10);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        let trace = disp.input_trace_text();
        assert!(trace.contains("A968 action=start start=(15,25)"));
        assert!(trace.contains("part=10 highlighted_item=none outcome=visible_active_hit"));
    }

    #[test]
    fn track_control_scrollbar_arrow_calls_action_proc_with_part_code() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let trap_pc = 0x0012_3454;
        let action_proc = bus.alloc(2);
        bus.write_word(action_proc, 0x4E75); // RTS
        let (ctrl_handle, _) =
            alloc_scrollbar_control(&mut disp, &mut bus, (60, 240, 220, 256), 255, 0, 40, 0, 100);

        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, action_proc);
        bus.write_word(sp + 4, 210);
        bus.write_word(sp + 6, 248);
        bus.write_long(sp + 8, ctrl_handle);
        bus.write_word(sp + 12, 0xBEEF);

        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_word(sp + 12), 21, "down arrow is inDownButton");
        let trampoline = disp.control_def_trampoline;
        assert_ne!(trampoline, 0);
        assert_eq!(cpu.read_reg(Register::PC), trampoline);
        assert_eq!(cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(bus.read_long(sp - 4), trap_pc);
        assert_eq!(bus.read_word(trampoline), 0x48E7);
        assert_eq!(bus.read_word(trampoline + 4), 0x2F3C);
        assert_eq!(bus.read_long(trampoline + 6), ctrl_handle);
        assert_eq!(bus.read_word(trampoline + 10), 0x3F3C);
        assert_eq!(bus.read_word(trampoline + 12), 21);
        assert_eq!(bus.read_word(trampoline + 14), 0x4EB9);
        assert_eq!(bus.read_long(trampoline + 16), action_proc);
        assert_eq!(bus.read_word(trampoline + 30), 0x4E75);

        let tracking = disp.control_tracking.as_ref().unwrap();
        assert!(tracking.scrollbar_callback_pending);
        assert_eq!(tracking.scrollbar_part, 21);

        // Returning from the initial action callback retains TrackControl.
        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(Register::A7, sp);
        disp.input_state.mouse_button = true;
        disp.input_state.mouse_pos = (210, 248);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert!(
            !disp
                .control_tracking
                .as_ref()
                .unwrap()
                .scrollbar_callback_pending
        );
        assert_eq!(cpu.read_reg(Register::A7), sp);

        // A held arrow repeats at the classic 20 Hz cadence.
        disp.tick_count += TrapDispatcher::SCROLLBAR_ACTION_REPEAT_TICKS;
        cpu.write_reg(Register::PC, trap_pc + 2);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert!(
            disp.control_tracking
                .as_ref()
                .unwrap()
                .scrollbar_callback_pending
        );
        assert_eq!(cpu.read_reg(Register::PC), trampoline);

        // Once that callback returns, mouse-up completes the retained trap.
        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(Register::A7, sp);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        disp.input_state.mouse_button = false;
        cpu.write_reg(Register::PC, trap_pc + 2);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert!(disp.control_tracking.is_none());
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(bus.read_word(sp + 12), 21);
    }

    #[test]
    fn track_control_button_systemless_theme_tracks_pressed_state_until_release() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
        disp.enable_input_trace_capture();
        let sp = 0x300000u32;
        let window = *disp.current_port;
        let row_bytes = 64u32;
        let base = bus.alloc(row_bytes * 342);
        disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut bus, base, row_bytes, 342);
        let (ctrl_handle, ctrl_ptr) =
            alloc_button_control(&mut disp, &mut bus, window, (20, 20, 40, 80));
        let probe_x = 30;
        let probe_y = 30;

        disp.input_state.mouse_button = true;
        disp.input_state.mouse_pos = (probe_y, probe_x);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, probe_y as u16);
        bus.write_word(sp + 6, probe_x as u16);
        bus.write_long(sp + 8, ctrl_handle);
        bus.write_word(sp + 12, 0xBEEF);

        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp,
            "simple TrackControl should defer the stack pop until mouse-up"
        );
        assert!(disp.control_tracking.is_some());
        assert_eq!(bus.read_word(sp + 12), 0xBEEF);
        assert_eq!(bus.read_byte(ctrl_ptr + 17), 1);
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, probe_x, probe_y),
            "held simple TrackControl should route pressed button chrome through the provider"
        );

        disp.input_state.mouse_pos = (10, 10);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_byte(ctrl_ptr + 17), 0);
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, probe_x, probe_y),
            "dragging outside should redraw unpressed provider chrome"
        );

        disp.input_state.mouse_pos = (probe_y, probe_x);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_byte(ctrl_ptr + 17), 1);
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, probe_x, probe_y),
            "dragging back inside should restore provider pressed chrome"
        );

        disp.input_state.mouse_button = false;
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(bus.read_word(sp + 12), 10);
        assert_eq!(bus.read_byte(ctrl_ptr + 17), 0);
        assert!(disp.control_tracking.is_none());
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, probe_x, probe_y),
            "release should restore unpressed provider chrome before returning"
        );
        let trace = disp.input_trace_text();
        assert!(trace.contains("outcome=simple_tracking_started"));
        assert!(trace.contains("outcome=simple_no_part"));
        assert!(trace.contains("outcome=simple_part_highlighted"));
        assert!(trace.contains("part=10 highlighted_item=none outcome=simple_part_selected"));
    }

    fn trackcontrol_button_release_result_for_theme(
        theme_id: UiThemeId,
        release_inside: bool,
    ) -> (u16, u32, u8, bool) {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.set_ui_theme_id(theme_id);
        let sp = 0x300000u32;
        let window = *disp.current_port;
        let screen_base = bus.read_long(window + 2);
        let row_bytes = (bus.read_word(window + 6) & 0x3FFF) as u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        let (ctrl_handle, ctrl_ptr) =
            alloc_button_control(&mut disp, &mut bus, window, (20, 20, 40, 80));

        disp.input_state.mouse_button = true;
        disp.input_state.mouse_pos = (30, 30);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, 30);
        bus.write_word(sp + 6, 30);
        bus.write_long(sp + 8, ctrl_handle);
        bus.write_word(sp + 12, 0xBEEF);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        if !release_inside {
            disp.input_state.mouse_pos = (10, 10);
            disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
        }
        disp.input_state.mouse_button = false;
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        (
            bus.read_word(sp + 12),
            cpu.read_reg(Register::A7),
            bus.read_byte(ctrl_ptr + 17),
            disp.control_tracking.is_some(),
        )
    }

    #[test]
    fn systemless_theme_does_not_change_trackcontrol_part_codes() {
        // IM:I I-323: TrackControl returns the same part code on inside
        // release and 0 on outside release. Theme chrome must not alter those
        // guest-visible Control Manager results.
        let classic_inside =
            trackcontrol_button_release_result_for_theme(UiThemeId::ClassicSystem7, true);
        let themed_inside =
            trackcontrol_button_release_result_for_theme(UiThemeId::SystemlessDefault, true);
        let classic_outside =
            trackcontrol_button_release_result_for_theme(UiThemeId::ClassicSystem7, false);
        let themed_outside =
            trackcontrol_button_release_result_for_theme(UiThemeId::SystemlessDefault, false);

        assert_eq!(classic_inside, (10, 0x30000C, 0, false));
        assert_eq!(classic_outside, (0, 0x30000C, 0, false));
        assert_eq!(
            themed_inside, classic_inside,
            "systemless-default must not change TrackControl inside-release part codes"
        );
        assert_eq!(
            themed_outside, classic_outside,
            "systemless-default must not change TrackControl outside-release part codes"
        );
    }

    fn trackcontrol_popup_value_result_for_theme(
        theme_id: UiThemeId,
        select_second_item: bool,
    ) -> (u16, u32, i16, bool) {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.set_ui_theme_id(theme_id);
        let sp = 0x300000u32;
        let window = *disp.current_port;
        let screen_base = bus.read_long(window + 2);
        let row_bytes = (bus.read_word(window + 6) & 0x3FFF) as u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut bus, screen_base, row_bytes, 342);

        let owner = bus.read_long(bus.read_long(cpu.read_reg(Register::A5)));
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 130), 255, 0);
        bus.write_long(ctrl_ptr + 4, owner);
        bus.write_word(ctrl_ptr + 18, 1);
        bus.write_word(ctrl_ptr + 20, 902);
        bus.write_word(ctrl_ptr + 22, 0);
        disp.control_proc_ids.insert(ctrl_ptr, 1009);
        disp.menus.push(Menu {
            id: 902,
            title: "Mode".to_string(),
            items: vec![
                MenuItem {
                    text: "First".to_string(),
                    icon: 0,
                    key_equiv: 0,
                    mark: 0,
                    style: 0,
                    enabled: true,
                },
                MenuItem {
                    text: "Second".to_string(),
                    icon: 0,
                    key_equiv: 0,
                    mark: 0,
                    style: 0,
                    enabled: true,
                },
            ],
            enabled: true,
            handle: 0,
            in_menu_bar: false,
            visible_in_menu_bar: false,
            hierarchical: false,
        });

        disp.input_state.mouse_button = true;
        disp.input_state.mouse_pos = (15, 25);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xFFFF_FFFF);
        bus.write_word(sp + 4, 15);
        bus.write_word(sp + 6, 25);
        bus.write_long(sp + 8, ctrl_handle);
        bus.write_word(sp + 12, 0xBEEF);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp);

        let (dropdown_top, dropdown_left, dropdown_bottom, _) = disp
            .control_tracking
            .as_ref()
            .map(|tracking| tracking.dropdown_rect)
            .expect("popup tracking should open a dropdown");
        if select_second_item {
            disp.input_state.mouse_pos = (dropdown_top + 1 + 16 + 1, dropdown_left + 5);
        } else {
            disp.input_state.mouse_pos = (dropdown_bottom + 8, dropdown_left + 5);
        }
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        disp.input_state.mouse_button = false;
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        (
            bus.read_word(sp + 12),
            cpu.read_reg(Register::A7),
            bus.read_word(ctrl_ptr + 18) as i16,
            disp.control_tracking.is_some(),
        )
    }

    #[test]
    fn systemless_theme_does_not_change_popup_trackcontrol_value_results() {
        // MTE 1992 describes popup contrlValue as the chosen menu item
        // number, and TrackControl as returning the release part code.
        // Theme chrome must not alter either guest-visible result.
        let classic_select =
            trackcontrol_popup_value_result_for_theme(UiThemeId::ClassicSystem7, true);
        let themed_select =
            trackcontrol_popup_value_result_for_theme(UiThemeId::SystemlessDefault, true);
        let classic_no_selection =
            trackcontrol_popup_value_result_for_theme(UiThemeId::ClassicSystem7, false);
        let themed_no_selection =
            trackcontrol_popup_value_result_for_theme(UiThemeId::SystemlessDefault, false);

        assert_eq!(classic_select, (10, 0x30000C, 2, false));
        assert_eq!(classic_no_selection, (0, 0x30000C, 1, false));
        assert_eq!(
            themed_select, classic_select,
            "systemless-default must not change popup TrackControl selected values"
        );
        assert_eq!(
            themed_no_selection, classic_no_selection,
            "systemless-default must not change popup TrackControl no-selection values"
        );
    }

    #[test]
    fn track_control_popup_menu_tracks_and_updates_value_on_release() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.enable_input_trace_capture();
        let sp = 0x300000u32;
        let owner = bus.read_long(bus.read_long(cpu.read_reg(Register::A5)));
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 130), 255, 0);
        bus.write_long(ctrl_ptr + 4, owner);
        bus.write_word(ctrl_ptr + 18, 1);
        bus.write_word(ctrl_ptr + 20, 900); // popupMenuProc stores MENU id in min
        bus.write_word(ctrl_ptr + 22, 0);
        disp.control_proc_ids.insert(ctrl_ptr, 1009);
        disp.menus.push(Menu {
            id: 900,
            title: "Squadies".to_string(),
            items: vec![
                MenuItem {
                    text: "Duke".to_string(),
                    icon: 0,
                    key_equiv: 0,
                    mark: 0,
                    style: 0,
                    enabled: true,
                },
                MenuItem {
                    text: "Carnage".to_string(),
                    icon: 0,
                    key_equiv: 0,
                    mark: 0,
                    style: 0,
                    enabled: true,
                },
            ],
            enabled: true,
            handle: 0,
            in_menu_bar: false,
            hierarchical: false,
            visible_in_menu_bar: false,
        });

        disp.input_state.mouse_button = true;
        disp.input_state.mouse_pos = (15, 25);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, 15);
        bus.write_word(sp + 6, 25);
        bus.write_long(sp + 8, ctrl_handle);
        bus.write_word(sp + 12, 0xBEEF);

        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp,
            "popup tracking should defer the TrackControl stack pop"
        );
        let dropdown_rect = disp
            .control_tracking
            .as_ref()
            .map(|tracking| tracking.dropdown_rect)
            .expect("popup tracking should open a dropdown");
        let (dropdown_top, dropdown_left, dropdown_bottom, _) = dropdown_rect;
        assert_eq!(
            (dropdown_top, dropdown_left, dropdown_bottom),
            (10, 20, 42),
            "popup tracking should align selected item 1 with the control box, \
             not open below the control bottom"
        );
        assert_eq!(bus.read_word(sp + 12), 0xBEEF);

        disp.input_state.mouse_pos = (dropdown_top + 16 + 1, dropdown_left + 5);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(
            disp.control_tracking
                .as_ref()
                .map(|tracking| tracking.highlighted_item),
            Some(2)
        );

        disp.input_state.mouse_button = false;
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(bus.read_word(sp + 12), 10);
        assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 2);
        assert!(disp.control_tracking.is_none());
        let trace = disp.input_trace_text();
        assert!(trace.contains("A968 action=start start=(15,25)"));
        assert!(trace.contains("outcome=open_popup_tracking"));
        assert!(trace.contains("A968 action=tracking_update"));
        assert!(trace.contains("tracking=menu:idle dialog:idle control:active"));
        assert!(trace.contains("highlighted_item=2 outcome=popup_item_highlighted"));
        assert!(trace.contains("A968 action=tracking_finish"));
        assert!(trace.contains("part=10 highlighted_item=2 outcome=popup_item_selected"));
    }

    #[test]
    fn track_control_popup_systemless_theme_routes_highlight_through_menu_provider() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
        disp.enable_input_trace_capture();
        let sp = 0x300000u32;
        let window = *disp.current_port;
        let screen_base = bus.read_long(window + 2);
        let row_bytes = (bus.read_word(window + 6) & 0x3FFF) as u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut bus, screen_base, row_bytes, 342);

        let owner = bus.read_long(bus.read_long(cpu.read_reg(Register::A5)));
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 130), 255, 0);
        bus.write_long(ctrl_ptr + 4, owner);
        bus.write_word(ctrl_ptr + 18, 1);
        bus.write_word(ctrl_ptr + 20, 901);
        bus.write_word(ctrl_ptr + 22, 0);
        disp.control_proc_ids.insert(ctrl_ptr, 1009);
        disp.menus.push(Menu {
            id: 901,
            title: "Formation".to_string(),
            items: vec![
                MenuItem {
                    text: "Duke".to_string(),
                    icon: 0,
                    key_equiv: 0,
                    mark: 0,
                    style: 0,
                    enabled: true,
                },
                MenuItem {
                    text: "Carnage".to_string(),
                    icon: 0,
                    key_equiv: 0,
                    mark: 0,
                    style: 0,
                    enabled: true,
                },
            ],
            enabled: true,
            handle: 0,
            in_menu_bar: false,
            visible_in_menu_bar: false,
            hierarchical: false,
        });

        disp.input_state.mouse_button = true;
        disp.input_state.mouse_pos = (15, 25);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, 15);
        bus.write_word(sp + 6, 25);
        bus.write_long(sp + 8, ctrl_handle);
        bus.write_word(sp + 12, 0xBEEF);

        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let (dropdown_top, dropdown_left, _dropdown_bottom, dropdown_right) = disp
            .control_tracking
            .as_ref()
            .map(|tracking| tracking.dropdown_rect)
            .expect("popup tracking should open a dropdown");
        let item_two_top = dropdown_top + 1 + 16;
        let raw_inversion_probe_x = dropdown_right - 10;
        let raw_inversion_probe_y = item_two_top + 5;
        assert!(
            !screen_pixel_is_set(
                &bus,
                screen_base,
                row_bytes,
                raw_inversion_probe_x,
                raw_inversion_probe_y
            ),
            "unhighlighted popup item row should leave blank row space clear"
        );

        disp.input_state.mouse_pos = (item_two_top + 1, dropdown_left + 5);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(
            disp.control_tracking
                .as_ref()
                .map(|tracking| tracking.highlighted_item),
            Some(2)
        );
        let provider_rail_pixels = count_set_pixels(
            &bus,
            screen_base,
            row_bytes,
            item_two_top,
            dropdown_left,
            item_two_top + 16,
            dropdown_left + 12,
        );
        assert!(
            provider_rail_pixels > 0,
            "systemless-default popup tracking should redraw provider-owned item rail chrome"
        );
        assert!(
            !screen_pixel_is_set(
                &bus,
                screen_base,
                row_bytes,
                raw_inversion_probe_x,
                raw_inversion_probe_y
            ),
            "systemless-default popup tracking must not use raw full-row inversion"
        );

        disp.input_state.mouse_button = false;
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(bus.read_word(sp + 12), 10);
        assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 2);
        assert!(disp.control_tracking.is_none());
        let trace = disp.input_trace_text();
        assert!(trace.contains("A968 action=tracking_update"));
        assert!(trace.contains("highlighted_item=2 outcome=popup_item_highlighted"));
        assert!(trace.contains("part=10 highlighted_item=2 outcome=popup_item_selected"));
    }

    #[test]
    fn track_control_point_outside_returns_zero() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (ctrl_handle, _) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, 5);
        bus.write_word(sp + 6, 5);
        bus.write_long(sp + 8, ctrl_handle);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_word(sp + 12), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }

    #[test]
    fn track_control_inactive_or_invisible_control_returns_zero() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (inactive_handle, _) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 255);
        let (invisible_handle, _) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 0, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, 15);
        bus.write_word(sp + 6, 25);
        bus.write_long(sp + 8, inactive_handle);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 12), 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, 15);
        bus.write_word(sp + 6, 25);
        bus.write_long(sp + 8, invisible_handle);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 12), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }

    // DrawControls semantics per IM:I (1985) I-322 and MTE (1992) 5-87..5-88.
    #[test]
    fn drawcontrols_pops_window_arg_and_preserves_control_list_links() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = 0x300000u32;
        let a5 = cpu.read_reg(Register::A5);
        let qd_globals = bus.read_long(a5);
        let window_ptr = bus.read_long(qd_globals);

        let (first_handle, first_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
        let (second_handle, second_ptr) = alloc_control_handle(&mut bus, (18, 28, 38, 68), 255, 0);
        bus.write_long(first_ptr + 4, window_ptr);
        bus.write_long(second_ptr + 4, window_ptr);
        bus.write_long(first_ptr, 0);
        bus.write_long(second_ptr, first_handle);
        bus.write_long(window_ptr + 140, second_handle);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_ptr);
        disp.dispatch_control(true, 0x169, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_long(window_ptr + 140), second_handle);
        assert_eq!(bus.read_long(second_ptr), first_handle);
        assert_eq!(bus.read_long(first_ptr), 0);
    }

    #[test]
    fn drawcontrols_dispatches_visible_application_cdefs_in_control_list_draw_order() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = 0x300000u32;
        let return_pc = 0x1234_5678;
        let a5 = cpu.read_reg(Register::A5);
        let qd_globals = bus.read_long(a5);
        let window_ptr = bus.read_long(qd_globals);
        let proc_id = (160i16 << 4) | 5;
        let cdef_proc = disp.install_test_resource(&mut bus, *b"CDEF", 160, &[0x4E, 0x56, 0, 0]);

        let first_ptr = bus.alloc(296);
        let first_handle = bus.alloc(4);
        bus.write_long(first_handle, first_ptr);
        disp.initialize_control_record(
            &mut bus,
            first_ptr,
            window_ptr,
            (10, 20, 40, 80),
            b"",
            true,
            0,
            0,
            1,
            proc_id,
            0,
        );
        let second_ptr = bus.alloc(296);
        let second_handle = bus.alloc(4);
        bus.write_long(second_handle, second_ptr);
        disp.initialize_control_record(
            &mut bus,
            second_ptr,
            window_ptr,
            (50, 20, 80, 80),
            b"",
            true,
            0,
            0,
            1,
            proc_id,
            0,
        );
        bus.write_long(first_ptr, 0);
        bus.write_long(second_ptr, first_handle);
        bus.write_long(window_ptr + 140, second_handle);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        bus.write_long(sp, window_ptr);
        disp.dispatch_control(true, 0x169, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let first_tramp = disp.control_def_trampoline;
        assert_eq!(cpu.read_reg(Register::PC), first_tramp);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(bus.read_long(sp), return_pc);
        assert_eq!(bus.read_long(first_tramp + 16), first_handle);
        assert_eq!(
            bus.read_word(first_tramp + 22),
            super::super::TrapDispatcher::CDEF_DRAW_CNTL_MSG as u16
        );
        assert_eq!(bus.read_long(first_tramp + 32), cdef_proc);

        let second_tramp = bus.read_long(first_tramp + 56);
        assert_ne!(second_tramp, 0);
        assert_eq!(bus.read_long(second_tramp + 16), second_handle);
        assert_eq!(
            bus.read_word(second_tramp + 22),
            super::super::TrapDispatcher::CDEF_DRAW_CNTL_MSG as u16
        );
        assert_eq!(bus.read_long(second_tramp + 32), cdef_proc);
        assert_eq!(bus.read_word(second_tramp + 54), 0x2F3C);
        assert_eq!(bus.read_word(second_tramp + 60), 0xA873);
        assert_eq!(bus.read_word(second_tramp + 68), 0xAA31);
        assert_eq!(bus.read_word(second_tramp + 70), 0x4E75);
    }

    // FindControl semantics per IM:I (1985) I-323 and MTE (1992) 5-89.
    #[test]
    fn findcontrol_visible_active_hit_returns_inbutton_and_control_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let window_ptr = bus.alloc(200);
        let which_ctrl_out = bus.alloc(4);
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
        bus.write_long(ctrl_ptr, 0);
        bus.write_long(window_ptr + 140, ctrl_handle);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, which_ctrl_out);
        bus.write_long(sp + 4, window_ptr);
        bus.write_word(sp + 8, 15);
        bus.write_word(sp + 10, 25);
        disp.dispatch_control(true, 0x16C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_long(which_ctrl_out), ctrl_handle);
        assert_eq!(bus.read_word(sp + 12), 10);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }

    #[test]
    fn findcontrol_inactive_invisible_or_miss_returns_zero_and_nil() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let window_ptr = bus.alloc(200);
        let which_ctrl_out = bus.alloc(4);

        let (inactive_handle, inactive_ptr) =
            alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 255);
        bus.write_long(inactive_ptr, 0);
        bus.write_long(window_ptr + 140, inactive_handle);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(which_ctrl_out, 0xDEAD_BEEF);
        bus.write_long(sp, which_ctrl_out);
        bus.write_long(sp + 4, window_ptr);
        bus.write_word(sp + 8, 15);
        bus.write_word(sp + 10, 25);
        disp.dispatch_control(true, 0x16C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_long(which_ctrl_out), 0);
        assert_eq!(bus.read_word(sp + 12), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);

        let (invisible_handle, invisible_ptr) =
            alloc_control_handle(&mut bus, (10, 20, 30, 60), 0, 0);
        bus.write_long(invisible_ptr, 0);
        bus.write_long(window_ptr + 140, invisible_handle);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(which_ctrl_out, 0xDEAD_BEEF);
        bus.write_long(sp, which_ctrl_out);
        bus.write_long(sp + 4, window_ptr);
        bus.write_word(sp + 8, 15);
        bus.write_word(sp + 10, 25);
        disp.dispatch_control(true, 0x16C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_long(which_ctrl_out), 0);
        assert_eq!(bus.read_word(sp + 12), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);

        let (visible_handle, visible_ptr) =
            alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
        bus.write_long(visible_ptr, 0);
        bus.write_long(window_ptr + 140, visible_handle);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(which_ctrl_out, 0xDEAD_BEEF);
        bus.write_long(sp, which_ctrl_out);
        bus.write_long(sp + 4, window_ptr);
        bus.write_word(sp + 8, 5);
        bus.write_word(sp + 10, 5);
        disp.dispatch_control(true, 0x16C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_long(which_ctrl_out), 0);
        assert_eq!(bus.read_word(sp + 12), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }

    #[test]
    fn testcontrol_returns_inbutton_for_push_buttons_and_incheckbox_for_check_family() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;

        let (button_handle, button_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
        disp.control_proc_ids.insert(button_ptr, 0);
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 15);
        bus.write_word(sp + 2, 25);
        bus.write_long(sp + 4, button_handle);
        bus.write_word(sp + 8, 0xDEAD);
        disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 8), 10);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        let (checkbox_handle, checkbox_ptr) =
            alloc_control_handle(&mut bus, (40, 50, 80, 120), 255, 0);
        disp.control_proc_ids.insert(checkbox_ptr, 1);
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 60);
        bus.write_word(sp + 2, 70);
        bus.write_long(sp + 4, checkbox_handle);
        bus.write_word(sp + 8, 0xBEEF);
        disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 8), 11);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        let (radio_handle, radio_ptr) = alloc_control_handle(&mut bus, (90, 30, 120, 100), 255, 0);
        disp.control_proc_ids.insert(radio_ptr, 2);
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 100);
        bus.write_word(sp + 2, 40);
        bus.write_long(sp + 4, radio_handle);
        bus.write_word(sp + 8, 0xCAFE);
        disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 8), 11);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn testcontrol_returns_zero_for_inactive_invisible_or_miss() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;

        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
        disp.control_proc_ids.insert(ctrl_ptr, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 5);
        bus.write_word(sp + 2, 5);
        bus.write_long(sp + 4, ctrl_handle);
        bus.write_word(sp + 8, 0xDEAD);
        disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 8), 0);

        bus.write_byte(ctrl_ptr + 17, 255);
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 15);
        bus.write_word(sp + 2, 25);
        bus.write_long(sp + 4, ctrl_handle);
        bus.write_word(sp + 8, 0xBEEF);
        disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 8), 0);

        bus.write_byte(ctrl_ptr + 17, 0);
        bus.write_byte(ctrl_ptr + 16, 0);
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 15);
        bus.write_word(sp + 2, 25);
        bus.write_long(sp + 4, ctrl_handle);
        bus.write_word(sp + 8, 0xCAFE);
        disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 8), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn testcontrol_function_protocol_consumes_controlhandle_and_point_and_writes_integer_result() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
        disp.control_proc_ids.insert(ctrl_ptr, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 15);
        bus.write_word(sp + 2, 25);
        bus.write_long(sp + 4, ctrl_handle);
        bus.write_word(sp + 8, 0xCAFE);
        bus.write_word(sp + 10, 0xBEEF);

        disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_word(sp + 8), 10);
        assert_eq!(
            bus.read_word(sp + 10),
            0xBEEF,
            "TestControl must only overwrite the INTEGER result slot"
        );
    }

    #[test]
    fn testcontrol_returns_standard_scrollbar_part_codes_for_vertical_scrollbar() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let (scroll_handle, _) =
            alloc_scrollbar_control(&mut disp, &mut bus, (60, 240, 220, 256), 255, 0, 40, 0, 100);

        for (pt_v, pt_h, expected) in [
            (70i16, 248i16, 20u16),
            (95, 248, 22),
            (126, 248, 129),
            (150, 248, 23),
            (210, 248, 21),
        ] {
            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, pt_v as u16);
            bus.write_word(sp + 2, pt_h as u16);
            bus.write_long(sp + 4, scroll_handle);
            bus.write_word(sp + 8, 0xBEEF);
            bus.write_word(sp + 10, 0xCAFE);

            disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();

            assert_eq!(cpu.read_reg(Register::A7), sp + 8);
            assert_eq!(bus.read_word(sp + 8), expected);
            assert_eq!(bus.read_word(sp + 10), 0xCAFE);
        }
    }

    fn dispatch_testcontrol_part<C: CpuOps>(
        disp: &mut TrapDispatcher,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        sp: u32,
        ctrl_handle: u32,
        point: (i16, i16),
    ) -> (u16, u32, u16) {
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, point.0 as u16);
        bus.write_word(sp + 2, point.1 as u16);
        bus.write_long(sp + 4, ctrl_handle);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_word(sp + 10, 0xCAFE);

        disp.dispatch_control(true, 0x166, cpu, bus)
            .unwrap()
            .unwrap();

        (
            bus.read_word(sp + 8),
            cpu.read_reg(Register::A7),
            bus.read_word(sp + 10),
        )
    }

    fn testcontrol_results_for_theme(theme_id: UiThemeId) -> Vec<(&'static str, u16, u32, u16)> {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let sp = 0x300000u32;
        let mut results = Vec::new();

        let (button_handle, button_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
        disp.control_proc_ids.insert(button_ptr, 0);
        let (part, stack, tail) =
            dispatch_testcontrol_part(&mut disp, &mut cpu, &mut bus, sp, button_handle, (15, 25));
        results.push(("button", part, stack, tail));

        let (checkbox_handle, checkbox_ptr) =
            alloc_control_handle(&mut bus, (40, 50, 80, 120), 255, 0);
        disp.control_proc_ids.insert(checkbox_ptr, 1);
        let (part, stack, tail) =
            dispatch_testcontrol_part(&mut disp, &mut cpu, &mut bus, sp, checkbox_handle, (60, 70));
        results.push(("checkbox", part, stack, tail));

        let (radio_handle, radio_ptr) = alloc_control_handle(&mut bus, (90, 30, 120, 100), 255, 0);
        disp.control_proc_ids.insert(radio_ptr, 2);
        let (part, stack, tail) =
            dispatch_testcontrol_part(&mut disp, &mut cpu, &mut bus, sp, radio_handle, (100, 40));
        results.push(("radio", part, stack, tail));

        let (scroll_handle, _) =
            alloc_scrollbar_control(&mut disp, &mut bus, (60, 240, 220, 256), 255, 0, 40, 0, 100);
        for (name, point) in [
            ("scroll_up_arrow", (70i16, 248i16)),
            ("scroll_page_up", (95, 248)),
            ("scroll_thumb", (126, 248)),
            ("scroll_page_down", (150, 248)),
            ("scroll_down_arrow", (210, 248)),
        ] {
            let (part, stack, tail) =
                dispatch_testcontrol_part(&mut disp, &mut cpu, &mut bus, sp, scroll_handle, point);
            results.push((name, part, stack, tail));
        }

        let (outside_handle, outside_ptr) =
            alloc_control_handle(&mut bus, (130, 20, 160, 80), 255, 0);
        disp.control_proc_ids.insert(outside_ptr, 0);
        let (part, stack, tail) =
            dispatch_testcontrol_part(&mut disp, &mut cpu, &mut bus, sp, outside_handle, (120, 15));
        results.push(("outside", part, stack, tail));

        let (inactive_handle, inactive_ptr) =
            alloc_control_handle(&mut bus, (170, 20, 200, 80), 255, 255);
        disp.control_proc_ids.insert(inactive_ptr, 0);
        let (part, stack, tail) = dispatch_testcontrol_part(
            &mut disp,
            &mut cpu,
            &mut bus,
            sp,
            inactive_handle,
            (180, 30),
        );
        results.push(("inactive", part, stack, tail));

        let (invisible_handle, invisible_ptr) =
            alloc_control_handle(&mut bus, (210, 20, 240, 80), 0, 0);
        disp.control_proc_ids.insert(invisible_ptr, 0);
        let (part, stack, tail) = dispatch_testcontrol_part(
            &mut disp,
            &mut cpu,
            &mut bus,
            sp,
            invisible_handle,
            (220, 30),
        );
        results.push(("invisible", part, stack, tail));

        results
    }

    #[test]
    fn systemless_theme_does_not_change_testcontrol_part_codes() {
        // IM:I I-315 defines standard Control Manager part codes. IM:I I-325
        // specifies that TestControl returns those codes for visible active
        // controls, or 0 for misses, invisible controls, and inactive controls.
        // Theme chrome must not alter those guest-visible hit-test results.
        let classic = testcontrol_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = testcontrol_results_for_theme(UiThemeId::SystemlessDefault);
        let expected_stack = 0x300008u32;
        let expected_tail = 0xCAFEu16;

        assert_eq!(
            classic,
            vec![
                ("button", 10, expected_stack, expected_tail),
                ("checkbox", 11, expected_stack, expected_tail),
                ("radio", 11, expected_stack, expected_tail),
                ("scroll_up_arrow", 20, expected_stack, expected_tail),
                ("scroll_page_up", 22, expected_stack, expected_tail),
                ("scroll_thumb", 129, expected_stack, expected_tail),
                ("scroll_page_down", 23, expected_stack, expected_tail),
                ("scroll_down_arrow", 21, expected_stack, expected_tail),
                ("outside", 0, expected_stack, expected_tail),
                ("inactive", 0, expected_stack, expected_tail),
                ("invisible", 0, expected_stack, expected_tail),
            ]
        );
        assert_eq!(
            themed, classic,
            "systemless-default must not change TestControl part codes or stack protocol"
        );
    }

    // Draw1Control signature/no-op baseline per MTE (1992) 5-88.
    #[test]
    fn draw1control_nil_handle_is_noop_and_pops_arg() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);

        disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn testcontrol_custom_cdef_arms_test_message_with_point_and_result_slot() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let return_pc = 0x8765_4321;
        let proc_id = (160i16 << 4) | 7;
        let cdef_proc = disp.install_test_resource(&mut bus, *b"CDEF", 160, &[0x4E, 0x56, 0, 0]);
        let ctrl_ptr = bus.alloc(296);
        let ctrl_handle = bus.alloc(4);
        bus.write_long(ctrl_handle, ctrl_ptr);
        disp.initialize_control_record(
            &mut bus,
            ctrl_ptr,
            0,
            (10, 20, 40, 80),
            b"",
            true,
            0,
            0,
            1,
            proc_id,
            0,
        );
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        bus.write_word(sp, 23);
        bus.write_word(sp + 2, 45);
        bus.write_long(sp + 4, ctrl_handle);
        bus.write_word(sp + 8, 0);

        disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let tramp = disp.control_def_trampoline;
        assert_eq!(cpu.read_reg(Register::PC), tramp);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_long(sp + 4), return_pc);
        assert_eq!(bus.read_word(tramp + 12), 7);
        assert_eq!(bus.read_long(tramp + 16), ctrl_handle);
        assert_eq!(
            bus.read_word(tramp + 22),
            super::super::TrapDispatcher::CDEF_TEST_CNTL_MSG as u16
        );
        assert_eq!(bus.read_long(tramp + 26), (23u32 << 16) | 45);
        assert_eq!(bus.read_long(tramp + 32), cdef_proc);
        assert_eq!(bus.read_long(tramp + 40), sp + 8);
    }

    #[test]
    fn draw1control_treats_any_nonzero_contrlvis_as_visible() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(16 * 128);
        let row_bytes = 16u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 128, 128, 1);
        bus.write_long(0x0824, screen_base);
        bus.write_word(0x0828, row_bytes as u16);
        clear_1bpp_screen(&mut bus, screen_base, row_bytes, 128);

        let window_ptr = *disp.current_port;
        bus.write_word(window_ptr + 16, 0);
        bus.write_word(window_ptr + 18, 0);
        bus.write_word(window_ptr + 20, 128);
        bus.write_word(window_ptr + 22, 128);
        let ctrl_ptr = bus.alloc(296);
        let ctrl_handle = bus.alloc(4);
        bus.write_long(ctrl_handle, ctrl_ptr);
        disp.initialize_control_record(
            &mut bus,
            ctrl_ptr,
            window_ptr,
            (20, 20, 44, 100),
            b"Play",
            false,
            0,
            0,
            1,
            0,
            0,
        );
        bus.write_byte(ctrl_ptr + 16, 1);

        let sp = 0x300000u32;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctrl_handle);
        disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert!(
            count_set_pixels(&bus, screen_base, row_bytes, 20, 20, 44, 100) > 40,
            "drawCntl skips only contrlVis == 0 (Inside Macintosh I-329)"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn draw1control_unknown_cdef_pict_title_draws_resource_pair() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = 0x300000u32;
        let window = 0x181000u32;
        disp.set_current_port_for_test(window);
        let base = bus.read_long(window + 2);
        let row_bytes = (bus.read_word(window + 6) & 0x3FFF) as u32;
        disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);

        disp.install_test_resource(
            &mut bus,
            *b"PICT",
            4096,
            &fill_rect_pict_resource((0, 0, 8, 8), (0, 0, 8, 4)),
        );
        disp.install_test_resource(
            &mut bus,
            *b"PICT",
            4097,
            &fill_rect_pict_resource((0, 0, 8, 8), (0, 4, 8, 8)),
        );

        let (ctrl_handle, ctrl_ptr) =
            alloc_button_control(&mut disp, &mut bus, window, (20, 20, 40, 100));
        disp.control_proc_ids.insert(ctrl_ptr, 3216);
        bus.write_byte(ctrl_ptr + 40, 4);
        bus.write_word(ctrl_ptr + 41, 4096);
        bus.write_word(ctrl_ptr + 43, 4097);

        clear_1bpp_screen(&mut bus, base, row_bytes, 342);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctrl_handle);
        disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, 35, 30),
            "normal unknown CDEF with a PICT-pair title should draw the first PICT"
        );
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, 85, 30),
            "normal state should not draw the highlighted PICT"
        );

        clear_1bpp_screen(&mut bus, base, row_bytes, 342);
        bus.write_byte(ctrl_ptr + 17, 1);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctrl_handle);
        disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, 35, 30),
            "highlighted state should not draw the normal PICT"
        );
        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, 85, 30),
            "highlighted unknown CDEF with a PICT-pair title should draw the second PICT"
        );
    }

    #[test]
    fn draw1control_unknown_cdef_falls_back_to_button_title_chrome() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = 0x300000u32;
        let window = 0x181000u32;
        disp.set_current_port_for_test(window);
        let base = bus.read_long(window + 2);
        let row_bytes = (bus.read_word(window + 6) & 0x3FFF) as u32;
        disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut bus, base, row_bytes, 342);

        let (blank_handle, blank_ptr) =
            alloc_button_control(&mut disp, &mut bus, window, (20, 20, 40, 140));
        let (titled_handle, titled_ptr) =
            alloc_button_control(&mut disp, &mut bus, window, (60, 20, 80, 140));
        for ptr in [blank_ptr, titled_ptr] {
            disp.control_proc_ids.insert(ptr, 3216);
        }
        bus.write_byte(titled_ptr + 40, 4);
        bus.write_bytes(titled_ptr + 41, b"PLAY");

        for handle in [blank_handle, titled_handle] {
            cpu.write_reg(Register::A7, sp);
            bus.write_long(sp, handle);
            disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        }

        let blank_chrome = count_set_pixels(&bus, base, row_bytes, 20, 20, 40, 140);
        assert!(
            blank_chrome > 0,
            "unknown CDEF fallback should draw visible control chrome"
        );

        let mut title_differences = 0;
        for dy in 0..20 {
            for dx in 0..120 {
                let blank_pixel = screen_pixel_is_set(&bus, base, row_bytes, 20 + dx, 20 + dy);
                let titled_pixel = screen_pixel_is_set(&bus, base, row_bytes, 20 + dx, 60 + dy);
                if blank_pixel != titled_pixel {
                    title_differences += 1;
                }
            }
        }
        assert!(
            title_differences > 8,
            "unknown CDEF fallback should draw the ControlRecord title"
        );

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 65);
        bus.write_word(sp + 2, 30);
        bus.write_long(sp + 4, titled_handle);
        bus.write_word(sp + 8, 0xDEAD);
        disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 8), 10);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn draw1control_popup_control_preserves_current_gdevice_and_pops_arg() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = 0x300000u32;
        let window_ptr = *disp.current_port;
        let gdevice_before = *disp.current_gdevice;
        let (ctrl_handle, ctrl_ptr) =
            alloc_button_control(&mut disp, &mut bus, window_ptr, (20, 20, 260, 60));
        disp.control_proc_ids.insert(ctrl_ptr, 1008);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctrl_handle);

        disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(*disp.current_gdevice, gdevice_before);
    }

    #[test]
    fn background_scrollbar_drawing_is_clipped_behind_front_window() {
        // Standard CDEF drawing is clipped by the owner window's visRgn.
        // A background window's scrollbar must therefore never paint through
        // a front window even though the HLE scrollbar renderer writes direct
        // framebuffer pixels. IM:I I-145 and I-326; MTE 1992 pp. 4-69..4-70.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = 0x300000u32;
        disp.set_screen_mode_for_test(screen_base, 320, 320, 240, 8);
        bus.write_long(0x0824, screen_base);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let back = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            back,
            screen_base,
            40,
            40,
            220,
            280,
            "Back",
            0,
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
            90,
            90,
            200,
            220,
            "Front",
            0,
            true,
            false,
            false,
            0,
        );
        assert_eq!(disp.window_list, vec![front, back]);

        // Back-local (60,40..76,200) maps to global (100,80..116,240),
        // crossing straight through the front content.
        let (_handle, ctrl_ptr) =
            alloc_scrollbar_control(&mut disp, &mut bus, (60, 40, 76, 200), 0xFF, 0, 5, 0, 10);
        bus.write_long(ctrl_ptr + 4, back);

        for y in 100u32..116 {
            for x in 90u32..220 {
                bus.write_byte(screen_base + y * 320 + x, 0x4D);
            }
        }
        // An uncovered portion should still prove the scrollbar was drawn.
        for y in 100u32..116 {
            for x in 80u32..90 {
                bus.write_byte(screen_base + y * 320 + x, 0x4D);
            }
        }

        disp.draw_control(&mut cpu, &mut bus, ctrl_ptr);

        for y in 100u32..116 {
            for x in 90u32..220 {
                assert_eq!(
                    bus.read_byte(screen_base + y * 320 + x),
                    0x4D,
                    "background scrollbar painted through front window at ({x},{y})"
                );
            }
        }
        assert!(
            (100u32..116)
                .any(|y| { (80u32..90).any(|x| bus.read_byte(screen_base + y * 320 + x) != 0x4D) }),
            "visible portion of the background scrollbar should still draw"
        );
    }

    #[test]
    fn draw1control_systemless_theme_routes_popup_control_chrome_through_provider() {
        let sp = 0x300000u32;
        let bounds = (20, 20, 40, 120);

        let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
        let classic_window = *classic.current_port;
        let classic_base = classic_bus.read_long(classic_window + 2);
        let classic_row_bytes = (classic_bus.read_word(classic_window + 6) & 0x3FFF) as u32;
        classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
        let (classic_handle, classic_ptr) =
            alloc_button_control(&mut classic, &mut classic_bus, classic_window, bounds);
        classic.control_proc_ids.insert(classic_ptr, 1008);
        classic_cpu.write_reg(Register::A7, sp);
        classic_bus.write_long(sp, classic_handle);
        classic
            .dispatch_control(true, 0x16D, &mut classic_cpu, &mut classic_bus)
            .unwrap()
            .unwrap();

        let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        let themed_window = *themed.current_port;
        let themed_base = themed_bus.read_long(themed_window + 2);
        let themed_row_bytes = (themed_bus.read_word(themed_window + 6) & 0x3FFF) as u32;
        themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
        let (themed_handle, themed_ptr) =
            alloc_button_control(&mut themed, &mut themed_bus, themed_window, bounds);
        themed.control_proc_ids.insert(themed_ptr, 1008);
        themed_cpu.write_reg(Register::A7, sp);
        themed_bus.write_long(sp, themed_handle);
        themed
            .dispatch_control(true, 0x16D, &mut themed_cpu, &mut themed_bus)
            .unwrap()
            .unwrap();

        assert!(
            screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 100, 37),
            "classic popup CDEF should draw its resolved auto-width offset shadow"
        );
        assert!(
            screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 101, 37),
            "classic popup CDEF should extend the resolved shadow one pixel past the provider frame"
        );
        assert!(
            !screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 101, 37),
            "systemless-default popup provider should not draw the classic resolved shadow extension"
        );

        themed_cpu.write_reg(Register::A7, sp);
        themed_bus.write_word(sp, 25);
        themed_bus.write_word(sp + 2, 25);
        themed_bus.write_long(sp + 4, themed_handle);
        themed_bus.write_word(sp + 8, 0xDEAD);
        themed
            .dispatch_control(true, 0x166, &mut themed_cpu, &mut themed_bus)
            .unwrap()
            .unwrap();
        assert_eq!(themed_bus.read_word(sp + 8), 10);
        assert_eq!(themed_cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn draw1control_systemless_theme_routes_popup_control_hilite_states_through_provider() {
        let sp = 0x300000u32;
        let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        let themed_window = *themed.current_port;
        let row_bytes = 64u32;
        let base = themed_bus.alloc(row_bytes * 342);
        themed.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut themed_bus, base, row_bytes, 342);

        let (normal_handle, normal_ptr) = alloc_button_control(
            &mut themed,
            &mut themed_bus,
            themed_window,
            (20, 20, 40, 120),
        );
        let (pressed_handle, pressed_ptr) = alloc_button_control(
            &mut themed,
            &mut themed_bus,
            themed_window,
            (52, 20, 72, 120),
        );
        let (inactive_handle, inactive_ptr) = alloc_button_control(
            &mut themed,
            &mut themed_bus,
            themed_window,
            (84, 20, 104, 120),
        );
        for ptr in [normal_ptr, pressed_ptr, inactive_ptr] {
            themed.control_proc_ids.insert(ptr, 1008);
        }
        themed_bus.write_byte(pressed_ptr + 17, 1);
        themed_bus.write_byte(inactive_ptr + 17, 255);

        for handle in [normal_handle, pressed_handle, inactive_handle] {
            themed_cpu.write_reg(Register::A7, sp);
            themed_bus.write_long(sp, handle);
            themed
                .dispatch_control(true, 0x16D, &mut themed_cpu, &mut themed_bus)
                .unwrap()
                .unwrap();
        }

        assert!(
            !screen_pixel_is_set(&themed_bus, base, row_bytes, 22, 30),
            "normal systemless-default popup should keep the state inset clear"
        );
        assert!(
            screen_pixel_is_set(&themed_bus, base, row_bytes, 30, 57),
            "pressed systemless-default popup should route hilite state to provider fill"
        );
        let normal_chrome = count_set_pixels(&themed_bus, base, row_bytes, 21, 20, 38, 100);
        let inactive_chrome = count_set_pixels(&themed_bus, base, row_bytes, 85, 20, 102, 100);
        assert!(
            inactive_chrome > normal_chrome,
            "inactive systemless-default popup should route HiliteControl state to provider chrome ({inactive_chrome} <= {normal_chrome})"
        );
    }

    #[test]
    fn draw1control_systemless_theme_routes_push_button_chrome_through_provider() {
        let sp = 0x300000u32;
        let bounds = (20, 20, 40, 80);

        let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
        let classic_window = *classic.current_port;
        let classic_base = classic_bus.read_long(classic_window + 2);
        let classic_row_bytes = (classic_bus.read_word(classic_window + 6) & 0x3FFF) as u32;
        classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
        let (classic_handle, _) =
            alloc_button_control(&mut classic, &mut classic_bus, classic_window, bounds);
        classic_cpu.write_reg(Register::A7, sp);
        classic_bus.write_long(sp, classic_handle);
        classic
            .dispatch_control(true, 0x16D, &mut classic_cpu, &mut classic_bus)
            .unwrap()
            .unwrap();

        let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        let themed_window = *themed.current_port;
        let themed_base = themed_bus.read_long(themed_window + 2);
        let themed_row_bytes = (themed_bus.read_word(themed_window + 6) & 0x3FFF) as u32;
        themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
        let (themed_handle, _) =
            alloc_button_control(&mut themed, &mut themed_bus, themed_window, bounds);
        themed_cpu.write_reg(Register::A7, sp);
        themed_bus.write_long(sp, themed_handle);
        themed
            .dispatch_control(true, 0x16D, &mut themed_cpu, &mut themed_bus)
            .unwrap()
            .unwrap();

        assert!(
            !screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 20, 20),
            "classic round-rect CDEF should leave the outer corner as background"
        );
        assert!(
            screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 20, 20),
            "systemless-default provider chrome should own the HLE control corner pixel"
        );

        themed_cpu.write_reg(Register::A7, sp);
        themed_bus.write_word(sp, 25);
        themed_bus.write_word(sp + 2, 25);
        themed_bus.write_long(sp + 4, themed_handle);
        themed_bus.write_word(sp + 8, 0xDEAD);
        themed
            .dispatch_control(true, 0x166, &mut themed_cpu, &mut themed_bus)
            .unwrap()
            .unwrap();
        assert_eq!(themed_bus.read_word(sp + 8), 10);
        assert_eq!(themed_cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn draw1control_systemless_theme_routes_button_hilite_states_through_provider() {
        let sp = 0x300000u32;
        let (mut themed, mut cpu, mut bus) = setup_with_port();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        let window = *themed.current_port;
        let row_bytes = 64u32;
        let base = bus.alloc(row_bytes * 342);
        themed.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut bus, base, row_bytes, 342);

        let (enabled_handle, _enabled_ptr) =
            alloc_button_control(&mut themed, &mut bus, window, (20, 20, 40, 80));
        let (pressed_handle, pressed_ptr) =
            alloc_button_control(&mut themed, &mut bus, window, (60, 20, 80, 80));
        let (inactive_handle, inactive_ptr) =
            alloc_button_control(&mut themed, &mut bus, window, (100, 20, 120, 80));
        bus.write_byte(pressed_ptr + 17, 1);
        bus.write_byte(inactive_ptr + 17, 255);

        for handle in [enabled_handle, pressed_handle, inactive_handle] {
            cpu.write_reg(Register::A7, sp);
            bus.write_long(sp, handle);
            themed
                .dispatch_control(true, 0x16D, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        }

        let enabled_body = count_set_pixels(&bus, base, row_bytes, 23, 23, 37, 77);
        let pressed_body = count_set_pixels(&bus, base, row_bytes, 63, 23, 77, 77);
        assert!(
            pressed_body > enabled_body,
            "systemless-default should route contrlHilite=1 into the provider pressed state ({pressed_body} <= {enabled_body})"
        );

        // MTE 1992 glossary, "inactive control": inactive controls are
        // visually indicated and do not respond as active controls.
        let inactive_body = count_set_pixels(&bus, base, row_bytes, 103, 23, 117, 77);
        assert!(
            pressed_body > inactive_body,
            "systemless-default should keep inactive button chrome distinct from pressed chrome"
        );

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 65);
        bus.write_word(sp + 2, 25);
        bus.write_long(sp + 4, pressed_handle);
        bus.write_word(sp + 8, 0xDEAD);
        themed
            .dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 8), 10);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 105);
        bus.write_word(sp + 2, 25);
        bus.write_long(sp + 4, inactive_handle);
        bus.write_word(sp + 8, 0xDEAD);
        themed
            .dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 8), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn draw1control_systemless_theme_routes_checkbox_and_radio_chrome_through_provider() {
        let sp = 0x300000u32;

        let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
        let classic_window = *classic.current_port;
        let classic_base = classic_bus.read_long(classic_window + 2);
        let classic_row_bytes = (classic_bus.read_word(classic_window + 6) & 0x3FFF) as u32;
        classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
        let (classic_checkbox_handle, classic_checkbox_ptr) = alloc_button_control(
            &mut classic,
            &mut classic_bus,
            classic_window,
            (20, 20, 40, 120),
        );
        classic.control_proc_ids.insert(classic_checkbox_ptr, 1);
        classic_bus.write_word(classic_checkbox_ptr + 18, 1);
        let (classic_radio_handle, classic_radio_ptr) = alloc_button_control(
            &mut classic,
            &mut classic_bus,
            classic_window,
            (50, 20, 70, 120),
        );
        classic.control_proc_ids.insert(classic_radio_ptr, 2);
        classic_bus.write_word(classic_radio_ptr + 18, 1);
        for handle in [classic_checkbox_handle, classic_radio_handle] {
            classic_cpu.write_reg(Register::A7, sp);
            classic_bus.write_long(sp, handle);
            classic
                .dispatch_control(true, 0x16D, &mut classic_cpu, &mut classic_bus)
                .unwrap()
                .unwrap();
        }

        let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        let themed_window = *themed.current_port;
        let themed_base = themed_bus.read_long(themed_window + 2);
        let themed_row_bytes = (themed_bus.read_word(themed_window + 6) & 0x3FFF) as u32;
        themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
        let (themed_checkbox_handle, themed_checkbox_ptr) = alloc_button_control(
            &mut themed,
            &mut themed_bus,
            themed_window,
            (20, 20, 40, 120),
        );
        themed.control_proc_ids.insert(themed_checkbox_ptr, 1);
        themed_bus.write_word(themed_checkbox_ptr + 18, 1);
        let (themed_radio_handle, themed_radio_ptr) = alloc_button_control(
            &mut themed,
            &mut themed_bus,
            themed_window,
            (50, 20, 70, 120),
        );
        themed.control_proc_ids.insert(themed_radio_ptr, 2);
        themed_bus.write_word(themed_radio_ptr + 18, 1);
        for handle in [themed_checkbox_handle, themed_radio_handle] {
            themed_cpu.write_reg(Register::A7, sp);
            themed_bus.write_long(sp, handle);
            themed
                .dispatch_control(true, 0x16D, &mut themed_cpu, &mut themed_bus)
                .unwrap()
                .unwrap();
        }

        let classic_checkbox_pixels = count_set_pixels(
            &classic_bus,
            classic_base,
            classic_row_bytes,
            24,
            22,
            36,
            34,
        );
        let themed_checkbox_pixels =
            count_set_pixels(&themed_bus, themed_base, themed_row_bytes, 24, 22, 36, 34);
        assert!(
            themed_checkbox_pixels > classic_checkbox_pixels,
            "systemless-default checkbox provider should draw a denser selected mark ({themed_checkbox_pixels} <= {classic_checkbox_pixels})"
        );
        let classic_radio_pixels = count_set_pixels(
            &classic_bus,
            classic_base,
            classic_row_bytes,
            54,
            22,
            66,
            34,
        );
        let themed_radio_pixels =
            count_set_pixels(&themed_bus, themed_base, themed_row_bytes, 54, 22, 66, 34);
        assert!(
            themed_radio_pixels > classic_radio_pixels,
            "systemless-default radio provider should draw a denser selected mark ({themed_radio_pixels} <= {classic_radio_pixels})"
        );

        themed_cpu.write_reg(Register::A7, sp);
        themed_bus.write_word(sp, 25);
        themed_bus.write_word(sp + 2, 25);
        themed_bus.write_long(sp + 4, themed_checkbox_handle);
        themed_bus.write_word(sp + 8, 0xDEAD);
        themed
            .dispatch_control(true, 0x166, &mut themed_cpu, &mut themed_bus)
            .unwrap()
            .unwrap();
        assert_eq!(themed_bus.read_word(sp + 8), 11);
        assert_eq!(themed_cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn draw1control_systemless_theme_routes_checkbox_radio_hilite_states_through_provider() {
        let sp = 0x300000u32;
        let (mut themed, mut cpu, mut bus) = setup_with_port();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        let window = *themed.current_port;
        let row_bytes = 64u32;
        let base = bus.alloc(row_bytes * 342);
        themed.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut bus, base, row_bytes, 342);

        let (checkbox_enabled_handle, checkbox_enabled_ptr) =
            alloc_button_control(&mut themed, &mut bus, window, (20, 20, 40, 100));
        let (checkbox_pressed_handle, checkbox_pressed_ptr) =
            alloc_button_control(&mut themed, &mut bus, window, (52, 20, 72, 100));
        let (checkbox_inactive_handle, checkbox_inactive_ptr) =
            alloc_button_control(&mut themed, &mut bus, window, (84, 20, 104, 100));
        for ptr in [
            checkbox_enabled_ptr,
            checkbox_pressed_ptr,
            checkbox_inactive_ptr,
        ] {
            themed.control_proc_ids.insert(ptr, 1);
        }
        bus.write_byte(checkbox_pressed_ptr + 17, 1);
        bus.write_byte(checkbox_inactive_ptr + 17, 255);

        let (radio_enabled_handle, radio_enabled_ptr) =
            alloc_button_control(&mut themed, &mut bus, window, (20, 140, 40, 220));
        let (radio_pressed_handle, radio_pressed_ptr) =
            alloc_button_control(&mut themed, &mut bus, window, (52, 140, 72, 220));
        let (radio_inactive_handle, radio_inactive_ptr) =
            alloc_button_control(&mut themed, &mut bus, window, (84, 140, 104, 220));
        for ptr in [radio_enabled_ptr, radio_pressed_ptr, radio_inactive_ptr] {
            themed.control_proc_ids.insert(ptr, 2);
        }
        bus.write_byte(radio_pressed_ptr + 17, 1);
        bus.write_byte(radio_inactive_ptr + 17, 255);

        for handle in [
            checkbox_enabled_handle,
            checkbox_pressed_handle,
            checkbox_inactive_handle,
            radio_enabled_handle,
            radio_pressed_handle,
            radio_inactive_handle,
        ] {
            cpu.write_reg(Register::A7, sp);
            bus.write_long(sp, handle);
            themed
                .dispatch_control(true, 0x16D, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        }

        let checkbox_enabled_body = count_set_pixels(&bus, base, row_bytes, 24, 22, 36, 34);
        let checkbox_pressed_body = count_set_pixels(&bus, base, row_bytes, 56, 22, 68, 34);
        let checkbox_inactive_body = count_set_pixels(&bus, base, row_bytes, 88, 22, 100, 34);
        assert!(
            checkbox_pressed_body > checkbox_enabled_body,
            "systemless-default should route checkbox contrlHilite=1 to provider pressed fill ({checkbox_pressed_body} <= {checkbox_enabled_body})"
        );
        assert!(
            checkbox_pressed_body > checkbox_inactive_body,
            "systemless-default should keep inactive checkbox chrome distinct from pressed chrome"
        );

        let radio_enabled_body = count_set_pixels(&bus, base, row_bytes, 24, 142, 36, 154);
        let radio_pressed_body = count_set_pixels(&bus, base, row_bytes, 56, 142, 68, 154);
        let radio_inactive_body = count_set_pixels(&bus, base, row_bytes, 88, 142, 100, 154);
        assert!(
            radio_pressed_body > radio_enabled_body,
            "systemless-default should route radio contrlHilite=1 to provider pressed fill ({radio_pressed_body} <= {radio_enabled_body})"
        );
        assert!(
            radio_pressed_body > radio_inactive_body,
            "systemless-default should keep inactive radio chrome distinct from pressed chrome"
        );

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 57);
        bus.write_word(sp + 2, 145);
        bus.write_long(sp + 4, radio_pressed_handle);
        bus.write_word(sp + 8, 0xDEAD);
        themed
            .dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 8), 11);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn scrollbar_themes_share_the_presentation_provider_and_keep_part_codes() {
        let sp = 0x300000u32;
        let bounds = (20, 20, 180, 36);

        let (mut classic, _classic_cpu, mut classic_bus) = setup_with_port();
        let classic_window = *classic.current_port;
        let classic_base = classic_bus.read_long(classic_window + 2);
        let classic_row_bytes = (classic_bus.read_word(classic_window + 6) & 0x3FFF) as u32;
        classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
        let (_classic_handle, classic_ptr) =
            alloc_button_control(&mut classic, &mut classic_bus, classic_window, bounds);
        classic.control_proc_ids.insert(classic_ptr, 16);
        classic_bus.write_word(classic_ptr + 18, 40);
        classic_bus.write_word(classic_ptr + 20, 0);
        classic_bus.write_word(classic_ptr + 22, 100);
        assert!(
            classic.draw_theme_scrollbar_chrome(
                &mut classic_bus,
                bounds.0,
                bounds.1,
                bounds.2,
                bounds.3,
                40,
                0,
                100,
                0,
            ),
            "classic-system7 should use the architecture-neutral scrollbar renderer"
        );

        let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        let themed_window = *themed.current_port;
        let themed_base = themed_bus.read_long(themed_window + 2);
        let themed_row_bytes = (themed_bus.read_word(themed_window + 6) & 0x3FFF) as u32;
        themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
        let (themed_handle, themed_ptr) =
            alloc_button_control(&mut themed, &mut themed_bus, themed_window, bounds);
        themed.control_proc_ids.insert(themed_ptr, 16);
        themed_bus.write_word(themed_ptr + 18, 40);
        themed_bus.write_word(themed_ptr + 20, 0);
        themed_bus.write_word(themed_ptr + 22, 100);
        assert!(
            themed.draw_theme_scrollbar_chrome(
                &mut themed_bus,
                bounds.0,
                bounds.1,
                bounds.2,
                bounds.3,
                40,
                0,
                100,
                0,
            ),
            "systemless-default routes scrollBarProc chrome through the theme provider"
        );

        themed_cpu.write_reg(Register::A7, sp);
        themed_bus.write_word(sp, 85);
        themed_bus.write_word(sp + 2, 28);
        themed_bus.write_long(sp + 4, themed_handle);
        themed_bus.write_word(sp + 8, 0xDEAD);
        themed
            .dispatch_control(true, 0x166, &mut themed_cpu, &mut themed_bus)
            .unwrap()
            .unwrap();
        assert_eq!(themed_bus.read_word(sp + 8), 129);
        assert_eq!(themed_cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn draw1control_systemless_theme_routes_scrollbar_hilite_states_through_provider() {
        let sp = 0x300000u32;
        let (mut themed, mut cpu, mut bus) = setup_with_port();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        let row_bytes = 64u32;
        let base = bus.alloc(row_bytes * 342);
        themed.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
        clear_1bpp_screen(&mut bus, base, row_bytes, 342);

        let (normal_handle, normal_ptr) =
            alloc_scrollbar_control(&mut themed, &mut bus, (20, 20, 100, 36), 255, 0, 40, 0, 100);
        let (arrow_handle, _) = alloc_scrollbar_control(
            &mut themed,
            &mut bus,
            (20, 52, 100, 68),
            255,
            20,
            40,
            0,
            100,
        );
        let (page_handle, _) = alloc_scrollbar_control(
            &mut themed,
            &mut bus,
            (20, 84, 100, 100),
            255,
            22,
            40,
            0,
            100,
        );
        let (inactive_handle, inactive_ptr) = alloc_scrollbar_control(
            &mut themed,
            &mut bus,
            (20, 116, 100, 132),
            255,
            255,
            40,
            0,
            100,
        );

        for handle in [normal_handle, arrow_handle, page_handle, inactive_handle] {
            cpu.write_reg(Register::A7, sp);
            bus.write_long(sp, handle);
            themed
                .dispatch_control(true, 0x16D, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        }

        let normal_arrow_fill = count_set_pixels(&bus, base, row_bytes, 21, 21, 35, 35);
        let pressed_arrow_fill = count_set_pixels(&bus, base, row_bytes, 21, 53, 35, 67);
        assert!(
            pressed_arrow_fill > normal_arrow_fill,
            "systemless-default should route scrollbar arrow part hilite through provider fill ({pressed_arrow_fill} <= {normal_arrow_fill})"
        );

        let normal_page_before_fill = count_set_pixels(&bus, base, row_bytes, 37, 22, 47, 34);
        let pressed_page_before_fill = count_set_pixels(&bus, base, row_bytes, 37, 86, 47, 98);
        assert!(
            pressed_page_before_fill > normal_page_before_fill,
            "systemless-default should route scrollbar page-region hilite through provider fill ({pressed_page_before_fill} <= {normal_page_before_fill})"
        );

        assert!(
            screen_pixel_is_set(&bus, base, row_bytes, 28, 56),
            "enabled scrollbar should draw its provider thumb"
        );
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, 124, 56),
            "inactive scrollbar should suppress the provider thumb"
        );

        // IM:I I-313 / I-323: hilite values 1..253 name active parts,
        // while 255 makes the control inactive and hit testing returns no part.
        for (handle, pt_v, pt_h, expected) in [
            (arrow_handle, 24i16, 60i16, 20u16),
            (page_handle, 40, 92, 22),
            (normal_handle, 56, 28, 129),
            (inactive_handle, 56, 124, 0),
        ] {
            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, pt_v as u16);
            bus.write_word(sp + 2, pt_h as u16);
            bus.write_long(sp + 4, handle);
            bus.write_word(sp + 8, 0xDEAD);
            themed
                .dispatch_control(true, 0x166, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(bus.read_word(sp + 8), expected);
            assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        }

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 20);
        bus.write_long(sp + 2, normal_handle);
        themed
            .dispatch_control(true, 0x15D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_byte(normal_ptr + 17), 20);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert!(
            count_set_pixels(&bus, base, row_bytes, 21, 21, 35, 35) > normal_arrow_fill,
            "HiliteControl should redraw scroll-bar part hilites through the provider"
        );

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 255);
        bus.write_long(sp + 2, normal_handle);
        themed
            .dispatch_control(true, 0x15D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_byte(normal_ptr + 17), 255);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, 28, 56),
            "HiliteControl(255) should redraw the inactive provider state"
        );
        assert_eq!(bus.read_byte(inactive_ptr + 17), 255);
    }

    #[test]
    fn draw1control_out_of_range_handle_is_noop_and_pops_arg() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, bus.ram_size() + 4);

        disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn draw1control_handle_pointing_at_out_of_range_control_ptr_is_noop_and_pops_arg() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let handle = bus.alloc(4);
        assert_ne!(handle, 0);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, handle);
        bus.write_long(handle, bus.ram_size() + 0x1000);

        disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn draw1control_handle_pointing_to_truncated_control_record_is_noop_and_pops_arg() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let handle = bus.alloc(4);
        let ctrl_ptr = bus.ram_size().saturating_sub(41);
        assert_ne!(handle, 0);
        assert!(ctrl_ptr > 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, handle);
        bus.write_long(handle, ctrl_ptr);
        bus.write_byte(ctrl_ptr + 40, 16);

        disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn dragcontrol_pops_18_bytes_and_leaves_contrlrect_unchanged_on_no_drag_path() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = 0x300000u32;
        let window_ptr = *disp.current_port;
        let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (88, 96, 132, 220), 0, 0);
        let limit_rect_ptr = bus.alloc(8);
        let slop_rect_ptr = bus.alloc(8);
        let top_before = bus.read_word(ctrl_ptr + 8);
        let left_before = bus.read_word(ctrl_ptr + 10);
        let bottom_before = bus.read_word(ctrl_ptr + 12);
        let right_before = bus.read_word(ctrl_ptr + 14);

        bus.write_long(ctrl_ptr + 4, window_ptr);
        bus.write_long(window_ptr + 140, ctrl_handle);

        bus.write_word(limit_rect_ptr, 0);
        bus.write_word(limit_rect_ptr + 2, 0);
        bus.write_word(limit_rect_ptr + 4, 400);
        bus.write_word(limit_rect_ptr + 6, 640);
        bus.write_word(slop_rect_ptr, (-40i16) as u16);
        bus.write_word(slop_rect_ptr + 2, (-40i16) as u16);
        bus.write_word(slop_rect_ptr + 4, (-20i16) as u16);
        bus.write_word(slop_rect_ptr + 6, (-20i16) as u16);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0); // axis
        bus.write_long(sp + 2, slop_rect_ptr);
        bus.write_long(sp + 6, limit_rect_ptr);
        bus.write_word(sp + 10, 12); // startPt.v
        bus.write_word(sp + 12, 18); // startPt.h
        bus.write_long(sp + 14, ctrl_handle);

        disp.dispatch_control(true, 0x167, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 18);
        assert_eq!(bus.read_word(ctrl_ptr + 8), top_before);
        assert_eq!(bus.read_word(ctrl_ptr + 10), left_before);
        assert_eq!(bus.read_word(ctrl_ptr + 12), bottom_before);
        assert_eq!(bus.read_word(ctrl_ptr + 14), right_before);
    }

    fn alloc_button_control(
        disp: &mut super::super::TrapDispatcher,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        bounds: (i16, i16, i16, i16),
    ) -> (u32, u32) {
        let ctrl_ptr = bus.alloc(48);
        let ctrl_handle = bus.alloc(4);
        bus.write_long(ctrl_handle, ctrl_ptr);
        bus.write_long(ctrl_ptr, 0);
        bus.write_long(ctrl_ptr + 4, window_ptr);
        bus.write_word(ctrl_ptr + 8, bounds.0 as u16);
        bus.write_word(ctrl_ptr + 10, bounds.1 as u16);
        bus.write_word(ctrl_ptr + 12, bounds.2 as u16);
        bus.write_word(ctrl_ptr + 14, bounds.3 as u16);
        bus.write_byte(ctrl_ptr + 16, 255);
        bus.write_byte(ctrl_ptr + 17, 0);
        bus.write_word(ctrl_ptr + 18, 0);
        bus.write_word(ctrl_ptr + 20, 0);
        bus.write_word(ctrl_ptr + 22, 1);
        bus.write_long(ctrl_ptr + 24, 0);
        bus.write_long(ctrl_ptr + 28, 0);
        bus.write_long(ctrl_ptr + 32, 0);
        bus.write_long(ctrl_ptr + 36, 0);
        bus.write_byte(ctrl_ptr + 40, 0);
        disp.control_proc_ids.insert(ctrl_ptr, 0);
        (ctrl_handle, ctrl_ptr)
    }

    fn screen_pixel_is_set(bus: &MacMemoryBus, base: u32, row_bytes: u32, x: i16, y: i16) -> bool {
        let byte = bus.read_byte(base + (y as u32 * row_bytes) + ((x as u32) / 8));
        byte & (0x80u8 >> ((x as u8) & 7)) != 0
    }

    fn count_set_pixels(
        bus: &MacMemoryBus,
        base: u32,
        row_bytes: u32,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) -> u32 {
        let mut count = 0;
        for y in top..bottom {
            for x in left..right {
                if screen_pixel_is_set(bus, base, row_bytes, x, y) {
                    count += 1;
                }
            }
        }
        count
    }

    fn count_pixel_index(
        bus: &MacMemoryBus,
        base: u32,
        row_bytes: u32,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        pixel_index: u8,
    ) -> u32 {
        let mut count = 0;
        for y in top..bottom {
            for x in left..right {
                if bus.read_byte(base + y as u32 * row_bytes + x as u32) == pixel_index {
                    count += 1;
                }
            }
        }
        count
    }

    fn clear_1bpp_screen(bus: &mut MacMemoryBus, base: u32, row_bytes: u32, height: u32) {
        for y in 0..height {
            for x in 0..row_bytes {
                bus.write_byte(base + y * row_bytes + x, 0);
            }
        }
    }

    #[test]
    fn updtcontrol_repaints_intersecting_controls_and_empty_regions() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let window_ptr = 0x181000u32;
        let screen_base = bus.read_long(window_ptr + 2);
        let row_bytes = (bus.read_word(window_ptr + 6) & 0x3FFF) as u32;

        let (left_handle, left_ptr) =
            alloc_button_control(&mut disp, &mut bus, window_ptr, (20, 20, 60, 120));
        let (right_handle, right_ptr) =
            alloc_button_control(&mut disp, &mut bus, window_ptr, (20, 150, 60, 250));

        bus.write_long(window_ptr + 140, right_handle);
        bus.write_long(right_ptr, left_handle);
        bus.write_long(left_ptr, 0);
        bus.write_long(left_ptr + 4, window_ptr);
        bus.write_long(right_ptr + 4, window_ptr);

        let sp = 0x300000u32;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_ptr);
        disp.dispatch_control(true, 0x169, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let left_probe = (70, 40);
        let right_probe = (200, 40);
        assert!(!screen_pixel_is_set(
            &bus,
            screen_base,
            row_bytes,
            left_probe.0,
            left_probe.1
        ));
        assert!(!screen_pixel_is_set(
            &bus,
            screen_base,
            row_bytes,
            right_probe.0,
            right_probe.1
        ));

        bus.write_byte(left_ptr + 17, 1);
        let narrow_rgn = bus.alloc(10);
        bus.write_word(narrow_rgn, 10);
        bus.write_word(narrow_rgn + 2, 20);
        bus.write_word(narrow_rgn + 4, 20);
        bus.write_word(narrow_rgn + 6, 60);
        bus.write_word(narrow_rgn + 8, 120);
        let narrow_handle = bus.alloc(4);
        bus.write_long(narrow_handle, narrow_rgn);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, narrow_handle);
        bus.write_long(sp + 4, window_ptr);
        let sp_before = cpu.read_reg(Register::A7);
        disp.dispatch_control(true, 0x153, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp_before + 8);
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, left_probe.0, left_probe.1),
            "left control should redraw when its rect intersects the update region"
        );
        assert!(
            !screen_pixel_is_set(&bus, screen_base, row_bytes, right_probe.0, right_probe.1),
            "right control should remain unchanged when it does not intersect the update region"
        );

        bus.write_byte(right_ptr + 17, 1);
        let empty_rgn = bus.alloc(10);
        bus.write_word(empty_rgn, 10);
        bus.write_word(empty_rgn + 2, 0);
        bus.write_word(empty_rgn + 4, 0);
        bus.write_word(empty_rgn + 6, 0);
        bus.write_word(empty_rgn + 8, 0);
        let empty_handle = bus.alloc(4);
        bus.write_long(empty_handle, empty_rgn);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, empty_handle);
        bus.write_long(sp + 4, window_ptr);
        let sp_before = cpu.read_reg(Register::A7);
        disp.dispatch_control(true, 0x153, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp_before + 8);
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, left_probe.0, left_probe.1),
            "empty update regions should leave previously repainted controls unchanged"
        );
        assert!(
            !screen_pixel_is_set(&bus, screen_base, row_bytes, right_probe.0, right_probe.1),
            "empty update regions should not repaint controls outside the update region"
        );
    }

    #[test]
    fn updtcontrol_out_of_range_window_pointer_is_noop_and_pops_args() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, bus.ram_size() + 0x1000);

        let sp_before = cpu.read_reg(Register::A7);
        disp.dispatch_control(true, 0x153, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before + 8,
            "UpdtControl should still consume its Pascal frame"
        );
    }

    // GetCVariant per Inside Macintosh Volume V (1986), p. V-222: returns the
    // variation code packed in the low 4 bits of the control's procID (Inside
    // Macintosh Volume I 1985, p. I-323: control definition ID = 16 *
    // resourceID + variation_code).
    fn install_control_for_variant_test(
        disp: &mut crate::trap::TrapDispatcher,
        bus: &mut MacMemoryBus,
        proc_id: i16,
    ) -> u32 {
        let ctrl_ptr = bus.alloc(296);
        let ctrl_handle = bus.alloc(4);
        bus.write_long(ctrl_handle, ctrl_ptr);
        disp.control_proc_ids.insert(ctrl_ptr, proc_id);
        ctrl_handle
    }

    #[test]
    fn getcvariant_returns_variation_code_from_low_four_bits_of_proc_id() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;

        // pushButProc = 0 → variant 0
        let h_button = install_control_for_variant_test(&mut disp, &mut bus, 0);
        // checkBoxProc = 1 → variant 1
        let h_check = install_control_for_variant_test(&mut disp, &mut bus, 1);
        // radioButProc = 2 → variant 2
        let h_radio = install_control_for_variant_test(&mut disp, &mut bus, 2);
        // scrollBarProc = 16 → variant 0 (CDEF resource 1, variant 0)
        let h_scroll = install_control_for_variant_test(&mut disp, &mut bus, 16);
        // popupMenuProc + popupFixedWidth = 1008 + 1 = 1009 → variant 1
        let h_popup_fixed = install_control_for_variant_test(&mut disp, &mut bus, 1009);

        for (handle, expected) in [
            (h_button, 0i16),
            (h_check, 1),
            (h_radio, 2),
            (h_scroll, 0),
            (h_popup_fixed, 1),
        ] {
            cpu.write_reg(Register::A7, sp);
            bus.write_long(sp, handle);
            bus.write_word(sp + 4, 0xDEAD);
            disp.dispatch_control(true, 0x009, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::A7), sp + 4);
            assert_eq!(
                bus.read_word(sp + 4) as i16,
                expected,
                "GetCVariant must return low 4 bits of procID; got wrong value"
            );
        }
    }

    #[test]
    fn getcvariant_returns_zero_for_nil_control_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, 0xBEEF);

        disp.dispatch_control(true, 0x009, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(
            bus.read_word(sp + 4) as i16,
            0,
            "NIL theControl must return 0 (no variant) without crashing"
        );
    }

    #[test]
    fn getcvariant_function_protocol_pops_handle_and_writes_integer_result() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let handle = install_control_for_variant_test(&mut disp, &mut bus, 2);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, handle);
        bus.write_word(sp + 4, 0xCAFE);
        bus.write_word(sp + 6, 0xBABE);

        disp.dispatch_control(true, 0x009, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        // A7 advances exactly 4 bytes (handle popped); INTEGER result at SP+4.
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(sp + 4) as i16, 2);
        // Sentinel just past the 2-byte result slot survives.
        assert_eq!(
            bus.read_word(sp + 6),
            0xBABE,
            "trap must not write past the 2-byte INTEGER result slot"
        );
    }
}
