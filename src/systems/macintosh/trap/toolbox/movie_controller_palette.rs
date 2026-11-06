//! Indexed-movie palette requests owned by a classic movie controller.
use super::super::dispatch::TrapDispatcher;
use super::*;

impl TrapDispatcher {
    pub(super) fn release_classic_controller_palette(
        &mut self,
        bus: &mut MacMemoryBus,
        instance: u32,
    ) {
        let Some(palette) = self
            .movie_controllers
            .get_mut(&instance)
            .and_then(|c| c.palette.take())
        else {
            return;
        };
        // Unlink an owner disposed beneath another controller on this window.
        for controller in self.movie_controllers.values_mut() {
            if let Some(owned) = &mut controller.palette {
                if owned
                    .previous
                    .is_some_and(|previous| previous.0 == palette.handle)
                {
                    owned.previous = palette.previous;
                }
            }
        }
        let restore = self
            .window_palettes
            .get(&palette.window)
            .is_some_and(|current| current.0 == palette.handle);
        if restore {
            let (handle, updates) = palette.previous.unwrap_or((0, 0));
            self.set_window_palette_association(palette.window, handle, updates);
        }
        self.clear_palette_tracking(palette.handle);
        let pointer = bus.read_long(palette.handle);
        if pointer != 0 {
            bus.free(pointer);
        }
        bus.free(palette.handle);
        if restore {
            self.activate_associated_palette_for_window(bus, palette.window);
        }
    }

    pub(super) fn prepare_classic_controller_palette(
        &mut self,
        bus: &mut MacMemoryBus,
        instance: u32,
    ) {
        let Some(controller) = self.movie_controllers.get(&instance).copied() else {
            return;
        };
        // mcFlagsUseWindowPalette opts out of the controller's management.
        // QuickTime Components (1993), pp. 2-21, 2-27.
        if controller.control_flags & 8 != 0 {
            return;
        }
        let Some(movie) = self.movie_states.get(&controller.movie) else {
            return;
        };
        let window = movie.gworld_port;
        let indexed_window =
            self.window_list.contains(&window) && bus.read_word(window + 6) & 0xc000 != 0;
        if !indexed_window {
            self.release_classic_controller_palette(bus, instance);
            return;
        }
        let pixmap = bus.read_long(bus.read_long(window + 2));
        let depth = bus.read_word(pixmap + 32);
        let Some(video) = movie
            .media
            .as_ref()
            .filter(|video| matches!(video.depth, 1 | 2 | 4 | 8))
        else {
            self.release_classic_controller_palette(bus, instance);
            return;
        };
        if !matches!(depth, 1 | 2 | 4 | 8) {
            self.release_classic_controller_palette(bus, instance);
            return;
        }
        if let Some(owned) = controller.palette {
            if owned.movie == controller.movie && owned.window == window {
                let current = self.window_palettes.get(&window).map(|palette| palette.0);
                if current == Some(owned.handle)
                    || self.movie_controllers.values().any(|other| {
                        other
                            .palette
                            .is_some_and(|palette| Some(palette.handle) == current)
                    })
                {
                    return;
                }
                self.set_window_palette_association(window, owned.handle, -0x2000);
                self.activate_associated_palette_for_window(bus, window);
                return;
            }
        }
        let colors = if let Some(clut) = &video.clut {
            clut.iter()
                .take(256)
                .map(|color| color.map(|component| u16::from(component) * 257))
                .collect::<Vec<_>>()
        } else if let Some((clut, count)) = Self::standard_mac_indexed_clut(video.depth) {
            clut[..count].to_vec()
        } else {
            return;
        };
        if colors.is_empty() {
            return;
        }
        self.release_classic_controller_palette(bus, instance);
        let table = bus.alloc(8 + colors.len() as u32 * 8);
        if table == 0 {
            return;
        }
        let handle = bus.alloc(4);
        if handle == 0 {
            bus.free(table);
            return;
        }
        bus.write_long(handle, table);
        bus.write_long(table, 0);
        bus.write_word(table + 4, 0);
        bus.write_word(table + 6, colors.len() as u16 - 1);
        for (index, color) in colors.iter().enumerate() {
            let entry = table + 8 + index as u32 * 8;
            bus.write_word(entry, index as u16);
            for (channel, component) in color.iter().enumerate() {
                bus.write_word(entry + 2 + channel as u32 * 2, *component);
            }
        }
        let palette = self.create_palette_from_ctab(bus, colors.len() as u16, handle, 2, 0);
        bus.free(table);
        bus.free(handle);
        if palette == 0 {
            return;
        }
        let previous = self.window_palettes.get(&window).copied();
        self.movie_controllers.get_mut(&instance).unwrap().palette =
            Some(super::super::dispatch::ClassicMovieControllerPalette {
                handle: palette,
                window,
                movie: controller.movie,
                previous,
            });
        self.set_window_palette_association(window, palette, -0x2000);
        self.activate_associated_palette_for_window(bus, window);
    }
}
