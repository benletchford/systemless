//! Native macOS application identity derived from the foreground guest.

use std::ffi::c_uchar;
use std::slice;

use objc2::ClassType;
use objc2_app_kit::{
    NSApplication, NSBitmapFormat, NSBitmapImageRep, NSDeviceRGBColorSpace, NSImage,
};
use objc2_foundation::{MainThreadMarker, NSSize};
use systemless::game::{ApplicationIcon, ApplicationIconRepresentation};

/// Classic Finder icons are artwork, whereas a modern macOS application icon
/// is a complete canvas. Give the 32px/16px artwork transparent breathing room
/// instead of letting AppKit enlarge it to fill the entire Dock tile.
const APPLICATION_ICON_CANVAS_SCALE: usize = 2;

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
    let image_width = usize::from(largest.width).checked_mul(APPLICATION_ICON_CANVAS_SCALE)?;
    let image_height = usize::from(largest.height).checked_mul(APPLICATION_ICON_CANVAS_SCALE)?;
    let image = unsafe {
        NSImage::initWithSize(
            NSImage::alloc(),
            NSSize::new(image_width as f64, image_height as f64),
        )
    };

    let mut added = false;
    for representation in &icon.representations {
        let Some((width, height, rgba)) = padded_representation(representation) else {
            continue;
        };
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
        let bitmap_data = unsafe { slice::from_raw_parts_mut(bitmap.bitmapData(), rgba.len()) };
        bitmap_data.copy_from_slice(&rgba);
        unsafe { image.addRepresentation(&bitmap) };
        added = true;
    }

    added.then_some(image)
}

fn padded_representation(
    representation: &ApplicationIconRepresentation,
) -> Option<(usize, usize, Vec<u8>)> {
    let source_width = usize::from(representation.width);
    let source_height = usize::from(representation.height);
    let source_len = source_width.checked_mul(source_height)?.checked_mul(4)?;
    if representation.rgba.len() != source_len {
        return None;
    }

    let width = source_width.checked_mul(APPLICATION_ICON_CANVAS_SCALE)?;
    let height = source_height.checked_mul(APPLICATION_ICON_CANVAS_SCALE)?;
    let mut rgba = vec![0; width.checked_mul(height)?.checked_mul(4)?];
    let left = (width - source_width) / 2;
    let top = (height - source_height) / 2;
    for row in 0..source_height {
        let source_start = row * source_width * 4;
        let destination_start = ((top + row) * width + left) * 4;
        rgba[destination_start..destination_start + source_width * 4]
            .copy_from_slice(&representation.rgba[source_start..source_start + source_width * 4]);
    }

    Some((width, height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_icon_artwork_is_centered_on_a_double_sized_canvas() {
        let representation = ApplicationIconRepresentation {
            width: 2,
            height: 2,
            rgba: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        };

        let (width, height, rgba) = padded_representation(&representation).unwrap();

        assert_eq!((width, height), (4, 4));
        assert_eq!(&rgba[0..20], &[0; 20]);
        assert_eq!(&rgba[20..28], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&rgba[28..36], &[0; 8]);
        assert_eq!(&rgba[36..44], &[9, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(&rgba[44..], &[0; 20]);
    }

    #[test]
    fn malformed_icon_representation_is_ignored() {
        let representation = ApplicationIconRepresentation {
            width: 2,
            height: 2,
            rgba: vec![0; 15],
        };

        assert!(padded_representation(&representation).is_none());
    }
}
