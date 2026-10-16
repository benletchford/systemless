//! Resource loading and list installation share live guest handles with the Resource Manager.
use super::{bad, data, incoming, merge, record::Record, validate_list, Menus, EMPTY};
use crate::{
    font::Strike,
    memory::{GuestPtr, Memory},
    process::Process,
    Result,
};
impl Menus {
    pub(super) fn validate_menus(
        &self,
        memory: &Memory,
        process: &mut Process,
        list: &[u8],
    ) -> Result<()> {
        validate_list(list)?;
        let regular = usize::from(u16::from_be_bytes(list[..2].try_into().unwrap()));
        for partition in [&list[6..6 + regular], &list[12 + regular..]] {
            for entry in partition.as_chunks::<6>().0 {
                let h = u32::from_be_bytes(entry[..4].try_into().unwrap());
                let bytes = data(memory, process, h)?;
                Record::parse(&bytes)?;
            }
        }
        Ok(())
    }
    pub(super) fn measure_menu(
        &self,
        memory: &mut Memory,
        process: &mut Process,
        bytes: &mut [u8],
    ) -> Result<i32> {
        let menu = Record::parse(bytes)?;
        let plain = crate::text::font_bytes(memory, process, 0, 0, 0)?;
        let font = Strike::parse(&plain)?;
        let title = font.width(menu.title)?;
        let mut width = 32i32;
        let mut height = 0i32;
        for item in &menu.items {
            if item.text == b"-" {
                height += 6;
                continue;
            }
            let styled = crate::text::font_bytes(memory, process, 0, 0, item.style)?;
            let face = Strike::parse(&styled)?;
            let icon = if item.icon == 0 || item.command == 0x1c {
                0
            } else if matches!(item.command, 0x1d | 0x1e) {
                16
            } else {
                32
            };
            width = width.max(
                face.width(item.text)?
                    + icon
                    + 32
                    + if item.command > 0x20 || item.command == 0x1b {
                        32
                    } else {
                        0
                    },
            );
            height += 16.max(icon + 2);
        }
        let ports = process
            .ports
            .ok_or_else(|| bad("menu sizing requires InitGraf"))?;
        let screen = crate::bitmap::Bitmap::read(memory, ports.screen_bitmap)?.bounds;
        height = height.min(
            (screen.bottom - screen.top - i32::from(memory.u16(GuestPtr(0xbaa))? as i16)).max(0),
        );
        for (at, value) in [(2, width), (4, height)] {
            bytes[at..at + 2].copy_from_slice(
                &i16::try_from(value)
                    .map_err(|_| bad("menu dimensions overflow"))?
                    .to_be_bytes(),
            );
        }
        Ok(title)
    }
    pub(super) fn load_resources(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        id: i16,
        bar: bool,
    ) -> Result<u32> {
        self.check(memory, process)?;
        let resource = process.get_resource(memory, if bar { *b"MBAR" } else { *b"MENU" }, id)?;
        if resource == 0 {
            if process.res_error == 0 {
                process.res_error = -192;
            }
            return Ok(0);
        }
        let ids = if bar {
            super::record::bar_ids(&data(memory, process, resource)?)?
        } else {
            vec![id]
        };
        let mut colors = data(memory, process, self.colors)?;
        if bar {
            colors.clear();
        }
        let mut changes = std::collections::BTreeMap::new();
        let mut handles = Vec::new();
        let mut left = 11i32;
        let mut list = vec![0; 12 + ids.len() * 6];
        list[..2].copy_from_slice(&(ids.len() as u16 * 6).to_be_bytes());
        list[4..6].copy_from_slice(&id.to_be_bytes());
        for (i, id) in ids.into_iter().enumerate() {
            let h = if !bar {
                resource
            } else {
                process.get_resource(memory, *b"MENU", id)?
            };
            if h == 0 {
                if process.res_error == 0 {
                    process.res_error = -192;
                }
                return Ok(0);
            }
            let mut bytes = data(memory, process, h)?;
            let advance = self.measure_menu(memory, process, &mut bytes)?;
            changes.insert(h, bytes);
            handles.push(h);
            let offset = 6 + i * 6;
            list[offset..offset + 4].copy_from_slice(&h.to_be_bytes());
            list[offset + 4..offset + 6].copy_from_slice(
                &i16::try_from(left)
                    .map_err(|_| bad("menu title position overflow"))?
                    .to_be_bytes(),
            );
            left = left
                .checked_add(advance + 13)
                .ok_or_else(|| bad("menu title position overflow"))?;
            let color = process.get_resource(memory, *b"mctb", id)?;
            if color != 0 {
                colors = merge(colors, &incoming(&data(memory, process, color)?)?)?;
            } else if process.mem_error != 0 {
                process.res_error = process.mem_error;
                return Ok(0);
            }
        }
        if handles.is_empty() {
            list[..2].copy_from_slice(&EMPTY[..2]);
            left = 0;
        }
        list[2..4].copy_from_slice(
            &i16::try_from(left)
                .map_err(|_| bad("menu title position overflow"))?
                .to_be_bytes(),
        );
        super::background(&colors)?;
        let result = if bar {
            process.new_handle(memory, list.len() as u32, false)?
        } else {
            resource
        };
        if result == 0 {
            process.res_error = process.mem_error;
            return Ok(0);
        }
        if bar {
            changes.insert(result, list);
        }
        changes.insert(self.colors, colors);
        let staged = process.replace_handles(memory, changes.into_iter().collect());
        if staged.is_err() || process.mem_error != 0 {
            let error = process.mem_error;
            if bar {
                process.dispose_handle(memory, result)?;
            }
            process.mem_error = error;
            staged?;
            process.res_error = error;
            return Ok(0);
        }
        process.res_error = 0;
        Ok(result)
    }
}
