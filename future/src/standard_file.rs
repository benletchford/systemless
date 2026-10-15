//! Cooperative classic Standard File, backed by shared Window/Control/TextEdit/File state.
//! IM: Files, Standard File Package. Guest file filters run cooperatively in Runtime.
use crate::{
    controls::{NewControl, Op as ControlOp},
    files::DirectoryView,
    graphics::{Graphics, Rect},
    memory::{GuestPtr, Memory},
    process::Process,
    text::Field,
    textedit::Op as TextOp,
    windows::{NewWindow, Op as WindowOp},
    Error, Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PutFile {
    pub point: [i16; 2],
    pub prompt: GuestPtr,
    pub name: GuestPtr,
    pub hook: u32,
    pub reply: GuestPtr,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetFile {
    pub point: [i16; 2],
    pub filter: u32,
    pub count: i16,
    pub types: GuestPtr,
    pub hook: u32,
    pub reply: GuestPtr,
}
impl GetFile {
    pub(crate) fn validate(self, m: &Memory) -> Result<()> {
        m.validate_write(self.reply, 74)?;
        if self.hook != 0 {
            return Err(bad("SFGetFile dialog hooks require guest callbacks"));
        }
        if !(-1..=4).contains(&self.count) {
            return Err(bad("SFGetFile requires -1 or at most four file types"));
        }
        if self.count > 0 {
            m.read(self.types, self.count as usize * 4)?;
        }
        Ok(())
    }
}
mod input;
mod open;
mod render;
const WIDTH: i32 = 360;
const HEIGHT: i32 = 222;
const ROWS: usize = 6;
const SCRATCH: u32 = 768;
const NAME_RECT: Rect = Rect {
    left: 15,
    top: 174,
    right: 250,
    bottom: 192,
};
const BUTTONS: [(Rect, &[u8]); 5] = [
    (
        Rect {
            left: 270,
            top: 190,
            right: 345,
            bottom: 208,
        },
        b"Save",
    ),
    (
        Rect {
            left: 270,
            top: 160,
            right: 345,
            bottom: 178,
        },
        b"Cancel",
    ),
    (
        Rect {
            left: 270,
            top: 42,
            right: 345,
            bottom: 60,
        },
        b"Up",
    ),
    (
        Rect {
            left: 270,
            top: 72,
            right: 345,
            bottom: 90,
        },
        b"Previous",
    ),
    (
        Rect {
            left: 270,
            top: 102,
            right: 345,
            bottom: 120,
        },
        b"Next",
    ),
];
fn bad(s: &str) -> Error {
    Error::Graphics(s.into())
}
fn pascal(m: &Memory, p: GuestPtr) -> Result<Vec<u8>> {
    let n = usize::from(m.read(p, 1)?[0]);
    if n == 0 {
        return Ok(Vec::new());
    }
    let data =
        p.0.checked_add(1)
            .ok_or_else(|| bad("Standard File string address overflow"))?;
    Ok(m.read(GuestPtr(data), n)?.to_vec())
}
fn rectangle(m: &mut Memory, p: GuestPtr, r: Rect) -> Result<()> {
    let mut b = Vec::new();
    for v in [r.top, r.left, r.bottom, r.right] {
        b.extend(
            i16::try_from(v)
                .map_err(|_| bad("Standard File coordinate overflow"))?
                .to_be_bytes(),
        );
    }
    m.write(p, &b)
}
fn inside(r: Rect, p: [i16; 2]) -> bool {
    i32::from(p[1]) >= r.left
        && i32::from(p[1]) < r.right
        && i32::from(p[0]) >= r.top
        && i32::from(p[0]) < r.bottom
}
fn controls(m: &mut Memory, p: &mut Process, g: &Graphics, op: ControlOp) -> Result<u32> {
    let mut c = std::mem::take(&mut p.controls);
    let r = c.call(m, p, Some(g), op);
    p.controls = c;
    r
}
fn window(m: &mut Memory, p: &mut Process, g: &Graphics, op: WindowOp, w: u32) -> Result<u32> {
    let mut windows = std::mem::take(&mut p.windows);
    let r = windows.call(m, p, g, op, w);
    p.windows = windows;
    r
}
fn text_call(m: &mut Memory, p: &mut Process, op: TextOp, ticks: u32) -> Result<u32> {
    let mut te = std::mem::take(&mut p.textedit);
    let r = te.call(m, p, op, ticks);
    p.textedit = te;
    r
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Button(usize),
    Row(u32),
    Name,
}
pub(crate) struct Dialog {
    request: PutFile,
    open: Option<open::Open>,
    scratch: u32,
    window: u32,
    editor: u32,
    buttons: Vec<u32>,
    previous_port: u32,
    origin: [i16; 2],
    prompt: Vec<u8>,
    directory: DirectoryView,
    page: usize,
    selected: Option<u32>,
    last_folder: Option<(u32, u32)>,
    pressed: Option<Target>,
    replacing: bool,
    status: Vec<u8>,
}
impl Dialog {
    pub fn start(
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        request: PutFile,
        ticks: u32,
    ) -> Result<Self> {
        Self::start_impl(m, p, g, request, ticks, None)
    }
    fn start_impl(
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        request: PutFile,
        ticks: u32,
        open: Option<open::Open>,
    ) -> Result<Self> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            m.validate_write(request.reply, 74)?;
            if request.hook != 0 {
                return Err(bad("SFPutFile dialog hooks require guest callbacks"));
            }
            let prompt = pascal(m, request.prompt)?;
            let name = pascal(m, request.name)?;
            if name.len() > 31 || name.contains(&0) || name.contains(&b':') {
                return Err(bad(
                    "SFPutFile default name must be an HFS leaf name of at most 31 bytes",
                ));
            }
            if g.width < 376 || g.height < 258 {
                return Err(bad(
                    "Standard File requires at least a 376x258 guest screen",
                ));
            }
            let ports = p
                .ports
                .ok_or_else(|| bad("Standard File requires InitGraf"))?;
            p.windows.manager_port(m)?;
            crate::text::font_bytes(m, p, 0, 12, 0)?;
            let previous_port = m.u32(ports.current_slot)?;
            let menu = i32::from(m.u16(GuestPtr(0xbaa))? as i16).max(0);
            if menu + 8 > g.height as i32 - HEIGHT - 8 {
                return Err(bad("Standard File has no room below the guest menu bar"));
            }
            let left = i32::from(request.point[1]).clamp(8, g.width as i32 - WIDTH - 8);
            let top = i32::from(request.point[0]).clamp(menu + 8, g.height as i32 - HEIGHT - 8);
            let mut directory = p
                .files
                .directory_view(2)
                .map_err(|_| bad("Standard File application volume is unavailable"))?;
            if open.is_some() {
                directory.entries.retain(|e| e.directory);
            }
            // All caller data is snapshotted before guest allocations can reuse it.
            let scratch = p.new_pointer(m, SCRATCH, true)?;
            if scratch == 0 {
                return Err(bad("Standard File scratch allocation failed"));
            }
            let mut dialog = Self {
                request,
                open,
                scratch,
                window: 0,
                editor: 0,
                buttons: Vec::new(),
                previous_port,
                origin: [top as i16, left as i16],
                prompt,
                directory,
                page: 0,
                selected: None,
                last_folder: None,
                pressed: None,
                replacing: false,
                status: Vec::new(),
            };
            let result = dialog.construct(m, p, g, &name, ticks);
            if let Err(error) = result {
                let _ = dialog.close(m, p, g);
                return Err(error);
            }
            Ok(dialog)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
    fn slot(&self, n: u32) -> GuestPtr {
        GuestPtr(self.scratch + n)
    }
    fn construct(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        name: &[u8],
        ticks: u32,
    ) -> Result<()> {
        rectangle(
            m,
            self.slot(0),
            Rect::new(
                i32::from(self.origin[1]),
                i32::from(self.origin[0]),
                i32::from(self.origin[1]) + WIDTH,
                i32::from(self.origin[0]) + HEIGHT,
            ),
        )?;
        m.write(self.slot(32), &[0])?;
        let mut windows = std::mem::take(&mut p.windows);
        let result = windows.create(
            m,
            p,
            g,
            NewWindow {
                storage: GuestPtr(0),
                bounds: self.slot(0),
                title: self.slot(32),
                visible: true,
                proc_id: 1,
                behind: u32::MAX,
                go_away: false,
                ref_con: 0,
            },
        );
        p.windows = windows;
        self.window = result?;
        if self.window == 0 {
            return Err(bad("Standard File window allocation failed"));
        }
        m.write(GuestPtr(self.window + 108), &2u16.to_be_bytes())?;
        let ports = p.ports.unwrap();
        ports.set(m, GuestPtr(self.window))?;
        crate::text::set(m, ports, Field::Font, 0)?;
        crate::text::set(m, ports, Field::Size, 12)?;
        for (r, title) in BUTTONS {
            rectangle(m, self.slot(0), r)?;
            self.write_string(m, 32, title)?;
            let mut c = std::mem::take(&mut p.controls);
            let result = c.create(
                m,
                p,
                Some(g),
                NewControl {
                    owner: GuestPtr(self.window),
                    bounds: self.slot(0),
                    title: self.slot(32),
                    visible: true,
                    value: 0,
                    min: 0,
                    max: 1,
                    proc_id: 0,
                    ref_con: 0,
                },
            );
            p.controls = c;
            let h = result?;
            if h == 0 {
                return Err(bad("Standard File control allocation failed"));
            }
            self.buttons.push(h);
        }
        rectangle(m, self.slot(8), NAME_RECT)?;
        rectangle(
            m,
            self.slot(16),
            Rect::new(
                NAME_RECT.left,
                NAME_RECT.top,
                NAME_RECT.left + 1024,
                NAME_RECT.bottom,
            ),
        )?;
        self.editor = text_call(
            m,
            p,
            TextOp::New {
                dest: self.slot(16),
                view: self.slot(8),
                styled: false,
            },
            ticks,
        )?;
        if self.editor == 0 {
            return Err(bad("Standard File filename field allocation failed"));
        }
        self.set_name(m, p, name, ticks)?;
        if self.open.is_some() {
            return self.render(m, p, g, ticks);
        }
        // Keep the actual TERecord active so selection and caret use shared drawing.
        let mut te = std::mem::take(&mut p.textedit);
        let r = te.activate(m, p, g, self.editor, true, ticks);
        p.textedit = te;
        r?;
        self.render(m, p, g, ticks)
    }
    fn write_string(&self, m: &mut Memory, offset: u32, s: &[u8]) -> Result<()> {
        if s.len() > 255 {
            return Err(bad("Standard File label exceeds Str255"));
        }
        let mut bytes = vec![s.len() as u8];
        bytes.extend(s);
        m.write(self.slot(offset), &bytes)
    }
    fn name(&self, m: &Memory) -> Result<Vec<u8>> {
        let record = m.u32(GuestPtr(self.editor))?;
        let length = usize::from(m.u16(GuestPtr(record + 60))?);
        if length > 31 {
            return Err(bad("Standard File filename field exceeds HFS name size"));
        }
        let text = m.u32(GuestPtr(record + 62))?;
        let text = m.u32(GuestPtr(text))?;
        Ok(m.read(GuestPtr(text), length)?.to_vec())
    }
    fn set_name(&mut self, m: &mut Memory, p: &mut Process, name: &[u8], ticks: u32) -> Result<()> {
        m.write(self.slot(384), name)?;
        text_call(
            m,
            p,
            TextOp::SetText {
                source: self.slot(384),
                length: name.len() as u32,
                handle: self.editor,
            },
            ticks,
        )?;
        text_call(
            m,
            p,
            TextOp::Select {
                handle: self.editor,
                start: 0,
                end: name.len() as u32,
            },
            ticks,
        )?;
        Ok(())
    }
    fn can_save(&self, m: &Memory) -> Result<bool> {
        if self.open.is_some() {
            return Ok(self
                .selected
                .and_then(|id| self.directory.entries.iter().find(|e| e.id == id))
                .is_some_and(|e| !e.directory));
        }
        let name = self.name(m)?;
        if name.is_empty() || name.contains(&b':') || name.contains(&0) {
            return Ok(false);
        }
        Ok(!self
            .directory
            .entries
            .iter()
            .any(|e| e.name.eq_ignore_ascii_case(&name) && (e.directory || e.locked)))
    }
    pub fn finish(
        &mut self,
        m: &mut Memory,
        p: &mut Process,
        g: &Graphics,
        good: bool,
    ) -> Result<bool> {
        m.validate_write(self.request.reply, 74)?;
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            let selected = self
                .selected
                .and_then(|id| self.directory.entries.iter().find(|e| e.id == id));
            let name = if self.open.is_some() && good {
                selected
                    .filter(|e| !e.directory)
                    .ok_or_else(|| bad("Standard File has no selected file"))?
                    .name
                    .clone()
            } else {
                self.name(m)?
            };
            let kind = if self.open.is_some() && good {
                p.files
                    .file_type(selected.unwrap().id)
                    .ok_or_else(|| bad("Standard File selected file disappeared"))?
            } else {
                [0; 4]
            };
            let reference = if good {
                match p.files.open_working_directory(-1, self.directory.id, 0) {
                    Ok(reference) => Some(reference),
                    Err(-121) => {
                        self.status = b"Too many open folders. Choose Cancel.".to_vec();
                        self.render(m, p, g, m.u32(GuestPtr(0x16a))?)?;
                        return Ok(false);
                    }
                    Err(_) => return Err(bad("Standard File selected directory is unavailable")),
                }
            } else {
                None
            };
            self.close(m, p, g)?;
            if let Some(reference) = reference {
                let mut reply = [0; 74];
                reply[0] = 1;
                reply[2..6].copy_from_slice(&kind);
                reply[6..8].copy_from_slice(&reference.to_be_bytes());
                reply[10] = name.len() as u8;
                reply[11..11 + name.len()].copy_from_slice(&name);
                m.write(self.request.reply, &reply)?;
            } else {
                m.write(self.request.reply, &[0])?;
            }
            Ok(true)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
    fn close(&mut self, m: &mut Memory, p: &mut Process, g: &Graphics) -> Result<()> {
        if self.editor != 0 {
            text_call(m, p, TextOp::Dispose(self.editor), 0)?;
            self.editor = 0;
        }
        if self.window != 0 {
            window(m, p, g, WindowOp::Dispose, self.window)?;
            self.window = 0;
        }
        if self.scratch != 0 {
            p.dispose_pointer(self.scratch);
            self.scratch = 0;
        }
        if let Some(ports) = p.ports {
            ports.set(m, GuestPtr(self.previous_port))?;
        }
        Ok(())
    }
}
