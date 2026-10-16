//! Cooperative standard monochrome menu tracking. CPU adapters only own the
//! suspended return ABI; menu geometry, live records and drawing live here.
use super::{background, bad, data, record::Record, title_foreground, Menus, HEIGHT};
use crate::{
    bitmap::Bitmap,
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory},
    process::Process,
    Result,
};
const THE_MENU: GuestPtr = GuestPtr(0xa26);
const CHOICE: GuestPtr = GuestPtr(0xb54);
const DIM: [u8; 8] = [0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55];
fn inside(r: Rect, p: [i16; 2]) -> bool {
    let (y, x) = (i32::from(p[0]), i32::from(p[1]));
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}
fn union(a: Rect, b: Rect) -> Rect {
    Rect::new(
        a.left.min(b.left),
        a.top.min(b.top),
        a.right.max(b.right),
        a.bottom.max(b.bottom),
    )
}
struct Title {
    handle: u32,
    id: i16,
    rect: Rect,
    enabled: bool,
}
impl Title {
    fn highlight(&self) -> Rect {
        Rect::new(
            self.rect.left,
            self.rect.top,
            self.rect.right,
            self.rect.bottom - 1,
        )
    }
}
struct TrackingView {
    bitmap: Bitmap,
    titles: Vec<Title>,
    list: Vec<u8>,
    colors: Vec<u8>,
}
struct Paint {
    rect: Rect,
    clip: Rect,
    ink: Option<Vec<Rect>>,
    pattern: [u8; 8],
    foreground: u8,
    background: u8,
    xor: bool,
}
impl Paint {
    fn fill(rect: Rect, clip: Rect, foreground: u8) -> Self {
        Self {
            rect,
            clip,
            ink: None,
            pattern: [255; 8],
            foreground,
            background: 0,
            xor: false,
        }
    }
    fn invert(rect: Rect, clip: Rect) -> Self {
        Self {
            xor: true,
            ..Self::fill(rect, clip, 1)
        }
    }
    fn draw(&self, bitmap: Bitmap, m: &mut Memory, gpu: &Graphics) -> Result<()> {
        bitmap.execute_clipped(
            gpu,
            m,
            Draw::Fill {
                rect: self.rect,
                clip: self.clip,
                pattern: self.pattern,
                foreground: u32::from(self.foreground),
                background: u32::from(self.background),
                mode: if self.xor {
                    Transfer::Xor
                } else {
                    Transfer::Copy
                },
            },
            self.ink.as_deref(),
        )
    }
}
struct Plan {
    paints: Vec<Paint>,
    damage: Option<Rect>,
    key: Vec<u8>,
    hover: u32,
    selected: u32,
    id: Option<i16>,
}
pub struct Tracking {
    bitmap: Bitmap,
    snapshot: Vec<u8>,
    active: Option<i16>,
    damage: Option<Rect>,
    last: Option<Vec<u8>>,
}
impl Menus {
    fn tracking_titles(&self, m: &Memory, p: &mut Process) -> Result<TrackingView> {
        let (list, colors) = self.check(m, p)?;
        self.validate_menus(m, p, &list)?;
        let bitmap = Bitmap::read(
            m,
            p.ports
                .ok_or_else(|| bad("menu tracking requires InitGraf"))?
                .screen_bitmap,
        )?;
        let height = i32::from(m.u16(HEIGHT)? as i16);
        if height <= 0 || bitmap.bounds.top + height >= bitmap.bounds.bottom {
            return Err(bad("invalid tracking menu-bar height"));
        }
        let regular = usize::from(u16::from_be_bytes(list[..2].try_into().unwrap()));
        if list[8 + regular..12 + regular] != [0; 4] {
            return Err(bad("foreign menu tracking state is unsupported"));
        }
        let mut titles = Vec::new();
        for (i, e) in list[6..6 + regular].as_chunks::<6>().0.iter().enumerate() {
            let handle = u32::from_be_bytes(e[..4].try_into().unwrap());
            let bytes = data(m, p, handle)?;
            let menu = Record::parse(&bytes)?;
            let left = i32::from(i16::from_be_bytes(e[4..6].try_into().unwrap()));
            let right_at = if (i + 1) * 6 < regular {
                6 + (i + 1) * 6 + 4
            } else {
                2
            };
            let right = i32::from(i16::from_be_bytes(
                list[right_at..right_at + 2].try_into().unwrap(),
            ));
            if right <= left
                || titles
                    .iter()
                    .any(|t: &Title| t.id == menu.id || t.rect.right > left)
            {
                return Err(bad("invalid tracking title order or duplicate ID"));
            }
            titles.push(Title {
                handle,
                id: menu.id,
                rect: Rect::new(left, bitmap.bounds.top, right, bitmap.bounds.top + height),
                enabled: menu.enabled & 1 != 0,
            });
        }
        Ok(TrackingView {
            bitmap,
            titles,
            list,
            colors,
        })
    }
    pub(super) fn draw_highlight(
        &self,
        m: &mut Memory,
        p: &mut Process,
        gpu: &Graphics,
    ) -> Result<()> {
        let id = m.u16(THE_MENU)? as i16;
        if id != 0 {
            let TrackingView { bitmap, titles, .. } = self.tracking_titles(m, p)?;
            if let Some(t) = titles.iter().find(|t| t.id == id && t.enabled) {
                Paint::invert(t.highlight(), bitmap.bounds).draw(bitmap, m, gpu)?;
            }
        }
        Ok(())
    }
    pub fn hilite(&self, m: &mut Memory, p: &mut Process, gpu: &Graphics, id: i16) -> Result<()> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            let TrackingView { bitmap, titles, .. } = self.tracking_titles(m, p)?;
            let title = titles.iter().find(|t| id != 0 && t.id == id && t.enabled);
            let id = title.map_or(0, |t| t.id);
            m.validate_write(THE_MENU, 2)?;
            // DrawMenuBar itself preserves the requested highlight. Plan all
            // fallible list validation before publishing this new identity.
            let old = m.u16(THE_MENU)?;
            m.write(THE_MENU, &0u16.to_be_bytes())?;
            let draw = self.draw(m, p, gpu);
            m.write(THE_MENU, &old.to_be_bytes())?;
            draw?;
            if let Some(t) = title {
                Paint::invert(t.highlight(), bitmap.bounds).draw(bitmap, m, gpu)?;
            }
            m.write(THE_MENU, &id.to_be_bytes())
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
    pub fn begin_tracking(
        &self,
        m: &mut Memory,
        p: &mut Process,
        gpu: &Graphics,
        start: [i16; 2],
    ) -> Result<Option<Tracking>> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            // MBarEnable, MBarHook, MenuHook and MenuFlash need their own
            // callback/desk-accessory/timed-flash continuations, never stubs.
            if m.u16(GuestPtr(0xa20))? != 0
                || m.u32(GuestPtr(0xa2c))? != 0
                || m.u32(GuestPtr(0xa30))? != 0
                || m.u16(GuestPtr(0xa24))? != 0
            {
                return Err(bad(
                    "menu hooks, desk accessories and selection flashing are unsupported",
                ));
            }
            let TrackingView { bitmap, titles, .. } = self.tracking_titles(m, p)?;
            let active = titles
                .iter()
                .find(|t| inside(t.rect, start) && t.enabled)
                .map(|t| t.id);
            let Some(id) = active else { return Ok(None) };
            // Validate the selected menu's complete paint plan before touching
            // the screen or highlight. Later host edits are revalidated on polls.
            self.tracking_plan(m, p, Some(id), start)?;
            m.validate_write(CHOICE, 4)?;
            let (w, h) = bitmap.extent()?;
            m.validate_write(bitmap.base, bitmap.row_bytes * h)?;
            self.hilite(m, p, gpu, 0)?;
            let snapshot = bitmap.unpack(m)?;
            debug_assert_eq!(snapshot.len(), w * h);
            m.write(CHOICE, &0u32.to_be_bytes())?;
            Ok(Some(Tracking {
                bitmap,
                snapshot,
                active,
                damage: None,
                last: None,
            }))
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
    fn tracking_plan(
        &self,
        m: &mut Memory,
        p: &mut Process,
        active: Option<i16>,
        point: [i16; 2],
    ) -> Result<Plan> {
        if m.u16(GuestPtr(0xa20))? != 0
            || m.u32(GuestPtr(0xa2c))? != 0
            || m.u32(GuestPtr(0xa30))? != 0
            || m.u16(GuestPtr(0xa24))? != 0
        {
            return Err(bad(
                "menu hooks, desk accessories and selection flashing are unsupported",
            ));
        }
        let TrackingView {
            bitmap,
            titles,
            list,
            colors,
        } = self.tracking_titles(m, p)?;
        let in_bar = i32::from(point[0]) >= bitmap.bounds.top
            && i32::from(point[0]) < bitmap.bounds.top + i32::from(m.u16(HEIGHT)? as i16);
        let id = if in_bar {
            titles
                .iter()
                .find(|t| inside(t.rect, point) && t.enabled)
                .map(|t| t.id)
        } else {
            active
        };
        let Some(title) = titles.iter().find(|t| Some(t.id) == id && t.enabled) else {
            return Ok(Plan {
                paints: vec![],
                damage: None,
                key: vec![],
                hover: 0,
                selected: 0,
                id: None,
            });
        };
        if title.id <= 0 {
            return Err(bad("non-application menu ID dispatch is unsupported"));
        }
        let bytes = data(m, p, title.handle)?;
        let menu = Record::parse(&bytes)?;
        if menu.items.iter().any(|i| i.icon != 0 || i.command == 0x1b) {
            return Err(bad("menu icons and hierarchical tracking are unsupported"));
        }
        if colors
            .as_chunks::<30>()
            .0
            .iter()
            .any(|e| e[2..4] != [0; 2] && (e[..2] == title.id.to_be_bytes() || e[..2] == [0; 2]))
        {
            return Err(bad("per-item menu colors are unsupported"));
        }
        let bg = background(&colors)?;
        let fg = title_foreground(&colors, title.id, bg)?;
        let plain = crate::text::font_bytes(m, p, 0, 0, 0)?;
        let mut key = list;
        key.extend(&colors);
        key.extend(&bytes);
        key.extend(&plain);
        key.extend(title.id.to_be_bytes());
        key.extend(m.u16(HEIGHT)?.to_be_bytes());
        let mut width = 32i32;
        let mut height = 0i32;
        let mut fonts = Vec::new();
        for item in &menu.items {
            let face = crate::text::font_bytes(m, p, 0, 0, item.style)?;
            let strike = crate::font::Strike::parse(&face)?;
            width =
                width.max(strike.width(item.text)? + 32 + if item.command > 0x20 { 32 } else { 0 });
            height += if item.text == b"-" { 6 } else { 16 };
            key.extend(&face);
            fonts.push(face);
        }
        let top = title.rect.bottom;
        let left = title
            .rect
            .left
            .min(bitmap.bounds.right - width - 2)
            .max(bitmap.bounds.left);
        if width + 2 > bitmap.bounds.right - bitmap.bounds.left
            || top + height + 4 > bitmap.bounds.bottom
        {
            return Err(bad("scrolling or oversized menus are unsupported"));
        }
        let pane = Rect::new(left, top, left + width + 2, top + height + 2);
        let shadow = Rect::new(pane.left + 2, pane.top + 2, pane.right + 2, pane.bottom + 2);
        let mut paints = vec![
            Paint::invert(title.highlight(), bitmap.bounds),
            Paint::fill(shadow, bitmap.bounds, fg),
            Paint::fill(pane, bitmap.bounds, fg),
            Paint::fill(
                Rect::new(pane.left + 1, pane.top + 1, pane.right - 1, pane.bottom - 1),
                bitmap.bounds,
                bg,
            ),
        ];
        let mut y = top + 1;
        let mut hover = 0u16;
        let mut selected = 0u16;
        for (index, (item, face)) in menu.items.iter().zip(&fonts).enumerate() {
            let row_height = if item.text == b"-" { 6 } else { 16 };
            let row = Rect::new(left + 1, y, pane.right - 1, y + row_height);
            y += row_height;
            if item.text == b"-" {
                let mut line = Paint::fill(
                    Rect::new(row.left + 1, row.top + 2, row.right - 1, row.top + 3),
                    row,
                    fg,
                );
                line.pattern = DIM;
                line.background = bg;
                paints.push(line);
                continue;
            }
            let enabled =
                menu.enabled & 1 != 0 && (index >= 31 || menu.enabled & (1u32 << (index + 1)) != 0);
            let hit = inside(row, point);
            if hit {
                hover = (index + 1) as u16;
                if enabled {
                    selected = hover;
                }
            }
            let hilite = hit && enabled;
            if hilite {
                paints.push(Paint::fill(row, bitmap.bounds, fg));
            }
            let strike = crate::font::Strike::parse(face)?;
            let baseline = row.top
                + (16 - i32::from(strike.metrics.ascent) - i32::from(strike.metrics.descent)) / 2
                + i32::from(strike.metrics.ascent);
            let mut runs = vec![(item.text.to_vec(), row.left + 15)];
            if item.mark != 0 {
                runs.push((vec![item.mark], row.left + 2));
            }
            if item.command > 0x20 {
                runs.push((vec![0x11, item.command], row.right - 28));
            }
            for (text, x) in runs {
                let (plans, _) = crate::text::plan_strike(
                    &strike,
                    &text,
                    i16::try_from(x).map_err(|_| bad("menu text coordinate overflow"))?,
                    i16::try_from(baseline).map_err(|_| bad("menu baseline overflow"))?,
                )?;
                let ink: Vec<_> = plans.into_iter().flat_map(|(_, ink, _)| ink).collect();
                if !ink.is_empty() {
                    paints.push(Paint {
                        rect: row,
                        clip: row,
                        ink: Some(ink),
                        pattern: if enabled { [255; 8] } else { DIM },
                        foreground: if hilite { bg } else { fg },
                        background: if hilite { fg } else { bg },
                        xor: false,
                    });
                }
            }
        }
        let packed = |item: u16| {
            if item == 0 {
                0
            } else {
                (u32::from(title.id as u16) << 16) | u32::from(item)
            }
        };
        key.extend(selected.to_be_bytes());
        key.extend(hover.to_be_bytes());
        Ok(Plan {
            paints,
            damage: Some(union(title.rect, shadow)),
            key,
            hover: packed(hover),
            selected: packed(selected),
            id: Some(title.id),
        })
    }
    pub fn poll_tracking(
        &self,
        m: &mut Memory,
        p: &mut Process,
        gpu: &Graphics,
        t: &mut Tracking,
        point: [i16; 2],
        released: bool,
    ) -> Result<Option<u32>> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            let bitmap = Bitmap::read(
                m,
                p.ports
                    .ok_or_else(|| bad("menu tracking lost InitGraf"))?
                    .screen_bitmap,
            )?;
            if bitmap.base != t.bitmap.base
                || bitmap.row_bytes != t.bitmap.row_bytes
                || bitmap.bounds != t.bitmap.bounds
            {
                return Err(bad("screen bitmap changed during menu tracking"));
            }
            let plan = self.tracking_plan(m, p, t.active, point)?;
            m.validate_write(THE_MENU, 2)?;
            m.validate_write(CHOICE, 4)?;
            let (_, h) = bitmap.extent()?;
            m.validate_write(bitmap.base, bitmap.row_bytes * h)?;
            if released {
                t.restore(m, gpu)?;
                self.hilite(
                    m,
                    p,
                    gpu,
                    if plan.selected != 0 {
                        plan.id.unwrap_or(0)
                    } else {
                        0
                    },
                )?;
                m.write(CHOICE, &plan.hover.to_be_bytes())?;
                return Ok(Some(plan.selected));
            }
            if t.last.as_ref() != Some(&plan.key) {
                t.restore(m, gpu)?;
                for paint in &plan.paints {
                    paint.draw(bitmap, m, gpu)?;
                }
                t.damage = plan.damage;
                t.last = Some(plan.key);
            }
            t.active = plan.id;
            m.write(THE_MENU, &plan.id.unwrap_or(0).to_be_bytes())?;
            m.write(CHOICE, &plan.hover.to_be_bytes())?;
            Ok(None)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
}
impl Tracking {
    fn restore(&mut self, m: &mut Memory, gpu: &Graphics) -> Result<()> {
        if let Some(clip) = self.damage {
            self.bitmap.execute_source(
                gpu,
                m,
                Draw::Copy {
                    rect: self.bitmap.bounds,
                    clip,
                    source_x: self.bitmap.bounds.left,
                    source_y: self.bitmap.bounds.top,
                    mode: Transfer::Copy,
                },
                None,
                Some(&self.snapshot),
            )?;
            self.damage = None;
        }
        Ok(())
    }
}
