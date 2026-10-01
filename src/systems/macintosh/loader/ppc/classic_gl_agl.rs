//! Classic AGL pixel-format requests decoded from guest `GLint` arrays.
//!
//! Attribute values and the boolean/value distinction come from Apple's
//! AGL/agl.h (Mac OS X 10.2.8 SDK). Capability selection happens separately.
//! <https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX10.2.8.sdk/System/Library/Frameworks/AGL.framework/Versions/A/Headers/agl.h>

use super::PpcSectionMem;
use ppc::PpcMemory;

const MAX_ATTRIBUTE_WORDS: u32 = 64;
const FIRST_AGL_OBJECT: u32 = 0x0500_0000;
pub const AGL_BAD_ATTRIBUTE: u32 = 10000;
pub const AGL_BAD_PIXELFMT: u32 = 10002;
pub const AGL_BAD_GDEV: u32 = 10006;
pub const AGL_BAD_VALUE: u32 = 10008;
pub const AGL_BAD_POINTER: u32 = 10014;

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
}

impl Default for PpcAglState {
    fn default() -> Self {
        Self {
            next_handle: FIRST_AGL_OBJECT,
            pixel_formats: Vec::new(),
            error: 0,
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
            6 | 53 | 73 => 0,  // stereo, offscreen, accelerated
            8..=11 => 8,       // RGBA8 color storage
            12 => 24,          // depth storage
            13 => 8,           // stencil storage
            50 => 32,          // AGL_PIXEL_SIZE
            54 => 1,           // fullscreen capable
            70 => 0x0002_0200, // AGL_RENDERER_GENERIC_ID
            76 => 0,           // backing store not selected by the software surface
            80 => 1,           // AGL_WINDOW
            _ => return None,
        })
    }

    pub fn destroy_pixel_format(&mut self, handle: u32) -> bool {
        let old_len = self.pixel_formats.len();
        self.pixel_formats.retain(|format| format.handle != handle);
        self.pixel_formats.len() != old_len
    }
}

fn ppc_agl_software_format_matches(request: &PpcAglPixelFormatRequest) -> bool {
    // AGL_ACCELERATED asks for hardware rendering. The guest-backed software
    // surface must never be advertised as a hardware renderer.
    if !request.rgba
        || request.accelerated
        || request.stereo
        || request.offscreen
        || request.backing_store
    {
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
            70 => value == 0x0002_0200, // AGL_RENDERER_GENERIC_ID
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
    fn software_format_selection_tracks_opaque_lifetime_and_declines_acceleration() {
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
            ..request
        };
        assert_eq!(state.choose_pixel_format(accelerated), 0);
    }

    #[test]
    fn description_reports_actual_software_format_properties() {
        let mut state = PpcAglState::default();
        let handle = state.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            ..Default::default()
        });
        assert_eq!(state.describe_pixel_format(handle, 5), Some(1));
        assert_eq!(state.describe_pixel_format(handle, 12), Some(24));
        assert_eq!(state.describe_pixel_format(handle, 73), Some(0));
        assert_eq!(state.describe_pixel_format(handle, 999), None);
        state.destroy_pixel_format(handle);
        assert_eq!(state.describe_pixel_format(handle, 5), None);
    }
}
