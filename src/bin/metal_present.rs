//! Native macOS framebuffer presentation.
//!
//! `softbuffer`'s AppKit backend creates a new `CGImage` for every present.
//! Core Animation then copies and color-converts that image on the CPU.  A
//! continuously animating 800x600 game can spend most of a host core in that
//! conversion path.  This presenter keeps a `CAMetalLayer`, command queue,
//! render pipeline, and two upload textures alive for the window lifetime.
//! Each frame is one CPU-to-texture upload and a four-vertex nearest-neighbor
//! GPU draw.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, ProtocolObject};
use objc2::{msg_send, msg_send_id};
use objc2_foundation::{ns_string, CGRect, MainThreadMarker};
use objc2_metal::{
    MTLBuffer, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLCreateSystemDefaultDevice,
    MTLDevice, MTLLibrary, MTLLoadAction, MTLPixelFormat, MTLPrimitiveType, MTLRegion,
    MTLRenderCommandEncoder, MTLRenderPassDescriptor, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState, MTLResourceOptions, MTLStorageMode, MTLStoreAction, MTLTexture,
    MTLTextureDescriptor, MTLTextureUsage, MTLViewport,
};
use objc2_quartz_core::{CAAutoresizingMask, CALayer, CAMetalDrawable, CAMetalLayer};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use systemless::display::CursorImage;

// Double buffering prevents the CPU from overwriting the texture used by the
// in-flight GPU frame without allowing a third drawable to queue and add input
// latency. The 800x600 upload is far below the GPU throughput limit.
const FRAME_RESOURCE_COUNT: usize = 2;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GuestFrameUniforms {
    row_bytes: u32,
    width: u32,
    height: u32,
    pixel_size: u32,
    content_left: u32,
    content_top: u32,
    cursor_kind: u32,
    cursor_width: u32,
    cursor_height: u32,
    cursor_left: i32,
    cursor_top: i32,
}

#[repr(C)]
#[derive(Clone)]
struct GuestCursorData {
    data_rows: [u32; 16],
    mask_rows: [u32; 16],
    color_pixels: [u32; 256],
}

impl Default for GuestCursorData {
    fn default() -> Self {
        Self {
            data_rows: [0; 16],
            mask_rows: [0; 16],
            color_pixels: [0; 256],
        }
    }
}

pub struct MetalPresenter {
    _window: Rc<Window>,
    root_layer: Retained<CALayer>,
    layer: Retained<CAMetalLayer>,
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    guest_pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    guest_buffers: Vec<Retained<ProtocolObject<dyn MTLBuffer>>>,
    guest_buffer_size: usize,
    next_guest_buffer: usize,
    upload_textures: Vec<Retained<ProtocolObject<dyn MTLTexture>>>,
    upload_size: (u32, u32),
    drawable_size: (u32, u32),
    next_upload_texture: usize,
}

impl MetalPresenter {
    pub fn new(window: Rc<Window>) -> Result<Self, String> {
        MainThreadMarker::new()
            .ok_or_else(|| "Metal presenter must be created on the main thread".to_string())?;

        let handle = window
            .window_handle()
            .map_err(|error| format!("failed to get AppKit window handle: {error}"))?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err("Metal presenter requires an AppKit window".to_string());
        };

        // SAFETY: winit owns this NSView for at least as long as `window`,
        // which the presenter retains in `_window`. AppKit view/layer access
        // occurs on the main thread.
        let view: &NSObject = unsafe { handle.ns_view.cast().as_ref() };
        let _: () = unsafe { msg_send![view, setWantsLayer: true] };
        let root_layer: Option<Retained<CALayer>> = unsafe { msg_send_id![view, layer] };
        let root_layer =
            root_layer.ok_or_else(|| "AppKit did not create a root layer".to_string())?;

        // SAFETY: Metal returns a retained Objective-C protocol object or nil.
        let device = unsafe { Retained::retain(MTLCreateSystemDefaultDevice()) }
            .ok_or_else(|| "this Mac has no Metal device".to_string())?;
        let command_queue = device
            .newCommandQueue()
            .ok_or_else(|| "Metal failed to create a command queue".to_string())?;

        let layer = unsafe { CAMetalLayer::new() };
        unsafe {
            layer.setDevice(Some(&device));
            layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
            layer.setFramebufferOnly(true);
            layer.setMaximumDrawableCount(FRAME_RESOURCE_COUNT);
            layer.setPresentsWithTransaction(false);
            layer.setDisplaySyncEnabled(true);
        }
        layer.setOpaque(true);
        layer.setFrame(root_layer.bounds());
        layer.setAutoresizingMask(
            CAAutoresizingMask::kCALayerWidthSizable | CAAutoresizingMask::kCALayerHeightSizable,
        );
        layer.setContentsScale(root_layer.contentsScale());
        root_layer.addSublayer(&layer);

        let library = device
            .newLibraryWithSource_options_error(
                ns_string!(include_str!("metal_present.metal")),
                None,
            )
            .map_err(|error| format!("Metal shader compilation failed: {error}"))?;
        let vertex = library
            .newFunctionWithName(ns_string!("raster_vertex"))
            .ok_or_else(|| "Metal vertex function was not found".to_string())?;
        let fragment = library
            .newFunctionWithName(ns_string!("raster_fragment"))
            .ok_or_else(|| "Metal fragment function was not found".to_string())?;
        let guest_fragment = library
            .newFunctionWithName(ns_string!("guest_raster_fragment"))
            .ok_or_else(|| "Metal guest framebuffer function was not found".to_string())?;

        let descriptor = MTLRenderPipelineDescriptor::new();
        descriptor.setVertexFunction(Some(&vertex));
        descriptor.setFragmentFunction(Some(&fragment));
        unsafe {
            descriptor
                .colorAttachments()
                .objectAtIndexedSubscript(0)
                .setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        }
        let pipeline = device
            .newRenderPipelineStateWithDescriptor_error(&descriptor)
            .map_err(|error| format!("Metal pipeline creation failed: {error}"))?;

        descriptor.setFragmentFunction(Some(&guest_fragment));
        let guest_pipeline = device
            .newRenderPipelineStateWithDescriptor_error(&descriptor)
            .map_err(|error| format!("Metal guest pipeline creation failed: {error}"))?;

        Ok(Self {
            _window: window,
            root_layer,
            layer,
            device,
            command_queue,
            pipeline,
            guest_pipeline,
            guest_buffers: Vec::new(),
            guest_buffer_size: 0,
            next_guest_buffer: 0,
            upload_textures: Vec::new(),
            upload_size: (0, 0),
            drawable_size: (0, 0),
            next_upload_texture: 0,
        })
    }

    pub fn present(
        &mut self,
        pixels: &[u32],
        source_width: u32,
        source_height: u32,
        drawable_width: u32,
        drawable_height: u32,
    ) -> Result<(), String> {
        let expected_pixels = source_width as usize * source_height as usize;
        if pixels.len() < expected_pixels || expected_pixels == 0 {
            return Err("framebuffer dimensions do not match its pixel data".to_string());
        }
        if drawable_width == 0 || drawable_height == 0 {
            return Ok(());
        }

        self.ensure_upload_textures(source_width, source_height)?;
        self.resize_drawable(drawable_width, drawable_height);

        // Acquire the drawable before recycling the corresponding upload
        // texture. With a two-drawable layer, nextDrawable blocks until the
        // oldest in-flight frame is complete, making the matching texture safe
        // for the CPU to overwrite without adding a third queued frame.
        let Some(drawable) = (unsafe { self.layer.nextDrawable() }) else {
            return Ok(());
        };
        let upload_index = self.next_upload_texture;
        self.next_upload_texture = (upload_index + 1) % self.upload_textures.len();
        let upload = &self.upload_textures[upload_index];
        let region = MTLRegion {
            origin: objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
            size: objc2_metal::MTLSize {
                width: source_width as usize,
                height: source_height as usize,
                depth: 1,
            },
        };
        let pixel_bytes = NonNull::new(pixels.as_ptr().cast_mut().cast::<c_void>())
            .expect("a non-empty framebuffer has a non-null pointer");
        unsafe {
            upload.replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                region,
                0,
                pixel_bytes,
                source_width as usize * size_of::<u32>(),
            );
        }

        let drawable_texture = unsafe { drawable.texture() };
        let pass = unsafe { MTLRenderPassDescriptor::new() };
        let color = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
        color.setTexture(Some(&drawable_texture));
        color.setLoadAction(MTLLoadAction::Clear);
        color.setStoreAction(MTLStoreAction::Store);
        color.setClearColor(objc2_metal::MTLClearColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        });

        let command_buffer = self
            .command_queue
            .commandBuffer()
            .ok_or_else(|| "Metal failed to create a command buffer".to_string())?;
        let encoder = command_buffer
            .renderCommandEncoderWithDescriptor(&pass)
            .ok_or_else(|| "Metal failed to create a render encoder".to_string())?;
        encoder.setRenderPipelineState(&self.pipeline);
        unsafe { encoder.setFragmentTexture_atIndex(Some(upload), 0) };

        let viewport =
            aspect_fit_viewport(source_width, source_height, drawable_width, drawable_height);
        encoder.setViewport(MTLViewport {
            originX: viewport.0,
            originY: viewport.1,
            width: viewport.2,
            height: viewport.3,
            znear: 0.0,
            zfar: 1.0,
        });
        unsafe {
            encoder.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::TriangleStrip, 0, 4)
        };
        encoder.endEncoding();

        command_buffer.presentDrawable(ProtocolObject::from_ref(&*drawable));
        command_buffer.commit();
        Ok(())
    }

    /// Snapshot and present a native Classic Macintosh framebuffer. The GPU
    /// expands indexed/monochrome pixels and composites the guest cursor, so
    /// the CPU copies only the packed source bytes. Two staging buffers keep
    /// each submitted frame immutable until Metal has consumed it.
    pub fn present_guest_frame(
        &mut self,
        framebuffer: &[u8],
        screen_mode: (u32, u32, u16, u16, u16),
        content_rect: (u32, u32, u32, u32),
        palette: &[u32; 256],
        cursor: Option<(&CursorImage, (i16, i16))>,
        drawable_size: (u32, u32),
    ) -> Result<bool, String> {
        let (_, row_bytes, width, height, pixel_size) = screen_mode;
        let screen_width = u32::from(width);
        let screen_height = u32::from(height);
        let (content_left, content_top, width, height) = content_rect;
        let (drawable_width, drawable_height) = drawable_size;
        if !matches!(pixel_size, 1 | 8) {
            return Ok(false);
        }
        if content_left.saturating_add(width) > screen_width
            || content_top.saturating_add(height) > screen_height
        {
            return Err("detected content rectangle lies outside the guest screen".to_string());
        }
        if width == 0 || height == 0 || drawable_width == 0 || drawable_height == 0 {
            return Ok(true);
        }
        let frame_bytes = guest_frame_byte_len(row_bytes, screen_height)
            .ok_or_else(|| "guest framebuffer dimensions overflow host size".to_string())?;
        if framebuffer.len() < frame_bytes {
            return Err("guest framebuffer dimensions do not match its pixel data".to_string());
        }

        self.ensure_guest_buffers(frame_bytes)?;
        self.resize_drawable(drawable_width, drawable_height);

        // Acquiring the next drawable bounds the command queue to two frames.
        // The matching alternating staging buffer is therefore no longer in
        // use when the CPU overwrites it.
        let Some(drawable) = (unsafe { self.layer.nextDrawable() }) else {
            return Ok(true);
        };
        let buffer_index = self.next_guest_buffer;
        self.next_guest_buffer = (buffer_index + 1) % self.guest_buffers.len();
        let guest_buffer = &self.guest_buffers[buffer_index];
        unsafe {
            std::ptr::copy_nonoverlapping(
                framebuffer.as_ptr(),
                guest_buffer.contents().as_ptr().cast::<u8>(),
                frame_bytes,
            );
        }

        let (mut uniforms, cursor_data) = cursor
            .map(|(image, position)| guest_cursor_data(Some(image), position))
            .unwrap_or_else(|| guest_cursor_data(None, (0, 0)));
        uniforms.row_bytes = row_bytes;
        uniforms.width = width;
        uniforms.height = height;
        uniforms.pixel_size = u32::from(pixel_size);
        uniforms.content_left = content_left;
        uniforms.content_top = content_top;

        let drawable_texture = unsafe { drawable.texture() };
        let pass = unsafe { MTLRenderPassDescriptor::new() };
        let color = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
        color.setTexture(Some(&drawable_texture));
        color.setLoadAction(MTLLoadAction::Clear);
        color.setStoreAction(MTLStoreAction::Store);
        color.setClearColor(objc2_metal::MTLClearColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        });

        let command_buffer = self
            .command_queue
            .commandBuffer()
            .ok_or_else(|| "Metal failed to create a command buffer".to_string())?;
        let encoder = command_buffer
            .renderCommandEncoderWithDescriptor(&pass)
            .ok_or_else(|| "Metal failed to create a render encoder".to_string())?;
        encoder.setRenderPipelineState(&self.guest_pipeline);
        unsafe {
            encoder.setFragmentBuffer_offset_atIndex(Some(guest_buffer), 0, 0);
            encoder.setFragmentBytes_length_atIndex(
                non_null_bytes(palette),
                size_of::<[u32; 256]>(),
                1,
            );
            encoder.setFragmentBytes_length_atIndex(
                non_null_bytes(&uniforms),
                size_of::<GuestFrameUniforms>(),
                2,
            );
            encoder.setFragmentBytes_length_atIndex(
                non_null_bytes(&cursor_data),
                size_of::<GuestCursorData>(),
                3,
            );
        }

        let viewport = aspect_fit_viewport(width, height, drawable_width, drawable_height);
        encoder.setViewport(MTLViewport {
            originX: viewport.0,
            originY: viewport.1,
            width: viewport.2,
            height: viewport.3,
            znear: 0.0,
            zfar: 1.0,
        });
        unsafe {
            encoder.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::TriangleStrip, 0, 4)
        };
        encoder.endEncoding();

        command_buffer.presentDrawable(ProtocolObject::from_ref(&*drawable));
        command_buffer.commit();
        Ok(true)
    }

    fn ensure_guest_buffers(&mut self, frame_bytes: usize) -> Result<(), String> {
        if self.guest_buffer_size == frame_bytes {
            return Ok(());
        }

        let mut buffers = Vec::with_capacity(FRAME_RESOURCE_COUNT);
        for _ in 0..FRAME_RESOURCE_COUNT {
            buffers.push(
                self.device
                    .newBufferWithLength_options(
                        frame_bytes,
                        MTLResourceOptions::MTLResourceStorageModeShared,
                    )
                    .ok_or_else(|| "Metal failed to allocate a guest framebuffer".to_string())?,
            );
        }
        self.guest_buffers = buffers;
        self.guest_buffer_size = frame_bytes;
        self.next_guest_buffer = 0;
        Ok(())
    }

    fn ensure_upload_textures(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self.upload_size == (width, height) {
            return Ok(());
        }

        let descriptor = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                MTLPixelFormat::BGRA8Unorm,
                width as usize,
                height as usize,
                false,
            )
        };
        descriptor.setStorageMode(MTLStorageMode::Shared);
        descriptor.setUsage(MTLTextureUsage::ShaderRead);

        let mut textures = Vec::with_capacity(FRAME_RESOURCE_COUNT);
        for _ in 0..FRAME_RESOURCE_COUNT {
            textures.push(
                self.device
                    .newTextureWithDescriptor(&descriptor)
                    .ok_or_else(|| "Metal failed to allocate an upload texture".to_string())?,
            );
        }
        self.upload_textures = textures;
        self.upload_size = (width, height);
        self.next_upload_texture = 0;
        Ok(())
    }

    fn resize_drawable(&mut self, width: u32, height: u32) {
        self.layer.setFrame(self.root_layer.bounds());
        self.layer.setContentsScale(self.root_layer.contentsScale());
        if self.drawable_size != (width, height) {
            unsafe {
                self.layer.setDrawableSize(
                    CGRect::new(
                        objc2_foundation::CGPoint::new(0.0, 0.0),
                        objc2_foundation::CGSize::new(width as f64, height as f64),
                    )
                    .size,
                );
            }
            self.drawable_size = (width, height);
        }
    }
}

fn non_null_bytes<T>(value: &T) -> NonNull<c_void> {
    NonNull::from(value).cast::<c_void>()
}

fn guest_cursor_data(
    cursor: Option<&CursorImage>,
    mouse_pos: (i16, i16),
) -> (GuestFrameUniforms, GuestCursorData) {
    let mut uniforms = GuestFrameUniforms::default();
    let mut gpu = GuestCursorData::default();
    let Some(cursor) = cursor else {
        return (uniforms, gpu);
    };

    let (mouse_v, mouse_h) = mouse_pos;
    match cursor {
        CursorImage::Mono {
            data,
            mask,
            hot_v,
            hot_h,
        } => {
            uniforms.cursor_kind = 1;
            uniforms.cursor_width = 16;
            uniforms.cursor_height = 16;
            uniforms.cursor_left = i32::from(mouse_h) - i32::from(*hot_h);
            uniforms.cursor_top = i32::from(mouse_v) - i32::from(*hot_v);
            for row in 0..16 {
                gpu.data_rows[row] =
                    u32::from(u16::from_be_bytes([data[row * 2], data[row * 2 + 1]]));
                gpu.mask_rows[row] =
                    u32::from(u16::from_be_bytes([mask[row * 2], mask[row * 2 + 1]]));
            }
        }
        CursorImage::Color {
            width,
            height,
            pixels_argb,
            mask,
            hot_v,
            hot_h,
            ..
        } => {
            let source_width = *width;
            let width = source_width.min(16);
            let height = (*height).min(16);
            uniforms.cursor_kind = 2;
            uniforms.cursor_width = u32::from(width);
            uniforms.cursor_height = u32::from(height);
            uniforms.cursor_left = i32::from(mouse_h) - i32::from(*hot_h);
            uniforms.cursor_top = i32::from(mouse_v) - i32::from(*hot_v);
            for row in 0..16 {
                gpu.mask_rows[row] =
                    u32::from(u16::from_be_bytes([mask[row * 2], mask[row * 2 + 1]]));
            }
            for row in 0..usize::from(height) {
                let source_start = row * usize::from(source_width);
                let source_end = source_start + usize::from(width);
                let destination_start = row * usize::from(width);
                if source_end <= pixels_argb.len() {
                    gpu.color_pixels[destination_start..destination_start + usize::from(width)]
                        .copy_from_slice(&pixels_argb[source_start..source_end]);
                }
            }
        }
    }
    (uniforms, gpu)
}

fn aspect_fit_viewport(
    source_width: u32,
    source_height: u32,
    drawable_width: u32,
    drawable_height: u32,
) -> (f64, f64, f64, f64) {
    let scale = (drawable_width as f64 / source_width as f64)
        .min(drawable_height as f64 / source_height as f64);
    let width = source_width as f64 * scale;
    let height = source_height as f64 * scale;
    (
        (drawable_width as f64 - width) * 0.5,
        (drawable_height as f64 - height) * 0.5,
        width,
        height,
    )
}

fn guest_frame_byte_len(row_bytes: u32, screen_height: u32) -> Option<usize> {
    usize::try_from(row_bytes)
        .ok()?
        .checked_mul(screen_height as usize)
}

#[cfg(test)]
mod tests {
    use super::{aspect_fit_viewport, guest_cursor_data, guest_frame_byte_len};
    use systemless::display::CursorImage;

    #[test]
    fn viewport_scales_continuously_and_centers_letterboxing() {
        assert_eq!(
            aspect_fit_viewport(800, 600, 1920, 1080),
            (240.0, 0.0, 1440.0, 1080.0)
        );
        assert_eq!(
            aspect_fit_viewport(800, 600, 1200, 900),
            (0.0, 0.0, 1200.0, 900.0)
        );
    }

    #[test]
    fn cropped_presentation_still_uploads_the_full_guest_framebuffer() {
        // A 640x392 crop within an 800x600 screen may sample source rows well
        // below row 392, so the staging buffer must retain all 600 rows.
        assert_eq!(guest_frame_byte_len(800, 600), Some(480_000));
    }

    #[test]
    fn monochrome_cursor_is_packed_for_metal_without_bit_reversal() {
        let mut data = [0u8; 32];
        let mut mask = [0u8; 32];
        data[0..2].copy_from_slice(&0x8001u16.to_be_bytes());
        mask[0..2].copy_from_slice(&0xC003u16.to_be_bytes());
        let cursor = CursorImage::Mono {
            data,
            mask,
            hot_v: 3,
            hot_h: 4,
        };

        let (uniforms, gpu) = guest_cursor_data(Some(&cursor), (20, 30));

        assert_eq!(uniforms.cursor_kind, 1);
        assert_eq!((uniforms.cursor_left, uniforms.cursor_top), (26, 17));
        assert_eq!(gpu.data_rows[0], 0x8001);
        assert_eq!(gpu.mask_rows[0], 0xC003);
    }
}
