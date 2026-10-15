//! Resource-backed dialog drawing through shared TextEdit, controls and wgpu.
use super::*;
use crate::{
    graphics::{Draw, Transfer},
    textedit::TextBox,
};
enum Content {
    Text(Rect, Vec<u8>),
    Picture(Rect, u32),
    Icon(Rect, Vec<u8>),
}
fn expanded(bytes: &[u8], parameters: &[Vec<u8>; 4]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'^' && bytes.get(at + 1).is_some_and(|v| matches!(v, b'0'..=b'3')) {
            let p = &parameters[usize::from(bytes[at + 1] - b'0')];
            out.extend(&p[1..]);
            at += 2;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
        if out.len() > 32767 {
            return Err(bad("expanded dialog text exceeds TextEdit limit"));
        }
    }
    Ok(out)
}
impl Dialogs {
    pub fn draw(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: Option<&Graphics>,
        dialog: u32,
    ) -> Result<()> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
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
            if m.read(GuestPtr(dialog + 110), 1)?[0] == 0 {
                return Ok(());
            }
            let ports = p.ports.ok_or_else(|| bad("DrawDialog requires InitGraf"))?;
            let previous = m.u32(ports.current_slot)?;
            m.validate_write(ports.current_slot, 4)?;
            let saved = m.read(GuestPtr(dialog), 108)?.to_vec();
            m.validate_write(GuestPtr(dialog), 108)?;
            ports.set(m, GuestPtr(dialog))?;
            let target = ports.drawing_target(m);
            m.write(ports.current_slot, &previous.to_be_bytes())?;
            let (bitmap, clips) = target?;
            if clips.is_empty() {
                return Ok(());
            }
            let g = g.ok_or_else(|| bad("visible DrawDialog requires a GPU"))?;
            let parameters = self.check(m, p)?;
            let mut prepared = Vec::new();
            let mut capacity = 16usize;
            for item in list {
                let r = item.rect;
                if !clips.iter().any(|c| {
                    r.left < c.right && r.right > c.left && r.top < c.bottom && r.bottom > c.top
                }) {
                    continue;
                }
                let h = u32::from_be_bytes(data[item.offset..item.offset + 4].try_into().unwrap());
                match item.kind & 0x7f {
                    4..=6 => {}
                    8 => {
                        let text = expanded(&payload(m, p, h)?, &parameters)?;
                        capacity = capacity.max(16 + text.len());
                        prepared.push(Content::Text(r, text));
                    }
                    64 if h != 0 => {
                        let bytes = payload(m, p, h)?;
                        crate::picture::decode(&bytes)?;
                        prepared.push(Content::Picture(r, h));
                    }
                    32 if h != 0 => {
                        let bytes = payload(m, p, h)?;
                        if bytes.len() != 128 {
                            return Err(bad("ICON requires a 32 by 32 one-bit bitmap"));
                        }
                        let indices = (0..1024)
                            .map(|i| (bytes[i / 8] >> (7 - i % 8)) & 1)
                            .collect();
                        prepared.push(Content::Icon(r, indices));
                    }
                    32 | 64 => {}
                    0 if h == 0 => {}
                    0 => return Err(bad("dialog item draw callbacks are unsupported")),
                    _ => return Err(bad("unsupported dialog drawing item")),
                }
            }
            let scratch = p.new_pointer(m, capacity as u32, false)?;
            if scratch == 0 {
                return Err(bad("dialog drawing scratch allocation failed"));
            }
            let result = (|| {
                ports.set(m, GuestPtr(dialog))?;
                for item in prepared {
                    let r = match &item {
                        Content::Text(r, _) | Content::Picture(r, _) | Content::Icon(r, _) => *r,
                    };
                    m.write(GuestPtr(scratch), &rect_bytes(r)?)?;
                    match item {
                        Content::Text(_, bytes) => {
                            m.write(GuestPtr(scratch + 16), &bytes)?;
                            let mut te = std::mem::take(&mut p.textedit);
                            let result = te.text_box(
                                m,
                                p,
                                g,
                                TextBox {
                                    text: GuestPtr(scratch + 16),
                                    length: bytes.len() as i32,
                                    rectangle: GuestPtr(scratch),
                                    alignment: 0,
                                },
                                0,
                            );
                            p.textedit = te;
                            result?;
                        }
                        Content::Picture(_, h) => {
                            crate::picture::draw(m, p, ports, g, h, GuestPtr(scratch))?
                        }
                        Content::Icon(_, indices) => bitmap.execute_source(
                            g,
                            m,
                            Draw::Blit {
                                mask: None,
                                rect: r,
                                clip: bitmap.bounds,
                                source: Rect::new(0, 0, 32, 32),
                                width: 32,
                                height: 32,
                                foreground: 1,
                                background: 0,
                                invert: false,
                                mode: Transfer::Copy,
                            },
                            Some(&clips),
                            Some(&indices),
                        )?,
                    }
                }
                self.show_default(m, p, g, dialog)?;
                Ok(())
            })();
            m.write(GuestPtr(dialog), &saved)?;
            m.write(ports.current_slot, &previous.to_be_bytes())?;
            p.dispose_pointer(scratch);
            result
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
}
impl Dialogs {
    pub(super) fn redraw_text(
        &self,
        m: &mut Memory,
        p: &mut Process,
        g: Option<&Graphics>,
        dialog: u32,
        rect: Rect,
        bytes: &[u8],
    ) -> Result<()> {
        if m.read(GuestPtr(dialog + 110), 1)?[0] == 0 {
            return Ok(());
        }
        let ports = p
            .ports
            .ok_or_else(|| bad("dialog text requires InitGraf"))?;
        let previous = m.u32(ports.current_slot)?;
        m.validate_write(ports.current_slot, 4)?;
        let saved = m.read(GuestPtr(dialog), 108)?.to_vec();
        m.validate_write(GuestPtr(dialog), 108)?;
        ports.set(m, GuestPtr(dialog))?;
        let target = ports.drawing_target(m);
        m.write(ports.current_slot, &previous.to_be_bytes())?;
        let (_, clips) = target?;
        if !clips.iter().any(|c| {
            rect.left < c.right && rect.right > c.left && rect.top < c.bottom && rect.bottom > c.top
        }) {
            return Ok(());
        }
        let g = g.ok_or_else(|| bad("visible dialog text requires a GPU"))?;
        let text = expanded(bytes, &self.check(m, p)?)?;
        let scratch = p.new_pointer(m, 16 + text.len() as u32, false)?;
        if scratch == 0 {
            return Err(bad("dialog text scratch allocation failed"));
        }
        let result = (|| {
            m.write(GuestPtr(scratch), &rect_bytes(rect)?)?;
            m.write(GuestPtr(scratch + 16), &text)?;
            ports.set(m, GuestPtr(dialog))?;
            let mut te = std::mem::take(&mut p.textedit);
            let result = te.text_box(
                m,
                p,
                g,
                TextBox {
                    text: GuestPtr(scratch + 16),
                    length: text.len() as i32,
                    rectangle: GuestPtr(scratch),
                    alignment: 0,
                },
                0,
            );
            p.textedit = te;
            result
        })();
        let allocation_error = p.mem_error;
        m.write(GuestPtr(dialog), &saved)?;
        m.write(ports.current_slot, &previous.to_be_bytes())?;
        p.dispose_pointer(scratch);
        if result.is_err() {
            p.mem_error = allocation_error;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn substitutions_are_literal_nonrecursive_and_bounded() {
        let parameters = [
            b"\x02^1".to_vec(),
            b"\x01X".to_vec(),
            vec![0],
            b"\x01Z".to_vec(),
        ];
        assert_eq!(
            expanded(b"^0 ^1 ^2 ^3 ^4 ^^0", &parameters).unwrap(),
            b"^1 X  Z ^4 ^^1"
        );
        let huge = [b'^', b'0'].repeat(129);
        let parameters = std::array::from_fn(|_| {
            let mut s = vec![255];
            s.extend([b'A'; 255]);
            s
        });
        assert!(expanded(&huge, &parameters).is_err());
    }
}
