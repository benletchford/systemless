//! Native macOS application identity derived from the foreground guest.

use std::ffi::c_uchar;
use std::slice;

use objc2::ClassType;
use objc2_app_kit::{
    NSApplication, NSBitmapFormat, NSBitmapImageRep, NSDeviceRGBColorSpace, NSImage,
};
use objc2_foundation::{MainThreadMarker, NSSize};
use systemless::game::{ApplicationIcon, ApplicationIconRepresentation};

/// Classic icons are visually denser than modern macOS icons. Keep the guest
/// artwork at 80% of the canvas so it matches neighboring Dock icon footprints.
const APPLICATION_ICON_CANVAS_NUMERATOR: usize = 5;
const APPLICATION_ICON_CANVAS_DENOMINATOR: usize = 4;

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
    let (image_width, image_height) = image_dimensions(icon)?;
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

fn image_dimensions(icon: &ApplicationIcon) -> Option<(usize, usize)> {
    let largest = icon.representations.iter().max_by_key(|representation| {
        u32::from(representation.width) * u32::from(representation.height)
    })?;
    Some((
        canvas_dimension(usize::from(largest.width))?,
        canvas_dimension(usize::from(largest.height))?,
    ))
}

fn canvas_dimension(source: usize) -> Option<usize> {
    source
        .checked_mul(APPLICATION_ICON_CANVAS_NUMERATOR)?
        .checked_div(APPLICATION_ICON_CANVAS_DENOMINATOR)
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

    let width = canvas_dimension(source_width)?;
    let height = canvas_dimension(source_height)?;
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
    use systemless::game::ApplicationIconRepresentation;

    #[test]
    fn classic_icon_uses_eighty_percent_of_canvas() {
        let icon = ApplicationIcon {
            representations: vec![
                ApplicationIconRepresentation {
                    width: 32,
                    height: 32,
                    rgba: vec![0; 32 * 32 * 4],
                },
                ApplicationIconRepresentation {
                    width: 16,
                    height: 16,
                    rgba: vec![0; 16 * 16 * 4],
                },
            ],
        };

        assert_eq!(image_dimensions(&icon), Some((40, 40)));
        assert_eq!(canvas_dimension(16), Some(20));
    }

    #[test]
    fn classic_icon_artwork_is_centered_on_canvas() {
        let representation = ApplicationIconRepresentation {
            width: 16,
            height: 16,
            rgba: vec![255; 16 * 16 * 4],
        };

        let (width, height, rgba) = padded_representation(&representation).unwrap();

        assert_eq!((width, height), (20, 20));
        assert_eq!(&rgba[..(2 * width + 2) * 4], vec![0; (2 * width + 2) * 4]);
        assert_eq!(&rgba[(2 * width + 2) * 4..(2 * width + 3) * 4], &[255; 4]);
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
