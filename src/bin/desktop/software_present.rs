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
use objc2::{msg_send, msg_send_id, rc::Retained, runtime::NSObject, ClassType};
#[cfg(target_os = "macos")]
use objc2_quartz_core::{CALayer, CAMetalLayer};

#[cfg(target_os = "macos")]
fn appkit_view(window: &winit::window::Window) -> Result<Retained<NSObject>, String> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let _main = objc2_foundation::MainThreadMarker::new()
        .ok_or("AppKit presentation requires the main thread")?;
    let handle = window.window_handle().map_err(|e| e.to_string())?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("software presenter requires an AppKit view".into());
    };
    // SAFETY: winit's borrowed handle owns a valid NSView. Retaining it covers
    // this main-thread operation; callers also keep the Window alive.
    unsafe { Retained::retain(handle.ns_view.cast::<NSObject>().as_ptr()) }
        .ok_or_else(|| "AppKit view unavailable".into())
}
#[cfg(target_os = "macos")]
fn view_layer(view: &NSObject) -> Option<Retained<CALayer>> {
    // SAFETY: appkit_view validated the main-thread NSView handle.
    unsafe { msg_send_id![view, layer] }
}
#[cfg(target_os = "macos")]
fn child_layers(root: &CALayer) -> Vec<Retained<CALayer>> {
    // SAFETY: these retained Core Animation layers are inspected on their UI owner.
    unsafe { root.sublayers() }.map_or_else(Vec::new, |layers| layers.to_vec_retained())
}
#[cfg(target_os = "macos")]
fn is_metal(layer: &CALayer) -> bool {
    // SAFETY: the receiver is a valid CALayer and this is NSObject's class query.
    unsafe { msg_send![layer, isKindOfClass: CAMetalLayer::class()] }
}
#[cfg(target_os = "macos")]
fn detach_metal_children(root: &CALayer) {
    // Snapshot before mutation: the parent owns its sublayers too.
    for layer in child_layers(root) {
        if is_metal(&layer) {
            // SAFETY: only obsolete renderer children of our owned view are removed.
            layer.removeFromSuperlayer();
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) struct SoftwarePresenter {
    surface:
        softbuffer::Surface<std::rc::Rc<winit::window::Window>, std::rc::Rc<winit::window::Window>>,
    size: Option<(u32, u32)>,
    owned_layers: Vec<Retained<CALayer>>,
    cropped: Vec<u32>,
    scaled: Vec<u32>,
}

#[cfg(target_os = "macos")]
impl SoftwarePresenter {
    pub(super) fn new(window: std::rc::Rc<winit::window::Window>) -> Result<Self, String> {
        let view = appkit_view(&window)?;
        // SAFETY: this is the owned NSView, accessed on the main thread.
        let _: () = unsafe { msg_send![&*view, setWantsLayer: true] };
        let root = view_layer(&view).ok_or("AppKit root layer unavailable")?;
        if is_metal(&root) {
            return Err("software presentation requires an AppKit-managed root layer".into());
        }
        detach_metal_children(&root);
        let before = child_layers(&root);
        let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
        let surface = softbuffer::Surface::new(&context, window).map_err(|e| e.to_string())?;
        let owned_layers = child_layers(&root)
            .into_iter()
            .filter(|layer| {
                !before
                    .iter()
                    .any(|old| Retained::as_ptr(old) == Retained::as_ptr(layer))
            })
            .collect();
        Ok(Self {
            surface,
            size: None,
            owned_layers,
            cropped: Vec::new(),
            scaled: Vec::new(),
        })
    }
    #[cfg(feature = "test-support")]
    pub(super) fn layer_snapshot(&self) -> serde_json::Value {
        let root = appkit_view(self.surface.window())
            .ok()
            .and_then(|view| view_layer(&view));
        let Some(root) = root else {
            return serde_json::json!({"published": false, "error": "root unavailable"});
        };
        let children = child_layers(&root);
        let metal_layers =
            usize::from(is_metal(&root)) + children.iter().filter(|layer| is_metal(layer)).count();
        // SAFETY: contents/superlayer are inspected on the same main-thread UI owner.
        let image_layers = children
            .iter()
            .filter(|layer| !is_metal(layer) && unsafe { layer.contents() }.is_some())
            .count();
        let published = self.owned_layers.iter().any(|layer| unsafe {
            layer
                .superlayer()
                .is_some_and(|parent| Retained::as_ptr(&parent) == Retained::as_ptr(&root))
                && layer.contents().is_some()
        });
        serde_json::json!({"metal_layers": metal_layers, "image_layers": image_layers, "owned_layers": self.owned_layers.len(), "published": published})
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

#[cfg(target_os = "macos")]
impl Drop for SoftwarePresenter {
    fn drop(&mut self) {
        // softbuffer unregisters observers when its surface drops, but its CPU
        // layer remains parent-owned. Detach only the layers this instance added.
        for layer in &self.owned_layers {
            // Rc<Window> keeps all UI ownership on this main thread.
            layer.removeFromSuperlayer();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    #[test]
    fn software_layer_cleanup_keeps_cpu_children_and_detaches_metal() {
        objc2::rc::autoreleasepool(|_| {
            // Core Animation objects are independent of AppKit views here.
            let root = CALayer::new();
            let cpu = CALayer::new();
            let gpu = unsafe { CAMetalLayer::new() };
            root.addSublayer(&cpu);
            root.addSublayer(&gpu);
            detach_metal_children(&root);
            let children = child_layers(&root);
            assert_eq!(children.len(), 1);
            assert_eq!(Retained::as_ptr(&children[0]), Retained::as_ptr(&cpu));
            assert!(unsafe { gpu.superlayer() }.is_none());
        });
    }
    #[test]
    fn software_crop_stride_and_rgb_match_the_selected_pixels() {
        let pixels = [
            0xff000001, 0xff000002, 0xff000003, 0xff000004, 0xff000005, 0xff000006, 0xff000007,
            0xff000008, 0xff000009,
        ];
        let mut output = [0; 2];
        write_rgb(
            &pixels,
            RasterLayout {
                left: 1,
                top: 1,
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
        assert_eq!(output, [5, 8]);
        let mut row = [0; 3];
        write_rgb(
            &pixels,
            RasterLayout {
                left: 0,
                top: 1,
                width: 3,
                height: 1,
                buffer_width: 3,
            },
            (3, 1),
            &mut row,
            &mut Vec::new(),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(row, [4, 5, 6]);
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
