use super::*;
impl Dialog {
    fn local(&self, point: [i16; 2]) -> [i16; 2] {
        [
            point[0].wrapping_sub(self.origin[0]),
            point[1].wrapping_sub(self.origin[1]),
        ]
    }
    fn target(&self, m: &Memory, point: [i16; 2]) -> Result<Option<Target>> {
        let point = self.local(point);
        for (i, (r, _)) in BUTTONS.iter().enumerate() {
            if inside(*r, point) {
                let data = m.u32(GuestPtr(self.buttons[i]))?;
                return Ok((m.read(GuestPtr(data + 17), 1)?[0] != 255).then_some(Target::Button(i)));
            }
        }
        if self.replacing {
            return Ok(None);
        }
        if self.open.is_none() && inside(NAME_RECT, point) {
            return Ok(Some(Target::Name));
        }
        if inside(Rect::new(15, 45, 250, 141), point) {
            let index = self.page + ((i32::from(point[0]) - 45) / 16) as usize;
            return Ok(self.directory.entries.get(index).map(|e| Target::Row(e.id)));
        }
        Ok(None)
    }
    fn navigate(&mut self, p: &Process, directory: u32) -> Result<()> {
        self.directory = p
            .files
            .directory_view(directory)
            .map_err(|_| bad("Standard File directory disappeared"))?;
        if let Some(open) = &mut self.open {
            open.source = None;
            self.directory.entries.clear();
        }
        self.page = 0;
        self.selected = None;
        self.last_folder = None;
        self.status.clear();
        Ok(())
    }
    fn accept(&mut self, m: &Memory) -> Result<Option<bool>> {
        if !self.can_save(m)? {
            return Ok(None);
        }
        if self.open.is_some() {
            return Ok(Some(true));
        }
        let name = self.name(m)?;
        if !self.replacing
            && self
                .directory
                .entries
                .iter()
                .any(|e| e.name.eq_ignore_ascii_case(&name))
        {
            self.replacing = true;
            self.status = b"Choose Replace to overwrite this file.".to_vec();
            Ok(None)
        } else {
            Ok(Some(true))
        }
    }
    /// One OS input event at a time; no guest instructions run inside the dialog.
    pub(crate) fn poll(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        event: Option<[u8; 16]>,
        ticks: u32,
    ) -> Result<Option<bool>> {
        m.validate_write(self.request.reply, 74)?;
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            if !p.windows.contains(m, self.window)? {
                return Err(bad("Standard File dialog window disappeared"));
            }
            text_call(m, p, TextOp::GetText(self.editor), ticks)?;
            if self.open.is_none() {
                let fresh = p
                    .files
                    .directory_view(self.directory.id)
                    .map_err(|_| bad("Standard File directory disappeared"))?;
                if fresh != self.directory {
                    self.directory = fresh;
                    self.page = self
                        .page
                        .min(self.directory.entries.len().saturating_sub(1) / ROWS * ROWS);
                    self.render(m, p, g, ticks)?;
                }
            }
            let Some(event) = event else {
                if let Some(Target::Button(i)) = self.pressed {
                    let point = [p.input.vertical, p.input.horizontal];
                    let on = self.target(m, point)? == self.pressed;
                    let value = if on { 10 } else { 0 };
                    let data = m.u32(GuestPtr(self.buttons[i]))?;
                    if m.read(GuestPtr(data + 17), 1)?[0] != value {
                        controls(m, p, g, ControlOp::Hilite(self.buttons[i], value))?;
                    }
                }
                if self.open.is_none() && self.pressed.is_none() && !self.replacing {
                    let mut te = std::mem::take(&mut p.textedit);
                    let r = te.idle(m, p, Some(g), self.editor, ticks);
                    p.textedit = te;
                    r?;
                }
                return Ok(None);
            };
            let what = u16::from_be_bytes(event[..2].try_into().unwrap());
            let point = [
                i16::from_be_bytes(event[10..12].try_into().unwrap()),
                i16::from_be_bytes(event[12..14].try_into().unwrap()),
            ];
            match what {
                1 => {
                    self.pressed = self.target(m, point)?;
                    if let Some(Target::Button(i)) = self.pressed {
                        controls(m, p, g, ControlOp::Hilite(self.buttons[i], 10))?;
                    }
                }
                2 => {
                    let hit = self.target(m, point)?;
                    let pressed = self.pressed.take();
                    if hit == pressed {
                        match hit {
                            Some(Target::Button(0)) => {
                                if let Some(done) = self.accept(m)? {
                                    return Ok(Some(done));
                                }
                            }
                            Some(Target::Button(1)) => {
                                if self.replacing {
                                    self.replacing = false;
                                    self.status.clear();
                                } else {
                                    return Ok(Some(false));
                                }
                            }
                            Some(Target::Button(2)) => self.navigate(p, self.directory.parent)?,
                            Some(Target::Button(3)) => {
                                self.page = self.page.saturating_sub(ROWS);
                                self.selected = None;
                            }
                            Some(Target::Button(4)) => {
                                self.page += ROWS;
                                self.selected = None;
                            }
                            Some(Target::Row(id)) => {
                                let entry = self
                                    .directory
                                    .entries
                                    .iter()
                                    .find(|e| e.id == id)
                                    .unwrap()
                                    .clone();
                                self.selected = Some(id);
                                if entry.directory {
                                    if self.last_folder.is_some_and(|(last, when)| {
                                        last == id && ticks.wrapping_sub(when) <= 30
                                    }) {
                                        self.navigate(p, id)?;
                                    } else {
                                        self.last_folder = Some((id, ticks));
                                    }
                                } else if self.open.is_some() {
                                    if self.last_folder.is_some_and(|(last, when)| {
                                        last == id && ticks.wrapping_sub(when) <= 30
                                    }) {
                                        return Ok(Some(true));
                                    }
                                    self.last_folder = Some((id, ticks));
                                } else {
                                    self.set_name(m, p, &entry.name, ticks)?;
                                    self.last_folder = None;
                                }
                            }
                            Some(Target::Name) => {
                                self.selected = None;
                                let name = self.name(m)?;
                                let record = m.u32(GuestPtr(self.editor))?;
                                let left = crate::quickdraw::read_rect(m, GuestPtr(record))?.left;
                                let x = i32::from(self.local(point)[1]) - left;
                                let data = crate::text::font_bytes(m, p, 0, 12, 0)?;
                                let font = crate::font::Strike::parse(&data)?;
                                let caret = (0..=name.len())
                                    .min_by_key(|&n| {
                                        (font.width(&name[..n]).unwrap_or(i32::MAX / 2) - x).abs()
                                    })
                                    .unwrap_or(0)
                                    as u32;
                                text_call(
                                    m,
                                    p,
                                    TextOp::Select {
                                        start: caret,
                                        end: caret,
                                        handle: self.editor,
                                    },
                                    ticks,
                                )?;
                            }
                            _ => {}
                        }
                    }
                    self.render(m, p, g, ticks)?;
                }
                3 | 5 => {
                    let mut only_name = false;
                    let byte = event[5];
                    let modifiers = u16::from_be_bytes(event[14..].try_into().unwrap());
                    if byte == 27 || byte == b'.' && modifiers & 0x100 != 0 {
                        if self.replacing {
                            self.replacing = false;
                            self.status.clear();
                        } else {
                            return Ok(Some(false));
                        }
                    } else if modifiers & 0x100 != 0 {
                        if self.open.is_none() && matches!(byte, b'a' | b'A') && !self.replacing {
                            only_name = self.selected.is_none();
                            self.selected = None;
                            let length = self.name(m)?.len() as u32;
                            text_call(
                                m,
                                p,
                                TextOp::Select {
                                    handle: self.editor,
                                    start: 0,
                                    end: length,
                                },
                                ticks,
                            )?;
                        }
                    } else if matches!(byte, 3 | 13) {
                        if !self.replacing
                            && self
                                .selected
                                .and_then(|id| self.directory.entries.iter().find(|e| e.id == id))
                                .is_some_and(|e| e.directory)
                        {
                            self.navigate(p, self.selected.unwrap())?;
                        } else if let Some(done) = self.accept(m)? {
                            return Ok(Some(done));
                        }
                    } else if !self.replacing && matches!(byte, 0x1e | 0x1f) {
                        if !self.directory.entries.is_empty() {
                            let current = self.selected.and_then(|id| {
                                self.directory.entries.iter().position(|e| e.id == id)
                            });
                            let index = if byte == 0x1f {
                                current.map_or(0, |n| (n + 1).min(self.directory.entries.len() - 1))
                            } else {
                                current.map_or(0, |n| n.saturating_sub(1))
                            };
                            self.selected = Some(self.directory.entries[index].id);
                            self.page = index / ROWS * ROWS;
                        }
                    } else if self.open.is_none()
                        && !self.replacing
                        && (byte >= 32 && byte != 127 && byte != b':'
                            || matches!(byte, 8 | 127 | 0x1c | 0x1d))
                    {
                        let name = self.name(m)?;
                        let record = m.u32(GuestPtr(self.editor))?;
                        let a = usize::from(m.u16(GuestPtr(record + 32))?);
                        let b = usize::from(m.u16(GuestPtr(record + 34))?);
                        if matches!(byte, 8 | 127 | 0x1c | 0x1d)
                            || name.len().saturating_sub(a.abs_diff(b)) < 31
                        {
                            let mut te = std::mem::take(&mut p.textedit);
                            let r = te.key(m, p, g, self.editor, byte, ticks);
                            p.textedit = te;
                            r?;
                            only_name = self.selected.is_none();
                            self.selected = None;
                            self.last_folder = None;
                            self.status.clear();
                        }
                    }
                    if only_name {
                        self.render_name(m, p, g, ticks)?;
                    } else {
                        self.render(m, p, g, ticks)?;
                    }
                }
                _ => {}
            }
            Ok(None)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
}
