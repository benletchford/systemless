//! Classic AGL pixel-format requests decoded from guest `GLint` arrays.
//!
//! Attribute values and the boolean/value distinction come from Apple's
//! AGL/agl.h (Mac OS X 10.2.8 SDK). Capability selection happens separately.
//! <https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX10.2.8.sdk/System/Library/Frameworks/AGL.framework/Versions/A/Headers/agl.h>

use super::classic_gl::GlContext;
use super::classic_gl_framebuffer::{ClassicGlColorBuffer, ClassicGlFramebuffer};
use super::{PpcFrontBuffer, PpcSectionMem};
use ppc::PpcMemory;

const MAX_ATTRIBUTE_WORDS: u32 = 64;
const FIRST_AGL_OBJECT: u32 = 0x0500_0000;
pub const AGL_BAD_ATTRIBUTE: u32 = 10000;
pub const AGL_BAD_PROPERTY: u32 = 10001;
pub const AGL_BAD_PIXELFMT: u32 = 10002;
pub const AGL_BAD_RENDINFO: u32 = 10003;
pub const AGL_BAD_CONTEXT: u32 = 10004;
pub const AGL_BAD_GDEV: u32 = 10006;
pub const AGL_BAD_VALUE: u32 = 10008;
pub const AGL_BAD_ENUM: u32 = 10010;
pub const AGL_BAD_FULLSCREEN: u32 = 10012;
pub const AGL_BAD_POINTER: u32 = 10014;
/// Renderer ID reported for the Systemless OpenGL renderer. The value is in
/// the Apple renderer-ID space next to AGL_RENDERER_GENERIC_ID (0x00020200).
pub const AGL_RENDERER_SYSTEMLESS_ID: i32 = 0x0002_0F00;
pub const AGL_RENDERER_GENERIC_ID: i32 = 0x0002_0200;
/// AGL interface version implemented (agl.h defines AGL_VERSION_2_0).
pub const AGL_VERSION: (i32, i32) = (2, 0);
const AGL_SINGLEBUFFER_BIT: i32 = 0x4;
const AGL_DOUBLEBUFFER_BIT: i32 = 0x8;
const AGL_MONOSCOPIC_BIT: i32 = 0x1;
const AGL_0_BIT: i32 = 0x1;
const AGL_8_BIT: i32 = 0x80;
const AGL_16_BIT: i32 = 0x400;
const AGL_24_BIT: i32 = 0x800;
const AGL_RGB555_BIT: i32 = 0x200;
const AGL_ARGB1555_BIT: i32 = 0x400;
const AGL_RGB888_BIT: i32 = 0x4000;
const AGL_ARGB8888_BIT: i32 = 0x8000;
/// Opaque AGLRendererInfo handle for the single renderer.
pub const AGL_RENDERER_INFO_HANDLE: u32 = FIRST_AGL_OBJECT - 0x100;

/// AGL 2.0 entry points beyond pixel formats, contexts, drawables and swap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PpcAglFunction {
    QueryRendererInfo,
    NextRendererInfo,
    DestroyRendererInfo,
    DescribeRenderer,
    NextPixelFormat,
    DevicesOfPixelFormat,
    GetVersion,
    Configure,
    Enable,
    Disable,
    IsEnabled,
    SetInteger,
    GetInteger,
    ErrorString,
    ResetLibrary,
    UseFont,
    SetFullScreen,
    SetOffScreen,
    CopyContext,
    SetVirtualScreen,
    GetVirtualScreen,
}

impl PpcAglFunction {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "aglQueryRendererInfo" => Self::QueryRendererInfo,
            "aglNextRendererInfo" => Self::NextRendererInfo,
            "aglDestroyRendererInfo" => Self::DestroyRendererInfo,
            "aglDescribeRenderer" => Self::DescribeRenderer,
            "aglNextPixelFormat" => Self::NextPixelFormat,
            "aglDevicesOfPixelFormat" => Self::DevicesOfPixelFormat,
            "aglGetVersion" => Self::GetVersion,
            "aglConfigure" => Self::Configure,
            "aglEnable" => Self::Enable,
            "aglDisable" => Self::Disable,
            "aglIsEnabled" => Self::IsEnabled,
            "aglSetInteger" => Self::SetInteger,
            "aglGetInteger" => Self::GetInteger,
            "aglErrorString" => Self::ErrorString,
            "aglResetLibrary" => Self::ResetLibrary,
            "aglUseFont" => Self::UseFont,
            "aglSetFullScreen" => Self::SetFullScreen,
            "aglSetOffScreen" => Self::SetOffScreen,
            "aglCopyContext" => Self::CopyContext,
            "aglSetVirtualScreen" => Self::SetVirtualScreen,
            "aglGetVirtualScreen" => Self::GetVirtualScreen,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PpcAglPixelFormatRequest {
    pub rgba: bool,
    pub double_buffered: bool,
    pub stereo: bool,
    pub fullscreen: bool,
    pub offscreen: bool,
    pub accelerated: bool,
    pub no_recovery: bool,
    pub backing_store: bool,
    pub depth_bits: Option<i32>,
    pub stencil_bits: Option<i32>,
    pub alpha_bits: Option<i32>,
    pub red_bits: Option<i32>,
    pub green_bits: Option<i32>,
    pub blue_bits: Option<i32>,
    pub pixel_bits: Option<i32>,
    pub aux_buffers: Option<i32>,
    pub other_values: Vec<(i32, i32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcAglAttributeError {
    BadPointer,
    UnsupportedAttribute(i32),
    Unterminated,
}

#[derive(Debug, Clone)]
pub struct PpcAglPixelFormat {
    pub handle: u32,
    pub request: PpcAglPixelFormatRequest,
}

#[derive(Debug, Clone)]
pub struct PpcAglState {
    next_handle: u32,
    pixel_formats: Vec<PpcAglPixelFormat>,
    error: u32,
    contexts: Vec<PpcAglContext>,
    current_context: u32,
}

#[derive(Debug, Clone)]
pub struct PpcAglContext {
    pub handle: u32,
    pub format: PpcAglPixelFormatRequest,
    pub drawable: u32,
    pub fullscreen: bool,
    /// Client memory attached by aglSetOffScreen.
    pub offscreen: Option<PpcFrontBuffer>,
    pub framebuffer: Option<ClassicGlFramebuffer>,
    pub gl: Box<GlContext>,
    /// aglEnable/aglDisable options (AGL_SWAP_RECT, AGL_RASTERIZATION, ...).
    pub enables: std::collections::BTreeSet<u32>,
    /// aglSetInteger values, up to four integers per option.
    pub integers: std::collections::BTreeMap<u32, [i32; 4]>,
    /// The handle of the first context in this context's share group.
    pub share_group: u32,
    /// Whether this context currently holds the group's shared objects. Only
    /// one context is current at a time, so the objects move to whichever
    /// member becomes current.
    pub holds_shared_objects: bool,
}

impl Default for PpcAglState {
    fn default() -> Self {
        Self {
            next_handle: FIRST_AGL_OBJECT,
            pixel_formats: Vec::new(),
            error: 0,
            contexts: Vec::new(),
            current_context: 0,
        }
    }
}

impl PpcAglState {
    pub fn set_error(&mut self, error: u32) {
        if self.error == 0 {
            self.error = error;
        }
    }

    pub fn get_error(&mut self) -> u32 {
        std::mem::take(&mut self.error)
    }

    pub fn choose_pixel_format(&mut self, request: PpcAglPixelFormatRequest) -> u32 {
        if !ppc_agl_software_format_matches(&request) {
            return 0;
        }
        let handle = self.next_handle;
        let Some(next) = handle.checked_add(4) else {
            return 0;
        };
        self.next_handle = next;
        self.pixel_formats
            .push(PpcAglPixelFormat { handle, request });
        handle
    }

    pub fn pixel_format(&self, handle: u32) -> Option<&PpcAglPixelFormat> {
        self.pixel_formats
            .iter()
            .find(|format| format.handle == handle)
    }

    pub fn describe_pixel_format(&self, handle: u32, attribute: i32) -> Option<i32> {
        let request = &self.pixel_format(handle)?.request;
        Some(match attribute {
            2 | 3 | 7 | 14..=17 => 0,
            4 => 1, // AGL_RGBA
            5 => i32::from(request.double_buffered),
            6 | 53 => 0,       // stereo, offscreen
            8..=11 => 8,       // RGBA8 color storage
            12 => 24,          // depth storage
            13 => 8,           // stencil storage
            50 => 32,          // AGL_PIXEL_SIZE
            54 => 1,           // fullscreen capable
            70 => AGL_RENDERER_SYSTEMLESS_ID,
            71 | 72 | 73 | 75 | 83 => 1, // single, no-recovery, accelerated, robust, compliant
            76 => 0,           // backing store not selected
            78 => 1,           // AGL_MP_SAFE
            80 => 1,           // AGL_WINDOW
            _ => return None,
        })
    }

    pub fn destroy_pixel_format(&mut self, handle: u32) -> bool {
        let old_len = self.pixel_formats.len();
        self.pixel_formats.retain(|format| format.handle != handle);
        self.pixel_formats.len() != old_len
    }

    pub fn create_context(&mut self, format: u32, share: u32) -> u32 {
        // A share context joins its texture objects and display lists to the
        // new context's (AGL/agl.h aglCreateContext).
        let share_group = if share == 0 {
            None
        } else {
            match self.context(share) {
                Some(context) => Some(context.share_group),
                None => return 0,
            }
        };
        let Some(format) = self
            .pixel_format(format)
            .map(|format| format.request.clone())
        else {
            return 0;
        };
        let handle = self.next_handle;
        let Some(next) = handle.checked_add(4) else {
            return 0;
        };
        self.next_handle = next;
        let gl = Box::new(GlContext::new(format.double_buffered));
        self.contexts.push(PpcAglContext {
            handle,
            format,
            drawable: 0,
            fullscreen: false,
            offscreen: None,
            framebuffer: None,
            gl,
            enables: [220].into_iter().collect(), // AGL_RASTERIZATION
            integers: Default::default(),
            share_group: share_group.unwrap_or(handle),
            holds_shared_objects: share_group.is_none(),
        });
        handle
    }

    /// Move a share group's objects into `handle`, which is about to be used.
    fn gather_shared_objects(&mut self, handle: u32) {
        let Some(group) = self.context(handle).map(|context| context.share_group) else {
            return;
        };
        let Some(holder) = self
            .contexts
            .iter()
            .position(|context| context.share_group == group && context.holds_shared_objects)
        else {
            return;
        };
        if self.contexts[holder].handle == handle {
            return;
        }
        let objects = self.contexts[holder].gl.take_shared_objects();
        self.contexts[holder].holds_shared_objects = false;
        if let Some(context) = self.context_mut(handle) {
            context.gl.put_shared_objects(objects);
            context.holds_shared_objects = true;
        }
    }

    pub fn context(&self, handle: u32) -> Option<&PpcAglContext> {
        self.contexts
            .iter()
            .find(|context| context.handle == handle)
    }

    pub fn context_mut(&mut self, handle: u32) -> Option<&mut PpcAglContext> {
        self.contexts
            .iter_mut()
            .find(|context| context.handle == handle)
    }

    pub fn destroy_context(&mut self, handle: u32) -> bool {
        let Some(index) = self
            .contexts
            .iter()
            .position(|context| context.handle == handle)
        else {
            return false;
        };
        let mut removed = self.contexts.swap_remove(index);
        if self.current_context == handle {
            self.current_context = 0;
        }
        // The group's objects outlive any one member.
        if removed.holds_shared_objects {
            if let Some(heir) = self
                .contexts
                .iter_mut()
                .find(|context| context.share_group == removed.share_group)
            {
                heir.gl.put_shared_objects(removed.gl.take_shared_objects());
                heir.holds_shared_objects = true;
            }
        }
        true
    }

    pub fn set_current_context(&mut self, handle: u32) -> bool {
        if handle != 0 && self.context(handle).is_none() {
            return false;
        }
        if handle != 0 {
            self.gather_shared_objects(handle);
        }
        self.current_context = handle;
        true
    }

    pub fn current_context(&self) -> u32 {
        self.current_context
    }

    pub fn set_drawable(
        &mut self,
        handle: u32,
        drawable: u32,
        surface: Option<PpcFrontBuffer>,
    ) -> bool {
        let Some(context) = self.context_mut(handle) else {
            return false;
        };
        if drawable == 0 {
            context.drawable = 0;
            context.fullscreen = false;
            context.offscreen = None;
            context.framebuffer = None;
            return true;
        }
        let Some(surface) = surface.filter(|surface| ppc_agl_surface_supported(surface)) else {
            return false;
        };
        let Some(framebuffer) = ClassicGlFramebuffer::new(
            surface.width,
            surface.height,
            context.format.double_buffered,
        ) else {
            return false;
        };
        context.drawable = drawable;
        context.fullscreen = false;
        context.offscreen = None;
        context.gl.attach_drawable(surface.width, surface.height);
        context.framebuffer = Some(framebuffer);
        true
    }

    /// aglSetFullScreen: attach the context to the whole main screen.
    pub fn set_full_screen(&mut self, handle: u32, screen: Option<PpcFrontBuffer>) -> bool {
        let Some(context) = self.context_mut(handle) else {
            return false;
        };
        let Some(surface) = screen.filter(|surface| ppc_agl_surface_supported(surface)) else {
            return false;
        };
        let Some(framebuffer) = ClassicGlFramebuffer::new(
            surface.width,
            surface.height,
            context.format.double_buffered,
        ) else {
            return false;
        };
        context.drawable = 0;
        context.fullscreen = true;
        context.offscreen = None;
        context.gl.attach_drawable(surface.width, surface.height);
        context.framebuffer = Some(framebuffer);
        true
    }

    /// aglSetOffScreen: render into client memory described by `surface`.
    pub fn set_off_screen(&mut self, handle: u32, surface: PpcFrontBuffer) -> bool {
        let Some(context) = self.context_mut(handle) else {
            return false;
        };
        if !ppc_agl_surface_supported(&surface) {
            return false;
        }
        let Some(framebuffer) = ClassicGlFramebuffer::new(
            surface.width,
            surface.height,
            context.format.double_buffered,
        ) else {
            return false;
        };
        context.drawable = 0;
        context.fullscreen = false;
        context.offscreen = Some(surface);
        context.gl.attach_drawable(surface.width, surface.height);
        context.framebuffer = Some(framebuffer);
        true
    }

    pub fn update_context(&mut self, handle: u32, surface: Option<PpcFrontBuffer>) -> bool {
        let Some(context) = self.context_mut(handle) else {
            return false;
        };
        if context.drawable == 0 {
            return true;
        }
        let Some(surface) = surface.filter(|surface| ppc_agl_surface_supported(surface)) else {
            return false;
        };
        let resize = context.framebuffer.as_ref().is_none_or(|framebuffer| {
            framebuffer.width() != surface.width || framebuffer.height() != surface.height
        });
        if resize {
            let Some(framebuffer) = ClassicGlFramebuffer::new(
                surface.width,
                surface.height,
                context.format.double_buffered,
            ) else {
                return false;
            };
            context.framebuffer = Some(framebuffer);
        }
        true
    }

    pub fn swap_buffers(
        &mut self,
        handle: u32,
        memory: &mut PpcSectionMem,
        surface: PpcFrontBuffer,
    ) -> bool {
        let Some(context) = self.context_mut(handle) else {
            return false;
        };
        let Some(framebuffer) = context.framebuffer.as_mut() else {
            return false;
        };
        let buffer = if context.format.double_buffered {
            ClassicGlColorBuffer::Back
        } else {
            ClassicGlColorBuffer::Front
        };
        if !framebuffer.present(buffer, memory, surface) {
            return false;
        }
        if context.format.double_buffered {
            framebuffer.swap()
        } else {
            true
        }
    }

    /// Present a single-buffered context's front buffer (glFlush/glFinish).
    pub fn flush(&mut self, handle: u32, memory: &mut PpcSectionMem, surface: PpcFrontBuffer) -> bool {
        let Some(context) = self.context_mut(handle) else {
            return false;
        };
        if context.format.double_buffered {
            return true;
        }
        let Some(framebuffer) = context.framebuffer.as_ref() else {
            return false;
        };
        framebuffer.present(ClassicGlColorBuffer::Front, memory, surface)
    }

    #[cfg(test)]
    pub fn current_mut(&mut self) -> Option<&mut PpcAglContext> {
        let handle = self.current_context;
        self.context_mut(handle)
    }

    /// aglDescribeRenderer for the single Systemless renderer.
    pub fn describe_renderer(&self, property: i32) -> Option<i32> {
        Some(match property {
            70 => AGL_RENDERER_SYSTEMLESS_ID,
            53 => 0,                       // AGL_OFFSCREEN
            54 | 71 | 72 | 73 | 75 => 1,   // fullscreen, single, no recovery, accelerated, robust
            76 => 0,                       // AGL_BACKING_STORE
            78 | 80 | 81 | 83 => 1,        // MP safe, window, multiscreen, compliant
            100 => AGL_MONOSCOPIC_BIT | AGL_SINGLEBUFFER_BIT | AGL_DOUBLEBUFFER_BIT,
            101 | 102 => 0,                // min and max level
            103 => AGL_RGB555_BIT | AGL_ARGB1555_BIT | AGL_RGB888_BIT | AGL_ARGB8888_BIT,
            104 => 0,                      // no accumulation buffers
            105 => AGL_0_BIT | AGL_16_BIT | AGL_24_BIT,
            106 => AGL_0_BIT | AGL_8_BIT,
            107 => 0,
            120 => 32 * 1024 * 1024,       // AGL_VIDEO_MEMORY
            121 => 32 * 1024 * 1024,       // AGL_TEXTURE_MEMORY
            _ => return None,
        })
    }
}

/// Drawables are 16-bit (RGB555) or 32-bit (xRGB8888) direct-color pixmaps;
/// classic Mac OS OpenGL does not render to indexed screens.
fn ppc_agl_surface_supported(surface: &PpcFrontBuffer) -> bool {
    matches!(surface.depth, 16 | 32)
}

fn ppc_agl_software_format_matches(request: &PpcAglPixelFormatRequest) -> bool {
    // The Systemless renderer executes the OpenGL 1.2 command set natively
    // rather than as emulated guest code, so it satisfies AGL_ACCELERATED
    // requests; stereo, off-screen and backing-store formats are not offered.
    if !request.rgba || request.stereo || request.offscreen || request.backing_store {
        return false;
    }
    for (requested, available) in [
        (request.red_bits, 8),
        (request.green_bits, 8),
        (request.blue_bits, 8),
        (request.alpha_bits, 8),
        (request.depth_bits, 24),
        (request.stencil_bits, 8),
        (request.pixel_bits, 32),
        (request.aux_buffers, 0),
    ] {
        if requested.is_some_and(|bits| bits < 0 || bits > available) {
            return false;
        }
    }
    request
        .other_values
        .iter()
        .all(|&(attribute, value)| match attribute {
            3 | 14..=17 | 55..=57 => value == 0,
            70 => value == AGL_RENDERER_SYSTEMLESS_ID || value == AGL_RENDERER_GENERIC_ID,
            2 => value == 0,            // AGL_BUFFER_SIZE is for color-index formats
            _ => false,
        })
}

/// Reads a terminated AGL attribute list without crossing an unmapped guest
/// region or allowing an unbounded walk through guest memory.
pub fn ppc_agl_read_pixel_format_request(
    memory: &mut PpcSectionMem,
    address: u32,
) -> Result<PpcAglPixelFormatRequest, PpcAglAttributeError> {
    if address == 0 {
        return Err(PpcAglAttributeError::BadPointer);
    }
    let mut request = PpcAglPixelFormatRequest::default();
    let mut offset = 0;
    while offset < MAX_ATTRIBUTE_WORDS {
        let word_address = address
            .checked_add(
                offset
                    .checked_mul(4)
                    .ok_or(PpcAglAttributeError::BadPointer)?,
            )
            .ok_or(PpcAglAttributeError::BadPointer)?;
        let attribute = memory
            .read_u32_be(word_address)
            .ok_or(PpcAglAttributeError::BadPointer)? as i32;
        offset += 1;
        match attribute {
            0 => return Ok(request),              // AGL_NONE
            1 | 51 | 52 | 71 | 74 | 75 | 78 => {} // supported selection policies
            4 => request.rgba = true,
            5 => request.double_buffered = true,
            6 => request.stereo = true,
            53 => request.offscreen = true,
            54 => request.fullscreen = true,
            72 => request.no_recovery = true,
            73 => request.accelerated = true,
            76 => request.backing_store = true,
            2 | 3 | 7..=17 | 50 | 55..=57 | 70 => {
                if offset >= MAX_ATTRIBUTE_WORDS {
                    return Err(PpcAglAttributeError::Unterminated);
                }
                let value_address = address
                    .checked_add(
                        offset
                            .checked_mul(4)
                            .ok_or(PpcAglAttributeError::BadPointer)?,
                    )
                    .ok_or(PpcAglAttributeError::BadPointer)?;
                let value = memory
                    .read_u32_be(value_address)
                    .ok_or(PpcAglAttributeError::BadPointer)? as i32;
                offset += 1;
                match attribute {
                    7 => request.aux_buffers = Some(value),
                    8 => request.red_bits = Some(value),
                    9 => request.green_bits = Some(value),
                    10 => request.blue_bits = Some(value),
                    11 => request.alpha_bits = Some(value),
                    12 => request.depth_bits = Some(value),
                    13 => request.stencil_bits = Some(value),
                    50 => request.pixel_bits = Some(value),
                    _ => request.other_values.push((attribute, value)),
                }
            }
            _ => return Err(PpcAglAttributeError::UnsupportedAttribute(attribute)),
        }
    }
    Err(PpcAglAttributeError::Unterminated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppc::PpcMemory;

    #[test]
    fn reads_boolean_and_value_attributes_from_guest_memory() {
        let mut memory = PpcSectionMem::new();
        let words: [u32; 8] = [4, 5, 12, 24, 13, 8, 73, 0];
        memory.add_region(
            0x1000,
            words.iter().flat_map(|word| word.to_be_bytes()).collect(),
        );
        let request = ppc_agl_read_pixel_format_request(&mut memory, 0x1000).unwrap();
        assert!(request.rgba && request.double_buffered && request.accelerated);
        assert_eq!(request.depth_bits, Some(24));
        assert_eq!(request.stencil_bits, Some(8));
        assert_eq!(request.alpha_bits, None);
    }

    #[test]
    fn rejects_unmapped_and_unterminated_attribute_lists() {
        let mut memory = PpcSectionMem::new();
        assert_eq!(
            ppc_agl_read_pixel_format_request(&mut memory, 0),
            Err(PpcAglAttributeError::BadPointer)
        );
        memory.add_region(0x1000, 12u32.to_be_bytes().to_vec());
        assert_eq!(
            ppc_agl_read_pixel_format_request(&mut memory, 0x1000),
            Err(PpcAglAttributeError::BadPointer)
        );
        memory.add_region(0x2000, vec![0, 0, 0, 4].repeat(64));
        assert_eq!(
            ppc_agl_read_pixel_format_request(&mut memory, 0x2000),
            Err(PpcAglAttributeError::Unterminated)
        );
    }

    #[test]
    fn rejects_unknown_attributes_instead_of_misreading_following_words() {
        let mut memory = PpcSectionMem::new();
        memory.add_region(
            0x1000,
            [99u32, 0]
                .iter()
                .flat_map(|word| word.to_be_bytes())
                .collect(),
        );
        assert_eq!(
            ppc_agl_read_pixel_format_request(&mut memory, 0x1000),
            Err(PpcAglAttributeError::UnsupportedAttribute(99))
        );
    }

    #[test]
    fn format_selection_tracks_opaque_lifetime_and_offers_acceleration() {
        let mut state = PpcAglState::default();
        let request = PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            depth_bits: Some(24),
            stencil_bits: Some(8),
            ..Default::default()
        };
        let handle = state.choose_pixel_format(request.clone());
        assert_ne!(handle, 0);
        assert_eq!(state.pixel_format(handle).unwrap().request, request);
        state.destroy_pixel_format(handle);
        assert!(state.pixel_format(handle).is_none());
        let accelerated = PpcAglPixelFormatRequest {
            accelerated: true,
            ..request.clone()
        };
        assert_ne!(state.choose_pixel_format(accelerated), 0);
        let stereo = PpcAglPixelFormatRequest {
            stereo: true,
            ..request
        };
        assert_eq!(state.choose_pixel_format(stereo), 0);
    }

    #[test]
    fn description_reports_actual_renderer_format_properties() {
        let mut state = PpcAglState::default();
        let handle = state.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            ..Default::default()
        });
        assert_eq!(state.describe_pixel_format(handle, 5), Some(1));
        assert_eq!(state.describe_pixel_format(handle, 12), Some(24));
        assert_eq!(state.describe_pixel_format(handle, 73), Some(1));
        assert_eq!(state.describe_pixel_format(handle, 70), Some(AGL_RENDERER_SYSTEMLESS_ID));
        assert_eq!(state.describe_pixel_format(handle, 999), None);
        state.destroy_pixel_format(handle);
        assert_eq!(state.describe_pixel_format(handle, 5), None);
    }

    #[test]
    fn context_keeps_format_after_disposal_and_swaps_to_guest_pixels() {
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        assert_ne!(context, 0);
        agl.destroy_pixel_format(format);
        assert!(agl.set_current_context(context));
        assert_eq!(agl.current_context(), context);
        let surface = PpcFrontBuffer {
            base_addr: 0x1000,
            row_bytes: 2,
            width: 1,
            height: 1,
            depth: 16,
        };
        assert!(agl.set_drawable(context, 0x2000, Some(surface)));
        assert!(agl
            .context_mut(context)
            .unwrap()
            .framebuffer
            .as_mut()
            .unwrap()
            .clear_color(ClassicGlColorBuffer::Back, [255, 0, 0, 255]));
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x1000, vec![0; 2]);
        assert!(agl.swap_buffers(context, &mut memory, surface));
        assert_eq!(memory.read_u16_be(0x1000), Some(0x7c00));
        assert!(agl.destroy_context(context));
        assert_eq!(agl.current_context(), 0);
    }

    #[test]
    fn opengl_library_exports_map_to_gl_agl_and_glu_targets() {
        use super::super::classic_gl::functions::GlFunction;
        use super::super::{dispatcher_target_for_import, PpcImportDispatcherTarget};
        assert_eq!(
            dispatcher_target_for_import("OpenGLLibrary", "glVertex3f"),
            PpcImportDispatcherTarget::Gl(GlFunction::from_name("glVertex3f").unwrap())
        );
        assert_eq!(
            dispatcher_target_for_import("OpenGLLibrary", "aglQueryRendererInfo"),
            PpcImportDispatcherTarget::Agl(PpcAglFunction::QueryRendererInfo)
        );
        assert_eq!(
            dispatcher_target_for_import("OpenGLUtility", "gluPerspective"),
            PpcImportDispatcherTarget::Glu(super::super::classic_gl::glu::GluFunction::Perspective)
        );
        assert_eq!(
            dispatcher_target_for_import("OpenGLMemory", "glmGetInteger"),
            PpcImportDispatcherTarget::GlmGetInteger
        );
        assert_eq!(
            dispatcher_target_for_import("OpenGLLibrary", "glNotAFunction"),
            PpcImportDispatcherTarget::Unsupported
        );
    }

    #[test]
    fn renderer_description_matches_the_offered_formats() {
        let state = PpcAglState::default();
        assert_eq!(state.describe_renderer(73), Some(1));
        assert_eq!(state.describe_renderer(70), Some(AGL_RENDERER_SYSTEMLESS_ID));
        assert_eq!(state.describe_renderer(100), Some(0xD));
        assert_eq!(state.describe_renderer(105), Some(AGL_0_BIT | AGL_16_BIT | AGL_24_BIT));
        assert_eq!(state.describe_renderer(999), None);
    }

    #[test]
    fn gl_commands_render_into_the_context_and_swap_to_32_bit_pixels() {
        use super::super::classic_gl::{call, functions::GlFunction};
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            accelerated: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        let surface = PpcFrontBuffer {
            base_addr: 0x1000,
            row_bytes: 8,
            width: 2,
            height: 1,
            depth: 32,
        };
        assert!(agl.set_drawable(context, 0x2000, Some(surface)));
        assert!(agl.set_current_context(context));
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x1000, vec![0; 8]);
        let mut cpu = ppc::PpcCpu::new();
        cpu.fpr[1] = 1.0f64.to_bits();
        cpu.fpr[4] = 1.0f64.to_bits();
        let gl = agl.current_mut().unwrap();
        let framebuffer = gl.framebuffer.as_mut();
        call(&mut gl.gl, framebuffer, GlFunction::from_name("glClearColor").unwrap(), &cpu, &mut memory);
        cpu.gpr[3] = super::super::classic_gl::consts::COLOR_BUFFER_BIT;
        let gl = agl.current_mut().unwrap();
        let framebuffer = gl.framebuffer.as_mut();
        call(&mut gl.gl, framebuffer, GlFunction::from_name("glClear").unwrap(), &cpu, &mut memory);
        assert!(agl.swap_buffers(context, &mut memory, surface));
        assert_eq!(memory.read_u32_be(0x1000), Some(0x00FF_0000));
        assert_eq!(memory.read_u32_be(0x1004), Some(0x00FF_0000));
    }

    #[test]
    fn shared_contexts_hand_texture_objects_to_the_current_member() {
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            ..Default::default()
        });
        let first = agl.create_context(format, 0);
        assert_ne!(first, 0);
        assert!(agl.set_current_context(first));
        agl.context_mut(first)
            .unwrap()
            .gl
            .state
            .textures
            .insert(7, Default::default());

        let second = agl.create_context(format, first);
        assert_ne!(second, 0);
        assert_eq!(agl.create_context(format, 0xDEAD_0000), 0);
        assert!(agl.set_current_context(second));
        assert!(agl.context(second).unwrap().gl.state.textures.contains_key(&7));
        assert!(agl.context(first).unwrap().gl.state.textures.is_empty());

        // The objects outlive the context holding them.
        assert!(agl.destroy_context(second));
        assert!(agl.context(first).unwrap().gl.state.textures.contains_key(&7));
    }
}
