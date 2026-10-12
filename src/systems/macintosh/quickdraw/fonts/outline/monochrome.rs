//! Monochrome scan conversion of already-hinted source outlines.
//! No font lookup, shaping, advance or layout decisions happen here.
use super::OutlinePath;
use std::ffi::{c_char, c_long, c_void};

#[repr(C)]
struct Point {
    x: c_long,
    y: c_long,
}

extern "C" {
    fn systemless_mono_new() -> *mut c_void;
    fn systemless_mono_done(raster: *mut c_void);
    fn systemless_mono_render(
        raster: *mut c_void,
        points: *mut Point,
        tags: *mut c_char,
        contours: *mut i16,
        point_count: i16,
        contour_count: i16,
        width: i32,
        height: i32,
        buffer: *mut u8,
    ) -> i32;
}

pub(super) struct Rasterizer(*mut c_void);

pub(super) fn pixels(
    raster: Option<&Rasterizer>,
    path: &OutlinePath,
    placement: &zeno::Placement,
    coverage: &[u8],
) -> Vec<u8> {
    // Conversion failure retains the same source and metrics; it must not
    // reject the face and authorize selecting a different font family.
    raster
        .and_then(|raster| raster.render(path, placement))
        .unwrap_or_else(|| {
            coverage
                .iter()
                .map(|&alpha| {
                    if alpha >= super::MONO_COVERAGE_THRESHOLD {
                        255
                    } else {
                        0
                    }
                })
                .collect()
        })
}

impl Rasterizer {
    pub(super) fn new() -> Option<Self> {
        // SAFETY: The adapter allocates an independently owned raster context.
        let raster = unsafe { systemless_mono_new() };
        if raster.is_null() {
            None
        } else {
            Some(Self(raster))
        }
    }

    pub(super) fn render(
        &self,
        path: &OutlinePath,
        placement: &zeno::Placement,
    ) -> Option<Vec<u8>> {
        let mut points = Vec::new();
        let mut tags = Vec::new();
        let mut contours = Vec::new();
        let offset_x = placement.left as f32;
        let offset_y = placement.top as f32 - placement.height as f32;
        let vector = |p: zeno::Point| Point {
            x: ((p.x - offset_x) * 64.).round() as c_long,
            y: ((p.y - offset_y) * 64.).round() as c_long,
        };
        let mut open = false;
        for command in &path.0 {
            match *command {
                zeno::Command::MoveTo(p) => {
                    if open {
                        contours.push(i16::try_from(points.len().checked_sub(1)?).ok()?);
                    }
                    points.push(vector(p));
                    tags.push(1);
                    open = true;
                }
                zeno::Command::LineTo(p) => {
                    points.push(vector(p));
                    tags.push(1);
                }
                zeno::Command::QuadTo(control, end) => {
                    points.extend([vector(control), vector(end)]);
                    tags.extend([0, 1]);
                }
                zeno::Command::CurveTo(first, second, end) => {
                    points.extend([vector(first), vector(second), vector(end)]);
                    tags.extend([2, 2, 1]);
                }
                zeno::Command::Close => {
                    if open {
                        contours.push(i16::try_from(points.len().checked_sub(1)?).ok()?);
                        open = false;
                    }
                }
            }
        }
        if open {
            contours.push(i16::try_from(points.len().checked_sub(1)?).ok()?);
        }
        let width = usize::try_from(placement.width).ok()?;
        let height = usize::try_from(placement.height).ok()?;
        if width == 0 || height == 0 {
            return Some(Vec::new());
        }
        let pitch = width.checked_add(7)? / 8;
        let mut packed = vec![0; pitch.checked_mul(height)?];
        // SAFETY: The adapter validates contours before conversion. All
        // arrays remain allocated for this synchronous call; counts fit the
        // ABI and the buffer contains pitch * rows bytes. No pointers escape.
        let status = unsafe {
            systemless_mono_render(
                self.0,
                points.as_mut_ptr(),
                tags.as_mut_ptr(),
                contours.as_mut_ptr(),
                i16::try_from(points.len()).ok()?,
                i16::try_from(contours.len()).ok()?,
                i32::try_from(width).ok()?,
                i32::try_from(height).ok()?,
                packed.as_mut_ptr(),
            )
        };
        if status != 0 {
            return None;
        }
        let mut pixels = Vec::with_capacity(width.checked_mul(height)?);
        for row in 0..height {
            for column in 0..width {
                pixels.push(
                    if packed[row * pitch + column / 8] & (0x80 >> (column % 8)) != 0 {
                        255
                    } else {
                        0
                    },
                );
            }
        }
        Some(pixels)
    }
}

impl Drop for Rasterizer {
    fn drop(&mut self) {
        // SAFETY: This context is uniquely owned and no operation outlives it.
        unsafe {
            systemless_mono_done(self.0);
        }
    }
}
