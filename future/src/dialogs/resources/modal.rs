//! Cooperative modal handling over canonical EventRecords, TextEdit and controls.
use super::*;
use crate::{
    controls::{TrackControl, Tracking},
    events,
};
pub(crate) struct Modal {
    pub dialog: u32,
    pub scratch: u32,
    pub filter: u32,
    pub destination: GuestPtr,
    tracking: Option<(Tracking, i16)>,
}
impl Dialogs {
    pub(crate) fn begin_modal(
        &self,
        m: &mut Memory,
        p: &mut Process,
        filter: u32,
        destination: GuestPtr,
    ) -> Result<Modal> {
        if destination.0 == 0 {
            return Err(bad("nil ModalDialog itemHit"));
        }
        m.validate_write(destination, 2)?;
        let dialog = p
            .ports
            .ok_or_else(|| bad("ModalDialog requires InitGraf"))?
            .current(m)?
            .0;
        self.modal_items(m, p, dialog)?;
        if m.read(GuestPtr(dialog + 110), 1)?[0] == 0 {
            return Err(bad("ModalDialog requires a visible dialog"));
        }
        let errors = (p.mem_error, p.res_error);
        let scratch = p.new_pointer(m, 32, true)?;
        if scratch == 0 {
            return Err(bad("ModalDialog scratch allocation failed"));
        }
        (p.mem_error, p.res_error) = errors;
        Ok(Modal {
            dialog,
            scratch,
            filter,
            destination,
            tracking: None,
        })
    }
    fn modal_items(
        &self,
        m: &Memory,
        p: &mut Process,
        dialog: u32,
    ) -> Result<(Vec<u8>, Vec<Item>)> {
        let record = self
            .records
            .get(&dialog)
            .filter(|r| !r.closed)
            .ok_or_else(|| bad("unknown modal dialog"))?;
        if !p.windows.contains(m, dialog)?
            || m.u16(GuestPtr(dialog + 108))? != 2
            || m.u32(GuestPtr(dialog + 156))? != record.items
            || m.u32(GuestPtr(dialog + 160))? != record.editor
        {
            return Err(bad("modal dialog ownership changed"));
        }
        let raw = payload(m, p, record.items)?;
        let list = items(&raw)?;
        Ok((raw, list))
    }
    pub(crate) fn modal_event(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        modal: &mut Modal,
        ticks: u32,
    ) -> Result<Option<i16>> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            let (raw, list) = self.modal_items(m, p, modal.dialog)?;
            let event = m.read(GuestPtr(modal.scratch), 16)?.to_vec();
            let what = u16::from_be_bytes(event[..2].try_into().unwrap());
            let message = u32::from_be_bytes(event[2..6].try_into().unwrap());
            let modifiers = u16::from_be_bytes(event[14..].try_into().unwrap());
            let editor = self.records[&modal.dialog].editor;
            if modal.filter == 0 && matches!(what, 3 | 5) && matches!(message as u8, 3 | 13) {
                let number = m.u16(GuestPtr(modal.dialog + 168))? as i16;
                if let Some(item) = number
                    .checked_sub(1)
                    .and_then(|n| usize::try_from(n).ok())
                    .and_then(|n| list.get(n))
                {
                    if item.kind == 4 {
                        let h = u32::from_be_bytes(
                            raw[item.offset..item.offset + 4].try_into().unwrap(),
                        );
                        let mut c = std::mem::take(&mut p.controls);
                        let r = c.call(
                            m,
                            p,
                            None,
                            ControlOp::Test(
                                h,
                                ((item.rect.left + item.rect.right) / 2) as i16,
                                ((item.rect.top + item.rect.bottom) / 2) as i16,
                            ),
                        );
                        p.controls = c;
                        if r? != 0 {
                            return Ok(Some(number));
                        }
                    }
                }
                return Ok(None);
            }
            if matches!(what, 3 | 5) && editor != 0 {
                if modifiers & 0x100 != 0 {
                    return Ok(None);
                }
                let current = m.u16(GuestPtr(modal.dialog + 164))? as i16;
                if message as u8 == 9 {
                    let fields: Vec<_> = list
                        .iter()
                        .enumerate()
                        .filter(|(_, i)| i.kind & 0x7f == 16)
                        .map(|(n, _)| n)
                        .collect();
                    if let Some(index) = fields.iter().position(|&n| n as i16 == current) {
                        let next = if modifiers & 0x200 != 0 {
                            (index + fields.len() - 1) % fields.len()
                        } else {
                            (index + 1) % fields.len()
                        };
                        self.select_text(
                            m,
                            p,
                            Some(g),
                            modal.dialog,
                            (fields[next] + 1) as i16,
                            (0, 32767),
                        )?;
                    }
                    return Ok(None);
                }
                if let Ok(index) = usize::try_from(current) {
                    if let Some(item) = list.get(index).filter(|i| i.kind & 0x7f == 16) {
                        let mut te = std::mem::take(&mut p.textedit);
                        let result = te.key(m, p, g, editor, message as u8, ticks);
                        p.textedit = te;
                        result?;
                        if item.kind & 0x80 == 0 {
                            return Ok(Some(current + 1));
                        }
                    }
                }
            } else if what == 1 {
                let ports = p
                    .ports
                    .ok_or_else(|| bad("modal input requires InitGraf"))?;
                let point = ports.point_coordinates_for_port(
                    m,
                    GuestPtr(modal.dialog),
                    event[10..14].try_into().unwrap(),
                    false,
                )?;
                let v = i32::from(i16::from_be_bytes(point[..2].try_into().unwrap()));
                let h = i32::from(i16::from_be_bytes(point[2..].try_into().unwrap()));
                if modal.outside_click(m, p)? {
                    if m.u32(GuestPtr(0xa9c))? != 0 {
                        return Err(bad("custom dialog alert sound callbacks are unsupported"));
                    }
                    return Ok(None);
                }
                let item = list.iter().enumerate().find(|(_, i)| {
                    h >= i.rect.left && h < i.rect.right && v >= i.rect.top && v < i.rect.bottom
                });
                if let Some((index, item)) = item {
                    if item.kind & 0x7f == 16 {
                        return Err(bad(
                            "modal editable-text mouse selection requires TextEdit click tracking",
                        ));
                    }
                    if item.kind & 0x80 == 0 {
                        if matches!(item.kind, 4..=6) {
                            let handle = u32::from_be_bytes(
                                raw[item.offset..item.offset + 4].try_into().unwrap(),
                            );
                            let mut c = std::mem::take(&mut p.controls);
                            let result = c.begin_tracking(
                                m,
                                p,
                                TrackControl {
                                    handle,
                                    point: [v as i16, h as i16],
                                    action: 0,
                                },
                            );
                            p.controls = c;
                            modal.tracking = result?.map(|tracking| (tracking, (index + 1) as i16));
                        } else {
                            return Ok(Some((index + 1) as i16));
                        }
                    }
                } else {
                    let r = crate::quickdraw::read_rect(m, GuestPtr(modal.dialog + 16))?;
                    if (h < r.left || h >= r.right || v < r.top || v >= r.bottom)
                        && m.u32(GuestPtr(0xa9c))? != 0
                    {
                        return Err(bad("custom dialog alert sound callbacks are unsupported"));
                    }
                }
            } else if what == 6 && message == modal.dialog {
                window(m, p, g, WindowOp::Begin, modal.dialog)?;
                let result = self.draw(m, p, Some(g), modal.dialog);
                let cleanup = window(m, p, g, WindowOp::End, modal.dialog);
                result?;
                cleanup?;
            } else if what == 8 && message == modal.dialog && editor != 0 {
                let mut te = std::mem::take(&mut p.textedit);
                let result = te.activate(m, p, g, editor, modifiers & 1 != 0, ticks);
                p.textedit = te;
                result?;
            } else if what == 0 && editor != 0 {
                let mut te = std::mem::take(&mut p.textedit);
                let result = te.idle(m, p, Some(g), editor, ticks);
                p.textedit = te;
                result?;
            }
            Ok(None)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
}
impl Modal {
    pub fn outside_click(&self, m: &Memory, p: &Process) -> Result<bool> {
        if m.u16(self.event())? != 1 {
            return Ok(false);
        }
        let point = p
            .ports
            .ok_or_else(|| bad("modal input requires InitGraf"))?
            .point_coordinates_for_port(
                m,
                GuestPtr(self.dialog),
                m.read(GuestPtr(self.scratch + 10), 4)?.try_into().unwrap(),
                false,
            )?;
        let v = i32::from(i16::from_be_bytes(point[..2].try_into().unwrap()));
        let h = i32::from(i16::from_be_bytes(point[2..].try_into().unwrap()));
        let r = crate::quickdraw::read_rect(m, GuestPtr(self.dialog + 16))?;
        Ok(h < r.left || h >= r.right || v < r.top || v >= r.bottom)
    }
    pub fn event(&self) -> GuestPtr {
        GuestPtr(self.scratch)
    }
    pub fn hit(&self) -> GuestPtr {
        GuestPtr(self.scratch + 16)
    }
    pub fn tracking(&self) -> bool {
        self.tracking.is_some()
    }
    pub fn poll_tracking(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
    ) -> Result<Option<i16>> {
        let Some((tracking, number)) = &self.tracking else {
            return Ok(None);
        };
        let release = events::prepare_remove(m, 1 << 2)?;
        let released = release.is_some() || !events::button(m)?;
        let point = if let Some(e) = &release {
            e.record[10..14].try_into().unwrap()
        } else {
            m.read(events::MOUSE, 4)?.try_into().unwrap()
        };
        let errors = (p.mem_error, p.res_error);
        let mut c = std::mem::take(&mut p.controls);
        let result = c.poll_tracking(m, p, Some(g), tracking, point, released);
        p.controls = c;
        (p.mem_error, p.res_error) = errors;
        if let Some(part) = result? {
            if let Some(e) = release {
                e.commit(m)?;
            }
            let hit = (part != 0).then_some(*number);
            self.tracking = None;
            return Ok(hit);
        }
        Ok(None)
    }
    pub fn finish(self, m: &mut Memory, p: &mut Process, hit: i16) -> Result<()> {
        m.validate_write(self.destination, 2)?;
        m.write(self.destination, &hit.to_be_bytes())?;
        p.dispose_pointer(self.scratch);
        Ok(())
    }
}
