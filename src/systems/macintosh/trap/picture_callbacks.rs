//! Cooperative classic picture drawing callbacks.
//!
//! Supported vector, text, and comment operations retain native callback
//! lifetime and Pascal return handling. Unsupported mixed streams retain the
//! existing renderer without partially executing this interpreter.

use super::dispatch::{PortStateSnapshot, TrapDispatcher};
use super::pict::{
    picture_commands, picture_stream_progress, PictureCommand, PictureStreamProgress,
};
use crate::cpu::{CpuOps, Register};
use crate::guest_call::GuestCallTarget;
use crate::guest_procedure::GuestIsa;
use crate::memory::{MacMemoryBus, MemoryBus};
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub(crate) struct PictureCallbackFrame {
    commands: VecDeque<PictureCommand>,
    source: [i16; 4],
    destination: [i16; 4],
    last_rect: [i16; 4],
    oval_size: [i16; 2],
    text_location: [i16; 2],
    line_location: [i16; 2],
    picture_clip: Option<Vec<u8>>,
    mapped_clip: u32,
    caller_sp: u32,
    caller_pc: u32,
    gate: u32,
    rect_buffer: u32,
    temporary: Vec<u32>,
    port_state: PortStateSnapshot,
    font_ids: std::collections::BTreeMap<i16, i16>,
    glyph_state: (bool, bool, u8, u8),
    hilite_color: (u16, u16, u16),
    owned_patterns: Vec<u32>,
}

fn rect(data: &[u8]) -> [i16; 4] {
    std::array::from_fn(|index| i16::from_be_bytes([data[index * 2], data[index * 2 + 1]]))
}

struct IndexedPictureBits {
    pixmap_header: [u8; 46],
    color_table: Vec<u8>,
    pixels: Vec<u8>,
    source: [i16; 4],
    destination: [i16; 4],
    mode: u16,
}

/// Decode an indexed 8-bit PackBitsRect without color matching: the bitmap
/// bottleneck must receive the source pixels and their original ColorTable.
fn indexed_picture_bits(command: &PictureCommand) -> Option<IndexedPictureBits> {
    if command.opcode != 0x98 {
        return None;
    }
    let data = &command.data;
    let word = |offset: usize| -> Option<u16> {
        Some(u16::from_be_bytes(
            data.get(offset..offset + 2)?.try_into().ok()?,
        ))
    };
    let header: [u8; 46] = data.get(..46)?.try_into().ok()?;
    let row_word = word(0)?;
    let row_bytes = usize::from(row_word & 0x3fff);
    let bounds = rect(data.get(2..10)?);
    let height = i32::from(bounds[2]) - i32::from(bounds[0]);
    let width = i32::from(bounds[3]) - i32::from(bounds[1]);
    if row_word & 0xc000 != 0x8000
        || word(12)? != 0
        || word(26)? != 0
        || word(28)? != 8
        || word(30)? != 1
        || word(32)? != 8
        || height <= 0
        || width <= 0
        || row_bytes < width as usize
    {
        return None;
    }
    let total = row_bytes.checked_mul(height as usize)?;
    if total > 32 * 1024 * 1024 {
        return None;
    }
    let entries = usize::from(word(52)?) + 1;
    if entries > 256 {
        return None;
    }
    let table_end = 54usize.checked_add(entries.checked_mul(8)?)?;
    let color_table = data.get(46..table_end)?.to_vec();
    let source = rect(data.get(table_end..table_end + 8)?);
    let destination = rect(data.get(table_end + 8..table_end + 16)?);
    let mode = word(table_end + 16)?;
    let mut pos = table_end + 18;
    let mut pixels = Vec::with_capacity(total);
    for _ in 0..height {
        if row_bytes < 8 {
            pixels.extend_from_slice(data.get(pos..pos.checked_add(row_bytes)?)?);
            pos += row_bytes;
            continue;
        }
        let count = if row_bytes > 250 {
            let value = usize::from(word(pos)?);
            pos += 2;
            value
        } else {
            let value = usize::from(*data.get(pos)?);
            pos += 1;
            value
        };
        let end = pos.checked_add(count)?;
        let packed = data.get(pos..end)?;
        let mut at = 0;
        let row_start = pixels.len();
        while at < packed.len() {
            let token = packed[at];
            at += 1;
            match token {
                0..=127 => {
                    let len = usize::from(token) + 1;
                    pixels.extend_from_slice(packed.get(at..at.checked_add(len)?)?);
                    at += len;
                }
                129..=255 => {
                    let value = *packed.get(at)?;
                    at += 1;
                    pixels.extend(std::iter::repeat_n(value, 257 - usize::from(token)));
                }
                128 => {}
            }
            if pixels.len() - row_start > row_bytes {
                return None;
            }
        }
        if pixels.len() - row_start != row_bytes {
            return None;
        }
        pos = end;
    }
    Some(IndexedPictureBits {
        pixmap_header: header,
        color_table,
        pixels,
        source,
        destination,
        mode,
    })
}

fn resident_picture(bus: &MacMemoryBus, pointer: u32) -> Option<Vec<u8>> {
    // Resident pictures can be copied in chunks; unlike getPicProc, reading
    // extra mapped bytes has no guest callback side effects. Geometric growth
    // avoids reparsing a long prefix for every small operation. An allocation
    // boundary, when known, also prevents accepting bytes from the next object.
    let limit = bus
        .get_alloc_size(pointer)
        .map_or(32 * 1024 * 1024, |size| {
            (size as usize).min(32 * 1024 * 1024)
        });
    let mut length = 10usize;
    loop {
        if length > limit || !bus.is_guest_address_mapped(pointer, length) {
            return None;
        }
        let bytes = bus.read_bytes(pointer, length);
        match picture_stream_progress(&bytes) {
            PictureStreamProgress::Complete(end) => return Some(bytes[..end].to_vec()),
            PictureStreamProgress::Need(required) if required > length && required <= limit => {
                let desired = required.max(length.saturating_mul(2)).min(limit);
                if bus.is_guest_address_mapped(pointer, desired) {
                    length = desired;
                } else {
                    // A valid picture may end immediately before a mapping
                    // hole. Find the largest readable prefix rather than
                    // rejecting it just because speculative growth crossed it.
                    let mut low = length;
                    let mut high = desired;
                    while low < high {
                        let middle = low + (high - low).div_ceil(2);
                        if bus.is_guest_address_mapped(pointer, middle) {
                            low = middle;
                        } else {
                            high = middle - 1;
                        }
                    }
                    if low < required {
                        return None;
                    }
                    length = low;
                }
            }
            _ => return None,
        }
    }
}

impl TrapDispatcher {
    pub(super) fn begin_picture_callbacks<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        handle: u32,
        destination: u32,
        streamed: Option<&[u8]>,
    ) -> bool {
        let globals = bus.read_long(cpu.read_reg(Register::A5));
        let port = bus.read_long(globals);
        if handle == 0 || !bus.is_guest_address_mapped(port, 108) {
            return false;
        }
        let procs = bus.read_long(port + 104);
        if procs == 0 || !bus.is_guest_address_mapped(procs, 40) {
            return false;
        }
        let has_custom = [
            (0, 0xa882),
            (4, 0xa890),
            (8, 0xa8a0),
            (12, 0xa8af),
            (16, 0xa8b6),
            (20, 0xa8bd),
            (32, 0xa8eb),
            (36, 0xa8f1),
        ]
        .into_iter()
        .any(|(slot, trap)| {
            let callback = bus.read_long(procs + slot);
            callback != 0 && bus.system_trap_gateway(trap) != Some(callback)
        });
        if !has_custom {
            return false;
        }
        let owned;
        let bytes = if let Some(bytes) = streamed {
            bytes
        } else {
            let Some(bytes) = resident_picture(bus, bus.read_long(handle)) else {
                return false;
            };
            owned = bytes;
            &owned
        };
        let Some(commands) = picture_commands(bytes) else {
            return false;
        };
        // Do not partially execute a stream before discovering an operation
        // whose state/ABI has not been implemented by this interpreter.
        if commands.iter().any(
            |command| !matches!(command.opcode, 0x00 | 0x01 | 0x03..=0x05 | 0x07..=0x08 | 0x0b..=0x0d | 0x11 | 0x1a..=0x1b | 0x1d..=0x1e | 0x20..=0x23 | 0x28..=0x2c | 0x2e | 0x0c00 | 0x30..=0x34 | 0x38..=0x3c | 0x40..=0x44 | 0x48..=0x4c | 0x50..=0x54 | 0x58..=0x5c | 0x60..=0x64 | 0x68..=0x6c | 0x98 | 0xa0 | 0xa1 | 0xff),
        ) {
            return false;
        }
        if commands
            .iter()
            .any(|command| command.opcode == 0x98 && indexed_picture_bits(command).is_none())
        {
            return false;
        }
        if commands.iter().any(|command| {
            command.opcode == 0x01 && (command.data.len() < 10 || command.data.len() % 2 != 0)
        }) {
            return false;
        }
        if commands.iter().any(|command| match command.opcode {
            0x2c => {
                command.data.len() < 5
                    || 5 + usize::from(command.data[4]) > command.data.len()
                    || 3 + usize::from(command.data[4])
                        > usize::from(u16::from_be_bytes([command.data[0], command.data[1]]))
            }
            0x2e => {
                command.data.len() < 6 || u16::from_be_bytes([command.data[0], command.data[1]]) < 4
            }
            0x0c00 if command.data[..2] == [0xff, 0xfe] => {
                let source = rect(&command.data[12..20]);
                source[2] <= source[0] || source[3] <= source[1]
            }
            _ => false,
        }) {
            return false;
        }
        let source = rect(&bytes[2..10]);
        if source[2] <= source[0]
            || source[3] <= source[1]
            || !bus.is_guest_address_mapped(destination, 8)
        {
            return false;
        }
        let destination_rect = rect(&bus.read_bytes(destination, 8));
        if destination_rect[2] <= destination_rect[0] || destination_rect[3] <= destination_rect[1]
        {
            return false;
        }
        let gate = bus.alloc(2);
        let rect_buffer = bus.alloc(8);
        if gate == 0 || rect_buffer == 0 {
            if gate != 0 {
                bus.free(gate);
            }
            if rect_buffer != 0 {
                bus.free(rect_buffer);
            }
            return false;
        }
        bus.write_word(gate, 0xa8f6);
        self.set_current_port_state(bus, cpu, port, None);
        let port_state = self.capture_current_port_state(bus);
        let hilite_color = self.current_hilite_color(bus);
        let mut owned_patterns = Vec::new();
        if bus.read_word(port + 6) & 0xc000 == 0xc000 {
            // Private monochrome PixPats keep the caller's original color
            // patterns intact while picture defaults and attributes change.
            let mut handles = Vec::new();
            for fill in [0u8, 0xff, 0xff] {
                let data = bus.alloc(28);
                let handle = bus.alloc(4);
                if data == 0 || handle == 0 {
                    if data != 0 {
                        bus.free(data);
                    }
                    if handle != 0 {
                        bus.free(handle);
                    }
                    for allocation in owned_patterns {
                        bus.free(allocation);
                    }
                    bus.free(gate);
                    bus.free(rect_buffer);
                    return false;
                }
                bus.write_bytes(data, &[0; 28]);
                bus.write_bytes(data + 20, &[fill; 8]);
                bus.write_long(handle, data);
                owned_patterns.extend([data, handle]);
                handles.push(handle);
            }
            for (offset, handle) in [32, 58, 62].into_iter().zip(handles) {
                bus.write_long(port + offset, handle);
            }
        } else {
            bus.write_bytes(port + 40, &[0xff; 8]);
        }
        self.fg_color = (0, 0, 0);
        self.bg_color = (0xffff, 0xffff, 0xffff);
        self.pm_fg_color = None;
        self.pm_bg_color = None;
        self.bk_pat = [0; 8];
        self.pn_pat = [0xff; 8];
        self.pn_loc = (0, 0);
        let size: [i16; 2] = std::array::from_fn(|axis| {
            let source_extent = i32::from(source[axis + 2]) - i32::from(source[axis]);
            let target_extent =
                i32::from(destination_rect[axis + 2]) - i32::from(destination_rect[axis]);
            (target_extent / source_extent).clamp(1, i32::from(i16::MAX)) as i16
        });
        self.pn_size = (size[0], size[1]);
        self.pn_mode = 8;
        self.tx_font = 0;
        self.tx_face = 0;
        self.tx_mode = 1;
        self.tx_size = 0;
        bus.write_long(port + 76, 0); // SpExtra
        self.sync_current_port_draw_state(bus);
        self.resolve_current_port_color_pixels(bus, true, true, true);

        self.picture_callback_stack.push(PictureCallbackFrame {
            commands: commands.into(),
            source,
            destination: destination_rect,
            last_rect: [0; 4],
            oval_size: [0; 2],
            text_location: [0; 2],
            line_location: [0; 2],
            picture_clip: None,
            mapped_clip: 0,
            caller_sp: cpu.read_reg(Register::A7) + 8,
            caller_pc: cpu.read_reg(Register::PC),
            gate,
            rect_buffer,
            temporary: Vec::new(),
            port_state,
            font_ids: std::collections::BTreeMap::new(),
            hilite_color,
            owned_patterns,
            glyph_state: (
                self.outline_preferred,
                self.preserve_glyph,
                bus.read_byte(0x0bf4),
                bus.read_byte(0x0a63),
            ),
        });
        self.advance_picture_callbacks(cpu, bus);
        true
    }

    fn remap_picture_clip(bus: &mut MacMemoryBus, frame: &mut PictureCallbackFrame) -> bool {
        use crate::quickdraw::raster::region_rows;
        let Some(bytes) = frame.picture_clip.as_ref() else {
            return true;
        };
        let allocate_region = |bus: &mut MacMemoryBus, bytes: &[u8]| -> Option<u32> {
            let handle = bus.alloc(4);
            let data = bus.alloc(bytes.len() as u32);
            if handle == 0 || data == 0 {
                if handle != 0 {
                    bus.free(handle);
                }
                if data != 0 {
                    bus.free(data);
                }
                return None;
            }
            bus.write_long(handle, data);
            bus.write_bytes(data, bytes);
            Some(handle)
        };
        let Some(source_region) = allocate_region(bus, bytes) else {
            return false;
        };
        let user_bytes = frame
            .port_state
            .clip_region
            .as_ref()
            .map(|r| r.bytes.as_slice());
        let user_region = match user_bytes {
            Some(bytes) => match allocate_region(bus, bytes) {
                Some(handle) => Some(handle),
                None => {
                    bus.free(bus.read_long(source_region));
                    bus.free(source_region);
                    return false;
                }
            },
            None => None,
        };
        let success = (|| {
            if frame.mapped_clip == 0 {
                frame.mapped_clip = bus.alloc(4);
                if frame.mapped_clip == 0 {
                    return false;
                }
                bus.write_long(frame.mapped_clip, 0);
            }
            let bounds = rect(&bytes[2..10]);
            let map = |value: i32, axis: usize| {
                let extent = i64::from(frame.source[axis + 2]) - i64::from(frame.source[axis]);
                let target =
                    i64::from(frame.destination[axis + 2]) - i64::from(frame.destination[axis]);
                (i64::from(frame.destination[axis])
                    + (i64::from(value) - i64::from(frame.source[axis])) * target / extent)
                    .clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
            };
            let top = map(i32::from(bounds[0]), 0);
            let bottom = map(i32::from(bounds[2]), 0);
            let mut rows = vec![Vec::new(); (i32::from(bottom) - i32::from(top)).max(0) as usize];
            if bounds[2] > bounds[0] && bounds[3] > bounds[1] && bottom > top {
                let original = Self::region_rows_for_band(bus, source_region, bounds[0], bounds[2]);
                for (offset, row) in original.iter().enumerate() {
                    let y = i32::from(bounds[0]) + offset as i32;
                    let y0 = map(y, 0);
                    let y1 = map(y + 1, 0);
                    let mapped = region_rows::intervals_to_endpoints(
                        row.chunks_exact(2)
                            .map(|span| (map(i32::from(span[0]), 1), map(i32::from(span[1]), 1)))
                            .collect(),
                    );
                    for dy in i32::from(y0)..i32::from(y1) {
                        let index = (dy - i32::from(top)) as usize;
                        rows[index] = region_rows::union_rows(&rows[index], &mapped);
                    }
                }
            }
            if let Some(user_region) = user_region {
                let user_rows = Self::region_rows_for_band(bus, user_region, top, bottom);
                for (row, user) in rows.iter_mut().zip(user_rows) {
                    *row = region_rows::intersect_rows(row, &user);
                }
            }
            // Region records have a 16-bit byte size. Refuse a result that
            // cannot be represented instead of truncating its serialized size.
            let mut words = 1usize;
            let mut previous: &[i16] = &[];
            for row in rows
                .iter()
                .map(Vec::as_slice)
                .chain(std::iter::once(&[][..]))
            {
                if row != previous {
                    words += 2 + region_rows::xor_rows(previous, row).len();
                }
                previous = row;
            }
            if 10 + words * 2 > u16::MAX as usize {
                return false;
            }
            if !Self::write_region_from_rows(bus, frame.mapped_clip, top, &rows) {
                return false;
            }
            bus.write_long(frame.port_state.port + 28, frame.mapped_clip);
            true
        })();
        for handle in [Some(source_region), user_region].into_iter().flatten() {
            bus.free(bus.read_long(handle));
            bus.free(handle);
        }
        success
    }

    fn advance_picture_callbacks<C: CpuOps>(&mut self, cpu: &mut C, bus: &mut MacMemoryBus) {
        let mut frame = self.picture_callback_stack.pop().unwrap();
        loop {
            // Temporary comment data remains valid for the entire guest call,
            // including nested Toolbox calls, and is released on its return.
            for allocation in frame.temporary.drain(..) {
                bus.free(allocation);
            }
            let Some(command) = frame.commands.pop_front() else {
                self.restore_current_port_state(bus, cpu, &frame.port_state);
                self.write_port_hilite_color(bus, frame.port_state.port, frame.hilite_color);
                for allocation in frame.owned_patterns.drain(..) {
                    bus.free(allocation);
                }
                self.outline_preferred = frame.glyph_state.0;
                self.preserve_glyph = frame.glyph_state.1;
                bus.write_byte(0x0bf4, frame.glyph_state.2);
                bus.write_byte(0x0a63, frame.glyph_state.3);
                if frame.mapped_clip != 0 {
                    bus.free(bus.read_long(frame.mapped_clip));
                    bus.free(frame.mapped_clip);
                }
                bus.free(frame.rect_buffer);
                bus.free(frame.gate);
                cpu.write_reg(Register::A7, frame.caller_sp);
                cpu.write_reg(Register::PC, frame.caller_pc);
                return;
            };
            if command.opcode == 0x01 {
                frame.picture_clip = Some(command.data.clone());
                if !Self::remap_picture_clip(bus, &mut frame) {
                    frame.commands.clear();
                }
                continue;
            }
            if command.opcode == 0x0c {
                let dh = i16::from_be_bytes([command.data[0], command.data[1]]);
                let dv = i16::from_be_bytes([command.data[2], command.data[3]]);
                for index in 0..4 {
                    frame.source[index] =
                        frame.source[index].wrapping_add(if index % 2 == 0 { dv } else { dh });
                }
                if frame.picture_clip.is_some() && !Self::remap_picture_clip(bus, &mut frame) {
                    frame.commands.clear();
                }
                continue;
            }
            let word = |offset: usize| {
                i16::from_be_bytes([command.data[offset], command.data[offset + 1]])
            };
            let attribute = match command.opcode {
                0x1d | 0x1e => {
                    let color = if command.opcode == 0x1d {
                        (word(0) as u16, word(2) as u16, word(4) as u16)
                    } else {
                        let address = crate::memory::globals::addr::HILITE_RGB;
                        (
                            bus.read_word(address),
                            bus.read_word(address + 2),
                            bus.read_word(address + 4),
                        )
                    };
                    self.write_port_hilite_color(bus, frame.port_state.port, color);
                    true
                }

                0x2c => {
                    let name_bytes = &command.data[5..5 + usize::from(command.data[4])];
                    if let Ok(name) = std::str::from_utf8(name_bytes) {
                        if let Some(id) = crate::quickdraw::fonts::font_id_for_name(name)
                            .or_else(|| self.loaded_font_id_for_name(name))
                            .filter(|id| *id != 0)
                        {
                            frame.font_ids.insert(word(2), id);
                        }
                    }
                    true
                }
                0x2e => {
                    self.outline_preferred = command.data[2] != 0;
                    self.preserve_glyph = command.data[3] != 0;
                    bus.write_byte(0x0bf4, command.data[4]);
                    bus.write_byte(0x0a63, command.data[5]);
                    true
                }
                0x0c00 => {
                    if command.data[..2] == [0xff, 0xfe] {
                        frame.source = rect(&command.data[12..20]);
                        if frame.picture_clip.is_some()
                            && !Self::remap_picture_clip(bus, &mut frame)
                        {
                            frame.commands.clear();
                        }
                    }
                    true
                }

                0x03 => {
                    self.tx_font = frame.font_ids.get(&word(0)).copied().unwrap_or(word(0));
                    true
                }
                0x04 => {
                    self.tx_face = i16::from(command.data[0]);
                    true
                }
                0x05 => {
                    self.tx_mode = word(0);
                    true
                }
                0x07 | 0x0b => {
                    let mut size = [0; 2];
                    for axis in 0..2 {
                        let source_extent =
                            i64::from(frame.source[axis + 2]) - i64::from(frame.source[axis]);
                        let target_extent = i64::from(frame.destination[axis + 2])
                            - i64::from(frame.destination[axis]);
                        // Picture pen dimensions use ScalePt, whose minimum is (1,1).
                        size[axis] = (i64::from(word(axis * 2)) * target_extent / source_extent)
                            .clamp(1, i64::from(i16::MAX))
                            as i16;
                    }
                    if command.opcode == 0x07 {
                        self.pn_size = (size[0], size[1]);
                    } else {
                        frame.oval_size = size;
                    }
                    true
                }
                0x08 => {
                    self.pn_mode = word(0);
                    true
                }
                0x0d => {
                    self.tx_size = word(0);
                    true
                }
                0x1a => {
                    self.fg_color = (word(0) as u16, word(2) as u16, word(4) as u16);
                    self.pm_fg_color = None;
                    true
                }
                0x1b => {
                    self.bg_color = (word(0) as u16, word(2) as u16, word(4) as u16);
                    self.pm_bg_color = None;
                    true
                }
                _ => false,
            };
            if attribute {
                self.sync_current_port_draw_state(bus);
                self.resolve_current_port_color_pixels(
                    bus,
                    command.opcode == 0x1a,
                    command.opcode == 0x1b,
                    true,
                );
                continue;
            }
            let (slot, default_trap, arguments) = match command.opcode {
                0x20..=0x23 => {
                    let mut offset = 0;
                    if command.opcode & 1 == 0 {
                        frame.line_location = [word(0), word(2)];
                        offset = 4;
                    }
                    let start = frame.line_location;
                    if command.opcode & 2 == 0 {
                        frame.line_location = [word(offset), word(offset + 2)];
                    } else {
                        frame.line_location[1] = frame.line_location[1]
                            .wrapping_add(i16::from(command.data[offset] as i8));
                        frame.line_location[0] = frame.line_location[0]
                            .wrapping_add(i16::from(command.data[offset + 1] as i8));
                    }
                    let map = |point: [i16; 2]| -> [i16; 2] {
                        std::array::from_fn(|axis| {
                            let from = i64::from(frame.source[axis]);
                            let extent = i64::from(frame.source[axis + 2]) - from;
                            let to = i64::from(frame.destination[axis]);
                            let target = i64::from(frame.destination[axis + 2]) - to;
                            (to + (i64::from(point[axis]) - from) * target / extent)
                                .clamp(i64::from(i16::MIN), i64::from(i16::MAX))
                                as i16
                        })
                    };
                    let start = map(start);
                    let end = map(frame.line_location);
                    self.pn_loc = (start[0], start[1]);
                    self.sync_current_port_draw_state(bus);
                    let mut arguments = Vec::with_capacity(4);
                    for coordinate in end {
                        arguments.extend_from_slice(&coordinate.to_be_bytes());
                    }
                    (4, 0xa890, arguments)
                }

                0x28..=0x2b => {
                    let count_offset = match command.opcode {
                        0x28 => {
                            frame.text_location = [
                                i16::from_be_bytes([command.data[0], command.data[1]]),
                                i16::from_be_bytes([command.data[2], command.data[3]]),
                            ];
                            4
                        }
                        0x29 => {
                            frame.text_location[1] =
                                frame.text_location[1].wrapping_add(i16::from(command.data[0]));
                            1
                        }
                        0x2a => {
                            frame.text_location[0] =
                                frame.text_location[0].wrapping_add(i16::from(command.data[0]));
                            1
                        }
                        _ => {
                            frame.text_location[1] =
                                frame.text_location[1].wrapping_add(i16::from(command.data[0]));
                            frame.text_location[0] =
                                frame.text_location[0].wrapping_add(i16::from(command.data[1]));
                            2
                        }
                    };
                    let count = usize::from(command.data[count_offset]);
                    let text = bus.alloc(count.max(1) as u32);
                    if text == 0 {
                        frame.commands.clear();
                        continue;
                    }
                    bus.write_bytes(
                        text,
                        &command.data[count_offset + 1..count_offset + 1 + count],
                    );
                    frame.temporary.push(text);
                    let mut mapped = [0i16; 2];
                    let mut numer = [0i16; 2];
                    let mut denom = [0i16; 2];
                    for axis in 0..2 {
                        let source_start = i32::from(frame.source[axis]);
                        let source_extent = i32::from(frame.source[axis + 2]) - source_start;
                        let target_start = i32::from(frame.destination[axis]);
                        let target_extent = i32::from(frame.destination[axis + 2]) - target_start;
                        let value = i64::from(target_start)
                            + i64::from(i32::from(frame.text_location[axis]) - source_start)
                                * i64::from(target_extent)
                                / i64::from(source_extent);
                        mapped[axis] = value.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
                        numer[axis] = target_extent as i16;
                        denom[axis] = source_extent as i16;
                    }
                    self.pn_loc = (mapped[0], mapped[1]);
                    self.sync_current_port_draw_state(bus);
                    let mut arguments = Vec::with_capacity(14);
                    for value in denom.into_iter().chain(numer) {
                        arguments.extend_from_slice(&value.to_be_bytes());
                    }
                    arguments.extend_from_slice(&text.to_be_bytes());
                    arguments.extend_from_slice(&(count as u16).to_be_bytes());
                    (0, 0xa882, arguments)
                }
                0x30..=0x34
                | 0x38..=0x3c
                | 0x40..=0x44
                | 0x48..=0x4c
                | 0x50..=0x54
                | 0x58..=0x5c
                | 0x60..=0x64
                | 0x68..=0x6c => {
                    if command.opcode & 8 == 0 {
                        frame.last_rect = rect(&command.data);
                    }
                    for index in 0..4 {
                        let axis = index % 2;
                        let source_start = i32::from(frame.source[axis]);
                        let source_extent = i32::from(frame.source[axis + 2]) - source_start;
                        let target_start = i32::from(frame.destination[axis]);
                        let target_extent = i32::from(frame.destination[axis + 2]) - target_start;
                        let value = i64::from(target_start)
                            + i64::from(i32::from(frame.last_rect[index]) - source_start)
                                * i64::from(target_extent)
                                / i64::from(source_extent);
                        bus.write_word(
                            frame.rect_buffer + index as u32 * 2,
                            value.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16 as u16,
                        );
                    }
                    let mut arguments = Vec::new();
                    let (slot, trap) = match command.opcode & 0xf0 {
                        0x40 => {
                            for dimension in frame.oval_size {
                                arguments.extend_from_slice(&dimension.to_be_bytes());
                            }
                            (12, 0xa8af)
                        }
                        0x50 => (16, 0xa8b6),
                        0x60 => {
                            let offset = if command.opcode & 8 == 0 { 8 } else { 0 };
                            arguments.extend_from_slice(&command.data[offset + 2..offset + 4]);
                            arguments.extend_from_slice(&command.data[offset..offset + 2]);
                            (20, 0xa8bd)
                        }
                        _ => (8, 0xa8a0),
                    };
                    arguments.extend_from_slice(&frame.rect_buffer.to_be_bytes());
                    arguments.extend_from_slice(&[(command.opcode & 7) as u8, 0]);
                    (slot, trap, arguments)
                }
                0x98 => {
                    let Some(bitmap) = indexed_picture_bits(&command) else {
                        frame.commands.clear();
                        continue;
                    };
                    let sizes = [
                        bitmap.pixels.len() as u32,
                        50,
                        bitmap.color_table.len() as u32,
                        4,
                        16,
                    ];
                    let mut allocations = Vec::with_capacity(sizes.len());
                    for size in sizes {
                        let ptr = bus.alloc(size);
                        if ptr == 0 {
                            break;
                        }
                        allocations.push(ptr);
                    }
                    if allocations.len() != sizes.len() {
                        for allocation in allocations {
                            bus.free(allocation);
                        }
                        frame.commands.clear();
                        continue;
                    }
                    let (pixels, pixmap, ctab, ctab_handle, rects) = (
                        allocations[0],
                        allocations[1],
                        allocations[2],
                        allocations[3],
                        allocations[4],
                    );
                    bus.write_bytes(pixels, &bitmap.pixels);
                    bus.write_long(pixmap, pixels);
                    bus.write_bytes(pixmap + 4, &bitmap.pixmap_header);
                    bus.write_bytes(ctab, &bitmap.color_table);
                    bus.write_long(ctab_handle, ctab);
                    bus.write_long(pixmap + 42, ctab_handle);
                    for index in 0..4 {
                        bus.write_word(rects + index as u32 * 2, bitmap.source[index] as u16);
                        let axis = index % 2;
                        let from = i64::from(frame.source[axis]);
                        let extent = i64::from(frame.source[axis + 2]) - from;
                        let target = i64::from(frame.destination[axis]);
                        let target_extent = i64::from(frame.destination[axis + 2]) - target;
                        let value = target
                            + (i64::from(bitmap.destination[index]) - from) * target_extent
                                / extent;
                        bus.write_word(
                            rects + 8 + index as u32 * 2,
                            value.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16 as u16,
                        );
                    }
                    frame.temporary.extend(allocations);
                    let mut arguments = 0u32.to_be_bytes().to_vec(); // no mask
                    arguments.extend_from_slice(&bitmap.mode.to_be_bytes());
                    arguments.extend_from_slice(&(rects + 8).to_be_bytes());
                    arguments.extend_from_slice(&rects.to_be_bytes());
                    arguments.extend_from_slice(&pixmap.to_be_bytes());
                    (32, 0xa8eb, arguments)
                }
                0xa0 | 0xa1 => {
                    let size = if command.opcode == 0xa1 {
                        u16::from_be_bytes([command.data[2], command.data[3]])
                    } else {
                        0
                    };
                    let mut handle = 0;
                    if size != 0 {
                        let data = bus.alloc(u32::from(size));
                        handle = bus.alloc(4);
                        if data == 0 || handle == 0 {
                            if data != 0 {
                                bus.free(data);
                            }
                            if handle != 0 {
                                bus.free(handle);
                            }
                            frame.commands.clear();
                            continue;
                        }
                        bus.write_bytes(data, &command.data[4..4 + usize::from(size)]);
                        bus.write_long(handle, data);
                        frame.temporary.extend([data, handle]);
                    }
                    let mut arguments = handle.to_be_bytes().to_vec();
                    arguments.extend_from_slice(&size.to_be_bytes());
                    arguments.extend_from_slice(&command.data[..2]);
                    (36, 0xa8f1, arguments)
                }
                _ => continue,
            };
            // Consult the live table for every operation; a guest callback
            // may replace a procedure before the next command executes.
            let procs = bus.read_long(frame.port_state.port + 104);
            let installed = if procs != 0 && bus.is_guest_address_mapped(procs, slot + 4) {
                bus.read_long(procs + slot as u32)
            } else {
                0
            };
            let callback = if installed != 0 {
                installed
            } else {
                bus.system_trap_gateway(default_trap).unwrap_or(0)
            };
            if slot == 36
                && (callback == 0 || bus.system_trap_gateway(default_trap) == Some(callback))
            {
                // The standard comment bottleneck deliberately ignores data.
                continue;
            }
            let Some(sp) = frame.caller_sp.checked_sub(4 + arguments.len() as u32) else {
                frame.commands.clear();
                continue;
            };
            if callback == 0
                || !bus.is_guest_address_writable(sp, 4 + arguments.len())
                || !self.guest_calls.begin_m68k_with_operation(
                    GuestCallTarget {
                        isa: GuestIsa::M68k,
                        entry: callback,
                        rtoc: 0,
                    },
                    frame.gate,
                    frame.caller_sp,
                    None,
                    None,
                )
            {
                frame.commands.clear();
                continue;
            }
            bus.write_long(sp, frame.gate);
            bus.write_bytes(sp + 4, &arguments);
            self.picture_callback_stack.push(frame);
            cpu.write_reg(Register::A7, sp);
            cpu.write_reg(Register::PC, callback);
            return;
        }
    }

    pub(crate) fn resume_picture_callbacks<C: CpuOps>(
        &mut self,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<crate::Result<()>> {
        let frame = self.picture_callback_stack.last()?;
        if cpu.read_reg(Register::PC) != frame.gate + 2
            || cpu.read_reg(Register::A7) != frame.caller_sp
        {
            return None;
        }
        self.advance_picture_callbacks(cpu, bus);
        Some(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::{M68kCpu, StepResult};
    use crate::trap::test_helpers::{setup_with_port, TEST_SP};

    #[test]
    fn rectangle_callbacks_execute_in_order_and_release_their_frames() {
        let (mut dispatcher, mock, mut bus) = setup_with_port();
        let mut cpu = M68kCpu::new();
        cpu.write_reg(Register::A5, mock.read_reg(Register::A5));
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::PC, 0x340002);
        let handle = 0x350100;
        let picture = 0x350200;
        let destination = 0x350300;
        let procs = 0x350400;
        let callback = 0x360000;
        let bytes = [
            0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1, 0x31, 0, 1, 0, 2, 0, 6, 0,
            7,    // paint rectangle
            0x3b, // invert same rectangle
            0xff,
        ];
        bus.write_bytes(picture, &bytes);
        bus.write_long(handle, picture);
        bus.write_bytes(destination, &bytes[2..10]);
        bus.write_long(0x181000 + 104, procs);
        bus.write_long(procs + 8, callback);
        let words = [
            0x5279, 0x0035, 0x0800, // addq.w #1,call_count
            0x7000, // moveq #0,d0
            0x102f, 8, // move.b 8(sp),d0: byte-sized GrafVerb
            0x33c0, 0x0035, 0x0802, // move.w d0,last_verb
            0x225f, // movea.l (sp)+,a1: return address
            0x5c8f, // addq.l #6,sp: Pascal arguments
            0x4ed1, // jmp (a1)
        ];
        for (index, word) in words.into_iter().enumerate() {
            bus.write_word(callback + index as u32 * 2, word);
        }
        bus.write_long(TEST_SP, destination);
        bus.write_long(TEST_SP + 4, handle);
        bus.write_long(TEST_SP + 8, 0xdead_beef);
        dispatcher
            .dispatch_quickdraw(true, 0xf6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::PC), callback);
        assert_eq!(bus.read_word(0x350800), 0);
        let gate = dispatcher.picture_callback_stack[0].gate;
        let buffer = dispatcher.picture_callback_stack[0].rect_buffer;
        for _ in 0..100 {
            if cpu.read_reg(Register::PC) == 0x340002 {
                break;
            }
            match cpu.step(&mut bus) {
                StepResult::Ok => {}
                StepResult::Aline(opcode) => {
                    dispatcher.dispatch(opcode, &mut cpu, &mut bus).unwrap()
                }
                _ => panic!("unexpected callback result"),
            }
        }
        assert_eq!(cpu.read_reg(Register::PC), 0x340002);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_long(TEST_SP + 8), 0xdead_beef);
        assert_eq!(bus.read_word(0x350800), 2);
        assert_eq!(bus.read_word(0x350802), 3);
        assert_eq!(dispatcher.guest_calls.depth(), 0);
        assert!(dispatcher.picture_callback_stack.is_empty());
        assert_eq!(bus.get_alloc_size(gate), None);
        assert_eq!(bus.get_alloc_size(buffer), None);
        assert_eq!(bus.read_bytes(picture, bytes.len()), bytes);
    }

    #[test]
    fn indexed_picture_bits_callback_receives_source_indices_and_table() {
        let (mut dispatcher, mock, mut bus) = setup_with_port();
        let mut cpu = M68kCpu::new();
        cpu.write_reg(Register::A5, mock.read_reg(Register::A5));
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::PC, 0x340002);
        let handle = 0x350100;
        let picture = 0x350200;
        let destination = 0x350600;
        let procs = 0x350700;
        let callback = 0x360000;
        let mut bytes = vec![0, 0, 0, 0, 0, 0, 0, 2, 0, 4, 0x11, 1, 0x98];
        let mut pm = [0u8; 46];
        pm[0..2].copy_from_slice(&0x8004u16.to_be_bytes());
        pm[6..8].copy_from_slice(&2u16.to_be_bytes());
        pm[8..10].copy_from_slice(&4u16.to_be_bytes());
        pm[28..30].copy_from_slice(&8u16.to_be_bytes());
        pm[30..32].copy_from_slice(&1u16.to_be_bytes());
        pm[32..34].copy_from_slice(&8u16.to_be_bytes());
        bytes.extend_from_slice(&pm);
        let table = [
            0, 0, 0, 77, 0x80, 0, 0, 3, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 1, 0x11, 0x11,
            0x22, 0x22, 0x33, 0x33, 0, 2, 0x44, 0x44, 0x55, 0x55, 0x66, 0x66, 0, 3, 0x77, 0x77,
            0x88, 0x88, 0x99, 0x99,
        ];
        bytes.extend_from_slice(&table);
        let source_rect = [0, 0, 0, 0, 0, 2, 0, 4];
        bytes.extend_from_slice(&source_rect);
        bytes.extend_from_slice(&source_rect);
        bytes.extend_from_slice(&[0, 0]); // srcCopy
        let pixels = [1, 2, 3, 0, 3, 2, 1, 0];
        bytes.extend_from_slice(&pixels); // rowBytes < 8: unpacked
        bytes.push(0xff);
        bus.write_bytes(picture, &bytes);
        bus.write_long(handle, picture);
        bus.write_bytes(destination, &[0, 0, 0, 0, 0, 4, 0, 8]);
        bus.write_long(0x181000 + 104, procs);
        bus.write_long(procs + 32, callback);
        // Copy the callback's ColorTable with the same classic Memory
        // Manager operation used by applications that retain picture colors.
        let callback_words = [
            0x206f, 18, // movea.l 18(sp),a0: source PixMap
            0x2068, 42,     // movea.l 42(a0),a0: ColorTable handle
            0xa9e1, // HandToHand
            0x23c8, 0x0035, 0x0800, // move.l a0,$350800
            0x225f, 0x4fef, 18, 0x4ed1, // Pascal return
        ];
        for (index, word) in callback_words.into_iter().enumerate() {
            bus.write_word(callback + index as u32 * 2, word);
        }
        bus.write_long(TEST_SP, destination);
        bus.write_long(TEST_SP + 4, handle);
        dispatcher
            .dispatch_quickdraw(true, 0xf6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::PC), callback);
        let sp = cpu.read_reg(Register::A7);
        assert_eq!(bus.read_long(sp + 4), 0); // mask
        assert_eq!(bus.read_word(sp + 8), 0); // srcCopy
        let dst = bus.read_long(sp + 10);
        let src = bus.read_long(sp + 14);
        let pixmap = bus.read_long(sp + 18);
        let pixel_ptr = bus.read_long(pixmap);
        let table_handle = bus.read_long(pixmap + 42);
        let table_ptr = bus.read_long(table_handle);
        assert_eq!(bus.read_bytes(src, 8), source_rect);
        assert_eq!(bus.read_bytes(dst, 8), [0, 0, 0, 0, 0, 4, 0, 8]);
        assert_eq!(bus.read_bytes(pixel_ptr, pixels.len()), pixels);
        assert_eq!(bus.read_bytes(table_ptr, table.len()), table);
        for _ in 0..100 {
            if cpu.read_reg(Register::PC) == 0x340002 {
                break;
            }
            match cpu.step(&mut bus) {
                StepResult::Ok => {}
                StepResult::Aline(opcode) => {
                    dispatcher.dispatch(opcode, &mut cpu, &mut bus).unwrap()
                }
                _ => panic!("unexpected callback result"),
            }
        }
        assert_eq!(cpu.read_reg(Register::PC), 0x340002);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(dispatcher.guest_calls.depth(), 0);
        assert!(dispatcher.picture_callback_stack.is_empty());
        let copied_handle = bus.read_long(0x350800);
        assert_ne!(copied_handle, 0);
        assert_ne!(copied_handle, table_handle);
        let copied_table = bus.read_long(copied_handle);
        assert_eq!(bus.read_bytes(copied_table, table.len()), table);
        for allocation in [pixmap, pixel_ptr, table_handle, table_ptr, src] {
            assert_eq!(bus.get_alloc_size(allocation), None);
        }
    }

    #[test]
    fn indexed_picture_bits_decodes_word_counted_rows_and_rejects_truncation() {
        let mut pm = [0u8; 46];
        pm[0..2].copy_from_slice(&0x8100u16.to_be_bytes());
        pm[6..8].copy_from_slice(&1u16.to_be_bytes());
        pm[8..10].copy_from_slice(&256u16.to_be_bytes());
        pm[28..30].copy_from_slice(&8u16.to_be_bytes());
        pm[30..32].copy_from_slice(&1u16.to_be_bytes());
        pm[32..34].copy_from_slice(&8u16.to_be_bytes());
        let mut data = pm.to_vec();
        data.extend_from_slice(&[0, 0, 0, 1, 0x80, 0, 0, 0]); // one CTable entry
        data.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
        let bounds = [0, 0, 0, 0, 0, 1, 1, 0];
        data.extend_from_slice(&bounds);
        data.extend_from_slice(&bounds);
        data.extend_from_slice(&[0, 0, 0, 4, 0x81, 42, 0x81, 43]);
        let command = PictureCommand {
            opcode: 0x98,
            data: data.clone(),
        };
        let decoded = indexed_picture_bits(&command).unwrap();
        assert_eq!(decoded.pixels.len(), 256);
        assert!(decoded.pixels[..128].iter().all(|pixel| *pixel == 42));
        assert!(decoded.pixels[128..].iter().all(|pixel| *pixel == 43));
        data.pop();
        assert!(indexed_picture_bits(&PictureCommand { opcode: 0x98, data }).is_none());
    }
}

#[cfg(test)]
mod coordinate_tests {
    use super::*;
    use crate::trap::test_helpers::{setup_with_port, TEST_SP};

    #[test]
    fn picture_origin_changes_source_mapping_without_moving_the_port() {
        let (mut dispatcher, mut cpu, mut bus) = setup_with_port();
        let picture = 0x350100;
        let handle = 0x350200;
        let destination = 0x350300;
        let procs = 0x350400;
        let callback = 0x360000;
        bus.write_long(handle, picture);
        bus.write_bytes(
            picture,
            &[
                0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1, 0x0c, 0, 2, 0xff, 0xff, // dh=2, dv=-1
                0x31, 0, 0, 0, 3, 0, 2, 0, 5, // paint (0,3)-(2,5)
                0xff,
            ],
        );
        bus.write_bytes(destination, &[0, 0, 0, 0, 0, 16, 0, 16]);
        bus.write_long(0x181000 + 104, procs);
        bus.write_long(procs + 8, callback);
        let original_port_rect = bus.read_bytes(0x181000 + 16, 8);
        let original_bounds = bus.read_bytes(0x181000 + 8, 8);
        bus.write_long(TEST_SP, destination);
        bus.write_long(TEST_SP + 4, handle);
        dispatcher
            .dispatch_quickdraw(true, 0xf6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::PC), callback);
        let sp = cpu.read_reg(Register::A7);
        let rectangle = bus.read_long(sp + 4);
        assert_eq!(rect(&bus.read_bytes(rectangle, 8)), [2, 2, 6, 6]);
        assert_eq!(bus.read_bytes(0x181000 + 16, 8), original_port_rect);
        assert_eq!(bus.read_bytes(0x181000 + 8, 8), original_bounds);
    }
}

#[cfg(test)]
mod comment_tests {
    use super::*;
    use crate::cpu::{M68kCpu, StepResult};
    use crate::trap::test_helpers::{setup_with_port, TEST_SP};

    #[test]
    fn comment_callbacks_receive_owned_data_and_clean_pascal_arguments() {
        let (mut dispatcher, mock, mut bus) = setup_with_port();
        let mut cpu = M68kCpu::new();
        cpu.write_reg(Register::A5, mock.read_reg(Register::A5));
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::PC, 0x340002);
        let picture = 0x350100;
        let handle = 0x350200;
        let destination = 0x350300;
        let procs = 0x350400;
        let callback = 0x360000;
        let bytes = [
            0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1, 0xa1, 0, 150, 0, 3, b'a', b'b', b'c', 0xa0, 0,
            151, 0xff,
        ];
        bus.write_bytes(picture, &bytes);
        bus.write_long(handle, picture);
        bus.write_bytes(destination, &bytes[2..10]);
        bus.write_long(0x181000 + 104, procs);
        bus.write_long(procs + 36, callback);
        let words = [
            0x5279, 0x0035, 0x0800, // addq.w #1,count
            0x302f, 10, // move.w 10(sp),d0: kind
            0x33c0, 0x0035, 0x0802, // move.w d0,last_kind
            0x225f, 0x508f, 0x4ed1, // pop return, pop 8 arguments, return
        ];
        for (index, word) in words.into_iter().enumerate() {
            bus.write_word(callback + index as u32 * 2, word);
        }
        bus.write_long(TEST_SP, destination);
        bus.write_long(TEST_SP + 4, handle);
        bus.write_long(TEST_SP + 8, 0xdead_beef);
        dispatcher
            .dispatch_quickdraw(true, 0xf6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::PC), callback);
        let sp = cpu.read_reg(Register::A7);
        assert_eq!(bus.read_word(sp + 8), 3);
        assert_eq!(bus.read_word(sp + 10), 150);
        let data_handle = bus.read_long(sp + 4);
        let data = bus.read_long(data_handle);
        assert_eq!(bus.read_bytes(data, 3), b"abc");
        assert_eq!(bus.get_alloc_size(data), Some(3));
        let mut saw_short = false;
        for _ in 0..100 {
            if cpu.read_reg(Register::PC) == 0x340002 {
                break;
            }
            if cpu.read_reg(Register::PC) == callback && bus.read_word(0x350800) == 1 {
                let sp = cpu.read_reg(Register::A7);
                assert_eq!(bus.read_long(sp + 4), 0);
                assert_eq!(bus.read_word(sp + 8), 0);
                assert_eq!(bus.read_word(sp + 10), 151);
                saw_short = true;
            }
            match cpu.step(&mut bus) {
                StepResult::Ok => {}
                StepResult::Aline(opcode) => {
                    dispatcher.dispatch(opcode, &mut cpu, &mut bus).unwrap()
                }
                _ => panic!("unexpected callback result"),
            }
        }
        assert!(saw_short);
        assert_eq!(cpu.read_reg(Register::PC), 0x340002);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_long(TEST_SP + 8), 0xdead_beef);
        assert_eq!(bus.read_word(0x350800), 2);
        assert_eq!(bus.read_word(0x350802), 151);
        assert_eq!(bus.get_alloc_size(data), None);
        assert_eq!(bus.get_alloc_size(data_handle), None);
        assert_eq!(dispatcher.guest_calls.depth(), 0);
    }
}

#[cfg(test)]
mod text_tests {
    use super::*;
    use crate::cpu::{M68kCpu, StepResult};
    use crate::trap::test_helpers::{setup_with_port, TEST_SP};

    #[test]
    fn native_text_callbacks_receive_ordered_text_locations_and_scaling() {
        let (mut dispatcher, mock, mut bus) = setup_with_port();
        let mut cpu = M68kCpu::new();
        cpu.write_reg(Register::A5, mock.read_reg(Register::A5));
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::PC, 0x340002);
        let original_pen = dispatcher.pn_loc;
        let picture = 0x350100;
        let handle = 0x350200;
        let destination = 0x350300;
        let procs = 0x350400;
        let callback = 0x360000;
        let bytes = [
            0, 0, 0, 0, 0, 0, 1, 44, 1, 44, 0x11, 1, 0x28, 0, 1, 0, 2, 3, b'o', b'n', b'e', 0x29,
            200, 1, b'2', 0x2a, 201, 0, 0x2b, 3, 4, 2, b'o', b'k', 0xff,
        ];
        bus.write_bytes(picture, &bytes);
        bus.write_long(handle, picture);
        bus.write_bytes(destination, &[0, 0, 0, 0, 2, 88, 2, 88]);
        bus.write_long(0x181000 + 104, procs);
        bus.write_long(procs, callback);
        let words = [
            0x5279, 0x0035, 0x0800, // addq.w #1,count
            0x225f, // movea.l (sp)+,a1: return address
            0x4fef, 14,     // lea 14(sp),sp: Pascal text arguments
            0x4ed1, // jmp (a1)
        ];
        for (index, word) in words.into_iter().enumerate() {
            bus.write_word(callback + index as u32 * 2, word);
        }
        bus.write_long(TEST_SP, destination);
        bus.write_long(TEST_SP + 4, handle);
        bus.write_long(TEST_SP + 8, 0xdead_beef);
        dispatcher
            .dispatch_quickdraw(true, 0xf6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let expected: [(&[u8], (i16, i16)); 4] = [
            (b"one", (2, 4)),
            (b"2", (2, 404)),
            (b"", (404, 404)),
            (b"ok", (412, 410)),
        ];
        let mut seen = 0;
        let mut last_buffer = None;
        for _ in 0..100 {
            if cpu.read_reg(Register::PC) == 0x340002 {
                break;
            }
            if cpu.read_reg(Register::PC) == callback {
                assert!(seen < expected.len());
                let sp = cpu.read_reg(Register::A7);
                assert_eq!(bus.read_bytes(sp + 4, 8), [1, 44, 1, 44, 2, 88, 2, 88]);
                let pointer = bus.read_long(sp + 12);
                let count = bus.read_word(sp + 16) as usize;
                assert_eq!(count, expected[seen].0.len());
                assert_eq!(bus.read_bytes(pointer, count), expected[seen].0);
                assert_eq!(dispatcher.pn_loc, expected[seen].1);
                assert_eq!(bus.read_word(0x181000 + 48) as i16, expected[seen].1 .0);
                assert_eq!(bus.read_word(0x181000 + 50) as i16, expected[seen].1 .1);
                last_buffer = Some(pointer);
                seen += 1;
            }
            match cpu.step(&mut bus) {
                StepResult::Ok => {}
                StepResult::Aline(opcode) => {
                    dispatcher.dispatch(opcode, &mut cpu, &mut bus).unwrap()
                }
                _ => panic!("unexpected text callback result"),
            }
        }
        assert_eq!(seen, 4);
        assert_eq!(bus.read_word(0x350800), 4);
        assert_eq!(cpu.read_reg(Register::PC), 0x340002);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_long(TEST_SP + 8), 0xdead_beef);
        assert_eq!(dispatcher.pn_loc, original_pen);
        assert_eq!(bus.get_alloc_size(last_buffer.unwrap()), None);
        assert_eq!(dispatcher.guest_calls.depth(), 0);
        assert!(dispatcher.picture_callback_stack.is_empty());
    }
}

#[cfg(test)]
mod attribute_tests {
    use super::*;
    use crate::cpu::{M68kCpu, StepResult};
    use crate::trap::test_helpers::{setup_with_port, TEST_SP};

    #[test]
    fn picture_attributes_are_visible_to_native_callbacks_and_restored() {
        let (mut dispatcher, mock, mut bus) = setup_with_port();
        let mut cpu = M68kCpu::new();
        cpu.write_reg(Register::A5, mock.read_reg(Register::A5));
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::PC, 0x340002);
        dispatcher.set_current_port_state(&mut bus, &mut cpu, 0x181000, None);
        dispatcher.tx_font = 4;
        dispatcher.tx_face = 1;
        dispatcher.tx_mode = 0;
        dispatcher.tx_size = 18;
        dispatcher.pn_size = (2, 3);
        dispatcher.pn_mode = 8;
        dispatcher.fg_color = (0xffff, 0, 0);
        dispatcher.bg_color = (0, 0xffff, 0);
        dispatcher.sync_current_port_draw_state(&mut bus);
        let original = dispatcher.capture_current_port_state(&bus).draw_state;
        let picture = 0x350100;
        let handle = 0x350200;
        let destination = 0x350300;
        let procs = 0x350400;
        let callback = 0x360000;
        let bytes = [
            0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1, 0x03, 0, 21, 0x04, 3, 0x05, 0, 1, 0x0d, 0, 12,
            0x07, 0, 3, 0, 2, 0x08, 0, 10, 0x1a, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0x1b, 0xab,
            0xcd, 0xef, 0x01, 0x23, 0x45, 0xa1, 0, 150, 0, 3, b'a', b'b', b'c', 0x07, 0, 0, 0, 0,
            0x0d, 0, 9, 0xa0, 0, 151, 0xff,
        ];
        bus.write_bytes(picture, &bytes);
        bus.write_long(handle, picture);
        bus.write_bytes(destination, &[0, 0, 0, 0, 0, 16, 0, 24]);
        bus.write_long(0x181000 + 104, procs);
        bus.write_long(procs + 36, callback);
        let words = [
            0x5279, 0x0035, 0x0800, // addq.w #1,count
            0x302f, 10, // move.w 10(sp),d0: kind
            0x33c0, 0x0035, 0x0802, // move.w d0,last_kind
            0x225f, 0x508f, 0x4ed1, // pop return, pop 8 arguments, return
        ];
        for (index, word) in words.into_iter().enumerate() {
            bus.write_word(callback + index as u32 * 2, word);
        }
        bus.write_long(TEST_SP, destination);
        bus.write_long(TEST_SP + 4, handle);
        bus.write_long(TEST_SP + 8, 0xdead_beef);
        dispatcher
            .dispatch_quickdraw(true, 0xf6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::PC), callback);
        assert_eq!(dispatcher.tx_font, 21);
        assert_eq!(dispatcher.tx_face, 3);
        assert_eq!(dispatcher.tx_mode, 1);
        assert_eq!(dispatcher.tx_size, 12);
        assert_eq!(dispatcher.pn_size, (6, 6));
        assert_eq!(dispatcher.pn_mode, 10);
        assert_eq!(dispatcher.fg_color, (0x1234, 0x5678, 0x9abc));
        assert_eq!(dispatcher.bg_color, (0xabcd, 0xef01, 0x2345));
        assert_eq!(bus.read_word(0x181000 + 52), 6);
        assert_eq!(bus.read_word(0x181000 + 68), 21);
        let sp = cpu.read_reg(Register::A7);
        assert_eq!(bus.read_word(sp + 8), 3);
        assert_eq!(bus.read_word(sp + 10), 150);
        let data_handle = bus.read_long(sp + 4);
        let data = bus.read_long(data_handle);
        assert_eq!(bus.read_bytes(data, 3), b"abc");
        assert_eq!(bus.get_alloc_size(data), Some(3));
        let mut saw_short = false;
        for _ in 0..100 {
            if cpu.read_reg(Register::PC) == 0x340002 {
                break;
            }
            if cpu.read_reg(Register::PC) == callback && bus.read_word(0x350800) == 1 {
                let sp = cpu.read_reg(Register::A7);
                assert_eq!(bus.read_long(sp + 4), 0);
                assert_eq!(bus.read_word(sp + 8), 0);
                assert_eq!(bus.read_word(sp + 10), 151);
                assert_eq!(dispatcher.pn_size, (1, 1));
                assert_eq!(dispatcher.tx_size, 9);
                assert_eq!(bus.read_word(0x181000 + 52), 1);
                assert_eq!(bus.read_word(0x181000 + 74), 9);
                saw_short = true;
            }
            match cpu.step(&mut bus) {
                StepResult::Ok => {}
                StepResult::Aline(opcode) => {
                    dispatcher.dispatch(opcode, &mut cpu, &mut bus).unwrap()
                }
                _ => panic!("unexpected callback result"),
            }
        }
        assert!(saw_short);
        assert_eq!(cpu.read_reg(Register::PC), 0x340002);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_long(TEST_SP + 8), 0xdead_beef);
        assert_eq!(bus.read_word(0x350800), 2);
        assert_eq!(bus.read_word(0x350802), 151);
        assert_eq!(bus.get_alloc_size(data), None);
        assert_eq!(bus.get_alloc_size(data_handle), None);
        assert_eq!(dispatcher.guest_calls.depth(), 0);
        assert_eq!(dispatcher.tx_font, original.tx_font);
        assert_eq!(dispatcher.tx_face, original.tx_face);
        assert_eq!(dispatcher.tx_mode, original.tx_mode);
        assert_eq!(dispatcher.tx_size, original.tx_size);
        assert_eq!(dispatcher.pn_size, original.pn_size);
        assert_eq!(dispatcher.pn_mode, original.pn_mode);
        assert_eq!(dispatcher.fg_color, original.fg_color);
        assert_eq!(dispatcher.bg_color, original.bg_color);
    }
}

#[cfg(test)]
mod clip_tests {
    use super::*;
    use crate::cpu::{M68kCpu, StepResult};
    use crate::trap::test_helpers::{setup_with_port, TEST_SP};

    #[test]
    fn complex_picture_clip_is_mapped_intersected_and_remapped_after_origin() {
        let (mut dispatcher, mock, mut bus) = setup_with_port();
        let mut cpu = M68kCpu::new();
        cpu.write_reg(Register::A5, mock.read_reg(Register::A5));
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::PC, 0x340002);
        let user_clip = 0x351000;
        let user_data = 0x351100;
        let original = [0, 10, 0, 1, 0, 1, 0, 7, 0, 7];
        bus.write_long(user_clip, user_data);
        bus.write_bytes(user_data, &original);
        bus.write_long(0x181000 + 28, user_clip);
        let mut bytes = vec![0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1, 0x01];
        // Two diagonal 2x2 squares. The missing corners must stay excluded.
        for word in [
            36u16, 0, 0, 4, 4, 0, 0, 2, 0x7fff, 2, 0, 4, 0x7fff, 4, 2, 4, 0x7fff, 0x7fff,
        ] {
            bytes.extend_from_slice(&word.to_be_bytes());
        }
        bytes.extend_from_slice(&[0xa0, 0, 150, 0x0c, 0, 1, 0, 0, 0xa0, 0, 151, 0xff]);
        bus.write_bytes(0x350100, &bytes);
        bus.write_long(0x350200, 0x350100);
        bus.write_bytes(0x350300, &[0, 0, 0, 0, 0, 16, 0, 16]);
        bus.write_long(0x181000 + 104, 0x350400);
        bus.write_long(0x350400 + 36, 0x360000);
        for (index, word) in [0x225fu16, 0x508f, 0x4ed1].into_iter().enumerate() {
            bus.write_word(0x360000 + index as u32 * 2, word);
        }
        bus.write_long(TEST_SP, 0x350300);
        bus.write_long(TEST_SP + 4, 0x350200);
        dispatcher
            .dispatch_quickdraw(true, 0xf6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let mut seen = 0;
        let mut mapped = 0;
        let mut mapped_data = 0;
        for _ in 0..80 {
            if cpu.read_reg(Register::PC) == 0x340002 {
                break;
            }
            if cpu.read_reg(Register::PC) == 0x360000 {
                mapped = bus.read_long(0x181000 + 28);
                mapped_data = bus.read_long(mapped);
                assert_ne!(mapped, user_clip);
                assert_eq!(bus.read_bytes(user_data, 10), original);
                assert!(TrapDispatcher::region_contains_point(&bus, mapped, 5, 5));
                assert!(!TrapDispatcher::region_contains_point(&bus, mapped, 0, 0));
                assert!(!TrapDispatcher::region_contains_point(&bus, mapped, 7, 7));
                assert_eq!(
                    TrapDispatcher::region_contains_point(&bus, mapped, 2, 3),
                    seen == 0
                );
                assert_eq!(
                    TrapDispatcher::region_contains_point(&bus, mapped, 5, 3),
                    seen == 1
                );
                seen += 1;
            }
            match cpu.step(&mut bus) {
                StepResult::Ok => {}
                StepResult::Aline(opcode) => {
                    dispatcher.dispatch(opcode, &mut cpu, &mut bus).unwrap()
                }
                _ => panic!("unexpected clip callback result"),
            }
        }
        assert_eq!(seen, 2);
        assert_eq!(cpu.read_reg(Register::PC), 0x340002);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_long(0x181000 + 28), user_clip);
        assert_eq!(bus.read_bytes(user_data, 10), original);
        assert_eq!(bus.get_alloc_size(mapped), None);
        assert_eq!(bus.get_alloc_size(mapped_data), None);
        assert_eq!(dispatcher.guest_calls.depth(), 0);
    }
}

#[cfg(test)]
mod shape_tests {
    use super::*;
    use crate::cpu::{M68kCpu, StepResult};
    use crate::trap::test_helpers::{setup_with_port, TEST_SP};

    #[test]
    fn native_shape_callbacks_share_rect_and_receive_pascal_dimensions_and_angles() {
        let (mut dispatcher, mock, mut bus) = setup_with_port();
        let mut cpu = M68kCpu::new();
        cpu.write_reg(Register::A5, mock.read_reg(Register::A5));
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::PC, 0x340002);
        let bytes = [
            0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1, 0x0b, 0, 2, 0,
            3, // oval size, scaled to height4/width9
            0x41, 0, 1, 0, 2, 0, 3, 0, 4,    // PaintRRect
            0x58, // FrameSameOval, shares preceding rectangle
            0x69, 0xff, 0xe2, 0, 90, // PaintSameArc, start-30/sweep90
            0xff,
        ];
        bus.write_bytes(0x350100, &bytes);
        bus.write_long(0x350200, 0x350100);
        bus.write_bytes(0x350300, &[0, 0, 0, 0, 0, 16, 0, 24]);
        bus.write_long(0x181000 + 104, 0x350400);
        for (slot, callback, count) in [(12, 0x360000, 10), (16, 0x360100, 6), (20, 0x360200, 10)] {
            bus.write_long(0x350400 + slot, callback);
            for (index, word) in [0x225fu16, 0x4fef, count, 0x4ed1].into_iter().enumerate() {
                bus.write_word(callback + index as u32 * 2, word);
            }
        }
        bus.write_long(TEST_SP, 0x350300);
        bus.write_long(TEST_SP + 4, 0x350200);
        bus.write_long(TEST_SP + 8, 0xdead_beef);
        dispatcher
            .dispatch_quickdraw(true, 0xf6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let mut seen = Vec::new();
        for _ in 0..80 {
            let pc = cpu.read_reg(Register::PC);
            if pc == 0x340002 {
                break;
            }
            if matches!(pc, 0x360000 | 0x360100 | 0x360200) {
                let sp = cpu.read_reg(Register::A7);
                let (rect_offset, verb) = if pc == 0x360100 { (4, 0) } else { (8, 1) };
                let pointer = bus.read_long(sp + rect_offset);
                assert_eq!(rect(&bus.read_bytes(pointer, 8)), [2, 6, 6, 12]);
                assert_eq!(bus.read_byte(sp + rect_offset + 4), verb);
                if pc == 0x360000 {
                    assert_eq!(bus.read_word(sp + 4), 4);
                    assert_eq!(bus.read_word(sp + 6), 9);
                } else if pc == 0x360200 {
                    assert_eq!(bus.read_word(sp + 4), 90);
                    assert_eq!(bus.read_word(sp + 6) as i16, -30);
                }
                seen.push(pc);
            }
            match cpu.step(&mut bus) {
                StepResult::Ok => {}
                StepResult::Aline(opcode) => {
                    dispatcher.dispatch(opcode, &mut cpu, &mut bus).unwrap()
                }
                _ => panic!("unexpected shape callback result"),
            }
        }
        assert_eq!(seen, [0x360000, 0x360100, 0x360200]);
        assert_eq!(cpu.read_reg(Register::PC), 0x340002);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_long(TEST_SP + 8), 0xdead_beef);
        assert_eq!(dispatcher.guest_calls.depth(), 0);
        assert!(dispatcher.picture_callback_stack.is_empty());
    }
}

#[cfg(test)]
mod line_tests {
    use super::*;
    use crate::cpu::{M68kCpu, StepResult};
    use crate::trap::test_helpers::{setup_with_port, TEST_SP};

    #[test]
    fn native_line_callbacks_map_signed_deltas_and_preserve_picture_line_position() {
        let (mut dispatcher, mock, mut bus) = setup_with_port();
        let mut cpu = M68kCpu::new();
        cpu.write_reg(Register::A5, mock.read_reg(Register::A5));
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::PC, 0x340002);
        let bytes = [
            0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1, 0x20, 0, 1, 0, 2, 0, 3, 0, 4, 0x21, 0, 5, 0, 6,
            0x22, 0, 7, 0, 8, 0xfe, 0xfd, 0x23, 1, 0xff, 0xff,
        ];
        bus.write_bytes(0x350100, &bytes);
        bus.write_long(0x350200, 0x350100);
        bus.write_bytes(0x350300, &[0, 0, 0, 0, 0, 16, 0, 24]);
        bus.write_long(0x181000 + 104, 0x350400);
        bus.write_long(0x350400 + 4, 0x360000);
        for (index, word) in [0x225fu16, 0x588f, 0x4ed1].into_iter().enumerate() {
            bus.write_word(0x360000 + index as u32 * 2, word);
        }
        bus.write_long(TEST_SP, 0x350300);
        bus.write_long(TEST_SP + 4, 0x350200);
        dispatcher
            .dispatch_quickdraw(true, 0xf6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let expected = [
            ((2, 6), (6, 12)),
            ((6, 12), (10, 18)),
            ((14, 24), (8, 18)),
            ((8, 18), (6, 21)),
        ];
        let mut seen = 0;
        for _ in 0..100 {
            if cpu.read_reg(Register::PC) == 0x340002 {
                break;
            }
            if cpu.read_reg(Register::PC) == 0x360000 {
                assert!(seen < expected.len());
                let sp = cpu.read_reg(Register::A7);
                assert_eq!(dispatcher.pn_loc, expected[seen].0);
                assert_eq!(
                    (bus.read_word(sp + 4) as i16, bus.read_word(sp + 6) as i16),
                    expected[seen].1
                );
                // Callback-owned pen changes must not alter the stored PICT endpoint.
                dispatcher.pn_loc = (99, 99);
                dispatcher.sync_current_port_draw_state(&mut bus);
                seen += 1;
            }
            match cpu.step(&mut bus) {
                StepResult::Ok => {}
                StepResult::Aline(opcode) => {
                    dispatcher.dispatch(opcode, &mut cpu, &mut bus).unwrap()
                }
                _ => panic!("unexpected line callback result"),
            }
        }
        assert_eq!(seen, 4);
        assert_eq!(cpu.read_reg(Register::PC), 0x340002);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(dispatcher.pn_loc, (0, 0));
        assert_eq!(dispatcher.guest_calls.depth(), 0);
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    use crate::cpu::{M68kCpu, StepResult};
    use crate::trap::test_helpers::{setup_with_port, TEST_SP};

    #[test]
    fn picture_font_mapping_header_and_glyph_state_are_scoped_to_playback() {
        let (mut dispatcher, mock, mut bus) = setup_with_port();
        let mut cpu = M68kCpu::new();
        cpu.write_reg(Register::A5, mock.read_reg(Register::A5));
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::PC, 0x340002);
        dispatcher.outline_preferred = true;
        dispatcher.preserve_glyph = true;
        bus.write_byte(0x0bf4, 0xa5);
        bus.write_byte(0x0a63, 0x5a);
        let mut bytes = vec![0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0, 0x11, 2, 0xff];
        let mut push = |opcode: u16, data: &[u8]| {
            bytes.extend_from_slice(&opcode.to_be_bytes());
            bytes.extend_from_slice(data);
            if bytes.len() % 2 != 0 {
                bytes.push(0);
            }
        };
        push(
            0x0c00,
            &[
                0xff, 0xfe, 0, 0, 0, 72, 0, 0, 0, 72, 0, 0, 0, 0, 0, 0, 0, 4, 0, 4, 0, 0, 0, 0,
            ],
        );
        push(
            0x2c,
            &[
                0, 12, 0, 77, 9, b'H', b'e', b'l', b'v', b'e', b't', b'i', b'c', b'a',
            ],
        );
        push(0x03, &[0, 77]);
        push(0x2e, &[0, 4, 0, 0, 0, 0]);
        push(0x31, &[0, 1, 0, 1, 0, 3, 0, 3]);
        push(0xff, &[]);
        bus.write_bytes(0x350100, &bytes);
        bus.write_long(0x350200, 0x350100);
        bus.write_bytes(0x350300, &[0, 0, 0, 0, 0, 8, 0, 8]);
        bus.write_long(0x181000 + 104, 0x350400);
        bus.write_long(0x350400 + 8, 0x360000);
        for (index, word) in [0x225fu16, 0x5c8f, 0x4ed1].into_iter().enumerate() {
            bus.write_word(0x360000 + index as u32 * 2, word);
        }
        bus.write_long(TEST_SP, 0x350300);
        bus.write_long(TEST_SP + 4, 0x350200);
        dispatcher
            .dispatch_quickdraw(true, 0xf6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::PC), 0x360000);
        let pointer = bus.read_long(cpu.read_reg(Register::A7) + 4);
        assert_eq!(rect(&bus.read_bytes(pointer, 8)), [2, 2, 6, 6]);
        assert_eq!(dispatcher.tx_font, 21);
        assert!(!dispatcher.outline_preferred);
        assert!(!dispatcher.preserve_glyph);
        assert_eq!(bus.read_byte(0x0bf4), 0);
        assert_eq!(bus.read_byte(0x0a63), 0);
        for _ in 0..30 {
            if cpu.read_reg(Register::PC) == 0x340002 {
                break;
            }
            match cpu.step(&mut bus) {
                StepResult::Ok => {}
                StepResult::Aline(opcode) => {
                    dispatcher.dispatch(opcode, &mut cpu, &mut bus).unwrap()
                }
                _ => panic!("unexpected metadata callback result"),
            }
        }
        assert_eq!(cpu.read_reg(Register::PC), 0x340002);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert!(dispatcher.outline_preferred);
        assert!(dispatcher.preserve_glyph);
        assert_eq!(bus.read_byte(0x0bf4), 0xa5);
        assert_eq!(bus.read_byte(0x0a63), 0x5a);
        assert_eq!(dispatcher.guest_calls.depth(), 0);
    }
}

#[cfg(test)]
mod default_state_tests {
    use super::*;
    use crate::cpu::{M68kCpu, StepResult};
    use crate::trap::test_helpers::{setup_with_port, TEST_SP};

    #[test]
    fn color_picture_defaults_and_temporary_patterns_preserve_caller_state() {
        let (mut d, mock, mut bus) = setup_with_port();
        let mut cpu = M68kCpu::new();
        cpu.write_reg(Register::A5, mock.read_reg(Register::A5));
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_bytes(0x350300, &[0, 0, 0, 0, 0, 8, 0, 12]);
        for offset in [0, 4, 8] {
            bus.write_long(TEST_SP + offset, 0);
        }
        bus.write_long(TEST_SP + 12, 0x350300);
        bus.write_word(TEST_SP + 16, 32);
        bus.write_long(TEST_SP + 18, 0x350500);
        d.dispatch_quickdraw(true, 0x31d, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let port = bus.read_long(0x350500);
        d.set_current_port_state(&mut bus, &mut cpu, port, None);
        let mut original_patterns = Vec::new();
        for offset in [32, 58, 62] {
            let data = bus.alloc(28);
            let handle = bus.alloc(4);
            bus.write_bytes(data, &[0; 28]);
            bus.write_bytes(data + 20, &[0x55; 8]);
            bus.write_long(handle, data);
            bus.write_long(port + offset, handle);
            original_patterns.push((offset, handle, data));
        }
        d.tx_font = 4;
        d.tx_size = 18;
        d.pn_size = (7, 8);
        d.pn_vis = -2;
        d.pn_mode = 10;
        d.fg_color = (0xffff, 0, 0);
        d.bg_color = (0, 0xffff, 0);
        d.sync_current_port_draw_state(&mut bus);
        d.write_port_hilite_color(&mut bus, port, (1, 2, 3));
        d.set_current_port_state(&mut bus, &mut cpu, port, None);
        let original = d.capture_current_port_state(&bus);
        bus.write_word(0x0da0, 0x1234);
        bus.write_word(0x0da2, 0x5678);
        bus.write_word(0x0da4, 0x9abc);
        let picture = [
            0, 0, 0, 0, 0, 0, 0, 4, 0, 4, 0x11, 1, 0x1e, 0xa0, 0, 150, 0xff,
        ];
        bus.write_bytes(0x350100, &picture);
        bus.write_long(0x350200, 0x350100);
        bus.write_long(port + 104, 0x350400);
        bus.write_long(0x350400 + 36, 0x360000);
        for (i, word) in [0x225fu16, 0x508f, 0x4ed1].into_iter().enumerate() {
            bus.write_word(0x360000 + i as u32 * 2, word);
        }
        cpu.write_reg(Register::PC, 0x340002);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0x350300);
        bus.write_long(TEST_SP + 4, 0x350200);
        d.dispatch_quickdraw(true, 0xf6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::PC), 0x360000);
        assert_eq!((d.tx_font, d.tx_size, d.tx_mode), (0, 0, 1));
        assert_eq!((d.pn_size, d.pn_vis, d.pn_mode), ((2, 3), -2, 8));
        assert_eq!(d.fg_color, (0, 0, 0));
        assert_eq!(d.bg_color, (0xffff, 0xffff, 0xffff));
        assert_eq!(d.current_hilite_color(&bus), (0x1234, 0x5678, 0x9abc));
        let mut temporary = Vec::new();
        for ((offset, original_handle, original_data), fill) in
            original_patterns.iter().zip([0, 0xff, 0xff])
        {
            let handle = bus.read_long(port + offset);
            let data = bus.read_long(handle);
            assert_ne!(handle, *original_handle);
            assert_eq!(bus.read_bytes(data + 20, 8), [fill; 8]);
            assert_eq!(bus.read_bytes(original_data + 20, 8), [0x55; 8]);
            temporary.extend([handle, data]);
        }
        for _ in 0..30 {
            if cpu.read_reg(Register::PC) == 0x340002 {
                break;
            }
            match cpu.step(&mut bus) {
                StepResult::Ok => {}
                StepResult::Aline(opcode) => d.dispatch(opcode, &mut cpu, &mut bus).unwrap(),
                _ => panic!("unexpected default-state callback result"),
            }
        }
        assert_eq!(cpu.read_reg(Register::PC), 0x340002);
        assert_eq!(d.capture_current_port_state(&bus), original);
        assert_eq!(d.current_hilite_color(&bus), (1, 2, 3));
        for allocation in temporary {
            assert_eq!(bus.get_alloc_size(allocation), None);
        }
        for (offset, handle, data) in original_patterns {
            assert_eq!(bus.read_long(port + offset), handle);
            assert_eq!(bus.read_bytes(data + 20, 8), [0x55; 8]);
        }
        assert_eq!(d.guest_calls.depth(), 0);
    }
}

#[cfg(test)]
mod reentrancy_tests {
    use super::*;
    use crate::cpu::{M68kCpu, StepResult};
    use crate::trap::test_helpers::{setup_with_port, TEST_SP};

    #[test]
    fn native_nested_picture_restores_outer_state_and_uses_replaced_procedure() {
        let (mut d, mock, mut bus) = setup_with_port();
        let mut cpu = M68kCpu::new();
        cpu.write_reg(Register::A5, mock.read_reg(Register::A5));
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::PC, 0x340002);
        let outer = [
            0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1, 0x0d, 0, 12, 0x31, 0, 1, 0, 2, 0, 6, 0, 7, 0x3b,
            0xff,
        ];
        let inner = [
            0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1, 0x0d, 0, 30, 0x31, 0, 2, 0, 3, 0, 4, 0, 5, 0xff,
        ];
        bus.write_bytes(0x350200, &outer);
        bus.write_bytes(0x350700, &inner);
        bus.write_long(0x350100, 0x350200);
        bus.write_long(0x350600, 0x350700);
        bus.write_bytes(0x350300, &outer[2..10]);
        bus.write_long(0x181000 + 104, 0x350400);
        bus.write_long(0x350408, 0x360000);
        d.set_current_port_state(&mut bus, &mut cpu, 0x181000, None);
        d.tx_size = 24;
        d.sync_current_port_draw_state(&mut bus);
        // First outer callback replaces rectProc, then invokes DrawPicture.
        // The nested call must return to this native callback before it
        // returns through the original outer picture's private gate.
        let first = [
            0x5279, 0x0035, 0x0800, // addq.w #1,outer_count
            0x23fc, 0x0036, 0x0100, 0x0035, 0x0408, // move.l #second,rectProc
            0x2f3c, 0x0035, 0x0600, // move.l #inner_handle,-(sp)
            0x2f3c, 0x0035, 0x0300, // move.l #destination,-(sp)
            0xa8f6, // DrawPicture
            0x33f9, 0x0018, 0x104a, 0x0035, 0x0804, // save restored txSize
            0x225f, 0x5c8f, 0x4ed1, // pop return, Pascal args, jump
        ];
        let second = [
            0x5279, 0x0035, 0x0802, // addq.w #1,replacement_count
            0x225f, 0x5c8f, 0x4ed1,
        ];
        for (base, words) in [(0x360000, first.as_slice()), (0x360100, second.as_slice())] {
            for (i, word) in words.iter().enumerate() {
                bus.write_word(base + i as u32 * 2, *word);
            }
        }
        bus.write_long(TEST_SP, 0x350300);
        bus.write_long(TEST_SP + 4, 0x350100);
        bus.write_long(TEST_SP + 8, 0xdead_beef);
        d.dispatch_quickdraw(true, 0xf6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let mut allocations = Vec::new();
        let mut saw_nested = false;
        for _ in 0..200 {
            for frame in &d.picture_callback_stack {
                allocations.extend([frame.gate, frame.rect_buffer]);
            }
            if d.picture_callback_stack.len() == 2 {
                saw_nested = true;
                assert_eq!(d.tx_size, 30);
                assert_eq!(d.guest_calls.depth(), 2);
            }
            if cpu.read_reg(Register::PC) == 0x340002 {
                break;
            }
            match cpu.step(&mut bus) {
                StepResult::Ok => {}
                StepResult::Aline(opcode) => d.dispatch(opcode, &mut cpu, &mut bus).unwrap(),
                _ => panic!("unexpected nested callback result"),
            }
        }
        assert!(saw_nested);
        assert_eq!(cpu.read_reg(Register::PC), 0x340002);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_long(TEST_SP + 8), 0xdead_beef);
        assert_eq!(bus.read_word(0x350800), 1);
        assert_eq!(bus.read_word(0x350802), 2);
        assert_eq!(bus.read_word(0x350804), 12);
        assert_eq!(d.tx_size, 24);
        assert_eq!(bus.read_word(0x181000 + 74), 24);
        assert_eq!(bus.read_long(0x350408), 0x360100);
        assert_eq!(d.guest_calls.depth(), 0);
        assert!(d.picture_callback_stack.is_empty());
        for address in allocations {
            assert_eq!(bus.get_alloc_size(address), None);
        }
    }

    #[test]
    fn unsupported_or_truncated_streams_decline_before_mutating_port_or_stack() {
        let (mut d, mut cpu, mut bus) = setup_with_port();
        bus.write_long(0x181000 + 104, 0x350400);
        bus.write_long(0x350408, 0x360000);
        bus.write_bytes(0x350300, &[0, 0, 0, 0, 0, 8, 0, 8]);
        d.set_current_port_state(&mut bus, &mut cpu, 0x181000, None);
        d.tx_size = 24;
        d.sync_current_port_draw_state(&mut bus);
        let port = bus.read_bytes(0x181000, 108);
        let sp = cpu.read_reg(Register::A7);
        let pc = cpu.read_reg(Register::PC);
        // BkPat is a valid PICT operation handled by the fallback renderer.
        // The earlier TxSize must not leak out of a rejected mixed stream.
        let unsupported = [
            0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1, 0x0d, 0, 30, 0x02, 0x55, 0x55, 0x55, 0x55, 0x55,
            0x55, 0x55, 0x55, 0xff,
        ];
        let truncated = [0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1, 0x31, 0, 1];
        for bytes in [unsupported.as_slice(), truncated.as_slice()] {
            assert!(!d.begin_picture_callbacks(
                &mut cpu,
                &mut bus,
                0x350100,
                0x350300,
                Some(bytes)
            ));
            assert_eq!(bus.read_bytes(0x181000, 108), port);
            assert_eq!(d.tx_size, 24);
            assert_eq!(cpu.read_reg(Register::A7), sp);
            assert_eq!(cpu.read_reg(Register::PC), pc);
            assert_eq!(d.guest_calls.depth(), 0);
            assert!(d.picture_callback_stack.is_empty());
        }
    }
}

#[cfg(test)]
mod resident_tests {
    use super::*;

    #[test]
    fn resident_picture_ignores_wrapped_size_and_stops_at_end_opcode() {
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let mut bytes = vec![0, 1, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1];
        bytes.resize(100_000, 0); // Many tiny NOP operations, beyond u16 picSize.
        bytes.push(0xff);
        let pointer = bus.alloc(bytes.len() as u32 + 16);
        bus.write_bytes(pointer, &bytes);
        bus.write_bytes(pointer + bytes.len() as u32, &[0x55; 16]);
        assert_eq!(resident_picture(&bus, pointer), Some(bytes));
    }

    #[test]
    fn resident_picture_does_not_take_terminator_from_following_allocation() {
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let bytes = [0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1];
        let pointer = bus.alloc(bytes.len() as u32);
        let allocated = bus.get_alloc_size(pointer).unwrap();
        bus.write_bytes(pointer, &bytes);
        // Even readable adjacent memory must not complete a truncated object.
        bus.write_byte(pointer + allocated, 0xff);
        assert_eq!(resident_picture(&bus, pointer), None);
    }

    #[test]
    fn resident_picture_accepts_end_opcode_immediately_before_mapping_hole() {
        use crate::memory::GuestAddressSpace;
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let bytes = vec![0, 0, 0, 0, 0, 0, 0, 8, 0, 8, 0x11, 1, 0xff];
        let mut memory = GuestAddressSpace::new();
        memory.add_region(0x0303_0000, bytes.clone());
        memory.add_region(0x0304_0000, bytes[..12].to_vec());
        bus.attach_guest_address_space(memory.shared_view());
        assert!(!bus.is_guest_address_mapped(0x0303_0000, 14));
        assert_eq!(resident_picture(&bus, 0x0303_0000), Some(bytes));
        assert_eq!(resident_picture(&bus, 0x0304_0000), None);
    }
}
