//! Still-image codecs embedded in QuickTime PICT ImageDescription records.

pub(super) fn decode(
    codec: u32,
    data: &[u8],
    width: usize,
    height: usize,
) -> Option<image::RgbImage> {
    let format = match codec.to_be_bytes() {
        [b'j', b'p', b'e', b'g'] => image::ImageFormat::Jpeg,
        [b'p', b'n', b'g', b' '] => image::ImageFormat::Png,
        _ => return None,
    };
    let pixels = width.checked_mul(height)?;
    if width == 0 || height == 0 || pixels > 16 * 1024 * 1024 || data.len() > 64 * 1024 * 1024 {
        return None;
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(data), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(u32::try_from(width).ok()?);
    limits.max_image_height = Some(u32::try_from(height).ok()?);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    if decoded.width() as usize != width || decoded.height() as usize != height {
        return None;
    }
    Some(decoded.into_rgb8())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    #[test]
    fn png_preserves_colours_and_rejects_wrong_dimensions_and_damage() {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                &[255, 0, 0, 0, 255, 0],
                2,
                1,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        let codec = u32::from_be_bytes(*b"png ");
        assert_eq!(
            decode(codec, &bytes, 2, 1).unwrap().into_raw(),
            [255, 0, 0, 0, 255, 0]
        );
        assert!(decode(codec, &bytes, 1, 1).is_none());
        assert!(decode(codec, &bytes, 3, 1).is_none());
        assert!(decode(codec, &bytes[..bytes.len() / 2], 2, 1).is_none());
        assert!(decode(codec, &bytes, usize::MAX, 2).is_none());
    }

    #[test]
    fn jpeg_decodes_real_payload_and_checks_declared_size() {
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 100)
            .encode(
                &[220, 30, 20].repeat(64),
                8,
                8,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        let codec = u32::from_be_bytes(*b"jpeg");
        let decoded = decode(codec, &bytes, 8, 8).unwrap();
        let pixel = decoded.get_pixel(0, 0).0;
        assert!(pixel[0] > 210 && pixel[1] < 40 && pixel[2] < 30);
        assert!(decode(codec, &bytes, 8, 9).is_none());
        assert!(decode(codec, b"invalid", 8, 8).is_none());
        assert!(decode(u32::from_be_bytes(*b"fake"), &bytes, 8, 8).is_none());
    }
}
