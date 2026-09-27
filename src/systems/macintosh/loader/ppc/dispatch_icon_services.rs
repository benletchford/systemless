//! Classic Icon Services references and QuickDraw drawing for PPC Carbon.

use super::*;
use std::collections::BTreeMap;

const ICON_SIZE: usize = 32;
const ICON_PLANE_SIZE: usize = ICON_SIZE * ICON_SIZE / 8;
const CUSTOM_ICON_RESOURCE_ID: i16 = -16455;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum PpcIconKey {
    File(String),
    Type(u32, u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PpcIconRef {
    key: PpcIconKey,
    owners: u32,
    /// A 32 by 32 image followed by its transparency mask, one bit per pixel.
    monochrome: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PpcIconRefState {
    objects: BTreeMap<u32, PpcIconRef>,
    by_key: BTreeMap<PpcIconKey, u32>,
}

impl PpcIconRefState {
    #[cfg(test)]
    pub(super) fn owners(&self, reference: u32) -> Option<u32> {
        self.objects.get(&reference).map(|object| object.owners)
    }

    fn acquire(
        &mut self,
        key: PpcIconKey,
        monochrome: Vec<u8>,
        process_memory_manager: &mut ProcessNativeMemoryManager,
        memory: &mut PpcSectionMem,
        heap_cursor: &mut u32,
        last_mem_error: &mut i16,
    ) -> u32 {
        if let Some(&reference) = self.by_key.get(&key) {
            if let Some(object) = self.objects.get_mut(&reference) {
                object.owners = object.owners.saturating_add(1);
                return reference;
            }
        }
        let reference = process_memory_manager.new_native_ptr(memory, 16, true);
        ppc_apply_process_native_allocator(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
        if reference == 0 {
            return 0;
        }
        if memory.write_bytes(reference, &[0; 16]).is_none() {
            let _ = process_memory_manager.dispose_native_ptr(reference);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            return 0;
        }
        self.by_key.insert(key.clone(), reference);
        self.objects.insert(
            reference,
            PpcIconRef {
                key,
                owners: 1,
                monochrome,
            },
        );
        reference
    }

    fn release(
        &mut self,
        reference: u32,
        process_memory_manager: &mut ProcessNativeMemoryManager,
        memory: &mut PpcSectionMem,
        heap_cursor: &mut u32,
        last_mem_error: &mut i16,
    ) -> bool {
        let Some(object) = self.objects.get_mut(&reference) else {
            return false;
        };
        if object.owners > 1 {
            object.owners -= 1;
            return true;
        }
        let key = object.key.clone();
        self.objects.remove(&reference);
        self.by_key.remove(&key);
        let _ = process_memory_manager.dispose_native_ptr(reference);
        ppc_apply_process_native_allocator(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
        true
    }
}

fn set_icon_bit(plane: &mut [u8], x: usize, y: usize) {
    plane[y * 4 + x / 8] |= 0x80 >> (x & 7);
}

fn generic_icon(folder: bool) -> Vec<u8> {
    let mut image = vec![0; ICON_PLANE_SIZE * 2];
    let (pixels, mask) = image.split_at_mut(ICON_PLANE_SIZE);
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let inside = if folder {
                (5..=26).contains(&x) && (10..=25).contains(&y)
                    || (5..=14).contains(&x) && (7..=10).contains(&y)
            } else {
                (7..=24).contains(&x) && (3..=28).contains(&y)
                    && !(x >= 20 && y < 8 && x + y < 28)
            };
            if !inside {
                continue;
            }
            set_icon_bit(mask, x, y);
            let border = if folder {
                x == 5 || x == 26 || y == 25 || y == 10 || (y == 7 && x <= 14)
            } else {
                x == 7 || x == 24 || y == 28 || y == 3 || (x >= 19 && y == 8)
            };
            if border {
                set_icon_bit(pixels, x, y);
            }
        }
    }
    image
}

fn file_icon(path: &str, folder: bool, resources: &[PpcVfsResourceRecord]) -> Vec<u8> {
    if let Some(resource) = resources.iter().find(|resource| {
        resource.path.eq_ignore_ascii_case(path)
            && resource.res_id == CUSTOM_ICON_RESOURCE_ID
            && resource.res_type == u32::from_be_bytes(*b"ICN#")
            && resource.data.len() >= ICON_PLANE_SIZE * 2
    }) {
        return resource.data[..ICON_PLANE_SIZE * 2].to_vec();
    }
    if let Some(resource) = resources.iter().find(|resource| {
        resource.path.eq_ignore_ascii_case(path)
            && resource.res_id == CUSTOM_ICON_RESOURCE_ID
            && resource.res_type == u32::from_be_bytes(*b"ICON")
            && resource.data.len() >= ICON_PLANE_SIZE
    }) {
        let image = &resource.data[..ICON_PLANE_SIZE];
        return [image, image].concat();
    }
    generic_icon(folder)
}

fn plot_icon(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
    rect_ptr: u32,
    icon: &PpcIconRef,
) -> i16 {
    let Some(rect) = ppc_read_rect(memory, rect_ptr) else {
        return PPC_PARAM_ERR;
    };
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, current_gworld) else {
        return PPC_PARAM_ERR;
    };
    let (top, left, bottom, right) = surface.local_rect(rect);
    let width = right - left;
    let height = bottom - top;
    if width <= 0 || height <= 0 {
        return PPC_PARAM_ERR;
    }
    let front = surface.front_buffer;
    for y in 0..height {
        for x in 0..width {
            let sx = (x * ICON_SIZE as i32 / width).clamp(0, 31) as usize;
            let sy = (y * ICON_SIZE as i32 / height).clamp(0, 31) as usize;
            let offset = sy * 4 + sx / 8;
            let bit = 0x80 >> (sx & 7);
            if icon.monochrome[ICON_PLANE_SIZE + offset] & bit == 0 {
                continue;
            }
            let color = if icon.monochrome[offset] & bit != 0 {
                PPC_RGB_BLACK
            } else {
                PPC_RGB_WHITE
            };
            let _ = ppc_quickdraw_write_pixel(memory, front, (left + x, top + y), color);
        }
    }
    PPC_NO_ERR
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_icon_services_import(
    binding: &PpcImportBinding,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    state: &mut PpcIconRefState,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    gworlds: &[PpcGWorldRecord],
    current_gworld: u32,
) -> Option<PpcImportAction> {
    let result = match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetIconRefFromFile => {
            let icon_out = cpu.gpr[4];
            let label_out = cpu.gpr[5];
            if (icon_out == 0 && label_out == 0)
                || (icon_out != 0 && !ppc_memory_can_write_bytes(memory, icon_out, 4))
                || (label_out != 0 && !ppc_memory_can_write_bytes(memory, label_out, 2))
            {
                PPC_PARAM_ERR
            } else {
                match ppc_path_for_fsspec(memory, vfs_directories, cpu.gpr[3]) {
                    Err(err) => err,
                    Ok(path) => {
                        let folder = ppc_directory_id_for_path(vfs_directories, &path).is_some();
                        let file = ppc_vfs_file_index(vfs_files, &path).map(|index| &vfs_files[index]);
                        let resource_file = vfs_resource_files
                            .iter()
                            .any(|record| record.path.eq_ignore_ascii_case(&path));
                        if !folder && file.is_none() && !resource_file {
                            PPC_FNF_ERR
                        } else {
                            let label = file.map_or(0, |file| (file.finder_flags >> 1) & 7);
                            let reference = if icon_out != 0 {
                                state.acquire(
                                    PpcIconKey::File(path.clone()),
                                    file_icon(&path, folder, vfs_resources),
                                    process_memory_manager,
                                    memory,
                                    heap_cursor,
                                    last_mem_error,
                                )
                            } else {
                                0
                            };
                            if icon_out != 0 && reference == 0 {
                                PPC_MEM_FULL_ERR
                            } else {
                                if icon_out != 0 {
                                    let _ = memory.write_u32_be(icon_out, reference);
                                }
                                if label_out != 0 {
                                    let _ = memory.write_u16_be(label_out, label);
                                }
                                PPC_NO_ERR
                            }
                        }
                    }
                }
            }
        }
        PpcImportDispatcherTarget::GetIconRef => {
            let icon_out = cpu.gpr[6];
            if icon_out == 0 || !ppc_memory_can_write_bytes(memory, icon_out, 4) {
                PPC_PARAM_ERR
            } else {
                let kind = cpu.gpr[5];
                let folder = kind == u32::from_be_bytes(*b"fldr");
                let reference = state.acquire(
                    PpcIconKey::Type(cpu.gpr[4], kind),
                    generic_icon(folder),
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                if reference == 0 {
                    PPC_MEM_FULL_ERR
                } else if memory.write_u32_be(icon_out, reference).is_none() {
                    PPC_PARAM_ERR
                } else {
                    PPC_NO_ERR
                }
            }
        }
        PpcImportDispatcherTarget::PlotIconRef => state
            .objects
            .get(&cpu.gpr[7])
            .map_or(PPC_PARAM_ERR, |icon| {
                plot_icon(memory, gworlds, current_gworld, cpu.gpr[3], icon)
            }),
        PpcImportDispatcherTarget::ReleaseIconRef => {
            if state.release(
                cpu.gpr[3],
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            ) {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            }
        }
        _ => return None,
    };
    Some(PpcImportAction::Return(ppc_i16_result(result)))
}
