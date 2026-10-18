/// Which pixels the presenter must show, and where they sit in the frame buffer
/// that carries them.
///
/// Both the software presenter and Metal use this validated view, so cropping
/// keeps the original buffer stride and selects the same source pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RasterLayout {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub buffer_width: u32,
}

impl RasterLayout {
    /// The rectangle covering all of `size`, for a frame presented uncropped.
    pub(super) fn whole(size: (u32, u32)) -> Self {
        Self {
            left: 0,
            top: 0,
            width: size.0,
            height: size.1,
            buffer_width: size.0,
        }
    }

    /// Byte offset of the rectangle's first texel inside a buffer of `byte_len`
    /// bytes, or `None` when the rectangle reaches past the buffer.
    pub(super) fn texel_byte_offset(&self, byte_len: usize) -> Option<usize> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        if self.left.checked_add(self.width)? > self.buffer_width {
            return None;
        }
        let last_row = u64::from(self.top).checked_add(u64::from(self.height) - 1)?;
        let last_texel = last_row
            .checked_mul(u64::from(self.buffer_width))?
            .checked_add(u64::from(self.left))?
            .checked_add(u64::from(self.width))?;
        if last_texel.checked_mul(size_of::<u32>() as u64)? > u64::try_from(byte_len).ok()? {
            return None;
        }
        let first_texel = u64::from(self.top)
            .checked_mul(u64::from(self.buffer_width))?
            .checked_add(u64::from(self.left))?;
        usize::try_from(first_texel.checked_mul(size_of::<u32>() as u64)?).ok()
    }
}
