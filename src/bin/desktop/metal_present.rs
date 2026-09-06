//! Native macOS framebuffer presentation.
//!
//! `softbuffer`'s AppKit backend creates a new `CGImage` for every present.
//! Core Animation then copies and color-converts that image on the CPU.  A
//! continuously animating 800x600 game can spend most of a host core in that
//! conversion path. This presenter keeps a `CAMetalLayer`, command queue,
//! render pipeline, and two upload textures alive for the window lifetime.
//! Native guest and high-resolution raster frames share a latest-frame mailbox to a Metal
//! worker paced by drawable availability. Thus a full drawable queue never
//! blocks AppKit input handling or 68k execution.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{NSObject, ProtocolObject};
use objc2::{msg_send, msg_send_id};
use objc2_foundation::{ns_string, CGRect, MainThreadMarker};
use objc2_metal::{
    MTLBuffer, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLCreateSystemDefaultDevice,
    MTLDevice, MTLDrawable, MTLLibrary, MTLLoadAction, MTLPixelFormat, MTLPrimitiveType, MTLRegion,
    MTLRenderCommandEncoder, MTLRenderPassDescriptor, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState, MTLResourceOptions, MTLStorageMode, MTLStoreAction, MTLTexture,
    MTLTextureDescriptor, MTLTextureUsage, MTLViewport,
};
use objc2_quartz_core::{
    kCAGravityBottom, CAAutoresizingMask, CALayer, CAMetalDrawable, CAMetalLayer,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use systemless::display::CursorImage;

// Double buffering prevents the CPU from overwriting the texture used by the
// in-flight GPU frame without allowing a third drawable to queue and add input
// latency. The 800x600 upload is far below the GPU throughput limit.
const FRAME_RESOURCE_COUNT: usize = 2;

#[repr(C)]
#[derive(Clone, Copy, Default, Eq, PartialEq)]
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
#[derive(Clone, Eq, PartialEq)]
struct GuestCursorData {
    data_rows: [u32; 16],
    mask_rows: [u32; 16],
    color_pixels: [u32; 256],
}

#[derive(Clone, Eq, PartialEq)]
struct GuestFrameMetadata {
    screen_layout: (u32, u16, u16, u16),
    content_rect: (u32, u32, u32, u32),
    palette: [u32; 256],
    uniforms: GuestFrameUniforms,
    cursor: GuestCursorData,
    drawable_size: (u32, u32),
}

#[derive(Default)]
struct GuestFrameProfile {
    attempts: u64,
    unchanged: u64,
    forced: u64,
    comparison_time: Duration,
    presentation_time: Duration,
}

#[derive(Clone, PartialEq, Eq)]
enum FrameMetadata {
    Guest(GuestFrameMetadata),
    Raster {
        source_size: (u32, u32),
        drawable_size: (u32, u32),
    },
}

struct GuestFrameSubmission {
    framebuffer: Vec<u8>,
    metadata: FrameMetadata,
}

#[derive(Default)]
struct GuestFrameMailboxState {
    pending: Option<GuestFrameSubmission>,
    recycled: Vec<Vec<u8>>,
    paused: bool,
    active: bool,
    drawable_in_use: bool,
    shutdown: bool,
    error: Option<String>,
    submitted: u64,
    coalesced: u64,
    drawable_wait_time: Duration,
    render_time: Duration,
}

#[derive(Default)]
struct GuestFrameMailbox {
    state: Mutex<GuestFrameMailboxState>,
    changed: Condvar,
}

struct GuestRenderWorker {
    layer: Retained<CAMetalLayer>,
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    guest_buffers: Vec<Retained<ProtocolObject<dyn MTLBuffer>>>,
    guest_buffer_size: usize,
    next_guest_buffer: usize,
    raster_pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    upload_textures: Vec<Retained<ProtocolObject<dyn MTLTexture>>>,
    upload_size: (u32, u32),
    next_upload_texture: usize,
}

// Metal devices, queues, immutable pipeline states, and CAMetalLayer drawable
// acquisition are explicitly usable from background rendering threads. Layer
// geometry is still changed only by `MetalPresenter` on AppKit's main thread;
// the worker only calls `nextDrawable` and encodes commands for that drawable.
// objc2 0.5 predates the framework sendability annotations, so it cannot infer
// those guarantees for the retained Objective-C objects.
unsafe impl Send for GuestRenderWorker {}

struct AsyncGuestPresenter {
    mailbox: Arc<GuestFrameMailbox>,
    thread: Option<JoinHandle<()>>,
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

fn replace_pending_guest_frame(
    state: &mut GuestFrameMailboxState,
    framebuffer: &[u8],
    layout: GuestVisibleByteLayout,
    metadata: FrameMetadata,
) {
    let mut pixels = if let Some(previous) = state.pending.take() {
        state.coalesced = state.coalesced.saturating_add(1);
        previous.framebuffer
    } else {
        state.recycled.pop().unwrap_or_default()
    };
    copy_guest_visible_pixels(&mut pixels, framebuffer, layout);
    state.pending = Some(GuestFrameSubmission {
        framebuffer: pixels,
        metadata,
    });
}

impl AsyncGuestPresenter {
    fn new(worker: GuestRenderWorker) -> Result<Self, String> {
        let mailbox = Arc::new(GuestFrameMailbox::default());
        let worker_mailbox = Arc::clone(&mailbox);
        let thread = std::thread::Builder::new()
            .name("systemless-metal-present".to_string())
            .spawn(move || worker.run(worker_mailbox))
            .map_err(|error| format!("failed to start Metal presenter thread: {error}"))?;
        Ok(Self {
            mailbox,
            thread: Some(thread),
        })
    }

    fn enqueue(
        &self,
        framebuffer: &[u8],
        layout: GuestVisibleByteLayout,
        metadata: GuestFrameMetadata,
    ) -> Result<(), String> {
        self.enqueue_frame(framebuffer, layout, FrameMetadata::Guest(metadata))
    }

    fn enqueue_raster(
        &self,
        pixels: &[u32],
        source_size: (u32, u32),
        drawable_size: (u32, u32),
    ) -> Result<(), String> {
        let bytes = argb_bytes(pixels);
        let row_bytes = source_size.0 as usize * size_of::<u32>();
        self.enqueue_frame(
            bytes,
            GuestVisibleByteLayout {
                first_row_offset: 0,
                row_stride: row_bytes,
                visible_row_bytes: row_bytes,
                row_count: source_size.1 as usize,
            },
            FrameMetadata::Raster {
                source_size,
                drawable_size,
            },
        )
    }

    fn enqueue_frame(
        &self,
        framebuffer: &[u8],
        layout: GuestVisibleByteLayout,
        metadata: FrameMetadata,
    ) -> Result<(), String> {
        let mut state = self
            .mailbox
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(error) = state.error.as_ref() {
            return Err(error.clone());
        }

        state.paused = false;
        replace_pending_guest_frame(&mut state, framebuffer, layout, metadata);
        self.mailbox.changed.notify_one();
        Ok(())
    }

    /// Stop accepting asynchronous work and wait until no command is being
    /// encoded. Pending work is discarded so a later mode switch cannot flash
    /// a stale guest frame over the synchronous fallback presentation.
    fn pause_and_wait(&self) {
        let mut state = self
            .mailbox
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.paused = true;
        if let Some(pending) = state.pending.take() {
            state.recycled.push(pending.framebuffer);
        }
        self.mailbox.changed.notify_all();
        while state.active || state.drawable_in_use {
            state = self
                .mailbox
                .changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    fn snapshot(&self) -> (u64, u64, Duration, Duration) {
        let state = self
            .mailbox
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        (
            state.submitted,
            state.coalesced,
            state.drawable_wait_time,
            state.render_time,
        )
    }
}

impl Drop for AsyncGuestPresenter {
    fn drop(&mut self) {
        {
            let mut state = self
                .mailbox
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            state.shutdown = true;
            state.pending = None;
            self.mailbox.changed.notify_all();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl GuestRenderWorker {
    fn run(mut self, mailbox: Arc<GuestFrameMailbox>) {
        loop {
            let mut state = mailbox
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            while !state.shutdown && (state.paused || state.pending.is_none()) {
                state = mailbox
                    .changed
                    .wait(state)
                    .unwrap_or_else(|error| error.into_inner());
            }
            if state.shutdown {
                break;
            }
            state.drawable_in_use = true;
            drop(state);

            let wait_start = Instant::now();
            // This is a raw Rust worker rather than an AppKit-created thread,
            // so it has no ambient Objective-C autorelease pool. Drain the
            // temporary QuartzCore objects created while acquiring a drawable.
            let drawable = {
                let _timing = crate::FramePhaseTimer::new("Metal drawable wait (worker)");
                autoreleasepool(|_| unsafe { self.layer.nextDrawable() })
            };
            let drawable_wait = wait_start.elapsed();

            let mut state = mailbox
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            state.drawable_wait_time += drawable_wait;
            if state.shutdown {
                state.drawable_in_use = false;
                mailbox.changed.notify_all();
                break;
            }
            if state.paused {
                state.drawable_in_use = false;
                mailbox.changed.notify_all();
                drop(state);
                drop(drawable);
                continue;
            }
            let Some(submission) = state.pending.take() else {
                state.drawable_in_use = false;
                mailbox.changed.notify_all();
                drop(state);
                drop(drawable);
                continue;
            };
            state.active = true;
            drop(state);

            let render_start = Instant::now();
            // Command buffers, pass descriptors, and driver bookkeeping may
            // also autorelease objects while encoding the frame.
            let result = autoreleasepool(|_| match drawable {
                Some(drawable) => self.submit(&submission, &drawable),
                None => Ok(()),
            });
            let render_time = render_start.elapsed();

            let mut state = mailbox
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            state.active = false;
            state.drawable_in_use = false;
            state.render_time += render_time;
            if result.is_ok() {
                state.submitted = state.submitted.saturating_add(1);
            } else if state.error.is_none() {
                state.error = result.err();
            }
            state.recycled.push(submission.framebuffer);
            mailbox.changed.notify_all();
        }
    }

    fn submit(
        &mut self,
        submission: &GuestFrameSubmission,
        drawable: &ProtocolObject<dyn CAMetalDrawable>,
    ) -> Result<(), String> {
        let framebuffer = &submission.framebuffer;
        let metadata = match &submission.metadata {
            FrameMetadata::Guest(metadata) => metadata,
            FrameMetadata::Raster {
                source_size,
                drawable_size,
            } => {
                if self.upload_size != *source_size {
                    self.upload_textures =
                        upload_textures(&self.device, source_size.0, source_size.1)?;
                    self.upload_size = *source_size;
                    self.next_upload_texture = 0;
                }
                let index = self.next_upload_texture;
                self.next_upload_texture = (index + 1) % self.upload_textures.len();
                return encode_raster_frame(
                    &self.command_queue,
                    &self.raster_pipeline,
                    &self.upload_textures[index],
                    drawable,
                    framebuffer,
                    *source_size,
                    *drawable_size,
                    false,
                );
            }
        };
        self.ensure_guest_buffers(framebuffer.len())?;

        let buffer_index = self.next_guest_buffer;
        self.next_guest_buffer = (buffer_index + 1) % self.guest_buffers.len();
        let guest_buffer = &self.guest_buffers[buffer_index];
        unsafe {
            std::ptr::copy_nonoverlapping(
                framebuffer.as_ptr(),
                guest_buffer.contents().as_ptr().cast::<u8>(),
                framebuffer.len(),
            );
        }

        encode_guest_frame(
            &self.command_queue,
            &self.pipeline,
            guest_buffer,
            drawable,
            metadata,
            false,
        )
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
    last_guest_metadata: Option<GuestFrameMetadata>,
    last_guest_visible_pixels: Vec<u8>,
    skip_unchanged_guest_frames: bool,
    profile_guest_frames: bool,
    guest_frame_profile: GuestFrameProfile,
    async_guest_presenter: AsyncGuestPresenter,
}

/// The GPU that scans out the main display. Rendering there avoids copying every
/// presented frame across GPUs on dual-GPU Macs when the window is scanned out
/// directly (fullscreen); windowed presentation is unaffected either way.
/// `SYSTEMLESS_METAL_DEVICE=default` keeps the system default device.
fn display_metal_device() -> Option<Retained<ProtocolObject<dyn MTLDevice>>> {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDirectDisplayCopyCurrentMetalDevice(
            display: u32,
        ) -> *mut ProtocolObject<dyn MTLDevice>;
    }
    if std::env::var("SYSTEMLESS_METAL_DEVICE")
        .map(|v| v == "default")
        .unwrap_or(false)
    {
        return None;
    }
    // Returns a +1 reference (Copy rule); null when the display has no Metal device.
    unsafe { Retained::from_raw(CGDirectDisplayCopyCurrentMetalDevice(CGMainDisplayID())) }
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
        let device = display_metal_device()
            .or_else(|| unsafe { Retained::retain(MTLCreateSystemDefaultDevice()) })
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
        // If AppKit changes the view bounds before the replacement Metal
        // drawable is committed, preserve the previous drawable's pixel size
        // instead of stretching it to the transient layer bounds.
        // Automatic dialog crops grow the native window upward while keeping
        // its bottom edge fixed. Pin the retained drawable to that edge so the
        // complete pre-dialog frame remains stationary until the resize
        // callback submits its replacement.
        layer.setContentsGravity(unsafe { kCAGravityBottom });
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

        let async_command_queue = device
            .newCommandQueue()
            .ok_or_else(|| "Metal failed to create the presenter command queue".to_string())?;
        let async_guest_presenter = AsyncGuestPresenter::new(GuestRenderWorker {
            layer: layer.clone(),
            device: device.clone(),
            command_queue: async_command_queue,
            pipeline: guest_pipeline.clone(),
            guest_buffers: Vec::new(),
            guest_buffer_size: 0,
            next_guest_buffer: 0,
            raster_pipeline: pipeline.clone(),
            upload_textures: Vec::new(),
            upload_size: (0, 0),
            next_upload_texture: 0,
        })?;

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
            last_guest_metadata: None,
            last_guest_visible_pixels: Vec::new(),
            skip_unchanged_guest_frames: std::env::var_os(
                "SYSTEMLESS_DISABLE_UNCHANGED_FRAME_SKIP",
            )
            .is_none(),
            profile_guest_frames: std::env::var_os("SYSTEMLESS_PROFILE_METAL_FRAMES").is_some(),
            guest_frame_profile: GuestFrameProfile::default(),
            async_guest_presenter,
        })
    }

    /// Couple the next drawable presentation to the current Core Animation
    /// transaction. This is used only while an AppKit window-frame mutation
    /// is in flight; normal gameplay keeps the lower-latency asynchronous
    /// Metal presentation path.
    pub fn set_transactional_presentation(&self, enabled: bool) {
        if enabled {
            self.async_guest_presenter.pause_and_wait();
        }
        unsafe { self.layer.setPresentsWithTransaction(enabled) };
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

        self.resize_drawable(drawable_width, drawable_height);
        self.last_guest_metadata = None;
        if !unsafe { self.layer.presentsWithTransaction() } {
            return self.async_guest_presenter.enqueue_raster(
                &pixels[..expected_pixels],
                (source_width, source_height),
                (drawable_width, drawable_height),
            );
        }
        self.async_guest_presenter.pause_and_wait();
        self.ensure_upload_textures(source_width, source_height)?;

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
        encode_raster_frame(
            &self.command_queue,
            &self.pipeline,
            upload,
            &drawable,
            argb_bytes(pixels),
            (source_width, source_height),
            (drawable_width, drawable_height),
            true,
        )?;
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
        force_present: bool,
    ) -> Result<bool, String> {
        let (_, row_bytes, width, height, pixel_size) = screen_mode;
        let screen_width = u32::from(width);
        let screen_height = u32::from(height);
        let (content_left, content_top, width, height) = content_rect;
        let (drawable_width, drawable_height) = drawable_size;
        let Some(packed_content_left) = guest_packed_content_left(pixel_size, content_left) else {
            return Ok(false);
        };
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

        let visible_layout = guest_visible_byte_layout(
            row_bytes,
            pixel_size,
            content_left,
            content_top,
            width,
            height,
        )
        .ok_or_else(|| "visible guest framebuffer rows are invalid".to_string())?;
        let (mut uniforms, cursor_data) = cursor
            .map(|(image, position)| guest_cursor_data(Some(image), position))
            .unwrap_or_else(|| guest_cursor_data(None, (0, 0)));
        let packed_origin_left = content_left - packed_content_left;
        uniforms.row_bytes = u32::try_from(visible_layout.visible_row_bytes)
            .map_err(|_| "visible guest row stride exceeds Metal's limit".to_string())?;
        uniforms.width = width;
        uniforms.height = height;
        uniforms.pixel_size = u32::from(pixel_size);
        uniforms.content_left = packed_content_left;
        uniforms.content_top = 0;
        uniforms.cursor_left -= i32::try_from(packed_origin_left)
            .map_err(|_| "guest crop origin exceeds Metal's limit".to_string())?;
        uniforms.cursor_top -= i32::try_from(content_top)
            .map_err(|_| "guest crop origin exceeds Metal's limit".to_string())?;
        let metadata = GuestFrameMetadata {
            screen_layout: (row_bytes, screen_mode.2, screen_mode.3, pixel_size),
            content_rect,
            palette: *palette,
            uniforms,
            cursor: cursor_data,
            drawable_size,
        };

        let presentation_start = Instant::now();
        self.guest_frame_profile.attempts += 1;
        if force_present {
            self.guest_frame_profile.forced += 1;
        }
        let compare_start = Instant::now();
        let unchanged = self.skip_unchanged_guest_frames
            && self.last_guest_metadata.as_ref() == Some(&metadata)
            && guest_visible_pixels_equal(
                &self.last_guest_visible_pixels,
                framebuffer,
                visible_layout,
            );
        if self.skip_unchanged_guest_frames {
            self.guest_frame_profile.comparison_time += compare_start.elapsed();
        }
        if unchanged && !force_present {
            self.guest_frame_profile.unchanged += 1;
            self.guest_frame_profile.presentation_time += presentation_start.elapsed();
            self.maybe_log_guest_frame_profile();
            return Ok(true);
        }

        self.resize_drawable(drawable_width, drawable_height);
        if unsafe { self.layer.presentsWithTransaction() } {
            let visible_bytes = visible_layout
                .visible_row_bytes
                .checked_mul(visible_layout.row_count)
                .ok_or_else(|| {
                    "visible guest framebuffer dimensions overflow host size".to_string()
                })?;
            self.ensure_guest_buffers(visible_bytes)?;
            let Some(drawable) = (unsafe { self.layer.nextDrawable() }) else {
                return Ok(true);
            };
            let buffer_index = self.next_guest_buffer;
            self.next_guest_buffer = (buffer_index + 1) % self.guest_buffers.len();
            let guest_buffer = &self.guest_buffers[buffer_index];
            copy_guest_visible_pixels_to_ptr(
                guest_buffer.contents().as_ptr().cast::<u8>(),
                framebuffer,
                visible_layout,
            );
            encode_guest_frame(
                &self.command_queue,
                &self.guest_pipeline,
                guest_buffer,
                &drawable,
                &metadata,
                true,
            )?;
        } else {
            self.async_guest_presenter
                .enqueue(framebuffer, visible_layout, metadata.clone())?;
        }
        if self.skip_unchanged_guest_frames {
            copy_guest_visible_pixels(
                &mut self.last_guest_visible_pixels,
                framebuffer,
                visible_layout,
            );
            self.last_guest_metadata = Some(metadata);
        }
        self.guest_frame_profile.presentation_time += presentation_start.elapsed();
        self.maybe_log_guest_frame_profile();
        Ok(true)
    }

    fn maybe_log_guest_frame_profile(&self) {
        let profile = &self.guest_frame_profile;
        if !self.profile_guest_frames
            || profile.attempts == 0
            || !profile.attempts.is_multiple_of(600)
        {
            return;
        }
        let skipped_percent = profile.unchanged as f64 * 100.0 / profile.attempts as f64;
        let compare_us =
            profile.comparison_time.as_secs_f64() * 1_000_000.0 / profile.attempts as f64;
        let presentation_us =
            profile.presentation_time.as_secs_f64() * 1_000_000.0 / profile.attempts as f64;
        let (async_submitted, coalesced, drawable_wait, render_time) =
            self.async_guest_presenter.snapshot();
        let submitted = async_submitted;
        let drawable_wait_us = if async_submitted == 0 {
            0.0
        } else {
            drawable_wait.as_secs_f64() * 1_000_000.0 / async_submitted as f64
        };
        let render_us = if async_submitted == 0 {
            0.0
        } else {
            render_time.as_secs_f64() * 1_000_000.0 / async_submitted as f64
        };
        eprintln!(
            "[METAL-PROFILE] attempts={} submitted={} coalesced={} unchanged_skipped={} forced={} skipped={:.1}% compare={:.1}us/frame enqueue={:.1}us/frame drawable_wait={:.1}us/submission render={:.1}us/submission",
            profile.attempts,
            submitted,
            coalesced,
            profile.unchanged,
            profile.forced,
            skipped_percent,
            compare_us,
            presentation_us,
            drawable_wait_us,
            render_us,
        );
    }

    fn ensure_upload_textures(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self.upload_size == (width, height) {
            return Ok(());
        }

        let textures = upload_textures(&self.device, width, height)?;
        self.upload_textures = textures;
        self.upload_size = (width, height);
        self.next_upload_texture = 0;
        Ok(())
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

    fn resize_drawable(&mut self, width: u32, height: u32) {
        if self.drawable_size != (width, height) {
            // CALayer property assignments participate in Core Animation
            // transactions even when the value is unchanged. Reasserting
            // frame and scale at the guest VBL rate can make an otherwise
            // trivial Metal layer miss physical-display deadlines.
            self.layer.setFrame(self.root_layer.bounds());
            self.layer.setContentsScale(self.root_layer.contentsScale());
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

fn argb_bytes(pixels: &[u32]) -> &[u8] {
    // All byte patterns of u32 are valid and the returned borrow cannot outlive
    // the source pixels. BGRA8Unorm consumes their native little-endian bytes.
    unsafe { std::slice::from_raw_parts(pixels.as_ptr().cast(), std::mem::size_of_val(pixels)) }
}

fn upload_textures(
    device: &ProtocolObject<dyn MTLDevice>,
    width: u32,
    height: u32,
) -> Result<Vec<Retained<ProtocolObject<dyn MTLTexture>>>, String> {
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
            device
                .newTextureWithDescriptor(&descriptor)
                .ok_or_else(|| "Metal failed to allocate an upload texture".to_string())?,
        );
    }
    Ok(textures)
}

fn finish_presentation(
    command_buffer: &ProtocolObject<dyn MTLCommandBuffer>,
    drawable: &ProtocolObject<dyn CAMetalDrawable>,
    transactional: bool,
) {
    if transactional {
        // Apple requires the render work to be scheduled before the
        // drawable itself is presented into the current CA transaction.
        // Using commandBuffer.presentDrawable here would bypass that
        // transaction and recreate the one-frame resize bounce.
        command_buffer.commit();
        command_buffer.waitUntilScheduled();
        let drawable: &ProtocolObject<dyn objc2_metal::MTLDrawable> =
            ProtocolObject::from_ref(drawable);
        drawable.present();
    } else {
        command_buffer.presentDrawable(ProtocolObject::from_ref(drawable));
        command_buffer.commit();
    }
}

fn encode_raster_frame(
    command_queue: &ProtocolObject<dyn MTLCommandQueue>,
    pipeline: &ProtocolObject<dyn MTLRenderPipelineState>,
    upload: &ProtocolObject<dyn MTLTexture>,
    drawable: &ProtocolObject<dyn CAMetalDrawable>,
    pixels: &[u8],
    source_size: (u32, u32),
    drawable_size: (u32, u32),
    transactional: bool,
) -> Result<(), String> {
    let (source_width, source_height) = source_size;
    let (drawable_width, drawable_height) = drawable_size;
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

    let command_buffer = command_queue
        .commandBuffer()
        .ok_or_else(|| "Metal failed to create a command buffer".to_string())?;
    let encoder = command_buffer
        .renderCommandEncoderWithDescriptor(&pass)
        .ok_or_else(|| "Metal failed to create a render encoder".to_string())?;
    encoder.setRenderPipelineState(pipeline);
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

    finish_presentation(&command_buffer, drawable, transactional);
    Ok(())
}

fn encode_guest_frame(
    command_queue: &ProtocolObject<dyn MTLCommandQueue>,
    pipeline: &ProtocolObject<dyn MTLRenderPipelineState>,
    guest_buffer: &ProtocolObject<dyn MTLBuffer>,
    drawable: &ProtocolObject<dyn CAMetalDrawable>,
    metadata: &GuestFrameMetadata,
    transactional: bool,
) -> Result<(), String> {
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

    let command_buffer = command_queue
        .commandBuffer()
        .ok_or_else(|| "Metal failed to create a command buffer".to_string())?;
    let encoder = command_buffer
        .renderCommandEncoderWithDescriptor(&pass)
        .ok_or_else(|| "Metal failed to create a render encoder".to_string())?;
    encoder.setRenderPipelineState(pipeline);
    unsafe {
        encoder.setFragmentBuffer_offset_atIndex(Some(guest_buffer), 0, 0);
        encoder.setFragmentBytes_length_atIndex(
            non_null_bytes(&metadata.palette),
            size_of::<[u32; 256]>(),
            1,
        );
        encoder.setFragmentBytes_length_atIndex(
            non_null_bytes(&metadata.uniforms),
            size_of::<GuestFrameUniforms>(),
            2,
        );
        encoder.setFragmentBytes_length_atIndex(
            non_null_bytes(&metadata.cursor),
            size_of::<GuestCursorData>(),
            3,
        );
    }

    let (_, _, width, height) = metadata.content_rect;
    let (drawable_width, drawable_height) = metadata.drawable_size;
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

    if transactional {
        command_buffer.commit();
        command_buffer.waitUntilScheduled();
        let drawable: &ProtocolObject<dyn objc2_metal::MTLDrawable> =
            ProtocolObject::from_ref(drawable);
        drawable.present();
    } else {
        command_buffer.presentDrawable(ProtocolObject::from_ref(drawable));
        command_buffer.commit();
    }
    Ok(())
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

fn guest_packed_content_left(pixel_size: u16, content_left: u32) -> Option<u32> {
    match pixel_size {
        1 => Some(content_left & 7),
        2 => Some(content_left & 3),
        4 => Some(content_left & 1),
        8 => Some(0),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct GuestVisibleByteLayout {
    first_row_offset: usize,
    row_stride: usize,
    visible_row_bytes: usize,
    row_count: usize,
}

fn guest_visible_byte_layout(
    row_bytes: u32,
    pixel_size: u16,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
) -> Option<GuestVisibleByteLayout> {
    let row_stride = usize::try_from(row_bytes).ok()?;
    let (first_byte, byte_end) = match pixel_size {
        1 | 2 | 4 => {
            let pixels_per_byte = 8 / u32::from(pixel_size);
            (
                usize::try_from(left / pixels_per_byte).ok()?,
                usize::try_from(
                    left.checked_add(width)?.checked_add(pixels_per_byte - 1)? / pixels_per_byte,
                )
                .ok()?,
            )
        }
        8 => (
            usize::try_from(left).ok()?,
            usize::try_from(left.checked_add(width)?).ok()?,
        ),
        _ => return None,
    };
    if byte_end > row_stride || byte_end <= first_byte || height == 0 {
        return None;
    }
    Some(GuestVisibleByteLayout {
        first_row_offset: usize::try_from(top)
            .ok()?
            .checked_mul(row_stride)?
            .checked_add(first_byte)?,
        row_stride,
        visible_row_bytes: byte_end - first_byte,
        row_count: usize::try_from(height).ok()?,
    })
}

fn guest_visible_pixels_equal(
    snapshot: &[u8],
    framebuffer: &[u8],
    layout: GuestVisibleByteLayout,
) -> bool {
    let Some(snapshot_len) = layout.visible_row_bytes.checked_mul(layout.row_count) else {
        return false;
    };
    if snapshot.len() != snapshot_len {
        return false;
    }
    for row in 0..layout.row_count {
        let Some(source_start) = row
            .checked_mul(layout.row_stride)
            .and_then(|offset| layout.first_row_offset.checked_add(offset))
        else {
            return false;
        };
        let Some(source_end) = source_start.checked_add(layout.visible_row_bytes) else {
            return false;
        };
        let snapshot_start = row * layout.visible_row_bytes;
        if framebuffer.get(source_start..source_end)
            != snapshot.get(snapshot_start..snapshot_start + layout.visible_row_bytes)
        {
            return false;
        }
    }
    true
}

fn copy_guest_visible_pixels(
    snapshot: &mut Vec<u8>,
    framebuffer: &[u8],
    layout: GuestVisibleByteLayout,
) {
    snapshot.clear();
    let capacity = layout
        .visible_row_bytes
        .checked_mul(layout.row_count)
        .unwrap_or(0);
    snapshot.reserve(capacity);
    for row in 0..layout.row_count {
        let source_start = layout.first_row_offset + row * layout.row_stride;
        let source_end = source_start + layout.visible_row_bytes;
        snapshot.extend_from_slice(&framebuffer[source_start..source_end]);
    }
}

fn copy_guest_visible_pixels_to_ptr(
    destination: *mut u8,
    framebuffer: &[u8],
    layout: GuestVisibleByteLayout,
) {
    for row in 0..layout.row_count {
        let source_start = layout.first_row_offset + row * layout.row_stride;
        let destination_start = row * layout.visible_row_bytes;
        // SAFETY: `guest_visible_byte_layout` validated every source row,
        // while the caller allocated visible_row_bytes * row_count bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(
                framebuffer.as_ptr().add(source_start),
                destination.add(destination_start),
                layout.visible_row_bytes,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        aspect_fit_viewport, copy_guest_visible_pixels, guest_cursor_data,
        guest_packed_content_left, guest_visible_byte_layout, guest_visible_pixels_equal,
        replace_pending_guest_frame, GuestCursorData, GuestFrameMailboxState, GuestFrameMetadata,
        GuestFrameUniforms, FrameMetadata,
    };
    use systemless::display::CursorImage;

    /// Exercise the production shader on the GPU, including its final reduction
    /// to drawable pixels. No AppKit window or unlocked desktop is required.
    fn render_rgba(source: &image::RgbaImage, width: u32, height: u32) -> image::RgbaImage {
        use super::*;
        autoreleasepool(|_| {
            let device = unsafe { Retained::retain(MTLCreateSystemDefaultDevice()) }
                .expect("Metal device required");
            let library = device
                .newLibraryWithSource_options_error(
                    ns_string!(include_str!("metal_present.metal")),
                    None,
                )
                .expect("compile presentation shader");
            let descriptor = MTLRenderPipelineDescriptor::new();
            descriptor.setVertexFunction(
                library
                    .newFunctionWithName(ns_string!("raster_vertex"))
                    .as_deref(),
            );
            descriptor.setFragmentFunction(
                library
                    .newFunctionWithName(ns_string!("raster_fragment"))
                    .as_deref(),
            );
            unsafe {
                descriptor
                    .colorAttachments()
                    .objectAtIndexedSubscript(0)
                    .setPixelFormat(MTLPixelFormat::RGBA8Unorm);
            }
            let pipeline = device
                .newRenderPipelineStateWithDescriptor_error(&descriptor)
                .unwrap();
            let texture = |w, h, usage| {
                let desc = unsafe {
                    MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                        MTLPixelFormat::RGBA8Unorm,
                        w as usize,
                        h as usize,
                        false,
                    )
                };
                desc.setStorageMode(MTLStorageMode::Shared);
                desc.setUsage(usage);
                device.newTextureWithDescriptor(&desc).unwrap()
            };
            let input = texture(source.width(), source.height(), MTLTextureUsage::ShaderRead);
            let output = texture(width, height, MTLTextureUsage::RenderTarget);
            let region = |w, h| MTLRegion {
                origin: objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
                size: objc2_metal::MTLSize {
                    width: w as usize,
                    height: h as usize,
                    depth: 1,
                },
            };
            unsafe {
                input.replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                    region(source.width(), source.height()),
                    0,
                    NonNull::new(source.as_ptr().cast_mut().cast()).unwrap(),
                    source.width() as usize * 4,
                );
            }
            let pass = unsafe { MTLRenderPassDescriptor::new() };
            let color = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
            color.setTexture(Some(&output));
            color.setLoadAction(MTLLoadAction::Clear);
            color.setStoreAction(MTLStoreAction::Store);
            let queue = device.newCommandQueue().unwrap();
            let command = queue.commandBuffer().unwrap();
            let encoder = command.renderCommandEncoderWithDescriptor(&pass).unwrap();
            encoder.setRenderPipelineState(&pipeline);
            encoder.setViewport(MTLViewport {
                originX: 0.0,
                originY: 0.0,
                width: width as f64,
                height: height as f64,
                znear: 0.0,
                zfar: 1.0,
            });
            unsafe {
                encoder.setFragmentTexture_atIndex(Some(&input), 0);
                encoder.drawPrimitives_vertexStart_vertexCount(
                    MTLPrimitiveType::TriangleStrip,
                    0,
                    4,
                );
            }
            encoder.endEncoding();
            command.commit();
            unsafe {
                command.waitUntilCompleted();
            }
            let mut result = image::RgbaImage::new(width, height);
            unsafe {
                output.getBytes_bytesPerRow_fromRegion_mipmapLevel(
                    NonNull::new(result.as_mut_ptr().cast()).unwrap(),
                    width as usize * 4,
                    region(width, height),
                    0,
                );
            }
            result
        })
    }

    #[test]
    fn minification_retains_thin_strokes_between_sample_centers() {
        let source = image::RgbaImage::from_fn(8, 8, |x, _| {
            let shade = if x % 2 == 0 { 0 } else { 255 };
            image::Rgba([shade, shade, shade, 255])
        });
        for (width, expected) in [
            (2, vec![128, 128]),
            (3, vec![96, 128, 159]),
            (5, vec![96, 96, 128, 159, 159]),
        ] {
            let result = render_rgba(&source, width, 3);
            // Exact area averages retain alternating one-pixel strokes at
            // integral and fractional reductions; allow UNorm rounding error.
            for (x, _, pixel) in result.enumerate_pixels() {
                assert!(
                    pixel[0].abs_diff(expected[x as usize]) <= 1,
                    "lost coverage at width {width}, column {x}"
                );
                assert_eq!(pixel[3], 255);
            }
        }
        let enlarged = render_rgba(&source, 16, 16);
        for (x, y, pixel) in enlarged.enumerate_pixels() {
            assert_eq!(pixel, source.get_pixel(x / 2, y / 2));
        }
    }

    #[test]
    #[ignore = "writes actual Metal output; set SYSTEMLESS_METAL_FONT_CAPTURE"]
    fn capture_showcase_at_mac_window_size() {
        let path = std::env::var_os("SYSTEMLESS_METAL_FONT_CAPTURE").expect("capture output path");
        let source = image::load_from_memory(include_bytes!(
            "../../../tests/toolbox-showcase/outline-fonts/first-paint/textedit-fresh.png"
        ))
        .unwrap()
        .into_rgba8();
        // Native menus hide the guest's 20-row menu bar. The reported window
        // displays the remaining 800x580 guest area at 960x696 physical pixels.
        let source = image::imageops::crop_imm(&source, 0, 80, 3200, 2320).to_image();
        render_rgba(&source, 960, 696).save(path).unwrap();
    }

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
    fn cropped_presentation_packs_only_visible_guest_rows() {
        let layout = guest_visible_byte_layout(800, 8, 80, 104, 640, 392).unwrap();
        assert_eq!(layout.visible_row_bytes * layout.row_count, 250_880);
    }

    #[test]
    fn cropped_four_bit_presentation_keeps_nibble_aligned_rows() {
        let layout = guest_visible_byte_layout(400, 4, 81, 10, 319, 20).unwrap();
        assert_eq!(layout.first_row_offset, 4_040);
        assert_eq!(layout.row_stride, 400);
        assert_eq!(layout.visible_row_bytes, 160);
        assert_eq!(layout.row_count, 20);
    }

    #[test]
    fn cropped_two_bit_presentation_keeps_msb_aligned_metadata_and_rows() {
        let layout = guest_visible_byte_layout(200, 2, 5, 10, 319, 20).unwrap();
        assert_eq!(guest_packed_content_left(2, 5), Some(1));
        assert_eq!(layout.first_row_offset, 2_001);
        assert_eq!(layout.row_stride, 200);
        assert_eq!(layout.visible_row_bytes, 80);
        assert_eq!(layout.row_count, 20);
    }

    #[test]
    fn guest_shader_decodes_two_bit_and_monochrome_pixels_through_the_palette() {
        let shader = include_str!("metal_present.metal");
        let (_, after_two_bit_header) = shader
            .split_once("} else if (frame.pixel_size == 2) {")
            .expect("guest shader must have an explicit 2bpp branch");
        let (two_bit_branch, monochrome_branch) = after_two_bit_header
            .split_once("} else {")
            .expect("guest shader must retain a final 1bpp branch");

        assert!(two_bit_branch.contains("x / 4"));
        assert!(two_bit_branch.contains("6 - 2 * (x & 3)"));
        assert!(two_bit_branch.contains("& 0x03"));
        assert!(two_bit_branch.contains("argb = palette[index]"));
        assert!(monochrome_branch.contains("x / 8"));
        assert!(monochrome_branch.contains("7 - (x & 7)"));
        assert!(monochrome_branch.contains("argb = palette[index]"));
    }

    #[test]
    fn raster_frames_enqueue_while_drawable_is_blocked_and_keep_latest_pixels() {
        use super::{argb_bytes, AsyncGuestPresenter, GuestFrameMailbox};
        use std::{sync::Arc, time::Duration};
        let mailbox = Arc::new(GuestFrameMailbox::default());
        {
            let mut state = mailbox.state.lock().unwrap();
            state.drawable_in_use = true;
            state.active = true;
        }
        let producer_mailbox = Arc::clone(&mailbox);
        let (sent, received) = std::sync::mpsc::channel();
        let producer = std::thread::spawn(move || {
            let presenter = AsyncGuestPresenter {
                mailbox: producer_mailbox,
                thread: None,
            };
            let mut pixels = vec![0xff123456; 16];
            for color in [0xff123456, 0xffabcdef] {
                pixels.fill(color);
                presenter.enqueue_raster(&pixels, (4, 4), (8, 8)).unwrap();
            }
            pixels.fill(0); // queued pixels must own their storage
            sent.send(presenter).unwrap();
        });
        let presenter = received
            .recv_timeout(Duration::from_secs(2))
            .expect("raster enqueue must not wait for a drawable or an active frame");
        producer.join().unwrap();
        {
            let state = mailbox.state.lock().unwrap();
            assert!(state.drawable_in_use && state.active);
            assert_eq!(state.coalesced, 1);
            let frame = state.pending.as_ref().unwrap();
            assert_eq!(frame.framebuffer, argb_bytes(&[0xffabcdef; 16]));
            assert!(
                frame.metadata
                    == FrameMetadata::Raster {
                        source_size: (4, 4),
                        drawable_size: (8, 8),
                    }
            );
        }
        // A mode switch replaces pending raster work in the same mailbox.
        let metadata = GuestFrameMetadata {
            screen_layout: (2, 2, 1, 8),
            content_rect: (0, 0, 2, 1),
            palette: [0; 256],
            uniforms: GuestFrameUniforms::default(),
            cursor: GuestCursorData::default(),
            drawable_size: (8, 8),
        };
        presenter
            .enqueue(
                &[7, 8],
                guest_visible_byte_layout(2, 8, 0, 0, 2, 1).unwrap(),
                metadata,
            )
            .unwrap();
        let state = mailbox.state.lock().unwrap();
        assert_eq!(state.coalesced, 2);
        assert_eq!(state.pending.as_ref().unwrap().framebuffer, [7, 8]);
    }

    #[test]
    fn busy_presenter_mailbox_keeps_only_the_latest_complete_frame() {
        let metadata = GuestFrameMetadata {
            screen_layout: (4, 4, 2, 8),
            content_rect: (0, 0, 4, 2),
            palette: [0; 256],
            uniforms: GuestFrameUniforms::default(),
            cursor: GuestCursorData::default(),
            drawable_size: (8, 4),
        };
        let mut state = GuestFrameMailboxState::default();

        let layout = guest_visible_byte_layout(4, 8, 0, 0, 4, 1).unwrap();
        replace_pending_guest_frame(
            &mut state,
            &[1, 2, 3, 4],
            layout,
            FrameMetadata::Guest(metadata.clone()),
        );
        replace_pending_guest_frame(
            &mut state,
            &[5, 6, 7, 8],
            layout,
            FrameMetadata::Guest(metadata.clone()),
        );

        assert_eq!(state.coalesced, 1);
        let pending = state.pending.as_ref().unwrap();
        assert_eq!(pending.framebuffer, [5, 6, 7, 8]);
        assert!(pending.metadata == FrameMetadata::Guest(metadata));
    }

    #[test]
    fn presenter_mailbox_packs_cropped_rows_without_surrounding_pixels() {
        let metadata = GuestFrameMetadata {
            screen_layout: (6, 6, 3, 8),
            content_rect: (2, 1, 3, 2),
            palette: [0; 256],
            uniforms: GuestFrameUniforms::default(),
            cursor: GuestCursorData::default(),
            drawable_size: (6, 4),
        };
        let framebuffer = [
            0, 1, 2, 3, 4, 5, // outside the crop
            6, 7, 8, 9, 10, 11, // visible bytes 8..=10
            12, 13, 14, 15, 16, 17, // visible bytes 14..=16
        ];
        let layout = guest_visible_byte_layout(6, 8, 2, 1, 3, 2).unwrap();
        let mut state = GuestFrameMailboxState::default();

        replace_pending_guest_frame(
            &mut state,
            &framebuffer,
            layout,
            FrameMetadata::Guest(metadata),
        );

        assert_eq!(state.pending.unwrap().framebuffer, [8, 9, 10, 14, 15, 16]);
    }

    #[test]
    fn unchanged_detection_compares_only_cropped_visible_rows() {
        let mut framebuffer = vec![0u8; 8 * 6];
        let layout = guest_visible_byte_layout(8, 8, 2, 1, 4, 3).unwrap();
        let mut snapshot = Vec::new();
        copy_guest_visible_pixels(&mut snapshot, &framebuffer, layout);
        assert!(guest_visible_pixels_equal(&snapshot, &framebuffer, layout));

        framebuffer[0] = 1;
        assert!(
            guest_visible_pixels_equal(&snapshot, &framebuffer, layout),
            "pixels outside the crop must not trigger a GPU submission"
        );
        framebuffer[2 * 8 + 4] = 1;
        assert!(!guest_visible_pixels_equal(&snapshot, &framebuffer, layout));
    }

    #[test]
    fn monochrome_visible_comparison_includes_boundary_bytes() {
        let mut framebuffer = vec![0u8; 4 * 3];
        let layout = guest_visible_byte_layout(4, 1, 7, 1, 10, 1).unwrap();
        let mut snapshot = Vec::new();
        copy_guest_visible_pixels(&mut snapshot, &framebuffer, layout);
        assert_eq!(snapshot.len(), 3);

        framebuffer[4 + 2] = 0x80;
        assert!(!guest_visible_pixels_equal(&snapshot, &framebuffer, layout));
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
