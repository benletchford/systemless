//! Guest WindowRecords and standard plain-dialog/no-grow document windows.
//! Layout and ordering compared with legacy trap/window.rs.
use crate::{
    bitmap::Bitmap,
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory},
    process::Process,
    quickdraw::read_rect,
    region_geometry::{self, Boolean},
    Error, Result,
};
use std::collections::BTreeMap;
pub const LIST: GuestPtr = GuestPtr(0x9d6);
const SIZE: usize = 156;
struct Paint {
    rgb: Option<([u16; 3], [u16; 3])>,
    pattern: [u8; 8],
    foreground: u32,
    background: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewWindow {
    pub storage: GuestPtr,
    pub bounds: GuestPtr,
    pub title: GuestPtr,
    pub visible: bool,
    pub proc_id: i16,
    pub behind: u32,
    pub go_away: bool,
    pub ref_con: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceWindow {
    pub id: i16,
    pub storage: GuestPtr,
    pub behind: u32,
    pub color: bool,
}
#[derive(Clone, Copy)]
struct WindowColors([[u16; 3]; 5]);
impl Default for WindowColors {
    fn default() -> Self {
        // Host standard window policy, matching existing NewCWindow drawing.
        Self([[65535; 3], [0; 3], [0; 3], [0; 3], [65535; 3]])
    }
}
impl WindowColors {
    fn read(memory: &Memory, process: &mut Process, handle: u32) -> Result<Self> {
        let size = process.handle_size(memory, handle)? as usize;
        let data = GuestPtr(memory.u32(GuestPtr(handle))?);
        let bytes = memory.read(data, size)?;
        if size < 8 || bytes[4..6] != [0, 0] {
            return Err(fail("invalid window color table header"));
        }
        let count = usize::from(u16::from_be_bytes(bytes[6..8].try_into().unwrap())) + 1;
        if count > 5 || size < 8 + count * 8 {
            return Err(fail("invalid window color table extent"));
        }
        let mut colors = Self::default();
        let mut seen = [false; 5];
        for entry in bytes[8..8 + count * 8].as_chunks::<8>().0 {
            let part = usize::from(u16::from_be_bytes([entry[0], entry[1]]));
            if part >= 5 || seen[part] {
                return Err(fail("unsupported or duplicate window color part"));
            }
            seen[part] = true;
            colors.0[part] =
                std::array::from_fn(|i| u16::from_be_bytes([entry[2 + i * 2], entry[3 + i * 2]]));
        }
        Ok(colors)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Init,
    Front,
    Close,
    Dispose,
    Begin,
    End,
    Show,
    Hide,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Window(u32),
    Behind { start: u32, clobbered: u32 },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Size {
        width: i16,
        height: i16,
        update: bool,
    },
    Visibility(bool),
    Move {
        horizontal: i16,
        vertical: i16,
        front: bool,
    },
    Select,
}
struct Shape {
    window: u32,
    content: Vec<u8>,
    structure: Vec<u8>,
    visible: bool,
    local: Rect,
    previous_vis: Vec<u8>,
    next_vis: Vec<u8>,
    pending: Vec<u8>,
}
#[derive(Default)]
pub struct Windows {
    manager: u32,
    gray: u32,
    records: BTreeMap<u32, bool>,
    definitions: BTreeMap<u32, i16>,
    colors: BTreeMap<u32, WindowColors>,
    update: Option<(u32, u32, u32)>,
}
fn fail(message: &str) -> Error {
    Error::Graphics(message.into())
}
fn rect_bytes(r: Rect) -> Result<Vec<u8>> {
    let mut bytes = 10u16.to_be_bytes().to_vec();
    let words = if r.left < r.right && r.top < r.bottom {
        [r.top, r.left, r.bottom, r.right]
    } else {
        [0; 4]
    };
    for word in words {
        bytes.extend(
            i16::try_from(word)
                .map_err(|_| fail("window coordinate overflow"))?
                .to_be_bytes(),
        );
    }
    Ok(bytes)
}
fn region(memory: &Memory, process: &mut Process, handle: u32) -> Result<Vec<u8>> {
    let size = process.handle_size(memory, handle)?;
    if size < 10 {
        return Err(fail("window region is not owned"));
    }
    let data = GuestPtr(memory.u32(GuestPtr(handle))?);
    let n = usize::from(memory.u16(data)?);
    if n > size as usize {
        return Err(fail("window region exceeds allocation"));
    }
    let bytes = memory.read(data, n)?.to_vec();
    region_geometry::decode(&bytes)?;
    Ok(bytes)
}
fn put_region(memory: &mut Memory, process: &mut Process, handle: u32, bytes: &[u8]) -> Result<()> {
    process.set_handle_size(memory, handle, bytes.len() as u32)?;
    if process.mem_error != 0 {
        return Err(fail("window region allocation failed"));
    }
    let data = GuestPtr(memory.u32(GuestPtr(handle))?);
    memory.write(data, bytes)
}
fn handle(memory: &Memory, window: u32, offset: u32) -> Result<u32> {
    memory.u32(GuestPtr(window + offset))
}
fn shifted(bytes: &[u8], dx: i32, dy: i32) -> Result<Vec<u8>> {
    let mut rects = region_geometry::decode(bytes)?;
    for r in &mut rects {
        r.left += dx;
        r.right += dx;
        r.top += dy;
        r.bottom += dy;
    }
    region_geometry::encode(&rects)
}
fn close_box(r: Rect) -> Rect {
    Rect::new(r.left + 8, r.top - 15, r.left + 19, r.top - 4)
}
fn region_bounds(bytes: &[u8]) -> Rect {
    let edges: [i32; 4] = std::array::from_fn(|i| {
        i32::from(i16::from_be_bytes([bytes[2 + i * 2], bytes[3 + i * 2]]))
    });
    Rect::new(edges[1], edges[0], edges[3], edges[2])
}
impl Windows {
    /// Find a global point without selecting, drawing or changing window state.
    pub fn find(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        point: [i16; 2],
        destination: GuestPtr,
    ) -> Result<i16> {
        memory.validate_write(destination, 4)?;
        let previous_error = process.mem_error;
        let result = self.find_point(memory, process, point);
        process.mem_error = previous_error;
        let (part, window) = result?;
        memory.write(destination, &window.to_be_bytes())?;
        Ok(part)
    }
    fn find_point(
        &self,
        memory: &Memory,
        process: &mut Process,
        point: [i16; 2],
    ) -> Result<(i16, u32)> {
        if self.manager == 0 {
            return Err(fail("FindWindow requires InitWindows"));
        }
        let ports = process
            .ports
            .ok_or_else(|| fail("FindWindow requires InitGraf"))?;
        let screen = Bitmap::read(memory, ports.screen_bitmap)?.bounds;
        let height = i32::from(memory.u16(GuestPtr(0xbaa))? as i16);
        if height < 0 {
            return Err(fail("negative menu-bar height"));
        }
        let inside = |r: Rect| {
            i32::from(point[0]) >= r.top
                && i32::from(point[0]) < r.bottom
                && i32::from(point[1]) >= r.left
                && i32::from(point[1]) < r.right
        };
        let mut hit = (0i16, 0u32);
        if inside(screen) {
            if height > 0 && i32::from(point[0]) < screen.top + height {
                hit = (1, 0);
            } else {
                for w in self.list(memory)? {
                    if memory.read(GuestPtr(w + 110), 1)?[0] == 0 {
                        continue;
                    }
                    let structure = region(memory, process, handle(memory, w, 114)?)?;
                    if !region_geometry::decode(&structure)?.into_iter().any(inside) {
                        continue;
                    }
                    if memory.u32(GuestPtr(w + 126))? != 0 {
                        return Err(fail("custom FindWindow WDEF callbacks are unsupported"));
                    }
                    let definition = *self
                        .definitions
                        .get(&w)
                        .ok_or_else(|| fail("unknown window definition"))?;
                    if !matches!(definition, 1 | 2 | 4) {
                        return Err(fail("unsupported FindWindow definition"));
                    }
                    if (memory.u16(GuestPtr(w + 108))? as i16) < 0 {
                        hit = (2, w);
                        break;
                    }
                    let content = region(memory, process, handle(memory, w, 118)?)?;
                    if region_geometry::decode(&content)?.into_iter().any(inside) {
                        hit = (3, w);
                        break;
                    }
                    if definition == 4 {
                        let r = region_bounds(&content);
                        let title = Rect::new(
                            r.left - 1,
                            (r.top - 19).max(screen.top + height),
                            r.right + 1,
                            r.top,
                        );
                        if inside(title) {
                            let close = memory.read(GuestPtr(w + 111), 2)?;
                            hit = if close[0] != 0 && close[1] != 0 && inside(close_box(r)) {
                                (6, w)
                            } else {
                                (4, w)
                            };
                        }
                    }
                    // Even a noninteractive frame occludes windows behind it.
                    break;
                }
            }
        }
        Ok(hit)
    }
    /// Documented System 7 IsDialogEvent policy: any event belongs to the
    /// front dialog, including null and updates for other windows. The caller
    /// must filter events DialogSelect cannot handle (Toolbox Essentials 6-138).
    pub fn is_dialog_event(&self, memory: &Memory, event: GuestPtr) -> Result<bool> {
        memory.read(event, 16)?;
        Ok(self.front(memory)?.is_some_and(|w| w.1 == 2))
    }
    fn front(&self, memory: &Memory) -> Result<Option<(u32, i16)>> {
        for w in self.list(memory)? {
            if memory.read(GuestPtr(w + 110), 1)?[0] != 0 {
                return Ok(Some((w, memory.u16(GuestPtr(w + 108))? as i16)));
            }
        }
        Ok(None)
    }
    pub fn contains(&self, memory: &Memory, w: u32) -> Result<bool> {
        Ok(self.list(memory)?.contains(&w))
    }
    fn list(&self, memory: &Memory) -> Result<Vec<u32>> {
        let mut result = Vec::new();
        let mut next = memory.u32(LIST)?;
        while next != 0 {
            if result.len() >= 64 || !self.records.contains_key(&next) || result.contains(&next) {
                return Err(fail("corrupt WindowList"));
            }
            memory.read(GuestPtr(next), SIZE)?;
            result.push(next);
            next = handle(memory, next, 144)?;
        }
        Ok(result)
    }
    fn relink(&self, memory: &mut Memory, list: &[u32]) -> Result<()> {
        memory.validate_write(LIST, 4)?;
        for &w in list {
            memory.validate_write(GuestPtr(w + 144), 4)?;
        }
        memory.write(LIST, &list.first().copied().unwrap_or(0).to_be_bytes())?;
        for (i, &w) in list.iter().enumerate() {
            memory.write(
                GuestPtr(w + 144),
                &list.get(i + 1).copied().unwrap_or(0).to_be_bytes(),
            )?;
        }
        Ok(())
    }
    fn active(&self, memory: &Memory) -> Result<u32> {
        for w in self.list(memory)? {
            if memory.read(GuestPtr(w + 111), 1)?[0] != 0 {
                return Ok(w);
            }
        }
        Ok(0)
    }
    fn set_active(&self, memory: &mut Memory, active: u32) -> Result<()> {
        let list = self.list(memory)?;
        for &w in &list {
            memory.validate_write(GuestPtr(w + 111), 1)?;
        }
        for w in list {
            memory.write(GuestPtr(w + 111), &[u8::from(w == active)])?;
        }
        Ok(())
    }
    fn activation_change(memory: &mut Memory, old: u32, new: u32) -> Result<()> {
        if old == new {
            return Ok(());
        }
        memory.validate_write(GuestPtr(0xa64), 8)?;
        if old != 0 {
            memory.write(GuestPtr(0xa68), &old.to_be_bytes())?;
            if memory.u32(GuestPtr(0xa64))? == old {
                memory.write(GuestPtr(0xa64), &0u32.to_be_bytes())?;
            }
        }
        if new != 0 {
            memory.write(GuestPtr(0xa64), &new.to_be_bytes())?;
        }
        Ok(())
    }
    pub(crate) fn activation(memory: &Memory) -> Result<Option<(GuestPtr, u32, bool)>> {
        for (slot, active) in [(GuestPtr(0xa68), false), (GuestPtr(0xa64), true)] {
            let window = memory.u32(slot)?;
            if window != 0 {
                return Ok(Some((slot, window, active)));
            }
        }
        Ok(None)
    }
    /// Return the existing monochrome manager port without allocating or changing
    /// the current port. InitWindows owns its lifetime.
    pub fn manager_port(&self, memory: &Memory) -> Result<u32> {
        if self.manager == 0 {
            return Err(fail("GetWMgrPort requires InitWindows"));
        }
        memory.read(GuestPtr(self.manager), 108)?;
        Ok(self.manager)
    }
    pub fn init(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
    ) -> Result<()> {
        if self.manager != 0 {
            return Ok(());
        }
        let ports = process
            .ports
            .ok_or_else(|| fail("InitWindows requires InitGraf"))?;
        let old = memory.u32(ports.current_slot)?;
        let manager = process.new_pointer(memory, 108, true)?;
        if manager == 0 {
            return Err(fail("Window Manager port allocation failed"));
        }
        if let Err(error) = ports.open(memory, process, GuestPtr(manager)) {
            process.dispose_pointer(manager);
            return Err(error);
        }
        let gray = process.new_handle(memory, 10, true)?;
        if gray == 0 {
            ports.close(memory, process, GuestPtr(manager))?;
            process.dispose_pointer(manager);
            memory.write(ports.current_slot, &old.to_be_bytes())?;
            return Err(fail("desktop region allocation failed"));
        }
        let mut desktop = Bitmap::read(memory, ports.screen_bitmap)?.bounds;
        desktop.top = desktop
            .top
            .max(i32::from(memory.u16(GuestPtr(0xbaa))? as i16));
        put_region(memory, process, gray, &rect_bytes(desktop)?)?;
        self.manager = manager;
        self.gray = gray;
        memory.write(GuestPtr(0x9de), &manager.to_be_bytes())?;
        memory.write(GuestPtr(0x9ee), &gray.to_be_bytes())?;
        memory.write(LIST, &0u32.to_be_bytes())?;
        memory.write(GuestPtr(0xa64), &[0; 8])?;
        memory.write(ports.current_slot, &old.to_be_bytes())?;
        self.desktop(memory, process, gpu, &rect_bytes(desktop)?)
    }
    fn paint(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
        coverage: &[u8],
        style: Paint,
    ) -> Result<()> {
        let ports = process
            .ports
            .ok_or_else(|| fail("window drawing requires InitGraf"))?;
        let old = memory.u32(ports.current_slot)?;
        let saved = memory.read(GuestPtr(self.manager), 108)?.to_vec();
        ports.set(memory, GuestPtr(self.manager))?;
        let result = (|| {
            memory.write(GuestPtr(self.manager + 80), &style.foreground.to_be_bytes())?;
            memory.write(GuestPtr(self.manager + 84), &style.background.to_be_bytes())?;
            memory.write(GuestPtr(self.manager + 56), &8u16.to_be_bytes())?;
            memory.write(GuestPtr(self.manager + 58), &style.pattern)?;
            let bounds = Bitmap::read(memory, ports.screen_bitmap)?.bounds;
            let coverage = region_geometry::decode(coverage)?;
            if let Some(rgb) = style.rgb {
                ports.paint_rgb_coverage(gpu, memory, bounds, &coverage, rgb)
            } else {
                ports.paint_coverage(gpu, memory, bounds, Some(&coverage))
            }
        })();
        memory.write(GuestPtr(self.manager), &saved)?;
        memory.write(ports.current_slot, &old.to_be_bytes())?;
        result
    }
    fn desktop(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
        coverage: &[u8],
    ) -> Result<()> {
        self.paint(
            memory,
            process,
            gpu,
            coverage,
            Paint {
                rgb: None,
                pattern: [0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55],
                foreground: 33,
                background: 30,
            },
        )
    }
    fn union_rectangles(rectangles: &[Rect]) -> Result<Vec<u8>> {
        rectangles
            .iter()
            .try_fold(rect_bytes(Rect::new(0, 0, 0, 0))?, |region, &r| {
                region_geometry::combine(&region, &rect_bytes(r)?, Boolean::Union)
            })
    }
    fn refresh_chrome(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
        except: u32,
    ) -> Result<()> {
        let list = self.list(memory)?;
        let desktop = region(memory, process, self.gray)?;
        let mut covered = rect_bytes(Rect::new(0, 0, 0, 0))?;
        for w in list {
            if memory.read(GuestPtr(w + 110), 1)?[0] == 0 {
                continue;
            }
            let structure = region(memory, process, handle(memory, w, 114)?)?;
            if w != except && self.definitions.get(&w) == Some(&4) {
                let dirty = region_geometry::combine(&structure, &desktop, Boolean::Intersection)?;
                let dirty = region_geometry::combine(&dirty, &covered, Boolean::Difference)?;
                self.chrome(memory, process, gpu, w, &dirty)?;
            }
            covered = region_geometry::combine(&covered, &structure, Boolean::Union)?;
        }
        Ok(())
    }
    fn modal_border(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
        w: u32,
        dirty: &[u8],
    ) -> Result<()> {
        let content = region(memory, process, handle(memory, w, 118)?)?;
        let r = read_rect(
            memory,
            GuestPtr(memory.u32(GuestPtr(handle(memory, w, 118)?))? + 2),
        )?;
        let outer = region_geometry::combine(
            &rect_bytes(Rect::new(r.left - 8, r.top - 8, r.right + 8, r.bottom + 8))?,
            &rect_bytes(Rect::new(r.left - 7, r.top - 7, r.right + 7, r.bottom + 7))?,
            Boolean::Difference,
        )?;
        let inner = region_geometry::combine(
            &rect_bytes(Rect::new(r.left - 5, r.top - 5, r.right + 5, r.bottom + 5))?,
            &rect_bytes(Rect::new(r.left - 3, r.top - 3, r.right + 3, r.bottom + 3))?,
            Boolean::Difference,
        )?;
        let ink = region_geometry::combine(&outer, &inner, Boolean::Union)?;
        let ink = region_geometry::combine(&ink, &content, Boolean::Difference)?;
        let ink = region_geometry::combine(&ink, dirty, Boolean::Intersection)?;
        self.paint(
            memory,
            process,
            gpu,
            &ink,
            Paint {
                rgb: self.colors.get(&w).map(|c| (c.0[1], c.0[0])),
                pattern: [255; 8],
                foreground: 33,
                background: 30,
            },
        )
    }
    fn chrome(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
        w: u32,
        dirty: &[u8],
    ) -> Result<()> {
        let r = read_rect(
            memory,
            GuestPtr(memory.u32(GuestPtr(handle(memory, w, 118)?))? + 2),
        )?;
        let menu = i32::from(memory.u16(GuestPtr(0xbaa))? as i16);
        let top = (r.top - 19).max(menu);
        let bottom = r.top - 1;
        let band = Rect::new(r.left - 1, top, r.right + 1, bottom + 1);
        let coverage = region_geometry::combine(&rect_bytes(band)?, dirty, Boolean::Intersection)?;
        self.paint(
            memory,
            process,
            gpu,
            &coverage,
            Paint {
                rgb: None,
                pattern: [0; 8],
                foreground: 33,
                background: 30,
            },
        )?;
        let title_handle = handle(memory, w, 134)?;
        let size = process.handle_size(memory, title_handle)?;
        let address = GuestPtr(memory.u32(GuestPtr(title_handle))?);
        let length = usize::from(memory.read(address, 1)?[0]);
        if length + 1 > size as usize {
            return Err(fail("invalid window title handle"));
        }
        let title = memory.read(GuestPtr(address.0 + 1), length)?.to_vec();
        let font_data = crate::text::font_bytes(memory, process, 0, 12, 0)?;
        let font = crate::font::Strike::parse(&font_data)?;
        let width = font.width(&title)?;
        let x = r.left - 1 + (r.right - r.left + 2 - width) / 2;
        let baseline = top
            + 1
            + (bottom - top - 1 - i32::from(font.metrics.ascent + font.metrics.descent)) / 2
            + i32::from(font.metrics.ascent);
        let active = memory.read(GuestPtr(w + 111), 1)?[0] != 0;
        let close = active && memory.read(GuestPtr(w + 112), 1)?[0] != 0;
        let mut ink = vec![
            Rect::new(r.left - 1, top, r.right + 1, top + 1),
            Rect::new(r.left - 1, bottom, r.right + 1, bottom + 1),
            Rect::new(r.left - 1, top, r.left, bottom + 1),
            Rect::new(r.right, top, r.right + 1, bottom + 1),
        ];
        if close {
            let hit = close_box(r);
            let cx = hit.left;
            let cy = hit.top;
            ink.extend([
                Rect::new(cx, cy, cx + 11, cy + 1),
                Rect::new(cx, cy, cx + 1, cy + 11),
                Rect::new(cx + 9, cy + 2, cx + 10, cy + 10),
                Rect::new(cx + 2, cy + 9, cx + 10, cy + 10),
            ]);
        }
        if active {
            let clear_left = if title.is_empty() { r.right - 1 } else { x - 6 };
            let clear_right = if title.is_empty() {
                r.right - 1
            } else {
                x + width + 6
            };
            for y in (top + 2..=bottom - 3).step_by(2) {
                let first_end = if close { r.left + 7 } else { clear_left };
                if r.left + 1 < first_end {
                    ink.push(Rect::new(r.left + 1, y, first_end, y + 1));
                }
                if close && r.left + 20 < clear_left {
                    ink.push(Rect::new(r.left + 20, y, clear_left, y + 1));
                }
                if clear_right < r.right - 1 {
                    ink.push(Rect::new(clear_right, y, r.right - 1, y + 1));
                }
            }
        }
        ink.extend([
            Rect::new(r.left - 1, r.top, r.left, r.bottom),
            Rect::new(r.right, r.top, r.right + 1, r.bottom),
            Rect::new(r.left - 1, r.bottom, r.right + 1, r.bottom + 1),
            Rect::new(r.right + 1, top, r.right + 2, r.bottom + 2),
            Rect::new(r.left, r.bottom + 1, r.right + 2, r.bottom + 2),
        ]);
        let ink =
            region_geometry::combine(&Self::union_rectangles(&ink)?, dirty, Boolean::Intersection)?;
        self.paint(
            memory,
            process,
            gpu,
            &ink,
            Paint {
                rgb: None,
                pattern: [255; 8],
                foreground: 33,
                background: 30,
            },
        )?;
        if !title.is_empty() {
            let ports = process
                .ports
                .ok_or_else(|| fail("window title requires InitGraf"))?;
            let old = memory.u32(ports.current_slot)?;
            let saved = memory.read(GuestPtr(self.manager), 108)?.to_vec();
            let clip = handle(memory, self.manager, 28)?;
            let old_clip = region(memory, process, clip)?;
            let text_rect = Rect::new(r.left + 20, top + 1, r.right - 1, bottom);
            let text_clip = region_geometry::combine(
                &rect_bytes(text_rect)?,
                &coverage,
                Boolean::Intersection,
            )?;
            let result = (|| {
                put_region(memory, process, clip, &text_clip)?;
                ports.set(memory, GuestPtr(self.manager))?;
                memory.write(GuestPtr(self.manager + 80), &33u32.to_be_bytes())?;
                memory.write(GuestPtr(self.manager + 84), &30u32.to_be_bytes())?;
                memory.write(GuestPtr(self.manager + 72), &1u16.to_be_bytes())?;
                crate::text::move_to(
                    memory,
                    ports,
                    i16::try_from(x).map_err(|_| fail("title coordinate overflow"))?,
                    i16::try_from(baseline).map_err(|_| fail("title coordinate overflow"))?,
                )?;
                crate::text::draw_strike(memory, ports, gpu, &title, &font)
            })();
            put_region(memory, process, clip, &old_clip)?;
            memory.write(GuestPtr(self.manager), &saved)?;
            memory.write(ports.current_slot, &old.to_be_bytes())?;
            result?;
        }
        Ok(())
    }
    /// Read the live loaded WIND handle, then reuse the regular window creator.
    pub fn from_resource(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
        request: ResourceWindow,
    ) -> Result<u32> {
        let ResourceWindow {
            id,
            storage,
            behind,
            color,
        } = request;
        // Classic GetNewWindow is color-aware when a matching wctb exists
        // (IM: Imaging With QuickDraw, CGrafPort), even without a forced color call.
        let color = color || process.find_resource(*b"wctb", id).is_some();
        let h = process.get_resource(memory, *b"WIND", id)?;
        if h == 0 {
            return Ok(0);
        }
        let n = process.handle_size(memory, h)? as usize;
        let data = GuestPtr(memory.u32(GuestPtr(h))?);
        let bytes = memory.read(data, n)?;
        if n < 19 {
            return Ok(0);
        }
        let title_end = 19 + usize::from(bytes[18]);
        if title_end > n {
            return Ok(0);
        }
        let aligned = (title_end + 1) & !1;
        if n > aligned && n < aligned + 2 {
            return Ok(0);
        }
        if n >= aligned + 2 && bytes[aligned..aligned + 2] != [0, 0] {
            return Err(fail("WIND positioning specification is unsupported"));
        }
        // Load optional colors through the current resource chain, then re-read
        // the WIND master: resource allocations must not retain stale pointers.
        let colors = if color {
            if process.find_resource(*b"wctb", id).is_some() {
                let table = process.get_resource(memory, *b"wctb", id)?;
                if table == 0 {
                    return Ok(0);
                }
                Some(WindowColors::read(memory, process, table)?)
            } else {
                Some(WindowColors::default())
            }
        } else {
            None
        };
        let data = GuestPtr(memory.u32(GuestPtr(h))?);
        let bytes = memory.read(data, n)?;
        let request = NewWindow {
            storage,
            bounds: data,
            title: GuestPtr(data.0 + 18),
            visible: bytes[10] != 0,
            proc_id: i16::from_be_bytes(bytes[8..10].try_into().unwrap()),
            behind,
            go_away: bytes[12] != 0,
            ref_con: u32::from_be_bytes(bytes[14..18].try_into().unwrap()),
        };
        self.create_kind(memory, process, gpu, request, colors)
    }
    fn paint_window(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
        w: u32,
        dirty: &[u8],
    ) -> Result<()> {
        let content = region(memory, process, handle(memory, w, 118)?)?;
        let structure = region(memory, process, handle(memory, w, 114)?)?;
        let border = region_geometry::combine(&structure, &content, Boolean::Difference)?;
        let border = region_geometry::combine(&border, dirty, Boolean::Intersection)?;
        let content = region_geometry::combine(&content, dirty, Boolean::Intersection)?;
        let pattern = crate::port_layout::pattern(
            memory,
            GuestPtr(w),
            crate::port_layout::Pattern::Background,
        )?;
        let is_color = crate::port_layout::color(memory, GuestPtr(w))?;
        let rgb = if is_color {
            let target = crate::port_layout::bitmap(memory, GuestPtr(w))?;
            let (fg, bg) = crate::port_layout::colors(memory, GuestPtr(w), &target)?;
            Some((target.rgb(fg)?, target.rgb(bg)?))
        } else {
            None
        };
        let origin = read_rect(
            memory,
            crate::port_layout::bounds_pointer(memory, GuestPtr(w))?,
        )?;
        let pattern = std::array::from_fn(|y| {
            pattern[((y as i32 + origin.top) & 7) as usize].rotate_right((-origin.left & 7) as u32)
        });
        self.paint(
            memory,
            process,
            gpu,
            &content,
            Paint {
                rgb,
                pattern,
                foreground: if is_color {
                    33
                } else {
                    memory.u32(GuestPtr(w + 80))?
                },
                background: if is_color {
                    30
                } else {
                    memory.u32(GuestPtr(w + 84))?
                },
            },
        )?;
        self.paint(
            memory,
            process,
            gpu,
            &border,
            Paint {
                rgb: self.colors.get(&w).map(|c| (c.0[1], c.0[0])),
                pattern: if matches!(self.definitions.get(&w), Some(1 | 4)) {
                    [0; 8]
                } else {
                    [255; 8]
                },
                foreground: 33,
                background: 30,
            },
        )?;
        if self.definitions.get(&w) == Some(&1) {
            self.modal_border(memory, process, gpu, w, dirty)?;
        }
        if self.definitions.get(&w) == Some(&4) {
            self.chrome(memory, process, gpu, w, dirty)?;
        }
        Ok(())
    }
    fn recalculate(&self, memory: &mut Memory, process: &mut Process) -> Result<()> {
        self.calculate_visibility(memory, process, None)
    }
    /// IM Toolbox Essentials: Window Manager, CalcVis/CalcVisBehind reference.
    pub fn visibility(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        request: Visibility,
    ) -> Result<()> {
        self.calculate_visibility(memory, process, Some(request))
    }
    fn calculate_visibility(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        request: Option<Visibility>,
    ) -> Result<()> {
        let list = self.list(memory)?;
        let (start, damage) = match request {
            Some(Visibility::Window(window)) => (
                list.iter()
                    .position(|&w| w == window)
                    .ok_or_else(|| fail("unknown CalcVis window"))?,
                None,
            ),
            Some(Visibility::Behind { start, clobbered }) => {
                let damage = region(memory, process, clobbered)?;
                let start = if start == 0 {
                    list.len()
                } else {
                    list.iter()
                        .position(|&w| w == start)
                        .ok_or_else(|| fail("unknown CalcVisBehind start window"))?
                };
                (start, Some(damage))
            }
            None => (0, None),
        };
        if start >= list.len() {
            return Ok(());
        }
        let desktop = region(memory, process, self.gray)?;
        let mut covered = rect_bytes(Rect::new(0, 0, 0, 0))?;
        let mut changes = Vec::new();
        for (index, w) in list.into_iter().enumerate() {
            if matches!(request, Some(Visibility::Window(_))) && index > start {
                break;
            }
            let visible = memory.read(GuestPtr(w + 110), 1)?[0] != 0;
            let structure = region(memory, process, handle(memory, w, 114)?)?;
            let selected = index >= start
                && match request {
                    Some(Visibility::Window(target)) => w == target,
                    _ => match &damage {
                        Some(damage) => !region_geometry::decode(&region_geometry::combine(
                            &structure,
                            damage,
                            Boolean::Intersection,
                        )?)?
                        .is_empty(),
                        None => true,
                    },
                };
            if selected {
                let content = region(memory, process, handle(memory, w, 118)?)?;
                let available = if visible {
                    region_geometry::combine(&content, &desktop, Boolean::Intersection)?
                } else {
                    rect_bytes(Rect::new(0, 0, 0, 0))?
                };
                let vis = region_geometry::combine(&available, &covered, Boolean::Difference)?;
                let local = read_rect(
                    memory,
                    crate::port_layout::bounds_pointer(memory, GuestPtr(w))?,
                )?;
                let local_vis = shifted(&vis, local.left, local.top)?;
                let destination = handle(memory, w, 24)?;
                region(memory, process, destination)?;
                if let Some((_, active_vis, saved)) =
                    self.update.filter(|&(active, _, _)| active == w)
                {
                    if active_vis != destination {
                        return Err(fail("update visibility handle changed"));
                    }
                    let restricted = region(memory, process, active_vis)?;
                    region(memory, process, saved)?;
                    changes.push((
                        active_vis,
                        region_geometry::combine(&restricted, &local_vis, Boolean::Intersection)?,
                    ));
                    changes.push((saved, local_vis));
                } else {
                    changes.push((destination, local_vis));
                }
            }
            if visible {
                covered = region_geometry::combine(&covered, &structure, Boolean::Union)?;
            }
        }
        if !changes.is_empty() {
            process.replace_handles(memory, changes)?;
            if process.mem_error != 0 {
                return Err(fail("window visibility allocation failed"));
            }
        }
        Ok(())
    }
    pub fn update_window(&self, memory: &Memory, process: &mut Process) -> Result<Option<u32>> {
        for w in self.list(memory)? {
            if self.update.is_some_and(|(active, _, _)| active == w)
                || memory.read(GuestPtr(w + 110), 1)?[0] == 0
            {
                continue;
            }
            let origin = read_rect(
                memory,
                crate::port_layout::bounds_pointer(memory, GuestPtr(w))?,
            )?;
            let visible = shifted(
                &region(memory, process, handle(memory, w, 24)?)?,
                -origin.left,
                -origin.top,
            )?;
            let dirty = region_geometry::combine(
                &region(memory, process, handle(memory, w, 122)?)?,
                &visible,
                Boolean::Intersection,
            )?;
            if !region_geometry::decode(&dirty)?.is_empty() {
                return Ok(Some(w));
            }
        }
        Ok(None)
    }
    fn begin(&mut self, memory: &mut Memory, process: &mut Process, w: u32) -> Result<()> {
        if self.update.is_some() {
            return Err(fail("nested BeginUpdate is unsupported"));
        }
        if !self.records.contains_key(&w) {
            return Err(fail("unknown update window"));
        }
        let vis = handle(memory, w, 24)?;
        let old = region(memory, process, vis)?;
        let update = handle(memory, w, 122)?;
        let origin = read_rect(
            memory,
            crate::port_layout::bounds_pointer(memory, GuestPtr(w))?,
        )?;
        let local = shifted(&region(memory, process, update)?, origin.left, origin.top)?;
        let clip = region_geometry::combine(&old, &local, Boolean::Intersection)?;
        let ports = process
            .ports
            .ok_or_else(|| fail("BeginUpdate requires InitGraf"))?;
        memory.validate_write(ports.current_slot, 4)?;
        let saved = process.new_handle(memory, old.len() as u32, false)?;
        if saved == 0 {
            return Err(fail("BeginUpdate region allocation failed"));
        }
        let result = process.replace_handles(
            memory,
            vec![
                (saved, old),
                (vis, clip),
                (update, rect_bytes(Rect::new(0, 0, 0, 0))?),
            ],
        );
        if result.is_err() || process.mem_error != 0 {
            let error = process.mem_error;
            process.dispose_handle(memory, saved)?;
            process.mem_error = error;
            result?;
            return Err(fail("BeginUpdate region resize failed"));
        }
        ports.set(memory, GuestPtr(w))?;
        self.update = Some((w, vis, saved));
        Ok(())
    }
    fn end(&mut self, memory: &mut Memory, process: &mut Process, w: u32) -> Result<()> {
        let (active, vis, saved) = self
            .update
            .ok_or_else(|| fail("EndUpdate without BeginUpdate"))?;
        if active != w || handle(memory, w, 24)? != vis {
            return Err(fail("update window or visibility handle changed"));
        }
        let bytes = region(memory, process, saved)?;
        memory.validate_write(GuestPtr(saved), 4)?;
        process.replace_handles(memory, vec![(vis, bytes)])?;
        if process.mem_error != 0 {
            return Err(fail("EndUpdate region restore failed"));
        }
        process.dispose_handle(memory, saved)?;
        self.update = None;
        Ok(())
    }
    pub fn create(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
        request: NewWindow,
    ) -> Result<u32> {
        self.create_kind(memory, process, gpu, request, None)
    }
    pub fn create_color(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
        request: NewWindow,
    ) -> Result<u32> {
        self.create_kind(memory, process, gpu, request, Some(WindowColors::default()))
    }
    fn create_kind(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
        request: NewWindow,
        colors: Option<WindowColors>,
    ) -> Result<u32> {
        let color = colors.is_some();
        if request.proc_id == 4
            && colors.is_some_and(|c| c.0[1..] != WindowColors::default().0[1..])
        {
            return Err(fail(
                "custom document-window title/frame colors are unsupported",
            ));
        }
        if !matches!(request.proc_id, 1 | 2 | 4) {
            return Err(fail(
                "window definition requires an unsupported standard or custom WDEF",
            ));
        }
        if self.manager == 0 {
            return Err(fail("NewWindow requires InitWindows"));
        }
        if self.update.is_some() {
            return Err(fail("window creation during BeginUpdate is unsupported"));
        }
        let ports = process
            .ports
            .ok_or_else(|| fail("NewWindow requires InitGraf"))?;
        let old_active = self.active(memory)?;
        memory.validate_write(GuestPtr(0xa64), 8)?;
        let mut list = self.list(memory)?;
        let index = if request.behind == u32::MAX {
            0
        } else if request.behind == 0 {
            list.len()
        } else {
            list.iter()
                .position(|w| *w == request.behind)
                .ok_or_else(|| fail("unknown window behind parameter"))?
                + 1
        };
        if list.len() >= 64 {
            return Err(fail("window count limit exceeded"));
        }
        let bounds = read_rect(memory, request.bounds)?;
        if bounds.left > bounds.right
            || bounds.top > bounds.bottom
            || (request.visible && (bounds.left == bounds.right || bounds.top == bounds.bottom))
        {
            return Err(fail("invalid or visible empty window bounds"));
        }
        let margin = if request.proc_id == 1 { 8 } else { 1 };
        let structure = Rect::new(
            bounds.left - margin,
            bounds.top - if request.proc_id == 4 { 19 } else { margin },
            bounds.right + if request.proc_id == 4 { 2 } else { margin },
            bounds.bottom + if request.proc_id == 4 { 2 } else { margin },
        );
        let structure = rect_bytes(structure)?;
        if request.proc_id == 4 && request.visible {
            crate::text::font_bytes(memory, process, 0, 12, 0)?;
        }
        let content = rect_bytes(bounds)?;
        let screen = Bitmap::read(memory, ports.screen_bitmap)?.bounds;
        let bitmap_bounds = Rect::new(
            screen.left - bounds.left,
            screen.top - bounds.top,
            screen.right - bounds.left,
            screen.bottom - bounds.top,
        );
        let bitmap_bounds = rect_bytes(bitmap_bounds)?;
        // Port rectangles retain degenerate dimensions. Region encodings
        // canonicalize empty bounds and cannot represent this port geometry.
        let mut local = vec![0, 0];
        for value in [0, 0, bounds.bottom - bounds.top, bounds.right - bounds.left] {
            local.extend(
                i16::try_from(value)
                    .map_err(|_| fail("window dimensions exceed signed words"))?
                    .to_be_bytes(),
            );
        }
        let n = usize::from(memory.read(request.title, 1)?[0]);
        let title = memory.read(request.title, n + 1)?.to_vec();
        let owned = request.storage.0 == 0;
        if !owned {
            memory.validate_write(request.storage, SIZE)?;
            let start = u64::from(request.storage.0);
            let end = start + SIZE as u64;
            for (address, size) in [
                (self.manager, 108),
                (ports.current_slot.0, 4),
                (LIST.0, 4),
                (0x9de, 4),
                (0x9ee, 4),
            ] {
                if start < u64::from(address) + size && u64::from(address) < end {
                    return Err(fail("window storage overlaps manager state"));
                }
            }
            if list.iter().any(|w| {
                u64::from(request.storage.0) < u64::from(*w) + SIZE as u64
                    && u64::from(*w) < u64::from(request.storage.0) + SIZE as u64
            }) {
                return Err(fail("window storage overlaps another window"));
            }
        }
        let w = if owned {
            process.new_pointer(memory, SIZE as u32, true)?
        } else {
            request.storage.0
        };
        if w == 0 {
            return Ok(0);
        }
        let old = memory.u32(ports.current_slot)?;
        let mut handles = Vec::new();
        for bytes in [
            &structure,
            &content,
            &if request.visible {
                content.clone()
            } else {
                rect_bytes(Rect::new(0, 0, 0, 0))?
            },
            &title,
        ] {
            let h = process.new_handle(memory, bytes.len() as u32, false)?;
            if h == 0 {
                for h in handles {
                    process.dispose_handle(memory, h)?;
                }
                if owned {
                    process.dispose_pointer(w);
                }
                process.mem_error = -108;
                return Ok(0);
            }
            let data = memory.u32(GuestPtr(h))?;
            memory.write(GuestPtr(data), bytes)?;
            handles.push(h);
        }
        let opened = if color {
            (|| {
                // A screen window is bound to the main screen, independently
                // of an application-selected offscreen graphics device.
                let mut devices = std::mem::take(&mut process.devices);
                let main = devices.call(memory, process, crate::devices::Op::Main);
                process.devices = devices;
                let main = main?;
                let saved = memory.u32(crate::devices::CURRENT)?;
                memory.validate_write(crate::devices::CURRENT, 4)?;
                memory.write(crate::devices::CURRENT, &main.to_be_bytes())?;
                let mut manager = std::mem::take(&mut process.color_ports);
                let result = manager.call(
                    memory,
                    process,
                    crate::color_ports::Action::Open,
                    GuestPtr(w),
                );
                process.color_ports = manager;
                memory.write(crate::devices::CURRENT, &saved.to_be_bytes())?;
                result
            })()
        } else {
            ports.open(memory, process, GuestPtr(w))
        };
        if opened.is_err() || process.mem_error == -108 {
            let exhausted = process.mem_error == -108;
            for h in handles {
                process.dispose_handle(memory, h)?;
            }
            if owned {
                process.dispose_pointer(w);
            }
            if exhausted {
                process.mem_error = -108;
            }
            opened?;
            return Ok(0);
        }
        let mut bytes = memory.read(GuestPtr(w), 108)?.to_vec();
        bytes.resize(SIZE, 0);
        if let Some(colors) = colors {
            let target = crate::port_layout::bitmap(memory, GuestPtr(w))?;
            let background = u32::from(target.nearest(colors.0[0])?);
            for (i, component) in colors.0[0].iter().enumerate() {
                bytes[42 + i * 2..44 + i * 2].copy_from_slice(&component.to_be_bytes());
            }
            bytes[84..88].copy_from_slice(&background.to_be_bytes());
            let target = crate::port_layout::bounds_pointer(memory, GuestPtr(w))?;
            memory.write(target, &bitmap_bounds[2..])?;
        } else {
            bytes[8..16].copy_from_slice(&bitmap_bounds[2..]);
        }
        bytes[16..24].copy_from_slice(&local[2..]);
        bytes[108..110].copy_from_slice(&8u16.to_be_bytes());
        bytes[110] = u8::from(request.visible);
        bytes[112] = u8::from(request.go_away);
        for (offset, h) in [
            (114, handles[0]),
            (118, handles[1]),
            (122, handles[2]),
            (134, handles[3]),
            (152, request.ref_con),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&h.to_be_bytes());
        }
        memory.write(GuestPtr(w), &bytes)?;
        self.records.insert(w, owned);
        self.definitions.insert(w, request.proc_id);
        if let Some(colors) = colors {
            self.colors.insert(w, colors);
        }
        list.insert(index, w);
        self.relink(memory, &list)?;
        self.recalculate(memory, process)?;
        let active = if request.visible && request.behind == u32::MAX {
            w
        } else {
            old_active
        };
        self.set_active(memory, active)?;
        Self::activation_change(memory, old_active, active)?;
        if request.visible {
            // Include the one-pixel border, subtracting structures above this window.
            let mut dirty = region_geometry::combine(
                &structure,
                &region(memory, process, self.gray)?,
                Boolean::Intersection,
            )?;
            for &above in &list[..index] {
                if memory.read(GuestPtr(above + 110), 1)?[0] != 0 {
                    dirty = region_geometry::combine(
                        &dirty,
                        &region(memory, process, handle(memory, above, 114)?)?,
                        Boolean::Difference,
                    )?;
                }
            }
            self.paint_window(memory, process, gpu, w, &dirty)?;
        }
        self.refresh_chrome(memory, process, gpu, w)?;
        // Classic creation leaves the port unchanged for hidden/backmost windows.
        if !request.visible || request.behind == 0 {
            memory.write(ports.current_slot, &old.to_be_bytes())?;
        }
        process.mem_error = 0;
        Ok(w)
    }
    pub fn invalidate(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        rectangle: GuestPtr,
        valid: bool,
    ) -> Result<()> {
        let ports = process
            .ports
            .ok_or_else(|| fail("window invalidation requires InitGraf"))?;
        let w = ports.current(memory)?.0;
        let r = read_rect(memory, rectangle)?;
        let change = self.invalidation(memory, process, w, r, valid)?;
        process.replace_handles(memory, vec![change])?;
        if process.mem_error != 0 {
            return Err(fail("window invalidation allocation failed"));
        }
        Ok(())
    }
    pub(crate) fn invalidation(
        &self,
        memory: &Memory,
        process: &mut Process,
        w: u32,
        r: Rect,
        valid: bool,
    ) -> Result<(u32, Vec<u8>)> {
        if !self.list(memory)?.contains(&w) {
            return Err(fail("invalidation owner is not a live window"));
        }
        let local = read_rect(
            memory,
            crate::port_layout::bounds_pointer(memory, GuestPtr(w))?,
        )?;
        let global = rect_bytes(Rect::new(
            r.left - local.left,
            r.top - local.top,
            r.right - local.left,
            r.bottom - local.top,
        ))?;
        let content = region(memory, process, handle(memory, w, 118)?)?;
        let global = region_geometry::combine(&global, &content, Boolean::Intersection)?;
        let update = handle(memory, w, 122)?;
        let old = region(memory, process, update)?;
        let new = region_geometry::combine(
            &old,
            &global,
            if valid {
                Boolean::Difference
            } else {
                Boolean::Union
            },
        )?;
        Ok((update, new))
    }
    pub fn reference(&self, memory: &mut Memory, w: u32, value: Option<u32>) -> Result<u32> {
        if !self.list(memory)?.contains(&w) {
            return Err(fail("unknown or closed window"));
        }
        let pointer = GuestPtr(w + 152);
        if let Some(value) = value {
            memory.write(pointer, &value.to_be_bytes())?;
        }
        memory.u32(pointer)
    }
    /// Commit all affected regions together, then repaint only exposure/frame
    /// damage. Common visible content keeps its existing guest bitmap pixels.
    pub fn change(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
        w: u32,
        change: Change,
    ) -> Result<()> {
        if self.update.is_some() {
            return Err(fail(
                "window geometry change during BeginUpdate is unsupported",
            ));
        }
        let list = self.list(memory)?;
        let old_active = self.active(memory)?;
        if !list.contains(&w) {
            return Err(fail("unknown or closed window"));
        }
        if let Change::Size { width, height, .. } = change {
            if width == 0 && height == 0 {
                return Ok(());
            }
            if width <= 0 || height <= 0 {
                return Err(fail("invalid window dimensions"));
            }
        }
        let visible = memory.read(GuestPtr(w + 110), 1)?[0] != 0;
        if let Change::Visibility(show) = change {
            if show == visible {
                return Ok(());
            }
            memory.validate_write(GuestPtr(w + 110), 1)?;
        }
        let mut states = Vec::new();
        for window in list {
            let local = read_rect(
                memory,
                crate::port_layout::bounds_pointer(memory, GuestPtr(window))?,
            )?;
            let old_vis = region(memory, process, handle(memory, window, 24)?)?;
            states.push(Shape {
                window,
                content: region(memory, process, handle(memory, window, 118)?)?,
                structure: region(memory, process, handle(memory, window, 114)?)?,
                visible: memory.read(GuestPtr(window + 110), 1)?[0] != 0,
                local,
                previous_vis: shifted(&old_vis, -local.left, -local.top)?,
                next_vis: Vec::new(),
                pending: region(memory, process, handle(memory, window, 122)?)?,
            });
        }
        let mut index = states.iter().position(|s| s.window == w).unwrap();
        let old_content = states[index].content.clone();
        let mut damage = if visible {
            states[index].structure.clone()
        } else {
            rect_bytes(Rect::new(0, 0, 0, 0))?
        };
        let mut changes = Vec::new();
        let mut port_rect = None;
        let mut bitmap_bounds = None;
        let mut movement = (0, 0);
        let selecting = matches!(change, Change::Select | Change::Move { front: true, .. });
        let mut copy_source = None;
        let mut copy_region = rect_bytes(Rect::new(0, 0, 0, 0))?;
        let mut copy_bounds = None;
        match change {
            Change::Size {
                width,
                height,
                update,
            } => {
                let p = GuestPtr(memory.u32(GuestPtr(handle(memory, w, 118)?))? + 2);
                let mut r = read_rect(memory, p)?;
                if r.left >= r.right || r.top >= r.bottom {
                    let port = read_rect(memory, GuestPtr(w + 16))?;
                    let screen = Bitmap::read(
                        memory,
                        process
                            .ports
                            .ok_or_else(|| fail("window size requires InitGraf"))?
                            .screen_bitmap,
                    )?
                    .bounds;
                    r.left = screen.left + port.left - states[index].local.left;
                    r.top = screen.top + port.top - states[index].local.top;
                }
                let content = Rect::new(
                    r.left,
                    r.top,
                    r.left + i32::from(width),
                    r.top + i32::from(height),
                );
                let kind = *self
                    .definitions
                    .get(&w)
                    .ok_or_else(|| fail("missing window definition identity"))?;
                let margin = if kind == 1 { 8 } else { 1 };
                let structure = Rect::new(
                    content.left - margin,
                    content.top - if kind == 4 { 19 } else { margin },
                    content.right + if kind == 4 { 2 } else { margin },
                    content.bottom + if kind == 4 { 2 } else { margin },
                );
                let old_local = read_rect(memory, GuestPtr(w + 16))?;
                let local = rect_bytes(Rect::new(
                    old_local.left,
                    old_local.top,
                    old_local.left + i32::from(width),
                    old_local.top + i32::from(height),
                ))?;
                memory.validate_write(GuestPtr(w + 16), 8)?;
                port_rect = Some(local[2..].to_vec());
                states[index].content = rect_bytes(content)?;
                states[index].structure = rect_bytes(structure)?;
                states[index].pending = region_geometry::combine(
                    &states[index].pending,
                    &states[index].content,
                    Boolean::Intersection,
                )?;
                if update {
                    let exposed = region_geometry::combine(
                        &states[index].content,
                        &old_content,
                        Boolean::Difference,
                    )?;
                    states[index].pending =
                        region_geometry::combine(&states[index].pending, &exposed, Boolean::Union)?;
                }
                changes.push((handle(memory, w, 118)?, states[index].content.clone()));
                changes.push((handle(memory, w, 114)?, states[index].structure.clone()));
            }
            Change::Visibility(show) => states[index].visible = show,
            Change::Move { .. } | Change::Select => {
                let r = read_rect(
                    memory,
                    GuestPtr(memory.u32(GuestPtr(handle(memory, w, 118)?))? + 2),
                )?;
                let (horizontal, vertical) = if let Change::Move {
                    horizontal,
                    vertical,
                    ..
                } = change
                {
                    (i32::from(horizontal), i32::from(vertical))
                } else {
                    (r.left, r.top)
                };
                let dx = horizontal - r.left;
                let dy = vertical - r.top;
                if dx == 0 && dy == 0 && (!selecting || (index == 0 && old_active == w)) {
                    return Ok(());
                }
                movement = (dx, dy);
                let old = states[index].local;
                let next = Rect::new(old.left - dx, old.top - dy, old.right - dx, old.bottom - dy);
                let bytes = rect_bytes(next)?;
                memory
                    .validate_write(crate::port_layout::bounds_pointer(memory, GuestPtr(w))?, 8)?;
                bitmap_bounds = Some(bytes[2..].to_vec());
                states[index].local = next;
                states[index].content = shifted(&states[index].content, dx, dy)?;
                states[index].structure = shifted(&states[index].structure, dx, dy)?;
                states[index].pending = shifted(&states[index].pending, dx, dy)?;
                changes.push((handle(memory, w, 118)?, states[index].content.clone()));
                changes.push((handle(memory, w, 114)?, states[index].structure.clone()));
                copy_bounds = Some((
                    r,
                    Rect::new(r.left + dx, r.top + dy, r.right + dx, r.bottom + dy),
                ));
                if states[index].visible {
                    let ports = process
                        .ports
                        .ok_or_else(|| fail("window movement requires InitGraf"))?;
                    copy_source = Some(Bitmap::read(memory, ports.screen_bitmap)?.unpack(memory)?);
                }
                if selecting {
                    memory.validate_write(LIST, 4)?;
                    memory.validate_write(GuestPtr(0xa64), 8)?;
                    for state in &states {
                        memory.validate_write(GuestPtr(state.window + 111), 1)?;
                        memory.validate_write(GuestPtr(state.window + 144), 4)?;
                    }
                    let target = states.remove(index);
                    states.insert(0, target);
                    index = 0;
                }
            }
        }
        if states[index].visible {
            damage = region_geometry::combine(&damage, &states[index].structure, Boolean::Union)?;
        }
        let desktop = region(memory, process, self.gray)?;
        let mut covered = rect_bytes(Rect::new(0, 0, 0, 0))?;
        for state in &mut states {
            let available = if state.visible {
                region_geometry::combine(&state.content, &desktop, Boolean::Intersection)?
            } else {
                rect_bytes(Rect::new(0, 0, 0, 0))?
            };
            state.next_vis = region_geometry::combine(&available, &covered, Boolean::Difference)?;
            if state.window == w && copy_bounds.is_some() {
                let old = shifted(&state.previous_vis, movement.0, movement.1)?;
                copy_region =
                    region_geometry::combine(&state.next_vis, &old, Boolean::Intersection)?;
                let exposed =
                    region_geometry::combine(&state.next_vis, &copy_region, Boolean::Difference)?;
                state.pending = region_geometry::combine(&state.pending, &exposed, Boolean::Union)?;
            } else if state.window != w || matches!(change, Change::Visibility(true)) {
                let exposed = region_geometry::combine(
                    &state.next_vis,
                    &state.previous_vis,
                    Boolean::Difference,
                )?;
                state.pending = region_geometry::combine(&state.pending, &exposed, Boolean::Union)?;
            }
            changes.push((
                handle(memory, state.window, 24)?,
                shifted(&state.next_vis, state.local.left, state.local.top)?,
            ));
            changes.push((handle(memory, state.window, 122)?, state.pending.clone()));
            if state.visible {
                covered = region_geometry::combine(&covered, &state.structure, Boolean::Union)?;
            }
        }
        process.replace_handles(memory, changes)?;
        if process.mem_error != 0 {
            return Err(fail("window region update allocation failed"));
        }
        if let Some(bounds) = bitmap_bounds {
            memory.write(
                crate::port_layout::bounds_pointer(memory, GuestPtr(w))?,
                &bounds,
            )?;
        }
        if selecting {
            let order: Vec<_> = states.iter().map(|s| s.window).collect();
            self.relink(memory, &order)?;
            for state in &states {
                memory.write(GuestPtr(state.window + 111), &[u8::from(state.window == w)])?;
            }
            Self::activation_change(memory, old_active, w)?;
        }
        if let Some(rect) = port_rect {
            memory.write(GuestPtr(w + 16), &rect)?;
        }
        if let Change::Visibility(show) = change {
            memory.write(GuestPtr(w + 110), &[u8::from(show)])?;
        }
        // ShowHide preserves highlighting/list order and queues no activation.
        let mut covered = rect_bytes(Rect::new(0, 0, 0, 0))?;
        let damage = region_geometry::combine(&damage, &desktop, Boolean::Intersection)?;
        let mut gray_damage = damage.clone();
        for state in &states {
            if !state.visible {
                continue;
            }
            let eligible = region_geometry::combine(&damage, &covered, Boolean::Difference)?;
            let eligible =
                region_geometry::combine(&eligible, &state.structure, Boolean::Intersection)?;
            let preserved = if state.window == w && copy_bounds.is_some() {
                copy_region.clone()
            } else {
                region_geometry::combine(
                    &state.previous_vis,
                    &state.next_vis,
                    Boolean::Intersection,
                )?
            };
            let redraw = region_geometry::combine(&eligible, &preserved, Boolean::Difference)?;
            self.paint_window(memory, process, gpu, state.window, &redraw)?;
            gray_damage =
                region_geometry::combine(&gray_damage, &state.structure, Boolean::Difference)?;
            covered = region_geometry::combine(&covered, &state.structure, Boolean::Union)?;
        }
        self.desktop(memory, process, gpu, &gray_damage)?;
        if let (Some(source), Some((old, new))) = (copy_source, copy_bounds) {
            let ports = process
                .ports
                .ok_or_else(|| fail("window movement requires InitGraf"))?;
            let screen = Bitmap::read(memory, ports.screen_bitmap)?;
            screen.execute_source(
                gpu,
                memory,
                Draw::Copy {
                    rect: new,
                    clip: screen.bounds,
                    source_x: old.left,
                    source_y: old.top,
                    mode: Transfer::Copy,
                },
                Some(&region_geometry::decode(&copy_region)?),
                Some(&source),
            )?;
        }
        self.refresh_chrome(memory, process, gpu, 0)?;
        Ok(())
    }
    pub fn call(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
        op: Op,
        w: u32,
    ) -> Result<u32> {
        if matches!(op, Op::Show | Op::Hide) {
            let list = self.list(memory)?;
            if !list.contains(&w) {
                return Err(fail("unknown or closed window"));
            }
            let visible = memory.read(GuestPtr(w + 110), 1)?[0] != 0;
            let show = op == Op::Show;
            if show == visible {
                return Ok(0);
            }
            let old_active = self.active(memory)?;
            let mut next = None;
            if !show && old_active == w {
                for &other in &list {
                    if other != w && memory.read(GuestPtr(other + 110), 1)?[0] != 0 {
                        next = Some(other);
                        break;
                    }
                }
            }
            memory.validate_write(GuestPtr(0xa64), 8)?;
            self.change(memory, process, gpu, w, Change::Visibility(show))?;
            if show && list.first() == Some(&w) {
                if old_active != w {
                    self.change(memory, process, gpu, w, Change::Select)?;
                } else {
                    memory.write(GuestPtr(0xa64), &w.to_be_bytes())?;
                }
            } else if !show && old_active == w {
                if let Some(next) = next {
                    self.change(memory, process, gpu, next, Change::Select)?;
                } else {
                    memory.write(GuestPtr(w + 111), &[0])?;
                    Self::activation_change(memory, w, 0)?;
                }
            }
            return Ok(0);
        }
        if op == Op::Begin {
            self.begin(memory, process, w)?;
            return Ok(0);
        }
        if op == Op::End {
            self.end(memory, process, w)?;
            return Ok(0);
        }
        if self.update.is_some() && matches!(op, Op::Close | Op::Dispose) {
            return Err(fail("window teardown during BeginUpdate is unsupported"));
        }
        if op == Op::Init {
            self.init(memory, process, gpu)?;
            return Ok(0);
        }
        let mut list = self.list(memory)?;
        if op == Op::Front {
            return Ok(self.front(memory)?.map_or(0, |(w, _)| w));
        }
        if op == Op::Dispose && !list.contains(&w) && self.records.get(&w) == Some(&true) {
            process.dispose_pointer(w);
            self.records.remove(&w);
            self.definitions.remove(&w);
            self.colors.remove(&w);
            return Ok(0);
        }
        let index = list
            .iter()
            .position(|entry| *entry == w)
            .ok_or_else(|| fail("unknown or closed window"))?;
        let mut dirty = region(memory, process, handle(memory, w, 114)?)?;
        for &above in &list[..index] {
            if memory.read(GuestPtr(above + 110), 1)?[0] != 0 {
                dirty = region_geometry::combine(
                    &dirty,
                    &region(memory, process, handle(memory, above, 114)?)?,
                    Boolean::Difference,
                )?;
            }
        }
        if memory.read(GuestPtr(w + 110), 1)?[0] == 0 {
            dirty = rect_bytes(Rect::new(0, 0, 0, 0))?;
        }
        if op == Op::Dispose && process.pointer_size(w) < SIZE as u32 {
            return Err(fail("DisposeWindow requires a live NewPtr allocation; use CloseWindow for static caller storage"));
        }
        if op == Op::Dispose && process.mem_error != 0 {
            return Err(fail("DisposeWindow storage is not an owned pointer"));
        }
        let color = crate::port_layout::color(memory, GuestPtr(w))?;
        let mut handles = [114, 118, 122, 134]
            .map(|offset| handle(memory, w, offset))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let mut all_handles = handles.clone();
        if color {
            let manager = std::mem::take(&mut process.color_ports);
            let attached = manager.attached(memory, process, GuestPtr(w));
            process.color_ports = manager;
            all_handles.extend(attached?);
        } else {
            handles.extend([handle(memory, w, 24)?, handle(memory, w, 28)?]);
            all_handles = handles.clone();
        }
        if all_handles
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != all_handles.len()
        {
            return Err(fail("window attached handles alias"));
        }
        for &h in &all_handles {
            if process.handle_size(memory, h)? == 0 {
                return Err(fail("corrupt window ownership"));
            }
        }
        let old_active = self.active(memory)?;
        memory.validate_write(GuestPtr(0xa64), 8)?;
        let mut controls = std::mem::take(&mut process.controls);
        let result = controls.close_window(memory, process, w);
        process.controls = controls;
        result?;
        list.remove(index);
        self.relink(memory, &list)?;
        self.recalculate(memory, process)?;
        for slot in [GuestPtr(0xa64), GuestPtr(0xa68)] {
            if memory.u32(slot)? == w {
                memory.write(slot, &0u32.to_be_bytes())?;
            }
        }
        if old_active == w {
            let mut active = 0;
            for &other in &list {
                if memory.read(GuestPtr(other + 110), 1)?[0] != 0 {
                    active = other;
                    break;
                }
            }
            self.set_active(memory, active)?;
            Self::activation_change(memory, 0, active)?;
        }
        let desktop = region(memory, process, self.gray)?;
        let mut gray_dirty = region_geometry::combine(&dirty, &desktop, Boolean::Intersection)?;
        let mut covered = rect_bytes(Rect::new(0, 0, 0, 0))?;
        for &other in &list {
            if memory.read(GuestPtr(other + 110), 1)?[0] == 0 {
                continue;
            }
            let exposed = region_geometry::combine(&dirty, &covered, Boolean::Difference)?;
            self.paint_window(memory, process, gpu, other, &exposed)?;
            let update = handle(memory, other, 122)?;
            let content = region(memory, process, handle(memory, other, 118)?)?;
            let exposed = region_geometry::combine(&exposed, &content, Boolean::Intersection)?;
            let bytes = region_geometry::combine(
                &region(memory, process, update)?,
                &exposed,
                Boolean::Union,
            )?;
            put_region(memory, process, update, &bytes)?;
            let structure = region(memory, process, handle(memory, other, 114)?)?;
            gray_dirty = region_geometry::combine(&gray_dirty, &structure, Boolean::Difference)?;
            covered = region_geometry::combine(&covered, &structure, Boolean::Union)?;
        }
        self.desktop(memory, process, gpu, &gray_dirty)?;
        self.refresh_chrome(memory, process, gpu, 0)?;
        for h in handles {
            process.dispose_handle(memory, h)?;
        }
        if color {
            let mut manager = std::mem::take(&mut process.color_ports);
            let result = manager.call(
                memory,
                process,
                crate::color_ports::Action::Close,
                GuestPtr(w),
            );
            process.color_ports = manager;
            result?;
        }
        memory.write(GuestPtr(w + 110), &[0])?;
        memory.write(GuestPtr(w + 144), &0u32.to_be_bytes())?;
        if let Some(ports) = process.ports {
            if memory.u32(ports.current_slot)? == w {
                memory.write(ports.current_slot, &self.manager.to_be_bytes())?;
            }
        }
        if op == Op::Dispose {
            process.dispose_pointer(w);
            self.records.remove(&w);
            self.definitions.remove(&w);
            self.colors.remove(&w);
        } else if !self.records[&w] {
            if process.pointer_size(w) >= SIZE as u32 && process.mem_error == 0 {
                self.records.insert(w, true);
            } else {
                self.records.remove(&w);
                self.definitions.remove(&w);
                self.colors.remove(&w);
            }
        }
        process.mem_error = 0;
        Ok(0)
    }
}

#[cfg(test)]
mod dialog_event_tests {
    use super::*;
    fn setup() -> (Memory, Windows) {
        let mut m = Memory::default();
        m.map(GuestPtr(0), vec![0; 0x3000], crate::memory::Permissions::RW)
            .unwrap();
        let mut windows = Windows::default();
        windows.records.insert(0x1000, false);
        windows.records.insert(0x1100, false);
        m.write(LIST, &0x1000u32.to_be_bytes()).unwrap();
        m.write(GuestPtr(0x1000 + 144), &0x1100u32.to_be_bytes())
            .unwrap();
        m.write(GuestPtr(0x1000 + 108), &[0, 8, 1, 1]).unwrap();
        m.write(GuestPtr(0x1100 + 108), &[0, 2, 1, 0]).unwrap();
        (m, windows)
    }
    #[test]
    fn dialog_events_follow_live_front_visibility_and_signed_kind_for_every_event() {
        let (mut m, windows) = setup();
        let event = GuestPtr(0x2000);
        assert!(!windows.is_dialog_event(&m, event).unwrap()); // dialog behind document
        m.write(GuestPtr(0x1000 + 110), &[0]).unwrap(); // hidden windows skipped
        for what in [0u16, 1, 2, 3, 4, 5, 6, 7, 8, 15, 23, 0xffff] {
            m.write(event, &what.to_be_bytes()).unwrap();
            m.write(GuestPtr(event.0 + 2), &0x1000u32.to_be_bytes())
                .unwrap();
            let before = m.read(GuestPtr(0), 0x3000).unwrap().to_vec();
            assert!(windows.is_dialog_event(&m, event).unwrap());
            assert_eq!(m.read(GuestPtr(0), 0x3000).unwrap(), before);
        }
        // kind is read from guest storage, not the creation definition or hilite.
        for kind in [0i16, -1, -32768, 8, 32767] {
            m.write(GuestPtr(0x1100 + 108), &kind.to_be_bytes())
                .unwrap();
            assert!(!windows.is_dialog_event(&m, event).unwrap());
        }
        m.write(GuestPtr(0x1100 + 108), &2i16.to_be_bytes())
            .unwrap();
        m.write(GuestPtr(0x1100 + 110), &[0]).unwrap();
        assert!(!windows.is_dialog_event(&m, event).unwrap());
        m.write(LIST, &[0; 4]).unwrap();
        assert!(!windows.is_dialog_event(&m, event).unwrap());
    }
    #[test]
    fn dialog_event_faults_preserve_memory_and_validate_event_and_window_list() {
        let (mut m, windows) = setup();
        let before = m.read(GuestPtr(0), 0x3000).unwrap().to_vec();
        assert!(windows.is_dialog_event(&m, GuestPtr(0x2ff1)).is_err());
        assert_eq!(m.read(GuestPtr(0), 0x3000).unwrap(), before);
        m.write(GuestPtr(0x1100 + 144), &0x1000u32.to_be_bytes())
            .unwrap();
        assert!(windows.is_dialog_event(&m, GuestPtr(0x2000)).is_err());
        m.write(LIST, &0x1200u32.to_be_bytes()).unwrap();
        assert!(windows.is_dialog_event(&m, GuestPtr(0x2000)).is_err());
    }
}
#[cfg(test)]
mod visibility_tests {
    use super::*;
    fn owned(m: &mut Memory, p: &mut Process, bytes: Vec<u8>) -> u32 {
        let h = p.new_handle(m, bytes.len() as u32, false).unwrap();
        m.write(GuestPtr(m.u32(GuestPtr(h)).unwrap()), &bytes)
            .unwrap();
        h
    }
    fn setup() -> (Memory, Process, Windows) {
        let mut m = Memory::default();
        m.map(GuestPtr(0), vec![0; 0x4000], crate::memory::Permissions::RW)
            .unwrap();
        let mut p = Process::default();
        let mut windows = Windows {
            gray: owned(
                &mut m,
                &mut p,
                rect_bytes(Rect::new(0, 10, 200, 200)).unwrap(),
            ),
            ..Windows::default()
        };
        for (index, (w, r)) in [
            (0x1000u32, Rect::new(20, 20, 60, 60)),
            (0x1100, Rect::new(0, 0, 100, 100)),
            (0x1200, Rect::new(120, 10, 150, 30)),
        ]
        .into_iter()
        .enumerate()
        {
            windows.records.insert(w, false);
            m.write(GuestPtr(w + 110), &[1]).unwrap();
            m.write(
                GuestPtr(w + 144),
                &if index < 2 { w + 0x100 } else { 0 }.to_be_bytes(),
            )
            .unwrap();
            // Negative bitmap bounds express a nonzero local coordinate origin.
            m.write(GuestPtr(w + 6), &[0, 2]).unwrap();
            let origin: Vec<_> = [-20i16, -10, 180, 190]
                .into_iter()
                .flat_map(i16::to_be_bytes)
                .collect();
            m.write(GuestPtr(w + 8), &origin).unwrap();
            for (offset, bytes) in [
                (24, rect_bytes(Rect::new(180, 180, 181, 181)).unwrap()),
                (28, rect_bytes(Rect::new(-100, -100, 200, 200)).unwrap()),
                (114, rect_bytes(r).unwrap()),
                (118, rect_bytes(r).unwrap()),
                (122, rect_bytes(Rect::new(5, 12, 15, 18)).unwrap()),
            ] {
                let h = owned(&mut m, &mut p, bytes);
                m.write(GuestPtr(w + offset), &h.to_be_bytes()).unwrap();
            }
        }
        m.write(LIST, &0x1000u32.to_be_bytes()).unwrap();
        p.ports = Some(crate::quickdraw::Ports {
            current_slot: GuestPtr(0x3000),
            screen_bitmap: GuestPtr(0x3100),
        });
        (m, p, windows)
    }
    fn get(m: &Memory, p: &mut Process, w: u32) -> Vec<u8> {
        region(m, p, handle(m, w, 24).unwrap()).unwrap()
    }
    fn has(bytes: &[u8], x: i32, y: i32) -> bool {
        region_geometry::decode(bytes)
            .unwrap()
            .iter()
            .any(|r| r.left <= x && x < r.right && r.top <= y && y < r.bottom)
    }
    #[test]
    fn selective_visibility_uses_live_occlusion_desktop_origin_and_exact_damage() {
        let (mut m, mut p, windows) = setup();
        let front = get(&m, &mut p, 0x1000);
        let far = get(&m, &mut p, 0x1200);
        let damage = owned(
            &mut m,
            &mut p,
            rect_bytes(Rect::new(0, 0, 100, 100)).unwrap(),
        );
        windows
            .visibility(
                &mut m,
                &mut p,
                Visibility::Behind {
                    start: 0x1100,
                    clobbered: damage,
                },
            )
            .unwrap();
        let back = get(&m, &mut p, 0x1100);
        assert!(has(&back, 0, -5));
        assert!(!has(&back, 20, 10));
        assert!(!has(&back, 0, -15));
        assert_eq!(get(&m, &mut p, 0x1000), front);
        assert_eq!(get(&m, &mut p, 0x1200), far);
        m.write(GuestPtr(0x1000 + 110), &[0]).unwrap();
        windows
            .visibility(&mut m, &mut p, Visibility::Window(0x1100))
            .unwrap();
        assert!(has(&get(&m, &mut p, 0x1100), 20, 10));
        let before = get(&m, &mut p, 0x1100);
        let hole = region_geometry::combine(
            &rect_bytes(Rect::new(-10, -10, 200, 200)).unwrap(),
            &rect_bytes(Rect::new(-5, -5, 110, 110)).unwrap(),
            Boolean::Difference,
        )
        .unwrap();
        let damage = owned(&mut m, &mut p, hole);
        windows
            .visibility(
                &mut m,
                &mut p,
                Visibility::Behind {
                    start: 0x1100,
                    clobbered: damage,
                },
            )
            .unwrap();
        assert_eq!(get(&m, &mut p, 0x1100), before);
        assert_ne!(get(&m, &mut p, 0x1200), far);
        let before = get(&m, &mut p, 0x1200);
        windows
            .visibility(
                &mut m,
                &mut p,
                Visibility::Behind {
                    start: 0,
                    clobbered: damage,
                },
            )
            .unwrap();
        assert_eq!(get(&m, &mut p, 0x1200), before);
    }
    #[test]
    fn recalculation_during_update_keeps_the_draw_clip_and_refreshes_saved_visibility() {
        let (mut m, mut p, mut windows) = setup();
        windows
            .visibility(&mut m, &mut p, Visibility::Window(0x1100))
            .unwrap();
        windows.begin(&mut m, &mut p, 0x1100).unwrap();
        let clip = get(&m, &mut p, 0x1100);
        m.write(GuestPtr(0x1000 + 110), &[0]).unwrap();
        windows
            .visibility(&mut m, &mut p, Visibility::Window(0x1100))
            .unwrap();
        assert_eq!(get(&m, &mut p, 0x1100), clip);
        windows.end(&mut m, &mut p, 0x1100).unwrap();
        let full = get(&m, &mut p, 0x1100);
        assert!(has(&full, 20, 10));
        assert!(!has(&clip, 20, 10));
        assert!(has(&full, 0, -5));
    }
    #[test]
    fn malformed_later_regions_and_failed_batch_growth_leave_earlier_outputs_unchanged() {
        let (mut m, mut p, windows) = setup();
        let first = get(&m, &mut p, 0x1000);
        let second = get(&m, &mut p, 0x1100);
        let h = handle(&m, 0x1200, 118).unwrap();
        let data = GuestPtr(m.u32(GuestPtr(h)).unwrap());
        m.write(data, &9u16.to_be_bytes()).unwrap();
        assert!(windows.recalculate(&mut m, &mut p).is_err());
        assert_eq!(get(&m, &mut p, 0x1000), first);
        assert_eq!(get(&m, &mut p, 0x1100), second);
        // CalcVis must not inspect irrelevant geometry behind its target.
        let structure = handle(&m, 0x1200, 114).unwrap();
        let data = GuestPtr(m.u32(GuestPtr(structure)).unwrap());
        m.write(data, &9u16.to_be_bytes()).unwrap();
        windows
            .visibility(&mut m, &mut p, Visibility::Window(0x1000))
            .unwrap();
        assert!(has(&get(&m, &mut p, 0x1000), 20, 10));
        let (mut m, mut p, windows) = setup();
        let first = get(&m, &mut p, 0x1000);
        let second = get(&m, &mut p, 0x1100);
        // Back visibility needs a complex record; lock its small allocation behind other live blocks.
        p.set_handle_lock(&m, handle(&m, 0x1100, 24).unwrap(), true)
            .unwrap();
        assert!(windows.recalculate(&mut m, &mut p).is_err());
        assert_eq!(p.mem_error, -108);
        assert_eq!(get(&m, &mut p, 0x1000), first);
        assert_eq!(get(&m, &mut p, 0x1100), second);
    }
}
