//! Native macOS application identity derived from the foreground guest.

use std::ffi::c_uchar;
use std::slice;

use objc2::ClassType;
use objc2_app_kit::{
    NSApplication, NSBitmapFormat, NSBitmapImageRep, NSDeviceRGBColorSpace, NSImage,
};
use objc2_foundation::{MainThreadMarker, NSSize};
use systemless::game::ApplicationIcon;

/// Replace the running process's Dock/application-switcher icon.
///
/// Passing `None` restores AppKit's default application icon.
pub fn set_application_icon(icon: Option<&ApplicationIcon>) {
    let mtm = MainThreadMarker::new().expect("application identity must update on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    let image = icon.and_then(make_image);
    unsafe { app.setApplicationIconImage(image.as_deref()) };
}

fn make_image(icon: &ApplicationIcon) -> Option<objc2::rc::Retained<NSImage>> {
    let largest = icon.representations.iter().max_by_key(|representation| {
        u32::from(representation.width) * u32::from(representation.height)
    })?;
    let image = unsafe {
        NSImage::initWithSize(
            NSImage::alloc(),
            NSSize::new(f64::from(largest.width), f64::from(largest.height)),
        )
    };

    let mut added = false;
    for representation in &icon.representations {
        let width = usize::from(representation.width);
        let height = usize::from(representation.height);
        let Some(expected_len) = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
        else {
            continue;
        };
        if representation.rgba.len() != expected_len {
            continue;
        }
        let bitmap = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                std::ptr::null_mut::<*mut c_uchar>(),
                width as isize,
                height as isize,
                8,
                4,
                true,
                false,
                NSDeviceRGBColorSpace,
                NSBitmapFormat::AlphaNonpremultiplied,
                width.saturating_mul(4) as isize,
                32,
            )
        };
        let Some(bitmap) = bitmap else {
            continue;
        };
        let bitmap_data = unsafe { slice::from_raw_parts_mut(bitmap.bitmapData(), expected_len) };
        bitmap_data.copy_from_slice(&representation.rgba);
        unsafe { image.addRepresentation(&bitmap) };
        added = true;
    }

    added.then_some(image)
}
