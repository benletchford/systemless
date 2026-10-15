//! Standard control geometry changes share owner drawing and invalidation.
use super::{fail, title, Controls};
use crate::{
    bitmap::Bitmap,
    graphics::{Graphics, Rect},
    memory::{GuestPtr, Memory},
    process::Process,
    quickdraw::read_rect,
    region_geometry, Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Geometry {
    Move { h: i16, v: i16 },
    Size { width: i16, height: i16 },
}
struct ClipSizes {
    handle: u32,
    old: u32,
    clipped: u32,
}
impl Controls {
    fn paint_requirements(
        &self,
        m: &mut Memory,
        p: &mut Process,
        gpu: Option<&Graphics>,
        h: u32,
        r: Rect,
    ) -> Result<Option<ClipSizes>> {
        let record = self.record(m, p, h)?;
        let (owner, kind) = self.records[&h];
        let vis = m.u32(GuestPtr(owner + 24))?;
        let vis_bytes = crate::regions::read(m, p, vis)?;
        if region_geometry::decode(&vis_bytes)?.is_empty() {
            return Ok(None);
        }
        gpu.ok_or_else(|| fail("visible control geometry requires a GPU"))?;
        let ports = p
            .ports
            .ok_or_else(|| fail("control geometry drawing requires InitGraf"))?;
        let port = m.read(GuestPtr(owner), 108)?;
        if [92, 96, 100, 104]
            .iter()
            .any(|&at| port[at..at + 4] != [0; 4])
        {
            return Err(fail(
                "control geometry with recording/custom procedures is unsupported",
            ));
        }
        for at in [80, 84] {
            if !matches!(m.u32(GuestPtr(owner + at))?, 30 | 33) {
                return Err(fail("unsupported owner monochrome color"));
            }
        }
        m.validate_write(GuestPtr(owner), 108)?;
        m.validate_write(ports.current_slot, 4)?;
        let bitmap = Bitmap::read(m, GuestPtr(owner + 2))?;
        let (_, height) = bitmap.extent()?;
        m.validate_write(bitmap.base, bitmap.row_bytes * height)?;
        let clip = m.u32(GuestPtr(owner + 28))?;
        let bytes = crate::regions::read(m, p, clip)?;
        let mut rectangles = region_geometry::decode(&bytes)?;
        for c in &mut rectangles {
            c.left = c.left.max(r.left);
            c.top = c.top.max(r.top);
            c.right = c.right.min(r.right);
            c.bottom = c.bottom.min(r.bottom);
        }
        rectangles.retain(|c| c.left < c.right && c.top < c.bottom);
        let clipped = region_geometry::encode(&rectangles)?;
        let hilite = m.read(GuestPtr(record.0 + 17), 1)?[0];
        if !matches!(hilite, 0 | 10 | 11 | 255) {
            return Err(fail("unsupported control highlight part"));
        }
        let label = title(m, GuestPtr(record.0 + 40))?;
        let data = crate::text::font_bytes(m, p, 0, 12, 0)?;
        let font = crate::font::Strike::parse(&data)?;
        let lines: Vec<_> = label[1..].split(|&c| c == 13).collect();
        let height = i32::from(font.metrics.ascent + font.metrics.descent);
        let step = height + i32::from(font.metrics.leading);
        let block = height + step * (lines.len() as i32 - 1);
        let mut baseline = r.top + (r.bottom - r.top - block) / 2 + i32::from(font.metrics.ascent);
        for line in lines {
            let x = if kind == 0 {
                r.left + (r.right - r.left - font.width(line)?) / 2
            } else {
                r.left + 12.min(r.bottom - r.top) + 6
            };
            crate::text::plan_strike(
                &font,
                line,
                i16::try_from(x).map_err(|_| fail("control title overflow"))?,
                i16::try_from(baseline).map_err(|_| fail("control baseline overflow"))?,
            )?;
            baseline += step;
        }
        Ok(Some(ClipSizes {
            handle: clip,
            old: bytes.len() as u32,
            clipped: clipped.len() as u32,
        }))
    }
    pub(super) fn change_geometry(
        &self,
        m: &mut Memory,
        p: &mut Process,
        gpu: Option<&Graphics>,
        h: u32,
        change: Geometry,
    ) -> Result<u32> {
        let errors = (p.mem_error, p.res_error);
        let result = (|| {
            if h == 0 {
                return Ok(0);
            }
            let record = self.record(m, p, h)?;
            let owner = self.records[&h].0;
            if !p.windows.contains(m, owner)? || !self.list(m, p, owner)?.contains(&h) {
                return Err(fail("geometry control is not in its live owner list"));
            }
            let old = read_rect(m, GuestPtr(record.0 + 8))?;
            if old.left >= old.right || old.top >= old.bottom {
                return Err(fail("invalid live control rectangle"));
            }
            let new = match change {
                Geometry::Move { h, v } => Rect::new(
                    i32::from(h),
                    i32::from(v),
                    i32::from(h) + old.right - old.left,
                    i32::from(v) + old.bottom - old.top,
                ),
                Geometry::Size { width, height } => {
                    if width <= 0 || height <= 0 {
                        return Err(fail("nonpositive control sizes are unsupported"));
                    }
                    Rect::new(
                        old.left,
                        old.top,
                        old.left + i32::from(width),
                        old.top + i32::from(height),
                    )
                }
            };
            let mut bytes = Vec::with_capacity(8);
            for edge in [new.top, new.left, new.bottom, new.right] {
                bytes.extend(
                    i16::try_from(edge)
                        .map_err(|_| fail("control geometry coordinate overflow"))?
                        .to_be_bytes(),
                );
            }
            m.validate_write(GuestPtr(record.0 + 8), 8)?;
            if m.read(GuestPtr(record.0 + 16), 1)?[0] == 0 {
                m.write(GuestPtr(record.0 + 8), &bytes)?;
                return Ok(0);
            }
            let old_paint = self.paint_requirements(m, p, gpu, h, old)?;
            let new_paint = self.paint_requirements(m, p, gpu, h, new)?;
            let windows = std::mem::take(&mut p.windows);
            let invalidation = windows.invalidation(m, p, owner, old, false);
            p.windows = windows;
            let (update, dirty) = invalidation?;
            let vis = m.u32(GuestPtr(owner + 24))?;
            let clip = m.u32(GuestPtr(owner + 28))?;
            if update == h || update == clip || update == vis {
                return Err(fail(
                    "control geometry update/visibility/clip alias is unsupported",
                ));
            }
            let mut sizes = vec![(update, dirty.len() as u32)];
            for plan in [&old_paint, &new_paint].into_iter().flatten() {
                sizes.push((plan.handle, plan.clipped));
                sizes.push((plan.handle, plan.old));
            }
            if !p.can_resize_handles(m, &sizes)? {
                return Err(fail("control geometry region allocation failed"));
            }
            // Allocation and all supported drawing inputs are checked before
            // publication; the remaining guest operations follow that sequence.
            p.replace_handles(m, vec![(update, dirty)])?;
            if p.mem_error != 0 {
                return Err(fail("control geometry invalidation allocation failed"));
            }
            self.draw_at(m, p, gpu, h, old, true)?;
            self.draw_at(m, p, gpu, h, new, false)?;
            m.write(GuestPtr(record.0 + 8), &bytes)?;
            Ok(0)
        })();
        (p.mem_error, p.res_error) = errors;
        result
    }
}
