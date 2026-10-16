//! Resource-backed classic dialog records and copied item lists.
use super::*;
mod drawing;
mod editing;
mod text;
use crate::{
    controls::{NewControl, Op as ControlOp},
    graphics::{Graphics, Rect},
    windows::{NewWindow, Op as WindowOp},
};
pub const RECORD_SIZE: u32 = 170;
pub(super) struct Record {
    items: u32,
    text: Vec<u32>,
    editor: u32,
    closed: bool,
    outline: Option<Rect>,
    automatic_default: bool,
}
struct Template {
    bounds: Rect,
    proc_id: i16,
    visible: bool,
    go_away: bool,
    reference: u32,
    items: i16,
    title: Vec<u8>,
    position: u16,
}
struct Item {
    offset: usize,
    rect: Rect,
    kind: u8,
    data: Vec<u8>,
}
fn rect(b: &[u8]) -> Rect {
    let w = |i| i16::from_be_bytes(b[i..i + 2].try_into().unwrap()) as i32;
    Rect::new(w(2), w(0), w(6), w(4))
}
fn rect_bytes(r: Rect) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for v in [r.top, r.left, r.bottom, r.right] {
        out.extend(
            i16::try_from(v)
                .map_err(|_| bad("dialog coordinate overflow"))?
                .to_be_bytes(),
        );
    }
    Ok(out)
}
fn template(b: &[u8]) -> Result<Template> {
    if b.len() < 21 {
        return Err(bad("truncated DLOG"));
    }
    let end = 21 + usize::from(b[20]);
    if end > b.len() {
        return Err(bad("truncated DLOG title"));
    }
    let aligned = (end + 1) & !1;
    let position = if b.len() == end || b.len() == aligned {
        0
    } else if b.len() == aligned + 2 {
        u16::from_be_bytes(b[aligned..aligned + 2].try_into().unwrap())
    } else {
        return Err(bad("invalid DLOG positioning data"));
    };
    let result = Template {
        bounds: rect(b),
        proc_id: i16::from_be_bytes(b[8..10].try_into().unwrap()),
        visible: b[10] != 0,
        go_away: b[12] != 0,
        reference: u32::from_be_bytes(b[14..18].try_into().unwrap()),
        items: i16::from_be_bytes(b[18..20].try_into().unwrap()),
        title: b[20..end].to_vec(),
        position,
    };
    if !matches!(position, 0 | 0x280a) || !matches!(result.proc_id, 1 | 2 | 4) {
        return Err(bad("unsupported dialog positioning or definition"));
    }
    Ok(result)
}
fn items(b: &[u8]) -> Result<Vec<Item>> {
    if b.len() < 2 {
        return Err(bad("truncated DITL"));
    }
    let count = i32::from(i16::from_be_bytes(b[..2].try_into().unwrap())) + 1;
    if !(0..=256).contains(&count) {
        return Err(bad("invalid dialog item count"));
    }
    let mut out = Vec::new();
    let mut at = 2usize;
    for _ in 0..count {
        let head = b
            .get(at..at + 14)
            .ok_or_else(|| bad("truncated dialog item"))?;
        let len = usize::from(head[13]);
        let end = at + 14 + len;
        let data = b
            .get(at + 14..end)
            .ok_or_else(|| bad("truncated dialog item data"))?
            .to_vec();
        let kind = head[12];
        let r = rect(&head[4..12]);
        match kind & 0x7f {
            0 | 8 => {}
            4..=6 | 16 => {
                if r.left >= r.right || r.top >= r.bottom {
                    return Err(bad("empty dialog control"));
                }
            }
            32 | 64 if len == 2 => {}
            _ => return Err(bad("unsupported dialog item type or data")),
        };
        out.push(Item {
            offset: at,
            rect: r,
            kind,
            data,
        });
        at = (end + 1) & !1;
        if at > b.len() {
            return Err(bad("missing dialog item padding"));
        }
    }
    if at != b.len() {
        return Err(bad("trailing DITL data"));
    }
    Ok(out)
}
fn payload(m: &Memory, p: &mut Process, h: u32) -> Result<Vec<u8>> {
    let n = p.handle_size(m, h)?;
    if p.mem_error != 0 {
        return Err(bad("dialog item list is not owned"));
    }
    let data = GuestPtr(m.u32(GuestPtr(h))?);
    Ok(m.read(data, n as usize)?.to_vec())
}
fn window(m: &mut Memory, p: &mut Process, g: &Graphics, op: WindowOp, w: u32) -> Result<u32> {
    let mut manager = std::mem::take(&mut p.windows);
    let result = manager.call(m, p, g, op, w);
    p.windows = manager;
    result
}
impl Dialogs {
    pub fn resource(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        id: i16,
        storage: GuestPtr,
        behind: u32,
    ) -> Result<u32> {
        if storage.0 != 0 {
            m.validate_write(storage, RECORD_SIZE as usize)?;
        }
        let source = p.get_resource(m, *b"DLOG", id)?;
        if source == 0 {
            return Ok(0);
        }
        let t = template(&payload(m, p, source)?)?;
        if p.find_resource(*b"dctb", id).is_some() {
            return Err(bad("dialog color tables are unsupported"));
        }
        let source = p.get_resource(m, *b"DITL", t.items)?;
        if source == 0 {
            return Ok(0);
        }
        let raw = payload(m, p, source)?;
        let parsed = items(&raw)?;
        let ports = p
            .ports
            .ok_or_else(|| bad("GetNewDialog requires InitGraf"))?;
        let previous = m.u32(ports.current_slot)?;
        m.validate_write(ports.current_slot, 4)?;
        let saved_storage = if storage.0 != 0 {
            Some(m.read(storage, RECORD_SIZE as usize)?.to_vec())
        } else {
            None
        };
        let mut scratch = 0;
        let mut w = storage.0;
        let mut copied = 0;
        let mut text = Vec::new();
        let mut editor = 0;
        let mut opened = false;
        let result = (|| {
            scratch = p.new_pointer(m, 288, true)?;
            if scratch == 0 {
                return Ok(0);
            }
            if w == 0 {
                w = p.new_pointer(m, RECORD_SIZE, true)?;
                if w == 0 {
                    return Ok(0);
                }
            }
            copied = p.new_handle(m, raw.len() as u32, false)?;
            if copied == 0 {
                return Ok(0);
            }
            let copy = GuestPtr(m.u32(GuestPtr(copied))?);
            m.write(copy, &raw)?;
            let mut bounds = t.bounds;
            if t.position == 0x280a {
                let screen = crate::bitmap::Bitmap::read(m, ports.screen_bitmap)?.bounds;
                let menu = i32::from(m.u16(GuestPtr(0xbaa))? as i16);
                if menu < 0 {
                    return Err(bad("invalid menu height"));
                }
                let width = bounds.right - bounds.left;
                let height = bounds.bottom - bounds.top;
                let left_margin = if t.proc_id == 1 { 8 } else { 1 };
                let right_margin = if t.proc_id == 4 { 2 } else { left_margin };
                let top_margin = if t.proc_id == 4 { 19 } else { left_margin };
                let bottom_margin = right_margin;
                let left = screen.left
                    + (screen.right - screen.left - width - left_margin - right_margin) / 2
                    + left_margin;
                let top = screen.top
                    + menu
                    + (screen.bottom - screen.top - menu - height - top_margin - bottom_margin) / 2
                    + top_margin;
                bounds = Rect::new(left, top, left + width, top + height);
            }
            m.write(GuestPtr(scratch), &rect_bytes(bounds)?)?;
            m.write(GuestPtr(scratch + 16), &t.title)?;
            let mut manager = std::mem::take(&mut p.windows);
            let created = manager.create(
                m,
                p,
                g,
                NewWindow {
                    storage: GuestPtr(w),
                    bounds: GuestPtr(scratch),
                    title: GuestPtr(scratch + 16),
                    visible: false,
                    proc_id: t.proc_id,
                    behind,
                    go_away: t.go_away,
                    ref_con: t.reference,
                },
            );
            p.windows = manager;
            if created? == 0 {
                return Ok(0);
            }
            opened = true;
            m.write(GuestPtr(w + 108), &2u16.to_be_bytes())?;
            m.write(GuestPtr(w + 156), &copied.to_be_bytes())?;
            m.write(GuestPtr(w + 160), &[0; 10])?;
            m.write(GuestPtr(w + 164), &(-1i16).to_be_bytes())?;
            m.write(GuestPtr(w + 168), &1u16.to_be_bytes())?;
            let font = m.u16(GuestPtr(0xafa))?;
            m.write(GuestPtr(w + 68), &font.to_be_bytes())?;
            for item in &parsed {
                let h = match item.kind & 0x7f {
                    4..=6 => {
                        m.write(GuestPtr(scratch), &rect_bytes(item.rect)?)?;
                        let mut title = vec![item.data.len() as u8];
                        title.extend(&item.data);
                        m.write(GuestPtr(scratch + 16), &title)?;
                        let mut controls = std::mem::take(&mut p.controls);
                        let result = controls.create(
                            m,
                            p,
                            Some(g),
                            NewControl {
                                owner: GuestPtr(w),
                                bounds: GuestPtr(scratch),
                                title: GuestPtr(scratch + 16),
                                visible: true,
                                value: 0,
                                min: 0,
                                max: 1,
                                proc_id: i16::from((item.kind & 0x7f) - 4),
                                ref_con: 0,
                            },
                        );
                        p.controls = controls;
                        let h = result?;
                        if h == 0 {
                            return Ok(0);
                        }
                        if item.kind & 0x80 != 0 {
                            let mut controls = std::mem::take(&mut p.controls);
                            let result = controls.call(m, p, Some(g), ControlOp::Hilite(h, 255));
                            p.controls = controls;
                            result?;
                        }
                        h
                    }
                    8 | 16 => {
                        let h = p.new_handle(m, item.data.len() as u32, false)?;
                        if h == 0 {
                            return Ok(0);
                        }
                        text.push(h);
                        let ptr = GuestPtr(m.u32(GuestPtr(h))?);
                        m.write(ptr, &item.data)?;
                        h
                    }
                    32 | 64 => {
                        let id = i16::from_be_bytes(item.data[..2].try_into().unwrap());
                        let h = p.get_resource(
                            m,
                            if item.kind & 0x7f == 32 {
                                *b"ICON"
                            } else {
                                *b"PICT"
                            },
                            id,
                        )?;
                        if h == 0 && p.res_error == -108 {
                            return Ok(0);
                        }
                        h
                    }
                    0 => 0,
                    _ => unreachable!(),
                };
                let copy = m.u32(GuestPtr(copied))?;
                m.write(GuestPtr(copy + item.offset as u32), &h.to_be_bytes())?;
            }
            if let Some((index, item)) = parsed
                .iter()
                .enumerate()
                .find(|(_, item)| item.kind & 0x7f == 16)
            {
                m.write(GuestPtr(scratch), &rect_bytes(item.rect)?)?;
                ports.set(m, GuestPtr(w))?;
                let mut te = std::mem::take(&mut p.textedit);
                let result = (|| {
                    editor = te.call(
                        m,
                        p,
                        crate::textedit::Op::New {
                            dest: GuestPtr(scratch),
                            view: GuestPtr(scratch),
                            styled: false,
                        },
                        0,
                    )?;
                    if editor == 0 {
                        return Ok(());
                    }
                    let copy = m.u32(GuestPtr(copied))?;
                    let h = m.u32(GuestPtr(copy + item.offset as u32))?;
                    te.bind_dialog_text(m, p, editor, h, GuestPtr(scratch))
                })();
                p.textedit = te;
                result?;
                if editor == 0 || p.mem_error != 0 {
                    return Ok(0);
                }
                m.write(GuestPtr(w + 160), &editor.to_be_bytes())?;
                m.write(GuestPtr(w + 164), &(index as i16).to_be_bytes())?;
            }
            if t.visible {
                window(m, p, g, WindowOp::Show, w)?;
                let mut controls = std::mem::take(&mut p.controls);
                let result = controls.call(m, p, Some(g), ControlOp::DrawAll(w));
                p.controls = controls;
                result?;
                ports.set(m, GuestPtr(w))?;
                for item in &parsed {
                    if matches!(item.kind & 0x7f, 4..=6) {
                        m.write(GuestPtr(scratch), &rect_bytes(item.rect)?)?;
                        let manager = std::mem::take(&mut p.windows);
                        let result = manager.invalidate(m, p, GuestPtr(scratch), true);
                        p.windows = manager;
                        result?;
                    }
                }
            }
            Ok(w)
        })();
        let allocation_error = p.mem_error;
        m.write(ports.current_slot, &previous.to_be_bytes())?;
        if result.as_ref().is_ok_and(|&v| v != 0) {
            self.records.insert(
                w,
                Record {
                    items: copied,
                    text,
                    editor,
                    closed: false,
                    outline: None,
                    automatic_default: false,
                },
            );
            if scratch != 0 {
                p.dispose_pointer(scratch);
            }
            p.mem_error = 0;
            p.res_error = 0;
            return result;
        }
        if editor != 0 {
            let mut te = std::mem::take(&mut p.textedit);
            let cleanup = te.call(m, p, crate::textedit::Op::Dispose(editor), 0);
            p.textedit = te;
            cleanup?;
        }
        if opened {
            window(
                m,
                p,
                g,
                if storage.0 == 0 {
                    WindowOp::Dispose
                } else {
                    WindowOp::Close
                },
                w,
            )?;
        } else if storage.0 == 0 && w != 0 {
            p.dispose_pointer(w);
        }
        for h in text {
            p.dispose_handle(m, h)?;
        }
        if copied != 0 {
            p.dispose_handle(m, copied)?;
        }
        if scratch != 0 {
            p.dispose_pointer(scratch);
        }
        if let Some(bytes) = saved_storage {
            m.write(storage, &bytes)?;
        }
        m.write(ports.current_slot, &previous.to_be_bytes())?;
        p.mem_error = allocation_error;
        result
    }
    pub fn item(
        &self,
        m: &mut Memory,
        p: &mut Process,
        dialog: u32,
        number: i16,
        outputs: [GuestPtr; 3],
    ) -> Result<()> {
        let [kind, handle, bounds] = outputs;
        let saved = (p.mem_error, p.res_error);
        let result = (|| {
            if kind.0 == 0 || handle.0 == 0 || bounds.0 == 0 {
                return Err(bad("nil dialog item output"));
            }
            m.validate_write(kind, 2)?;
            m.validate_write(handle, 4)?;
            m.validate_write(bounds, 8)?;
            let record = self
                .records
                .get(&dialog)
                .filter(|r| !r.closed)
                .ok_or_else(|| bad("unknown or closed dialog"))?;
            if !p.windows.contains(m, dialog)?
                || m.u16(GuestPtr(dialog + 108))? != 2
                || m.u32(GuestPtr(dialog + 156))? != record.items
            {
                return Err(bad("dialog record ownership changed"));
            }
            let data = payload(m, p, record.items)?;
            let list = items(&data)?;
            let item = number
                .checked_sub(1)
                .and_then(|n| usize::try_from(n).ok())
                .and_then(|n| list.get(n))
                .ok_or_else(|| bad("dialog item number out of range"))?;
            let value = data[item.offset..item.offset + 4].to_vec();
            let rectangle = data[item.offset + 4..item.offset + 12].to_vec();
            m.write(kind, &u16::from(item.kind).to_be_bytes())?;
            m.write(handle, &value)?;
            m.write(bounds, &rectangle)?;
            Ok(())
        })();
        (p.mem_error, p.res_error) = saved;
        result
    }
    pub fn dispose(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        dialog: u32,
        close: bool,
    ) -> Result<()> {
        let record = self
            .records
            .get(&dialog)
            .ok_or_else(|| bad("unknown dialog"))?;
        if !close && p.pointer_size(dialog) < RECORD_SIZE {
            return Err(bad(
                "DisposeDialog requires NewPtr storage; static storage requires CloseDialog",
            ));
        }
        if !record.closed {
            if m.u32(GuestPtr(dialog + 156))? != record.items {
                return Err(bad("dialog item ownership changed"));
            }
            if m.u32(GuestPtr(dialog + 160))? != record.editor {
                return Err(bad("dialog TextEdit ownership changed"));
            }
            if record.editor != 0 {
                let mut te = std::mem::take(&mut p.textedit);
                let result = te.call(m, p, crate::textedit::Op::Dispose(record.editor), 0);
                p.textedit = te;
                result?;
            }
            window(
                m,
                p,
                g,
                if close {
                    WindowOp::Close
                } else {
                    WindowOp::Dispose
                },
                dialog,
            )?;
            let record = self.records.get_mut(&dialog).unwrap();
            for h in record.text.drain(..) {
                p.dispose_handle(m, h)?;
            }
            p.dispose_handle(m, record.items)?;
            record.closed = true;
            if close {
                m.write(GuestPtr(dialog + 156), &[0; 14])?;
            }
        } else if !close {
            window(m, p, g, WindowOp::Dispose, dialog)?;
        }
        if !close {
            self.records.remove(&dialog);
        }
        p.mem_error = 0;
        Ok(())
    }
}

impl Dialogs {
    /// Repaint only registered dialogs after a Window Manager show. Their default
    /// item remains in the canonical guest record, including while hidden.
    pub(crate) fn show_default(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        dialog: u32,
    ) -> Result<()> {
        if self.records.get(&dialog).is_some_and(|r| !r.closed) {
            let mut controls = std::mem::take(&mut p.controls);
            let result = controls.call(m, p, Some(g), ControlOp::DrawAll(dialog));
            p.controls = controls;
            result?;
            let number = m.u16(GuestPtr(dialog + 168))? as i16;
            // An untouched NewDialog default field does not install an automatic
            // outline. Only SetDialogDefaultItem opts into the helper behavior.
            if self.records[&dialog].automatic_default {
                self.default_item(m, p, Some(g), dialog, number)?;
            }
        }
        Ok(())
    }
    pub fn default_item(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: Option<&Graphics>,
        dialog: u32,
        number: i16,
    ) -> Result<i16> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            let Some(record) = self.records.get(&dialog).filter(|r| !r.closed) else {
                return Ok(-50);
            };
            if !p.windows.contains(m, dialog)?
                || m.u16(GuestPtr(dialog + 108))? != 2
                || m.u32(GuestPtr(dialog + 156))? != record.items
            {
                return Err(bad("dialog record ownership changed"));
            }
            let data = payload(m, p, record.items)?;
            let list = items(&data)?;
            let target = if number == 0 {
                None
            } else {
                let Some(item) = number
                    .checked_sub(1)
                    .and_then(|n| usize::try_from(n).ok())
                    .and_then(|n| list.get(n))
                else {
                    return Ok(-50);
                };
                if item.kind & 0x7f != 4 {
                    return Ok(-50);
                }
                let h = u32::from_be_bytes(data[item.offset..item.offset + 4].try_into().unwrap());
                {
                    let controls = std::mem::take(&mut p.controls);
                    let result = controls.dialog_button(m, p, h, dialog);
                    p.controls = controls;
                    result?
                }
            };
            let target =
                target.map(|r| Rect::new(r.left - 4, r.top - 4, r.right + 4, r.bottom + 4));
            let coverage = |r: Rect| {
                let oval = ((r.bottom - r.top) / 2 - 4).max(4) as i16;
                crate::rounded::coverage(r, oval, oval, Some((3, 3)))
            };
            let new = target.map(|r| coverage(r).map(|c| (r, c))).transpose()?;
            let old = record
                .outline
                .map(|r| coverage(r).map(|c| (r, c)))
                .transpose()?;
            m.validate_write(GuestPtr(dialog + 168), 2)?;
            let visible = m.read(GuestPtr(dialog + 110), 1)?[0] != 0
                && !crate::region_geometry::decode(&crate::regions::read(
                    m,
                    p,
                    m.u32(GuestPtr(dialog + 24))?,
                )?)?
                .is_empty();
            if visible && (old.is_some() || new.is_some()) {
                let g = g.ok_or_else(|| bad("visible default button requires a GPU"))?;
                if crate::port_layout::color(m, GuestPtr(dialog))? {
                    return Err(bad("color dialog default outlines are unsupported"));
                }
                let ports = p
                    .ports
                    .ok_or_else(|| bad("default button requires InitGraf"))?;
                let previous = m.u32(ports.current_slot)?;
                let saved = m.read(GuestPtr(dialog), 108)?.to_vec();
                m.validate_write(ports.current_slot, 4)?;
                m.validate_write(GuestPtr(dialog), 108)?;
                let background = crate::port_layout::pattern(
                    m,
                    GuestPtr(dialog),
                    crate::port_layout::Pattern::Background,
                )?;
                let paint = (|| {
                    ports.set(m, GuestPtr(dialog))?;
                    m.write(GuestPtr(dialog + 56), &8u16.to_be_bytes())?;
                    if let Some((r, c)) = &old {
                        crate::port_layout::write_pattern(
                            m,
                            GuestPtr(dialog),
                            crate::port_layout::Pattern::Pen,
                            &background,
                        )?;
                        ports.paint_coverage(g, m, *r, Some(c))?;
                    }
                    if let Some((r, c)) = &new {
                        m.write(GuestPtr(dialog + 80), &33u32.to_be_bytes())?;
                        m.write(GuestPtr(dialog + 84), &30u32.to_be_bytes())?;
                        crate::port_layout::write_pattern(
                            m,
                            GuestPtr(dialog),
                            crate::port_layout::Pattern::Pen,
                            &[255; 8],
                        )?;
                        ports.paint_coverage(g, m, *r, Some(c))?;
                    }
                    Ok(())
                })();
                m.write(GuestPtr(dialog), &saved)?;
                m.write(ports.current_slot, &previous.to_be_bytes())?;
                paint?;
            }
            m.write(GuestPtr(dialog + 168), &number.to_be_bytes())?;
            let record = self.records.get_mut(&dialog).unwrap();
            record.outline = if visible { target } else { None };
            record.automatic_default = true;
            Ok(0)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
}
