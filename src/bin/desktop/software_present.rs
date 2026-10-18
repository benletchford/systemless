//! CPU presentation used explicitly while qualifying the future fallback.
use super::raster_layout::RasterLayout;

/// Resolve a cropped, row-strided ARGB image into an RGB software surface.
fn write_rgb(
    pixels: &[u32],
    layout: RasterLayout,
    target: (u32, u32),
    output: &mut [u32],
    cropped: &mut Vec<u32>,
    scaled: &mut Vec<u32>,
) -> Result<(), String> {
    let start = layout
        .texel_byte_offset(pixels.len().checked_mul(4).ok_or("image too large")?)
        .ok_or("framebuffer dimensions do not match its pixel data")?
        / 4;
    let expected = u64::from(target.0)
        .checked_mul(u64::from(target.1))
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("surface too large")?;
    if target.0 == 0 || target.1 == 0 || output.len() != expected {
        return Err("software surface dimensions do not match its pixels".into());
    }
    let source = if layout.buffer_width == layout.width {
        &pixels[start..start + layout.width as usize * layout.height as usize]
    } else {
        cropped.clear();
        for row in 0..layout.height as usize {
            let offset = start + row * layout.buffer_width as usize;
            cropped.extend_from_slice(&pixels[offset..offset + layout.width as usize]);
        }
        cropped.as_slice()
    };
    let (_, _, width, height) =
        super::aspect_fit_dimensions(layout.width, layout.height, target.0, target.1);
    // A nonzero drawable still needs one output pixel when aspect-fit rounds down.
    let (width, height) = (width.max(1), height.max(1));
    let (x, y) = ((target.0 - width) / 2, (target.1 - height) / 2);
    let visible = if (width, height) == (layout.width, layout.height) {
        source
    } else {
        systemless::systems::macintosh::display::resize_argb_coverage(
            source,
            (layout.width, layout.height),
            (width, height),
            scaled,
        );
        scaled.as_slice()
    };
    output.fill(0);
    for row in 0..height as usize {
        let offset = (y as usize + row) * target.0 as usize + x as usize;
        for (dst, src) in output[offset..offset + width as usize]
            .iter_mut()
            .zip(&visible[row * width as usize..(row + 1) * width as usize])
        {
            *dst = src & 0x00ff_ffff;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) struct SoftwarePresenter {
    surface:
        softbuffer::Surface<std::rc::Rc<winit::window::Window>, std::rc::Rc<winit::window::Window>>,
    size: Option<(u32, u32)>,
    cropped: Vec<u32>,
    scaled: Vec<u32>,
}

#[cfg(target_os = "macos")]
impl SoftwarePresenter {
    pub(super) fn new(window: std::rc::Rc<winit::window::Window>) -> Result<Self, String> {
        let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
        let surface = softbuffer::Surface::new(&context, window).map_err(|e| e.to_string())?;
        Ok(Self {
            surface,
            size: None,
            cropped: Vec::new(),
            scaled: Vec::new(),
        })
    }
    pub(super) fn present(
        &mut self,
        pixels: &[u32],
        layout: RasterLayout,
        target: (u32, u32),
    ) -> Result<(), String> {
        let (Some(width), Some(height)) = (
            std::num::NonZeroU32::new(target.0),
            std::num::NonZeroU32::new(target.1),
        ) else {
            return Ok(());
        };
        if self.size != Some(target) {
            self.surface
                .resize(width, height)
                .map_err(|e| e.to_string())?;
            self.size = Some(target);
        }
        let mut output = self.surface.buffer_mut().map_err(|e| e.to_string())?;
        write_rgb(
            pixels,
            layout,
            target,
            &mut output,
            &mut self.cropped,
            &mut self.scaled,
        )?;
        output.present().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn software_crop_stride_and_rgb_match_the_selected_pixels() {
        let pixels = [
            0xff000001, 0xff000002, 0xff000003, 0xff000004, 0xff000005, 0xff000006,
        ];
        let mut output = [0; 2];
        write_rgb(
            &pixels,
            RasterLayout {
                left: 1,
                top: 0,
                width: 1,
                height: 2,
                buffer_width: 3,
            },
            (1, 2),
            &mut output,
            &mut Vec::new(),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(output, [2, 5]);
    }
    #[test]
    fn software_letterbox_and_downsample_preserve_coverage() {
        let pixels = [0xff000000, 0xffffffff];
        let mut output = [0; 4];
        write_rgb(
            &pixels,
            RasterLayout::whole((2, 1)),
            (2, 2),
            &mut output,
            &mut Vec::new(),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(output, [0, 0xffffff, 0, 0]);
        let mut one = [0];
        write_rgb(
            &pixels,
            RasterLayout::whole((2, 1)),
            (1, 1),
            &mut one,
            &mut Vec::new(),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(one, [0x808080]);
    }
    #[test]
    fn software_invalid_views_fail_before_touching_output() {
        let mut output = [42];
        let layout = RasterLayout {
            left: 1,
            top: 0,
            width: 1,
            height: 1,
            buffer_width: 1,
        };
        assert!(write_rgb(
            &[0],
            layout,
            (1, 1),
            &mut output,
            &mut Vec::new(),
            &mut Vec::new()
        )
        .is_err());
        assert_eq!(output, [42]);
    }
}
