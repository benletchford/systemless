//! Keep the qualified Metal default while software presentation is qualified.
use super::{
    metal_present::MetalPresenter, raster_layout::RasterLayout, software_present::SoftwarePresenter,
};
use std::{rc::Rc, sync::Arc};
use systemless::{display::CursorImage, memory::CompactPresentation};
use winit::window::Window;

pub(super) enum Presenter {
    Metal(MetalPresenter),
    Software(SoftwarePresenter),
}
impl Presenter {
    pub(super) fn backend_name(&self) -> &'static str {
        match self {
            Self::Metal(_) => "metal",
            Self::Software(_) => "software",
        }
    }

    pub(super) fn new(window: Rc<Window>, software: bool) -> Result<Self, String> {
        if software {
            SoftwarePresenter::new(window).map(Self::Software)
        } else {
            MetalPresenter::new(window).map(Self::Metal)
        }
    }
    pub(super) fn set_transactional_presentation(&self, enabled: bool) {
        if let Self::Metal(p) = self {
            p.set_transactional_presentation(enabled);
        }
    }
    pub(super) fn supports_compact(&self) -> bool {
        match self {
            Self::Metal(p) => p.supports_compact(),
            Self::Software(_) => false,
        }
    }
    pub(super) fn present(
        &mut self,
        pixels: &[u32],
        width: u32,
        height: u32,
        dw: u32,
        dh: u32,
    ) -> Result<(), String> {
        match self {
            Self::Metal(p) => p.present(pixels, width, height, dw, dh),
            Self::Software(p) => p.present(pixels, RasterLayout::whole((width, height)), (dw, dh)),
        }
    }
    pub(super) fn present_owned(
        &mut self,
        pixels: Vec<u32>,
        layout: RasterLayout,
        dw: u32,
        dh: u32,
    ) -> Result<Vec<u32>, String> {
        match self {
            Self::Metal(p) => p.present_owned(pixels, layout, dw, dh),
            Self::Software(p) => {
                p.present(&pixels, layout, (dw, dh))?;
                Ok(pixels)
            }
        }
    }
    pub(super) fn present_guest_frame(
        &mut self,
        framebuffer: &[u8],
        mode: (u32, u32, u16, u16, u16),
        crop: (u32, u32, u32, u32),
        palette: &[u32; 256],
        cursor: Option<(&CursorImage, (i16, i16))>,
        target: (u32, u32),
        force: bool,
    ) -> Result<bool, String> {
        match self {
            Self::Metal(p) => {
                p.present_guest_frame(framebuffer, mode, crop, palette, cursor, target, force)
            }
            Self::Software(_) => Ok(false),
        }
    }
    pub(super) fn present_compact(
        &mut self,
        frame: &Arc<CompactPresentation>,
        crop: (u32, u32, u32, u32),
        target: (u32, u32),
        force: bool,
    ) -> Result<bool, String> {
        match self {
            Self::Metal(p) => p.present_compact(frame, crop, target, force),
            Self::Software(_) => Ok(false),
        }
    }
}
