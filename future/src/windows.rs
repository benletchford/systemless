//! Guest WindowRecords and standard plain-dialog windows (procID 2).
//! Layout and ordering compared with legacy trap/window.rs.
use crate::{
    bitmap::Bitmap,
    graphics::{Graphics, Rect},
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
pub enum Op {
    Init,
    Front,
    Close,
    Dispose,
    Begin,
    End,
}
#[derive(Default)]
pub struct Windows {
    manager: u32,
    gray: u32,
    records: BTreeMap<u32, bool>,
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
impl Windows {
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
            ports.paint_coverage(
                gpu,
                memory,
                bounds,
                Some(&region_geometry::decode(coverage)?),
            )
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
                pattern: [0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55],
                foreground: 33,
                background: 30,
            },
        )
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
        let pattern: [u8; 8] = memory.read(GuestPtr(w + 32), 8)?.try_into().unwrap();
        let origin = read_rect(memory, GuestPtr(w + 8))?;
        let pattern = std::array::from_fn(|y| {
            pattern[((y as i32 + origin.top) & 7) as usize].rotate_right((-origin.left & 7) as u32)
        });
        self.paint(
            memory,
            process,
            gpu,
            &content,
            Paint {
                pattern,
                foreground: memory.u32(GuestPtr(w + 80))?,
                background: memory.u32(GuestPtr(w + 84))?,
            },
        )?;
        self.paint(
            memory,
            process,
            gpu,
            &border,
            Paint {
                pattern: [255; 8],
                foreground: 33,
                background: 30,
            },
        )
    }
    fn recalculate(&self, memory: &mut Memory, process: &mut Process) -> Result<()> {
        let list = self.list(memory)?;
        let desktop = region(memory, process, self.gray)?;
        let mut covered = rect_bytes(Rect::new(0, 0, 0, 0))?;
        let mut front = true;
        for w in list {
            let visible = memory.read(GuestPtr(w + 110), 1)?[0] != 0;
            memory.write(GuestPtr(w + 111), &[u8::from(visible && front)])?;
            if visible {
                front = false;
            }
            let content = region(memory, process, handle(memory, w, 118)?)?;
            let available = if visible {
                region_geometry::combine(&content, &desktop, Boolean::Intersection)?
            } else {
                rect_bytes(Rect::new(0, 0, 0, 0))?
            };
            let vis = region_geometry::combine(&available, &covered, Boolean::Difference)?;
            let local = read_rect(memory, GuestPtr(w + 8))?;
            put_region(
                memory,
                process,
                handle(memory, w, 24)?,
                &shifted(&vis, local.left, local.top)?,
            )?;
            if visible {
                covered = region_geometry::combine(
                    &covered,
                    &region(memory, process, handle(memory, w, 114)?)?,
                    Boolean::Union,
                )?;
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
            let origin = read_rect(memory, GuestPtr(w + 8))?;
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
        let origin = read_rect(memory, GuestPtr(w + 8))?;
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
        if request.proc_id != 2 {
            return Err(fail(
                "window definition requires unimplemented title-bar or custom WDEF services",
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
        if bounds.left >= bounds.right || bounds.top >= bounds.bottom {
            return Err(fail("empty window bounds"));
        }
        let structure = Rect::new(
            bounds.left - 1,
            bounds.top - 1,
            bounds.right + 1,
            bounds.bottom + 1,
        );
        let structure = rect_bytes(structure)?;
        let content = rect_bytes(bounds)?;
        let screen = Bitmap::read(memory, ports.screen_bitmap)?.bounds;
        let bitmap_bounds = Rect::new(
            screen.left - bounds.left,
            screen.top - bounds.top,
            screen.right - bounds.left,
            screen.bottom - bounds.top,
        );
        let bitmap_bounds = rect_bytes(bitmap_bounds)?;
        let local = rect_bytes(Rect::new(
            0,
            0,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
        ))?;
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
        if let Err(error) = ports.open(memory, process, GuestPtr(w)) {
            for h in handles {
                process.dispose_handle(memory, h)?;
            }
            if owned {
                process.dispose_pointer(w);
            }
            return Err(error);
        }
        let mut bytes = memory.read(GuestPtr(w), 108)?.to_vec();
        bytes.resize(SIZE, 0);
        bytes[8..16].copy_from_slice(&bitmap_bounds[2..]);
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
        list.insert(index, w);
        self.relink(memory, &list)?;
        self.recalculate(memory, process)?;
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
        // Classic creation leaves the port unchanged for hidden/backmost windows.
        if !request.visible || request.behind == 0 {
            memory.write(ports.current_slot, &old.to_be_bytes())?;
        }
        process.mem_error = 0;
        Ok(w)
    }
    pub fn call(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: &Graphics,
        op: Op,
        w: u32,
    ) -> Result<u32> {
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
            for w in list {
                if memory.read(GuestPtr(w + 110), 1)?[0] != 0 {
                    return Ok(w);
                }
            }
            return Ok(0);
        }
        if op == Op::Dispose && !list.contains(&w) && self.records.get(&w) == Some(&true) {
            process.dispose_pointer(w);
            self.records.remove(&w);
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
        let handles = [24, 28, 114, 118, 122, 134]
            .map(|offset| handle(memory, w, offset))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        if handles
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != handles.len()
        {
            return Err(fail("window region handles alias"));
        }
        for &h in &handles {
            if process.handle_size(memory, h)? == 0 {
                return Err(fail("corrupt window ownership"));
            }
        }
        list.remove(index);
        self.relink(memory, &list)?;
        self.recalculate(memory, process)?;
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
        for h in handles {
            process.dispose_handle(memory, h)?;
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
        } else if !self.records[&w] {
            if process.pointer_size(w) >= SIZE as u32 && process.mem_error == 0 {
                self.records.insert(w, true);
            } else {
                self.records.remove(&w);
            }
        }
        process.mem_error = 0;
        Ok(0)
    }
}
