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
    MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice,
    MTLLibrary, MTLLoadAction, MTLPixelFormat, MTLPrimitiveType, MTLRegion,
    MTLRenderCommandEncoder, MTLRenderPassDescriptor, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState, MTLStorageMode, MTLStoreAction, MTLTexture, MTLTextureDescriptor,
    MTLTextureUsage, MTLViewport,
};
use objc2_quartz_core::{CALayer, CAMetalDrawable, CAMetalLayer};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

// Double buffering prevents the CPU from overwriting the texture used by the
// in-flight GPU frame without allowing a third drawable to queue and add input
// latency. The 800x600 upload is far below the GPU throughput limit.
const FRAME_RESOURCE_COUNT: usize = 2;

pub struct MetalPresenter {
    _window: Rc<Window>,
    root_layer: Retained<CALayer>,
    layer: Retained<CAMetalLayer>,
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
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

        Ok(Self {
            _window: window,
            root_layer,
            layer,
            device,
            command_queue,
            pipeline,
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
            integer_scaled_viewport(source_width, source_height, drawable_width, drawable_height);
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

fn integer_scaled_viewport(
    source_width: u32,
    source_height: u32,
    drawable_width: u32,
    drawable_height: u32,
) -> (f64, f64, f64, f64) {
    let scale = (drawable_width / source_width)
        .min(drawable_height / source_height)
        .max(1);
    (
        0.0,
        0.0,
        (source_width * scale) as f64,
        (source_height * scale) as f64,
    )
}

#[cfg(test)]
mod tests {
    use super::integer_scaled_viewport;

    #[test]
    fn integer_viewport_matches_software_presenter_scaling() {
        assert_eq!(
            integer_scaled_viewport(800, 600, 1920, 1080),
            (0.0, 0.0, 800.0, 600.0)
        );
        assert_eq!(
            integer_scaled_viewport(512, 342, 1200, 800),
            (0.0, 0.0, 1024.0, 684.0)
        );
    }
}
