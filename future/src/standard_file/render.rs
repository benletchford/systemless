use super::*;
use crate::{quickdraw::RectangleOp, text::Input};
impl Dialog {
    fn label(
        &self,
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        x: i16,
        y: i16,
        s: &[u8],
    ) -> Result<()> {
        let data = crate::text::font_bytes(m, p, 0, 12, 0)?;
        let font = crate::font::Strike::parse(&data)?;
        let limit = if x == 34 {
            214
        } else {
            WIDTH - i32::from(x) - 12
        };
        let mut n = s.len().min(255);
        while n > 0 && font.width(&s[..n])? > limit {
            n -= 1;
        }
        self.write_string(m, 64, &s[..n])?;
        let ports = p.ports.unwrap();
        crate::text::move_to(m, ports, x, y)?;
        crate::text::draw(m, p, ports, g, Input::Pascal(self.slot(64)))
    }
    fn shape(
        &self,
        m: &mut Memory,
        p: &Process,
        g: &Graphics,
        r: Rect,
        op: RectangleOp,
    ) -> Result<()> {
        rectangle(m, self.slot(0), r)?;
        p.ports.unwrap().draw_rectangle(m, g, self.slot(0), op)
    }
    pub(super) fn render(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        ticks: u32,
    ) -> Result<()> {
        let ports = p.ports.unwrap();
        ports.set(m, GuestPtr(self.window))?;
        self.shape(m, p, g, Rect::new(0, 0, WIDTH, HEIGHT), RectangleOp::Erase)?;
        self.label(
            m,
            p,
            g,
            15,
            18,
            if self.replacing {
                b"Replace the existing file?"
            } else {
                &self.prompt
            },
        )?;
        self.label(m, p, g, 15, 38, &self.directory.name)?;
        self.shape(m, p, g, Rect::new(13, 43, 252, 143), RectangleOp::Frame)?;
        let rows = self.directory.entries.clone();
        for (index, entry) in rows.iter().skip(self.page).take(ROWS).enumerate() {
            let y = 45 + index as i32 * 16;
            if entry.directory {
                self.shape(
                    m,
                    p,
                    g,
                    Rect::new(19, y + 3, 29, y + 12),
                    RectangleOp::Frame,
                )?;
            }
            self.label(m, p, g, 34, (y + 12) as i16, &entry.name)?;
            if self.selected == Some(entry.id) {
                self.shape(m, p, g, Rect::new(15, y, 250, y + 16), RectangleOp::Invert)?;
            }
        }
        if rows.is_empty() {
            self.label(m, p, g, 20, 60, b"No files")?;
        }
        self.label(m, p, g, 15, 166, b"Save as:")?;
        self.shape(m, p, g, Rect::new(13, 172, 252, 194), RectangleOp::Frame)?;
        self.label(m, p, g, 15, 215, &self.status)?;
        let enabled = [
            self.can_save(m)?,
            true,
            self.directory.id != 2 && !self.replacing,
            self.page > 0 && !self.replacing,
            self.page + ROWS < rows.len() && !self.replacing,
        ];
        let title = if self.replacing {
            b"Replace".as_slice()
        } else {
            b"Save".as_slice()
        };
        self.write_string(m, 32, title)?;
        controls(m, p, g, ControlOp::SetTitle(self.buttons[0], self.slot(32)))?;
        for (i, &on) in enabled.iter().enumerate() {
            let part = if !on {
                255
            } else if self.pressed == Some(Target::Button(i)) {
                10
            } else {
                0
            };
            controls(m, p, g, ControlOp::Hilite(self.buttons[i], part))?;
        }
        self.scroll_name(m, p, ticks)?;
        let mut te = std::mem::take(&mut p.textedit);
        let r = te.update(m, p, g, self.slot(8), self.editor);
        p.textedit = te;
        r
    }
    pub(super) fn render_name(
        &self,
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        ticks: u32,
    ) -> Result<()> {
        self.shape(m, p, g, Rect::new(13, 199, 255, HEIGHT), RectangleOp::Erase)?;
        self.label(m, p, g, 15, 215, &self.status)?;
        let value = if self.can_save(m)? { 0 } else { 255 };
        let data = m.u32(GuestPtr(self.buttons[0]))?;
        if m.read(GuestPtr(data + 17), 1)?[0] != value {
            controls(m, p, g, ControlOp::Hilite(self.buttons[0], value))?;
        }
        self.scroll_name(m, p, ticks)?;
        let mut te = std::mem::take(&mut p.textedit);
        let r = te.update(m, p, g, self.slot(8), self.editor);
        p.textedit = te;
        r
    }
    fn scroll_name(&self, m: &mut Memory, p: &mut Process, ticks: u32) -> Result<()> {
        let name = self.name(m)?;
        let record = m.u32(GuestPtr(self.editor))?;
        let caret = usize::from(m.u16(GuestPtr(record + 34))?).min(name.len());
        let data = crate::text::font_bytes(m, p, 0, 12, 0)?;
        let strike = crate::font::Strike::parse(&data)?;
        let width = strike.width(&name[..caret])?;
        let left = NAME_RECT.left - (width - (NAME_RECT.right - NAME_RECT.left - 2)).max(0);
        rectangle(
            m,
            GuestPtr(record),
            Rect::new(left, NAME_RECT.top, left + 1024, NAME_RECT.bottom),
        )?;
        text_call(m, p, TextOp::Calculate(self.editor), ticks)?;
        Ok(())
    }
}
