//! Classic Image Compression Manager file importers.
//! ABI selectors and Pascal frames: Apple Universal Interfaces 3.4,
//! ImageCompression.h (GetGraphicsImporterForFile and GraphicsImport*).

use super::*;

pub(crate) struct ClassicGraphicsImporter {
    image: image::RgbImage,
    bounds: (i16, i16, i16, i16),
    port: u32,
    device: u32,
}

fn writable(bus: &MacMemoryBus, ptr: u32, size: u32) -> bool {
    ptr != 0
        && ptr
            .checked_add(size)
            .is_some_and(|end| end <= bus.ram_size())
}

impl super::super::dispatch::TrapDispatcher {
    pub(super) fn classic_open_graphics_importer(
        &mut self,
        bus: &mut MacMemoryBus,
        spec: u32,
        out: u32,
    ) -> i16 {
        if !writable(bus, out, 4) {
            return -50;
        }
        bus.write_long(out, 0);
        if !writable(bus, spec, 70) {
            return -50;
        }
        let name = super::super::types::read_fsspec_name(bus, spec);
        let Some(path) =
            self.vfs_key_for_fsspec(bus.read_word(spec) as i16, bus.read_long(spec + 2), &name)
        else {
            return -43;
        };
        let Some(bytes) = self.vfs.get(&path) else {
            return -43;
        };
        let Ok(format) = image::guess_format(bytes) else {
            return -8961;
        };
        if !matches!(
            format,
            image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::Gif
        ) {
            return -8961;
        }
        let reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
        let Ok((width, height)) = reader.into_dimensions() else {
            return -8969;
        };
        if width == 0
            || height == 0
            || width > 8192
            || height > 8192
            || width.saturating_mul(height) > 16 * 1024 * 1024
        {
            return -50;
        }
        let Ok(image) = image::load_from_memory_with_format(bytes, format) else {
            return -8969;
        };
        // The RGB drawing path cannot yet honor image transparency.
        if image.color().has_alpha() && image.to_rgba8().pixels().any(|pixel| pixel[3] != 255) {
            return -8961;
        }
        let instance = self.next_synthetic_component_instance;
        let Some(next) = instance.checked_add(1) else {
            return -108;
        };
        self.next_synthetic_component_instance = next;
        self.classic_graphics_importers.insert(
            instance,
            ClassicGraphicsImporter {
                image: image.to_rgb8(),
                bounds: (0, 0, height as i16, width as i16),
                port: *self.current_port,
                device: *self.current_gdevice,
            },
        );
        bus.write_long(out, instance);
        0
    }

    pub(super) fn classic_graphics_importer_call(
        &mut self,
        bus: &mut MacMemoryBus,
        instance: u32,
        selector: u16,
        size: u32,
        sp: u32,
    ) -> i32 {
        let Some(importer) = self.classic_graphics_importers.get_mut(&instance) else {
            return -3000;
        };
        match (selector, size) {
            (0x000E | 0x0015, 4) => {
                let out = bus.read_long(sp + 4);
                if !writable(bus, out, 8) {
                    return -50;
                }
                let bounds = if selector == 0x000E {
                    (
                        0,
                        0,
                        importer.image.height() as i16,
                        importer.image.width() as i16,
                    )
                } else {
                    importer.bounds
                };
                for (index, value) in [bounds.0, bounds.1, bounds.2, bounds.3]
                    .into_iter()
                    .enumerate()
                {
                    bus.write_word(out + index as u32 * 2, value as u16);
                }
            }
            (0x0014, 4) => {
                let ptr = bus.read_long(sp + 4);
                if !writable(bus, ptr, 8) {
                    return -50;
                }
                let rect = (
                    bus.read_word(ptr) as i16,
                    bus.read_word(ptr + 2) as i16,
                    bus.read_word(ptr + 4) as i16,
                    bus.read_word(ptr + 6) as i16,
                );
                if rect.0 >= rect.2 || rect.1 >= rect.3 {
                    return -50;
                }
                importer.bounds = rect;
            }
            (0x0010, 8) => {
                let port = bus.read_long(sp + 8);
                if !writable(bus, port, 108) {
                    return -50;
                }
                importer.port = port;
                importer.device = bus.read_long(sp + 4);
            }
            (0x0011, 8) => {
                let device = bus.read_long(sp + 4);
                let port = bus.read_long(sp + 8);
                if !writable(bus, device, 4) || !writable(bus, port, 4) {
                    return -50;
                }
                bus.write_long(device, importer.device);
                bus.write_long(port, importer.port);
            }
            (0x000F, 0) => {
                // Move the record out while drawing so no image-sized clone is needed.
                let importer = self.classic_graphics_importers.remove(&instance).unwrap();
                self.blit_rgb_into_port(
                    bus,
                    importer.port,
                    importer.bounds,
                    importer.image.width() as usize,
                    importer.image.height() as usize,
                    importer.image.as_raw(),
                );
                self.classic_graphics_importers.insert(instance, importer);
            }
            _ => return -3000, // badComponentSelector, rather than pretending unsupported calls succeeded.
        }
        0
    }
}
