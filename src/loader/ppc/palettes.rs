//! PowerPC Palette Manager, CLUT, inverse table, and device color routines.

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PpcPaletteAllocation {
    pub(crate) palette: u32,
    pub(crate) gdevice: u32,
    pub(crate) entry_to_index: Vec<Option<u8>>,
    pub(crate) entry_mappings: Vec<PpcPaletteEntryMapping>,
    pub(crate) reserved_indices: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PpcPaletteEntryMapping {
    Unallocated,
    MatchOnly(u8),
    TolerantInstalled(u8),
    AnimatedReserved(u8),
    Explicit(u8),
}

impl PpcPaletteEntryMapping {
    pub(crate) fn index(self) -> Option<u8> {
        match self {
            Self::Unallocated => None,
            Self::MatchOnly(index)
            | Self::TolerantInstalled(index)
            | Self::AnimatedReserved(index)
            | Self::Explicit(index) => Some(index),
        }
    }

    pub(crate) fn animated_index(self) -> Option<u8> {
        match self {
            Self::AnimatedReserved(index) => Some(index),
            _ => None,
        }
    }

    pub(crate) fn owned_index(self) -> Option<u8> {
        match self {
            Self::TolerantInstalled(index) | Self::AnimatedReserved(index) => Some(index),
            _ => None,
        }
    }
}

pub(crate) fn ppc_copy_palette_resource(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    palette_id: i16,
) -> u32 {
    let Some(index) = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        u32::from_be_bytes(*b"pltt"),
        palette_id,
        false,
    ) else {
        return 0;
    };
    let resource = &vfs_resources[index].data;
    let Some(entries) = resource
        .get(..2)
        .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
    else {
        return 0;
    };
    let byte_count = 16usize.saturating_add(usize::from(entries).saturating_mul(16));
    if resource.len() < byte_count {
        return 0;
    }
    let handle = ppc_allocator_view_allocate_handle_with_bytes(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        &resource[..byte_count],
    );
    if handle == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    if let Some(palette) = memory.read_u32_be(handle) {
        // GetNewPalette initializes the private fields in the copied Palette
        // record; only pmEntries and the ColorInfo array come from 'pltt'.
        // Inside Macintosh Volume VI (1991), pp. 20-16, 20-19.
        for offset in (2..16).step_by(2) {
            let _ = memory.write_u16_be(palette + offset, 0);
        }
        for entry in 0..u32::from(entries) {
            let private = palette + 16 + entry * 16 + 10;
            let _ = memory.write_bytes(private, &[0; 6]);
        }
    }
    *last_mem_error = PPC_NO_ERR;
    handle
}

pub(crate) fn ppc_palette_entry_color(
    memory: &mut PpcSectionMem,
    palette_handle: u32,
    entry: u16,
) -> Option<(PpcRgbColor, bool)> {
    let palette_ptr = memory.read_u32_be(palette_handle)?;
    let entry_count = memory.read_u16_be(palette_ptr)?;
    if palette_ptr == 0 || entry >= entry_count {
        return None;
    }
    let info_ptr = palette_ptr
        .checked_add(16)?
        .checked_add(u32::from(entry).checked_mul(16)?)?;
    Some((
        PpcRgbColor {
            red: memory.read_u16_be(info_ptr)?,
            green: memory.read_u16_be(info_ptr + 2)?,
            blue: memory.read_u16_be(info_ptr + 4)?,
        },
        memory.read_u16_be(info_ptr + 6)? & 0x0008 != 0,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_palette_to_ctab(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    toolbox_startup: &mut PpcToolboxStartupState,
    palette_handle: u32,
    ctable_handle: u32,
) {
    // Inside Macintosh Volume VI (1991), p. 20-24: Palette2CTab copies
    // every palette color and resizes the destination ColorTable to match.
    // A NIL source or destination is explicitly a no-op.
    let Some(palette_ptr) = (palette_handle != 0)
        .then(|| memory.read_u32_be(palette_handle))
        .flatten()
        .filter(|ptr| *ptr != 0)
    else {
        return;
    };
    let Some(entry_count) = memory.read_u16_be(palette_ptr).map(u32::from) else {
        return;
    };
    if ctable_handle == 0 {
        return;
    }
    let Some(byte_count) = entry_count
        .checked_mul(8)
        .and_then(|entries| entries.checked_add(8))
    else {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return;
    };
    let resize_result = ppc_allocator_view_resize_handle(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        ctable_handle,
        byte_count,
    );
    if resize_result != PPC_NO_ERR {
        *last_mem_error = resize_result;
        return;
    }
    let Some(ctable_ptr) = memory.read_u32_be(ctable_handle).filter(|ptr| *ptr != 0) else {
        *last_mem_error = PPC_PARAM_ERR;
        return;
    };
    let seed = ppc_next_ct_seed(toolbox_startup);
    let header_written = memory.write_u32_be(ctable_ptr, seed).is_some()
        && memory.write_u16_be(ctable_ptr + 4, 0).is_some()
        && memory
            .write_u16_be(
                ctable_ptr + 6,
                if entry_count == 0 {
                    u16::MAX
                } else {
                    (entry_count - 1) as u16
                },
            )
            .is_some();
    let colors_written = (0..entry_count).all(|entry| {
        let info_ptr = palette_ptr + 16 + entry * 16;
        let spec_ptr = ctable_ptr + 8 + entry * 8;
        let (Some(red), Some(green), Some(blue)) = (
            memory.read_u16_be(info_ptr),
            memory.read_u16_be(info_ptr + 2),
            memory.read_u16_be(info_ptr + 4),
        ) else {
            return false;
        };
        memory.write_u16_be(spec_ptr, entry as u16).is_some()
            && memory.write_u16_be(spec_ptr + 2, red).is_some()
            && memory.write_u16_be(spec_ptr + 4, green).is_some()
            && memory.write_u16_be(spec_ptr + 6, blue).is_some()
    });
    *last_mem_error = if header_written && colors_written {
        PPC_NO_ERR
    } else {
        PPC_PARAM_ERR
    };
}
pub(crate) fn ppc_palette_allocated_index(
    toolbox_startup: &PpcToolboxStartupState,
    palette: u32,
    gdevice: u32,
    entry: usize,
) -> Option<u8> {
    toolbox_startup
        .palette_allocations
        .iter()
        .find(|allocation| allocation.palette == palette && allocation.gdevice == gdevice)
        .and_then(|allocation| allocation.entry_mappings.get(entry).copied())
        .and_then(PpcPaletteEntryMapping::index)
}

pub(crate) fn ppc_palette_indexed_override(
    toolbox_startup: &PpcToolboxStartupState,
    palette: u32,
    gdevice: u32,
    entry: usize,
) -> Option<Option<u8>> {
    toolbox_startup
        .palette_allocations
        .iter()
        .find(|allocation| allocation.palette == palette && allocation.gdevice == gdevice)
        .and_then(|allocation| allocation.entry_mappings.get(entry).copied())
        .map(|mapping| match mapping {
            PpcPaletteEntryMapping::AnimatedReserved(index)
            | PpcPaletteEntryMapping::Explicit(index) => Some(index),
            _ => None,
        })
}

pub(crate) fn ppc_release_palette_allocations(
    toolbox_startup: &mut PpcToolboxStartupState,
    palette: u32,
) -> Vec<(u32, u8, bool, bool)> {
    let active_devices = toolbox_startup
        .active_device_palettes
        .iter()
        .filter_map(|(gdevice, active_palette)| (*active_palette == palette).then_some(*gdevice))
        .collect::<Vec<_>>();
    toolbox_startup
        .active_device_palettes
        .retain(|_, active_palette| *active_palette != palette);
    let mut released = Vec::new();
    toolbox_startup.palette_allocations.retain(|allocation| {
        if allocation.palette == palette {
            released.extend(allocation.entry_mappings.iter().filter_map(|mapping| {
                mapping.owned_index().map(|index| {
                    (
                        allocation.gdevice,
                        index,
                        mapping.animated_index().is_some(),
                        active_devices.contains(&allocation.gdevice),
                    )
                })
            }));
            false
        } else {
            true
        }
    });
    released
}

pub(crate) fn ppc_closest_available_clut_index(
    red: u16,
    green: u16,
    blue: u16,
    clut: &[[u16; 3]; 256],
    unavailable: &[bool; 256],
    claimed: &[bool; 256],
) -> u8 {
    clut.iter()
        .enumerate()
        .filter(|(index, _)| !unavailable[*index] && !claimed[*index])
        .min_by_key(|(_, color)| {
            color[0].abs_diff(red) as u64 * color[0].abs_diff(red) as u64
                + color[1].abs_diff(green) as u64 * color[1].abs_diff(green) as u64
                + color[2].abs_diff(blue) as u64 * color[2].abs_diff(blue) as u64
        })
        .map_or(0, |(index, _)| index as u8)
}

pub(crate) fn ppc_release_palette_allocations_and_restore(
    memory: &mut PpcSectionMem,
    toolbox_startup: &mut PpcToolboxStartupState,
    palette: u32,
    current_gdevice: u32,
    screen_clut: &mut [[u16; 3]; 256],
    color_manager_clut: &mut [[u16; 3]; 256],
) {
    let defaults = TrapDispatcher::standard_mac_8bpp_clut();
    for (gdevice, index, animated, was_active) in
        ppc_release_palette_allocations(toolbox_startup, palette)
    {
        // Tolerant colors are replaceable, not reserved. An inactive
        // palette's stale tolerant mapping must not restore or preserve a
        // device cell when that palette is later disposed.
        if !animated && !was_active {
            continue;
        }
        let slot = usize::from(index);
        let still_reserved = toolbox_startup
            .palette_allocations
            .iter()
            .any(|allocation| {
                allocation.gdevice == gdevice
                    && allocation
                        .entry_mappings
                        .iter()
                        .any(|mapping| mapping.animated_index() == Some(index))
            });
        let still_active_owned = toolbox_startup
            .active_device_palettes
            .get(&gdevice)
            .is_some_and(|active_palette| {
                toolbox_startup
                    .palette_allocations
                    .iter()
                    .any(|allocation| {
                        allocation.gdevice == gdevice
                            && allocation.palette == *active_palette
                            && allocation
                                .entry_mappings
                                .iter()
                                .any(|mapping| mapping.owned_index() == Some(index))
                    })
            });
        let still_owned_during_active_tolerant_release = !animated
            && was_active
            && toolbox_startup
                .palette_allocations
                .iter()
                .any(|allocation| {
                    allocation.gdevice == gdevice
                        && allocation
                            .entry_mappings
                            .iter()
                            .any(|mapping| mapping.owned_index() == Some(index))
                });
        let protected = ppc_device_clut_protected(toolbox_startup, gdevice)[slot];
        if !still_reserved
            && !still_active_owned
            && !still_owned_during_active_tolerant_release
            && !protected
        {
            let mut device_clut = ppc_gdevice_ctable_handle(memory, gdevice)
                .and_then(|handle| ppc_read_ctable_clut(memory, handle, &defaults))
                .unwrap_or(defaults);
            device_clut[slot] = defaults[slot];
            if gdevice == current_gdevice {
                screen_clut[slot] = defaults[slot];
                color_manager_clut[slot] = defaults[slot];
                device_clut = *screen_clut;
            }
            ppc_write_device_color_table(memory, gdevice, &device_clut, toolbox_startup);
        }
    }
}

#[derive(Clone, Copy)]
struct PpcStolenPaletteReservation {
    index: u8,
    palette: u32,
    entry: usize,
    allocation_index: usize,
}

fn ppc_find_animated_palette_index_to_steal(
    memory: &mut PpcSectionMem,
    toolbox_startup: &PpcToolboxStartupState,
    palette_handle: u32,
    gdevice: u32,
    protected: &[bool; 256],
    globally_reserved: &[bool; 256],
    claimed: &[bool; 256],
) -> Option<PpcStolenPaletteReservation> {
    // When no unreserved cell remains for a tolerant color, the active
    // palette may cancel another palette's animated reservation on the same
    // device. Inside Macintosh Volume VI (1991), pp. 20-6, 20-10, 20-13.
    let mut candidates = toolbox_startup
        .palette_allocations
        .iter()
        .enumerate()
        .filter(|(_, allocation)| {
            allocation.gdevice == gdevice && allocation.palette != palette_handle
        })
        .flat_map(|(allocation_index, allocation)| {
            allocation
                .entry_mappings
                .iter()
                .copied()
                .enumerate()
                .filter_map(move |(entry, mapping)| {
                    let index = mapping.animated_index()?;
                    let slot = usize::from(index);
                    (allocation.reserved_indices.contains(&index)
                        && !protected[slot]
                        && !globally_reserved[slot]
                        && !claimed[slot])
                        .then_some(PpcStolenPaletteReservation {
                            index,
                            palette: allocation.palette,
                            entry,
                            allocation_index,
                        })
                })
        })
        .collect::<Vec<_>>();
    candidates
        .sort_unstable_by_key(|candidate| (candidate.index, candidate.palette, candidate.entry));
    candidates.into_iter().find(|candidate| {
        memory
            .read_u32_be(candidate.palette)
            .filter(|ptr| *ptr != 0)
            .and_then(|ptr| ptr.checked_add(16 + candidate.entry as u32 * 16))
            .and_then(|info| ppc_read_rgb_color(memory, info))
            .is_some()
    })
}

fn ppc_finalize_stolen_palette_reservation(
    memory: &mut PpcSectionMem,
    toolbox_startup: &mut PpcToolboxStartupState,
    stolen: PpcStolenPaletteReservation,
    screen_clut: &[[u16; 3]; 256],
    unavailable: &[bool; 256],
    current_reserved: &[u8],
) {
    let victim_color = memory
        .read_u32_be(stolen.palette)
        .filter(|ptr| *ptr != 0)
        .and_then(|ptr| ptr.checked_add(16 + stolen.entry as u32 * 16))
        .and_then(|info| ppc_read_rgb_color(memory, info));
    let Some(victim_color) = victim_color else {
        return;
    };
    let mut victim_unavailable = *unavailable;
    for index in current_reserved {
        victim_unavailable[usize::from(*index)] = true;
    }
    let replacement = ppc_closest_available_clut_index(
        victim_color.red,
        victim_color.green,
        victim_color.blue,
        screen_clut,
        &victim_unavailable,
        &[false; 256],
    );
    let allocation = &mut toolbox_startup.palette_allocations[stolen.allocation_index];
    allocation.entry_mappings[stolen.entry] = PpcPaletteEntryMapping::MatchOnly(replacement);
    allocation.entry_to_index[stolen.entry] = Some(replacement);
    allocation
        .reserved_indices
        .retain(|reserved| *reserved != stolen.index);
}

pub(crate) fn ppc_apply_palette(
    memory: &mut PpcSectionMem,
    palette_handle: u32,
    gdevice: u32,
    screen_clut: &mut [[u16; 3]; 256],
    toolbox_startup: &mut PpcToolboxStartupState,
) -> bool {
    const PALETTE_HEADER_SIZE: u32 = 16;
    const PALETTE_COLOR_INFO_SIZE: u32 = 16;
    const PM_TOLERANT: u16 = 0x0002;
    const PM_ANIMATED: u16 = 0x0004;
    const PM_EXPLICIT: u16 = 0x0008;
    const PM_INHIBIT_C8: u16 = 0x2000;

    let Some(palette_ptr) = memory.read_u32_be(palette_handle) else {
        return false;
    };
    if palette_ptr == 0 {
        return false;
    }
    let Some(entry_count) = memory.read_u16_be(palette_ptr) else {
        return false;
    };
    let entry_count = usize::from(entry_count);
    let mut entries = Vec::with_capacity(entry_count);
    for entry in 0..entry_count {
        let info = palette_ptr + PALETTE_HEADER_SIZE + entry as u32 * PALETTE_COLOR_INFO_SIZE;
        let Some(color) = ppc_read_rgb_color(memory, info) else {
            return false;
        };
        let (Some(usage), Some(tolerance)) =
            (memory.read_u16_be(info + 6), memory.read_u16_be(info + 8))
        else {
            return false;
        };
        entries.push(([color.red, color.green, color.blue], usage, tolerance));
    }
    let previous = toolbox_startup
        .palette_allocations
        .iter()
        .find(|allocation| allocation.palette == palette_handle && allocation.gdevice == gdevice)
        .cloned();
    let device_protected = ppc_device_clut_protected(toolbox_startup, gdevice);
    let device_reserved = ppc_device_clut_reserved(toolbox_startup, gdevice);
    // Rebuild from device defaults so reservations removed by a usage or
    // size change cannot leave stale animated RGB values behind. Stable
    // animated entries retain their recorded indexes and are written again
    // below. Inside Macintosh Volume VI (1991), p. 20-10.
    if let Some(previous) = &previous {
        let defaults = TrapDispatcher::standard_mac_8bpp_clut();
        for index in previous
            .entry_mappings
            .iter()
            .filter_map(|mapping| mapping.owned_index())
        {
            let slot = usize::from(index);
            let owned_elsewhere = toolbox_startup
                .palette_allocations
                .iter()
                .any(|allocation| {
                    allocation.gdevice == gdevice
                        && allocation.palette != palette_handle
                        && allocation
                            .entry_mappings
                            .iter()
                            .any(|mapping| mapping.owned_index() == Some(index))
                });
            if !device_protected[slot] && !owned_elsewhere {
                screen_clut[slot] = defaults[slot];
            }
        }
    }
    let mut unavailable = device_reserved;
    for allocation in &toolbox_startup.palette_allocations {
        if allocation.gdevice == gdevice && allocation.palette != palette_handle {
            for index in &allocation.reserved_indices {
                unavailable[usize::from(*index)] = true;
            }
        }
    }
    let mut entry_mappings = vec![PpcPaletteEntryMapping::Unallocated; entry_count];
    let mut reserved = Vec::new();
    let mut claimed = [false; 256];
    let mut allocation_blocked = unavailable;
    allocation_blocked[0] = true;
    allocation_blocked[255] = true;
    // Inside Macintosh Volume VI (1991), pp. 20-6, 20-8--20-12: combined
    // animated/explicit and tolerant/explicit entries have priority, then
    // animated and tolerant entries; courteous colors only match.
    for priority in 0..5 {
        for (entry, (rgb, usage, tolerance)) in entries.iter().copied().enumerate() {
            if usage & PM_INHIBIT_C8 != 0 {
                continue;
            }
            let animated = usage & PM_ANIMATED != 0;
            let tolerant = usage & PM_TOLERANT != 0;
            let explicit = usage & PM_EXPLICIT != 0;
            let entry_priority = match (animated, tolerant, explicit) {
                (true, _, true) => 0,
                (false, true, true) => 1,
                (true, _, false) => 2,
                (false, true, false) => 3,
                _ => 4,
            };
            if entry_priority != priority {
                continue;
            }
            if explicit {
                let index = entry as u8;
                if !animated && !tolerant {
                    entry_mappings[entry] = PpcPaletteEntryMapping::Explicit(index);
                    continue;
                }
                let slot = usize::from(index);
                if !device_protected[slot] && !allocation_blocked[slot] && !claimed[slot] {
                    screen_clut[slot] = rgb;
                    claimed[slot] = true;
                    if animated {
                        reserved.push(index);
                        entry_mappings[entry] = PpcPaletteEntryMapping::AnimatedReserved(index);
                    } else {
                        entry_mappings[entry] = PpcPaletteEntryMapping::TolerantInstalled(index);
                    }
                } else if animated {
                    // Inside Macintosh Volume VI (1991), p. 20-13:
                    // unallocated animated+explicit entries are courteous.
                    entry_mappings[entry] =
                        PpcPaletteEntryMapping::MatchOnly(ppc_closest_available_clut_index(
                            rgb[0],
                            rgb[1],
                            rgb[2],
                            screen_clut,
                            &unavailable,
                            &claimed,
                        ));
                } else {
                    // Unallocated tolerant+explicit entries retain ordinary
                    // tolerant semantics and may install the color elsewhere.
                    let closest = ppc_closest_available_clut_index(
                        rgb[0],
                        rgb[1],
                        rgb[2],
                        screen_clut,
                        &unavailable,
                        &claimed,
                    );
                    let candidate = screen_clut[usize::from(closest)];
                    let difference = (0..3)
                        .map(|channel| rgb[channel].abs_diff(candidate[channel]))
                        .max()
                        .unwrap_or(0);
                    if difference <= tolerance {
                        entry_mappings[entry] = PpcPaletteEntryMapping::MatchOnly(closest);
                    } else if let Some((slot, stolen)) = (0..256)
                        .find(|slot| {
                            !device_protected[*slot]
                                && !allocation_blocked[*slot]
                                && !claimed[*slot]
                        })
                        .map(|slot| (slot, None))
                        .or_else(|| {
                            ppc_find_animated_palette_index_to_steal(
                                memory,
                                toolbox_startup,
                                palette_handle,
                                gdevice,
                                &device_protected,
                                &device_reserved,
                                &claimed,
                            )
                            .map(|stolen| (usize::from(stolen.index), Some(stolen)))
                        })
                    {
                        if stolen.is_some() {
                            unavailable[slot] = false;
                        }
                        screen_clut[slot] = rgb;
                        claimed[slot] = true;
                        entry_mappings[entry] =
                            PpcPaletteEntryMapping::TolerantInstalled(slot as u8);
                        if let Some(stolen) = stolen {
                            ppc_finalize_stolen_palette_reservation(
                                memory,
                                toolbox_startup,
                                stolen,
                                screen_clut,
                                &unavailable,
                                &reserved,
                            );
                        }
                    } else {
                        entry_mappings[entry] = PpcPaletteEntryMapping::MatchOnly(closest);
                    }
                }
                continue;
            }
            if animated {
                let old = previous
                    .as_ref()
                    .and_then(|allocation| allocation.entry_mappings.get(entry))
                    .copied()
                    .and_then(PpcPaletteEntryMapping::animated_index)
                    .filter(|index| {
                        let slot = usize::from(*index);
                        !device_protected[slot] && !allocation_blocked[slot] && !claimed[slot]
                    });
                let aligned = (entry < 256).then_some(entry as u8).filter(|index| {
                    let slot = usize::from(*index);
                    !device_protected[slot] && !allocation_blocked[slot] && !claimed[slot]
                });
                let chosen = old.or(aligned).or_else(|| {
                    (0..256).find_map(|slot| {
                        let needed_by_later_aligned_entry = slot > entry
                            && slot < entries.len()
                            && entries[slot].1 & PM_ANIMATED != 0
                            && entries[slot].1 & PM_EXPLICIT == 0
                            && entries[slot].1 & PM_INHIBIT_C8 == 0;
                        (!needed_by_later_aligned_entry
                            && !device_protected[slot]
                            && !allocation_blocked[slot]
                            && !claimed[slot])
                            .then_some(slot as u8)
                    })
                });
                if let Some(index) = chosen {
                    let slot = usize::from(index);
                    entry_mappings[entry] = PpcPaletteEntryMapping::AnimatedReserved(index);
                    claimed[slot] = true;
                    screen_clut[slot] = rgb;
                    reserved.push(index);
                } else {
                    entry_mappings[entry] =
                        PpcPaletteEntryMapping::MatchOnly(ppc_closest_available_clut_index(
                            rgb[0],
                            rgb[1],
                            rgb[2],
                            screen_clut,
                            &unavailable,
                            &claimed,
                        ));
                }
                continue;
            }
            let closest = ppc_closest_available_clut_index(
                rgb[0],
                rgb[1],
                rgb[2],
                screen_clut,
                &unavailable,
                &claimed,
            );
            if tolerant {
                let candidate = screen_clut[usize::from(closest)];
                let difference = (0..3)
                    .map(|channel| rgb[channel].abs_diff(candidate[channel]))
                    .max()
                    .unwrap_or(0);
                if difference <= tolerance {
                    entry_mappings[entry] = PpcPaletteEntryMapping::MatchOnly(closest);
                    continue;
                }
                if let Some((slot, stolen)) = (0..256)
                    .find(|slot| {
                        !device_protected[*slot] && !allocation_blocked[*slot] && !claimed[*slot]
                    })
                    .map(|slot| (slot, None))
                    .or_else(|| {
                        ppc_find_animated_palette_index_to_steal(
                            memory,
                            toolbox_startup,
                            palette_handle,
                            gdevice,
                            &device_protected,
                            &device_reserved,
                            &claimed,
                        )
                        .map(|stolen| (usize::from(stolen.index), Some(stolen)))
                    })
                {
                    if stolen.is_some() {
                        unavailable[slot] = false;
                    }
                    screen_clut[slot] = rgb;
                    claimed[slot] = true;
                    entry_mappings[entry] = PpcPaletteEntryMapping::TolerantInstalled(slot as u8);
                    if let Some(stolen) = stolen {
                        ppc_finalize_stolen_palette_reservation(
                            memory,
                            toolbox_startup,
                            stolen,
                            screen_clut,
                            &unavailable,
                            &reserved,
                        );
                    }
                } else {
                    entry_mappings[entry] = PpcPaletteEntryMapping::MatchOnly(closest);
                }
            } else {
                entry_mappings[entry] = PpcPaletteEntryMapping::MatchOnly(closest);
            }
        }
    }
    toolbox_startup
        .palette_allocations
        .retain(|allocation| allocation.palette != palette_handle || allocation.gdevice != gdevice);
    toolbox_startup
        .palette_allocations
        .push(PpcPaletteAllocation {
            palette: palette_handle,
            gdevice,
            entry_to_index: entry_mappings
                .iter()
                .copied()
                .map(PpcPaletteEntryMapping::index)
                .collect(),
            entry_mappings,
            reserved_indices: reserved,
        });
    true
}

pub(crate) fn ppc_register_gdevice(toolbox_startup: &mut PpcToolboxStartupState, gdevice: u32) {
    if gdevice != 0 && !toolbox_startup.known_gdevices.contains(&gdevice) {
        toolbox_startup.known_gdevices.push(gdevice);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_activate_window_palette(
    memory: &mut PpcSectionMem,
    window: u32,
    gdevice: u32,
    current_gdevice: u32,
    screen_clut: &mut [[u16; 3]; 256],
    color_manager_clut: &mut [[u16; 3]; 256],
    toolbox_startup: &mut PpcToolboxStartupState,
) -> bool {
    ppc_register_gdevice(toolbox_startup, gdevice);
    let defaults = TrapDispatcher::standard_mac_8bpp_clut();
    let mut device_clut = if gdevice == current_gdevice {
        *screen_clut
    } else {
        ppc_gdevice_ctable_handle(memory, gdevice)
            .and_then(|handle| ppc_read_ctable_clut(memory, handle, &defaults))
            .unwrap_or(defaults)
    };
    let assigned_palette = memory
        .read_u32_be(window.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET))
        .unwrap_or(0);
    // Inside Macintosh Volume VI (1991), pp. 20-16, 20-19: a window with no
    // assigned palette uses the application's default palette.
    let palette_handle = if assigned_palette != 0 {
        assigned_palette
    } else {
        toolbox_startup.application_palette
    };
    if palette_handle == 0 {
        // Inside Macintosh Volume VI (1991), p. 20-16: when neither a window
        // palette nor an application-default palette exists, Palette Manager
        // uses the system default palette, falling back to its built-in one.
        let mut default_clut = defaults;
        // Merely hiding a window does not release its animated entries, so a
        // default environment must retain every still-reserved device cell.
        // Inside Macintosh Volume VI (1991), pp. 20-7, 20-10.
        for allocation in &toolbox_startup.palette_allocations {
            if allocation.gdevice == gdevice {
                for index in &allocation.reserved_indices {
                    default_clut[usize::from(*index)] = device_clut[usize::from(*index)];
                }
            }
        }
        let protected = ppc_device_clut_protected(toolbox_startup, gdevice);
        let reserved = ppc_device_clut_reserved(toolbox_startup, gdevice);
        for index in 0..256 {
            if protected[index] || reserved[index] {
                default_clut[index] = device_clut[index];
            }
        }
        device_clut = default_clut;
        if gdevice == current_gdevice {
            *screen_clut = device_clut;
            *color_manager_clut = device_clut;
        }
        toolbox_startup.active_device_palettes.remove(&gdevice);
        ppc_write_device_color_table(memory, gdevice, &device_clut, toolbox_startup);
        return true;
    }
    let previous_device_clut = device_clut;
    let applied_to_device = ppc_apply_palette(
        memory,
        palette_handle,
        gdevice,
        &mut device_clut,
        toolbox_startup,
    );
    if !applied_to_device {
        return false;
    }
    toolbox_startup
        .active_device_palettes
        .insert(gdevice, palette_handle);
    if gdevice == current_gdevice {
        *screen_clut = device_clut;
        for index in 0..256 {
            if device_clut[index] != previous_device_clut[index] {
                color_manager_clut[index] = device_clut[index];
            }
        }
    }
    ppc_write_device_color_table(memory, gdevice, &device_clut, toolbox_startup);
    true
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_activate_front_window_palette(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    fallback_gdevice: u32,
    screen_clut: &mut [[u16; 3]; 256],
    color_manager_clut: &mut [[u16; 3]; 256],
    toolbox_startup: &mut PpcToolboxStartupState,
) -> Option<u32> {
    let front = ppc_front_visible_window(memory, gworlds);
    let window = front.unwrap_or(PPC_MAIN_GWORLD);
    let gdevice = front.map_or(PPC_MAIN_GDEVICE, |front| {
        ppc_gworld_device(gworlds, front).unwrap_or(fallback_gdevice)
    });
    let assigned_palette = memory
        .read_u32_be(window.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET))
        .unwrap_or(0);
    if assigned_palette == 0
        && toolbox_startup.application_palette == 0
        && !toolbox_startup
            .active_device_palettes
            .contains_key(&gdevice)
    {
        // Inside Macintosh Volume V (1986), p. V-143: SetEntries directly
        // changes the current GDevice's color table. A front-window change
        // with no Palette Manager environment must not replace those colors;
        // there is no managed palette to deactivate or activate.
        return Some(window);
    }
    ppc_activate_window_palette(
        memory,
        window,
        gdevice,
        fallback_gdevice,
        screen_clut,
        color_manager_clut,
        toolbox_startup,
    )
    .then_some(window)
}

pub(crate) fn ppc_write_device_color_table(
    memory: &mut PpcSectionMem,
    gdevice_handle: u32,
    clut: &[[u16; 3]; 256],
    toolbox_startup: &mut PpcToolboxStartupState,
) {
    // On an indexed device, ActivatePalette asks the Color Manager to modify
    // the device entries needed by the active palette. The device PixMap's
    // ColorTable therefore describes the same logical colors as the hardware
    // CLUT used to display its pixel indexes.
    // Inside Macintosh Volume VI (1991), pp. 20-15, 20-20.
    let color_table = memory
        .read_u32_be(gdevice_handle)
        .filter(|device| *device != 0)
        .and_then(|device| memory.read_u32_be(device + 22))
        .filter(|pixmap_handle| *pixmap_handle != 0)
        .and_then(|pixmap_handle| memory.read_u32_be(pixmap_handle))
        .filter(|pixmap| *pixmap != 0)
        .and_then(|pixmap| memory.read_u32_be(pixmap + 42))
        .filter(|table_handle| *table_handle != 0)
        .and_then(|table_handle| memory.read_u32_be(table_handle))
        .filter(|table| *table != 0);
    let Some(color_table) = color_table else {
        return;
    };
    let entry_count = usize::from(memory.read_u16_be(color_table + 6).unwrap_or(255))
        .saturating_add(1)
        .min(clut.len());
    for (index, [red, green, blue]) in clut.iter().copied().take(entry_count).enumerate() {
        let entry = color_table + 8 + index as u32 * 8;
        let _ = memory.write_u16_be(entry, index as u16);
        let _ = memory.write_u16_be(entry + 2, red);
        let _ = memory.write_u16_be(entry + 4, green);
        let _ = memory.write_u16_be(entry + 6, blue);
    }
    let seed = ppc_next_ct_seed(toolbox_startup);
    let _ = memory.write_u32_be(color_table, seed);
}

pub(crate) fn ppc_next_ct_seed(toolbox_startup: &mut PpcToolboxStartupState) -> u32 {
    let seed = toolbox_startup.next_ct_seed.max(1024);
    toolbox_startup.next_ct_seed = seed.wrapping_add(1).max(1024);
    seed
}

pub(crate) fn ppc_device_clut_protected(
    toolbox_startup: &PpcToolboxStartupState,
    gdevice: u32,
) -> [bool; 256] {
    if gdevice == PPC_MAIN_GDEVICE {
        toolbox_startup.clut_protected
    } else {
        toolbox_startup
            .clut_protected_by_device
            .get(&gdevice)
            .copied()
            .unwrap_or([false; 256])
    }
}

pub(crate) fn ppc_device_clut_reserved(
    toolbox_startup: &PpcToolboxStartupState,
    gdevice: u32,
) -> [bool; 256] {
    if gdevice == PPC_MAIN_GDEVICE {
        toolbox_startup.clut_reserved
    } else {
        toolbox_startup
            .clut_reserved_by_device
            .get(&gdevice)
            .copied()
            .unwrap_or([false; 256])
    }
}

pub(crate) fn ppc_device_clut_protected_mut(
    toolbox_startup: &mut PpcToolboxStartupState,
    gdevice: u32,
) -> &mut [bool; 256] {
    if gdevice == PPC_MAIN_GDEVICE {
        &mut toolbox_startup.clut_protected
    } else {
        toolbox_startup
            .clut_protected_by_device
            .entry(gdevice)
            .or_insert([false; 256])
    }
}

pub(crate) fn ppc_device_clut_reserved_mut(
    toolbox_startup: &mut PpcToolboxStartupState,
    gdevice: u32,
) -> &mut [bool; 256] {
    if gdevice == PPC_MAIN_GDEVICE {
        &mut toolbox_startup.clut_reserved
    } else {
        toolbox_startup
            .clut_reserved_by_device
            .entry(gdevice)
            .or_insert([false; 256])
    }
}

pub(crate) fn ppc_set_clut_entry_flag(flags: &mut [bool; 256], index: i16, value: bool) {
    if let Ok(index) = usize::try_from(index) {
        if let Some(flag) = flags.get_mut(index) {
            *flag = value;
        }
    }
}

pub(crate) fn ppc_current_gdevice_record(
    memory: &mut PpcSectionMem,
    current_gdevice: u32,
) -> Option<u32> {
    memory
        .read_u32_be(current_gdevice)
        .filter(|gdevice| *gdevice != 0)
}

pub(crate) fn ppc_gdevice_ctable_handle(
    memory: &mut PpcSectionMem,
    current_gdevice: u32,
) -> Option<u32> {
    ppc_gdevice_ctable_handle_with_provenance(memory, current_gdevice).handle
}

pub(crate) fn ppc_gdevice_ctable_handle_with_provenance(
    memory: &mut PpcSectionMem,
    current_gdevice: u32,
) -> PpcOptionalHandleResolution {
    let Some(gdevice) = memory.read_u32_be(current_gdevice) else {
        return PpcOptionalHandleResolution {
            handle: None,
            known: false,
        };
    };
    if gdevice == 0 {
        return PpcOptionalHandleResolution {
            handle: None,
            known: false,
        };
    }
    let Some(pixmap_handle) = gdevice
        .checked_add(22)
        .and_then(|field| memory.read_u32_be(field))
    else {
        return PpcOptionalHandleResolution {
            handle: None,
            known: false,
        };
    };
    if pixmap_handle == 0 {
        return PpcOptionalHandleResolution {
            handle: None,
            known: false,
        };
    }
    let Some(pixmap) = memory
        .read_u32_be(pixmap_handle)
        .filter(|pixmap| *pixmap != 0)
    else {
        return PpcOptionalHandleResolution {
            handle: None,
            known: false,
        };
    };
    let Some(raw_handle) = pixmap
        .checked_add(42)
        .and_then(|field| memory.read_u32_be(field))
    else {
        return PpcOptionalHandleResolution {
            handle: None,
            known: false,
        };
    };
    PpcOptionalHandleResolution {
        handle: (raw_handle != 0).then_some(raw_handle),
        known: true,
    }
}

pub(crate) fn ppc_index_to_color(
    memory: &mut PpcSectionMem,
    current_gdevice: u32,
    screen_clut: &[[u16; 3]; 256],
    index: u32,
) -> PpcRgbColor {
    // Inside Macintosh Volume V (1986), p. V-141: Index2Color reads the
    // requested pixel value from the active GDevice's color table. ColorSpec
    // records are stored in pixel-value order in indexed device tables.
    let device_color = ppc_gdevice_ctable_handle(memory, current_gdevice).and_then(|handle| {
        let table = memory.read_u32_be(handle).filter(|table| *table != 0)?;
        let last_index = u32::from(memory.read_u16_be(table.checked_add(6)?)?);
        if index > last_index {
            return None;
        }
        let entry = table.checked_add(8)?.checked_add(index.checked_mul(8)?)?;
        Some(PpcRgbColor {
            red: memory.read_u16_be(entry.checked_add(2)?)?,
            green: memory.read_u16_be(entry.checked_add(4)?)?,
            blue: memory.read_u16_be(entry.checked_add(6)?)?,
        })
    });
    device_color.unwrap_or_else(|| {
        screen_clut
            .get(index as usize)
            .copied()
            .map(|[red, green, blue]| PpcRgbColor { red, green, blue })
            .unwrap_or(PPC_RGB_BLACK)
    })
}

pub(crate) fn ppc_color_to_index(
    memory: &mut PpcSectionMem,
    current_gdevice: u32,
    logical_screen_clut: &[[u16; 3]; 256],
    color: PpcRgbColor,
) -> u32 {
    let pixmap = ppc_current_gdevice_record(memory, current_gdevice)
        .and_then(|gdevice| memory.read_u32_be(gdevice.checked_add(22)?))
        .filter(|handle| *handle != 0)
        .and_then(|handle| memory.read_u32_be(handle))
        .filter(|pixmap| *pixmap != 0);
    let depth = pixmap
        .and_then(|pixmap| memory.read_u16_be(pixmap.checked_add(32)?))
        .unwrap_or(PPC_MAIN_PIXEL_DEPTH as u16);
    if depth > 8 {
        return u32::from(ppc_rgb_color_to_rgb555(color));
    }

    if current_gdevice == PPC_MAIN_GDEVICE && depth == 8 {
        // Match the main screen's logical ColorTable rather than scanning the
        // independently animated hardware CLUT. A real ColorTable replacement
        // changes this lookup; a hardware-only fade does not.
        return u32::from(TrapDispatcher::screen_itable_index(
            logical_screen_clut,
            [color.red, color.green, color.blue],
        ));
    }

    let entry_count = 1usize << usize::from(depth.clamp(1, 8));
    let fallback = *logical_screen_clut;
    let clut = pixmap
        .and_then(|pixmap| memory.read_u32_be(pixmap.checked_add(42)?))
        .filter(|handle| *handle != 0)
        .and_then(|handle| ppc_read_ctable_clut(memory, handle, &fallback))
        .unwrap_or(fallback);
    let wanted = [color.red, color.green, color.blue];
    if let Some(index) = clut[..entry_count]
        .iter()
        .position(|entry| *entry == wanted)
    {
        return index as u32;
    }

    let mut best_index = 0usize;
    let mut best_distance = u64::MAX;
    for (index, entry) in clut[..entry_count].iter().enumerate() {
        let red = i64::from(entry[0]) - i64::from(color.red);
        let green = i64::from(entry[1]) - i64::from(color.green);
        let blue = i64::from(entry[2]) - i64::from(color.blue);
        let distance = (red * red + green * green + blue * blue) as u64;
        if distance < best_distance {
            best_index = index;
            best_distance = distance;
        }
    }
    best_index as u32
}

pub(crate) fn ppc_read_itable_ctable(
    memory: &mut PpcSectionMem,
    ctable_handle: u32,
    screen_clut: &[[u16; 3]; 256],
    reserved: &[bool; 256],
) -> Option<(u32, Vec<(u8, [u16; 3])>)> {
    if ctable_handle == 0 {
        let colors = screen_clut
            .iter()
            .copied()
            .enumerate()
            .filter(|(index, _)| !reserved[*index])
            .map(|(index, rgb)| (index as u8, rgb))
            .collect();
        return Some((0, colors));
    }

    let ctable = memory
        .read_u32_be(ctable_handle)
        .filter(|ctable| *ctable != 0)?;
    let seed = memory.read_u32_be(ctable)?;
    let last_index = memory.read_u16_be(ctable.checked_add(6)?)? as i16;
    let entry_count = if last_index < 0 {
        0
    } else {
        usize::from(last_index as u16).saturating_add(1).min(256)
    };
    let mut colors = Vec::with_capacity(entry_count);
    for slot in 0..entry_count {
        let entry = ctable.checked_add(8 + slot as u32 * 8)?;
        let value = memory.read_u16_be(entry)?;
        let Ok(index) = u8::try_from(value) else {
            continue;
        };
        if reserved[usize::from(index)] {
            continue;
        }
        colors.push((
            index,
            [
                memory.read_u16_be(entry.checked_add(2)?)?,
                memory.read_u16_be(entry.checked_add(4)?)?,
                memory.read_u16_be(entry.checked_add(6)?)?,
            ],
        ));
    }
    Some((seed, colors))
}

pub(crate) fn ppc_inverse_table_bytes(colors: &[(u8, [u16; 3])], resolution: u16) -> Vec<u8> {
    let no_reserved_standard_indices = colors
        .iter()
        .enumerate()
        .all(|(slot, (index, _))| usize::from(*index) == slot);
    if resolution == 4 && no_reserved_standard_indices && colors.len() == 256 {
        let standard = TrapDispatcher::standard_mac_8bpp_clut();
        if colors
            .iter()
            .zip(standard.iter())
            .all(|((_, actual), expected)| actual == expected)
        {
            return TrapDispatcher::standard_mac_8bpp_itable().to_vec();
        }
    }
    if resolution == 4 && no_reserved_standard_indices && colors.len() == 16 {
        let standard = TrapDispatcher::standard_mac_4bpp_gworld_clut();
        if colors
            .iter()
            .zip(standard.iter())
            .all(|((_, actual), expected)| actual == expected)
        {
            return TrapDispatcher::standard_mac_4bpp_gworld_itable().to_vec();
        }
    }

    let entry_count = 1usize << (usize::from(resolution) * 3);
    let mut table = vec![0; entry_count];
    if colors.is_empty() {
        return table;
    }
    let component_mask = (1u32 << resolution) - 1;
    let component_shift = 16 - u32::from(resolution);
    let cell_center = 1u32 << (15 - u32::from(resolution));
    for (cell, destination) in table.iter_mut().enumerate() {
        let cell = cell as u32;
        let quantized_blue = cell & component_mask;
        let quantized_green = (cell >> resolution) & component_mask;
        let quantized_red = (cell >> (resolution * 2)) & component_mask;
        let target = [
            (quantized_red << component_shift) | cell_center,
            (quantized_green << component_shift) | cell_center,
            (quantized_blue << component_shift) | cell_center,
        ];
        let mut best_index = colors[0].0;
        let mut best_distance = u64::MAX;
        for &(index, rgb) in colors {
            let red = i64::from(target[0]) - i64::from(rgb[0]);
            let green = i64::from(target[1]) - i64::from(rgb[1]);
            let blue = i64::from(target[2]) - i64::from(rgb[2]);
            let distance = (red * red + green * green + blue * blue) as u64;
            if distance < best_distance {
                best_index = index;
                best_distance = distance;
            }
        }
        *destination = best_index;
    }
    table
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_make_itable(
    cpu: &PpcCpu,
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    current_gdevice: u32,
    screen_clut: &[[u16; 3]; 256],
    toolbox_startup: &mut PpcToolboxStartupState,
) {
    // Inside Macintosh Volume V (1986), p. V-142: MakeITable accepts only
    // 3-, 4-, and 5-bit component resolution. NIL handles and a zero
    // resolution select the corresponding fields of the current GDevice.
    let requested_resolution = cpu.gpr[5] as u16;
    let gdevice = ppc_current_gdevice_record(memory, current_gdevice);
    let resolution = if requested_resolution == 0 {
        gdevice
            .and_then(|gdevice| memory.read_u16_be(gdevice + 10))
            .unwrap_or(0)
    } else {
        requested_resolution
    };
    if !(3..=5).contains(&resolution) {
        toolbox_startup.last_quickdraw_error.set(PPC_C_RES_ERR);
        return;
    }

    let explicit_ctable_handle = cpu.gpr[3];
    let ctable_handle = if explicit_ctable_handle != 0 {
        explicit_ctable_handle
    } else {
        ppc_gdevice_ctable_handle(memory, current_gdevice).unwrap_or(0)
    };
    let Some((seed, colors)) = ppc_read_itable_ctable(
        memory,
        ctable_handle,
        screen_clut,
        &ppc_device_clut_reserved(toolbox_startup, current_gdevice),
    ) else {
        toolbox_startup.last_quickdraw_error.set(PPC_PARAM_ERR);
        return;
    };
    let table = ppc_inverse_table_bytes(&colors, resolution);
    let Some(record_size) = u32::try_from(table.len())
        .ok()
        .and_then(|size| size.checked_add(6))
    else {
        toolbox_startup.last_quickdraw_error.set(PPC_MEM_FULL_ERR);
        return;
    };

    let explicit_itable_handle = cpu.gpr[4];
    let mut itable_handle = explicit_itable_handle;
    if itable_handle == 0 {
        let Some(gdevice) = gdevice else {
            toolbox_startup.last_quickdraw_error.set(PPC_PARAM_ERR);
            return;
        };
        itable_handle = memory.read_u32_be(gdevice + 6).unwrap_or(0);
        if itable_handle == 0 {
            itable_handle = ppc_allocator_view_allocate_handle(
                allocator.as_deref_mut(),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                record_size,
                true,
            );
            if itable_handle == 0 || memory.write_u32_be(gdevice + 6, itable_handle).is_none() {
                toolbox_startup.last_quickdraw_error.set(PPC_MEM_FULL_ERR);
                return;
            }
        }
    }

    let resize_result = ppc_allocator_view_resize_handle(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        itable_handle,
        record_size,
    );
    if resize_result != PPC_NO_ERR {
        toolbox_startup.last_quickdraw_error.set(resize_result);
        return;
    }
    let Some(itable) = memory
        .read_u32_be(itable_handle)
        .filter(|itable| *itable != 0)
    else {
        toolbox_startup.last_quickdraw_error.set(PPC_PARAM_ERR);
        return;
    };
    let wrote_record = memory.write_u32_be(itable, seed).is_some()
        && memory.write_u16_be(itable + 4, resolution).is_some()
        && memory.write_bytes(itable + 6, &table).is_some();
    let quickdraw_error = if wrote_record {
        PPC_NO_ERR
    } else {
        PPC_PARAM_ERR
    };
    toolbox_startup.last_quickdraw_error.set(quickdraw_error);
}

pub(crate) fn ppc_set_entries(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    current_gdevice: u32,
    screen_clut: &mut [[u16; 3]; 256],
    color_manager_clut: &mut [[u16; 3]; 256],
    _tick_count: u32,
    toolbox_startup: &mut PpcToolboxStartupState,
) {
    let clut_protected = ppc_device_clut_protected(toolbox_startup, current_gdevice);
    let start = cpu.gpr[3] as u16 as i16;
    let count = cpu.gpr[4] as u16 as i16;
    let table = cpu.gpr[5];
    if ppc_apply_set_entries(
        memory,
        start,
        count,
        table,
        current_gdevice,
        screen_clut,
        &clut_protected,
        None,
        true,
        toolbox_startup,
    ) {
        // SetEntries changes the current GDevice ColorTable and invalidates
        // its inverse table. Model the automatic rebuild used on the next
        // indexed-color mapping operation. RestoreEntries intentionally does
        // not update this logical snapshot.
        *color_manager_clut = *screen_clut;
    }
}

pub(crate) fn ppc_restore_entries(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    current_gdevice: u32,
    screen_clut: &mut [[u16; 3]; 256],
) {
    let source_handle = cpu.gpr[3];
    let requested_destination_handle = cpu.gpr[4];
    let selection = cpu.gpr[5];
    let Some(source) = memory
        .read_u32_be(source_handle)
        .filter(|source| *source != 0)
    else {
        return;
    };
    if selection == 0 {
        return;
    }
    let request_last = memory.read_u16_be(selection).unwrap_or(u16::MAX) as i16;
    if request_last < 0 {
        return;
    }
    let source_last = usize::from(memory.read_u16_be(source + 6).unwrap_or(0));
    let device_table_handle = ppc_gdevice_ctable_handle(memory, current_gdevice).unwrap_or(0);
    let destination_handle = if requested_destination_handle == 0 {
        device_table_handle
    } else {
        requested_destination_handle
    };
    let destination = memory
        .read_u32_be(destination_handle)
        .filter(|destination| *destination != 0);
    let destination_last = destination
        .and_then(|destination| memory.read_u16_be(destination + 6))
        .map_or(255usize, usize::from);
    let updates_device = requested_destination_handle == 0
        || (device_table_handle != 0 && destination_handle == device_table_handle);

    // Inside Macintosh Volume V (1986), p. V-144: RestoreEntries unpacks
    // the source ColorTable into the selected destination slots without
    // changing the destination table seed or rebuilding its inverse table.
    for source_index in 0..=usize::from(request_last as u16) {
        if source_index > source_last {
            break;
        }
        let Some(request_slot) = selection.checked_add(2 + source_index as u32 * 2) else {
            break;
        };
        let Some(destination_index) = memory.read_u16_be(request_slot).map(|value| value as i16)
        else {
            break;
        };
        if destination_index < 0 || destination_index as usize > destination_last {
            let _ = memory.write_u16_be(request_slot, u16::MAX);
            continue;
        }
        let source_entry = source + 8 + source_index as u32 * 8;
        let (Some(value), Some(red), Some(green), Some(blue)) = (
            memory.read_u16_be(source_entry),
            memory.read_u16_be(source_entry + 2),
            memory.read_u16_be(source_entry + 4),
            memory.read_u16_be(source_entry + 6),
        ) else {
            break;
        };
        let destination_index = destination_index as usize;
        if let Some(destination) = destination {
            let destination_entry = destination + 8 + destination_index as u32 * 8;
            let _ = memory.write_u16_be(destination_entry, value);
            let _ = memory.write_u16_be(destination_entry + 2, red);
            let _ = memory.write_u16_be(destination_entry + 4, green);
            let _ = memory.write_u16_be(destination_entry + 6, blue);
        }
        if updates_device {
            screen_clut[destination_index] = [red, green, blue];
        }
    }
}

pub(crate) fn ppc_apply_set_entries(
    memory: &mut PpcSectionMem,
    start: i16,
    count: i16,
    table: u32,
    current_gdevice: u32,
    screen_clut: &mut [[u16; 3]; 256],
    clut_protected: &[bool; 256],
    replacement_clut: Option<&[[u16; 3]; 256]>,
    update_device_ctable: bool,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> bool {
    if count < 0 || count > 255 || table == 0 {
        return false;
    }

    // Inside Macintosh Volume V (1986), p. V-143: count is a zero-based
    // last index. A nonnegative start selects consecutive entries; -1 uses
    // each ColorSpec.value as its destination index. Validate every entry
    // before changing the device table so an invalid request is atomic.
    let mut updates = Vec::with_capacity(count as usize + 1);
    for offset in 0..=count as u32 {
        let Some(entry) = table.checked_add(offset.saturating_mul(8)) else {
            return false;
        };
        let Some(value) = memory.read_u16_be(entry) else {
            return false;
        };
        let index = if start == -1 {
            usize::from(value)
        } else if start >= 0 {
            let Some(index) = (start as usize).checked_add(offset as usize) else {
                return false;
            };
            index
        } else {
            return false;
        };
        if index >= screen_clut.len() {
            return false;
        }
        let (Some(mut red), Some(mut green), Some(mut blue)) = (
            entry
                .checked_add(2)
                .and_then(|addr| memory.read_u16_be(addr)),
            entry
                .checked_add(4)
                .and_then(|addr| memory.read_u16_be(addr)),
            entry
                .checked_add(6)
                .and_then(|addr| memory.read_u16_be(addr)),
        ) else {
            return false;
        };
        if let Some(replacement_clut) = replacement_clut {
            [red, green, blue] = replacement_clut[index];
        }
        if !clut_protected[index] {
            updates.push((index, [red, green, blue]));
        }
    }

    for &(index, color) in &updates {
        screen_clut[index] = color;
    }

    let color_table = update_device_ctable
        .then(|| {
            memory
                .read_u32_be(current_gdevice)
                .filter(|device| *device != 0)
                .and_then(|device| memory.read_u32_be(device + 22))
                .filter(|pixmap_handle| *pixmap_handle != 0)
                .and_then(|pixmap_handle| memory.read_u32_be(pixmap_handle))
                .filter(|pixmap| *pixmap != 0)
                .and_then(|pixmap| memory.read_u32_be(pixmap + 42))
                .filter(|table_handle| *table_handle != 0)
                .and_then(|table_handle| memory.read_u32_be(table_handle))
                .filter(|table_ptr| *table_ptr != 0)
        })
        .flatten();
    if let Some(color_table) = color_table {
        for &(index, [red, green, blue]) in &updates {
            let Some(entry) = color_table.checked_add(8 + index as u32 * 8) else {
                return false;
            };
            let Some(red_addr) = entry.checked_add(2) else {
                return false;
            };
            let Some(green_addr) = entry.checked_add(4) else {
                return false;
            };
            let Some(blue_addr) = entry.checked_add(6) else {
                return false;
            };
            let _ = memory.write_u16_be(entry, index as u16);
            let _ = memory.write_u16_be(red_addr, red);
            let _ = memory.write_u16_be(green_addr, green);
            let _ = memory.write_u16_be(blue_addr, blue);
        }
        let seed = ppc_next_ct_seed(toolbox_startup);
        let _ = memory.write_u32_be(color_table, seed);
    }
    true
}

pub(crate) fn ppc_restore_device_clut(
    memory: &mut PpcSectionMem,
    requested_gdevice: u32,
    current_gdevice: u32,
    screen_clut: &mut [[u16; 3]; 256],
    color_manager_clut: &mut [[u16; 3]; 256],
    toolbox_startup: &mut PpcToolboxStartupState,
) {
    let canonical = TrapDispatcher::standard_mac_8bpp_clut();
    let mut gdevices = if requested_gdevice == 0 {
        let mut represented = vec![PPC_MAIN_GDEVICE, current_gdevice];
        represented.extend(toolbox_startup.known_gdevices.iter().copied());
        represented.extend(toolbox_startup.active_device_palettes.keys().copied());
        represented.extend(
            toolbox_startup
                .palette_allocations
                .iter()
                .map(|allocation| allocation.gdevice),
        );
        represented.extend(toolbox_startup.clut_protected_by_device.keys().copied());
        represented.extend(toolbox_startup.clut_reserved_by_device.keys().copied());
        represented
    } else {
        vec![requested_gdevice]
    };
    gdevices.sort_unstable();
    gdevices.dedup();
    for gdevice in gdevices {
        if gdevice == 0 {
            continue;
        }
        let mut device_clut = if gdevice == current_gdevice {
            *screen_clut
        } else {
            ppc_gdevice_ctable_handle(memory, gdevice)
                .and_then(|handle| ppc_read_ctable_clut(memory, handle, &canonical))
                .unwrap_or(canonical)
        };
        let protected = ppc_device_clut_protected(toolbox_startup, gdevice);
        let reserved = ppc_device_clut_reserved(toolbox_startup, gdevice);
        let mut restored = canonical;
        for index in 0..256 {
            if protected[index] || reserved[index] {
                restored[index] = device_clut[index];
            }
        }
        device_clut = restored;
        toolbox_startup.active_device_palettes.remove(&gdevice);
        toolbox_startup
            .palette_allocations
            .retain(|allocation| allocation.gdevice != gdevice);
        ppc_write_device_color_table(memory, gdevice, &device_clut, toolbox_startup);
        if gdevice == current_gdevice {
            *screen_clut = device_clut;
            *color_manager_clut = device_clut;
        }
    }
    // Inside Macintosh Volume VI (1991), pp. 20-23--20-24:
    // RestoreDeviceClut restores the selected device (or all for NIL) to its
    // default colors and posts color updates for intersecting windows.
}

pub(crate) fn ppc_install_device_gamma(
    memory: &mut PpcSectionMem,
    vd_gamma_ptr: u32,
    device_gamma: &mut crate::display::DisplayGamma,
) -> i16 {
    let Some(gamma_ptr) = memory.read_u32_be(vd_gamma_ptr) else {
        return PPC_PARAM_ERR;
    };
    if gamma_ptr == 0 {
        *device_gamma = crate::display::linear_display_gamma();
        return PPC_NO_ERR;
    }

    let (
        Some(version),
        Some(gamma_type),
        Some(formula_size),
        Some(channel_count),
        Some(data_count),
        Some(data_width),
    ) = (
        memory.read_u16_be(gamma_ptr),
        memory.read_u16_be(gamma_ptr.saturating_add(2)),
        memory.read_u16_be(gamma_ptr.saturating_add(4)),
        memory.read_u16_be(gamma_ptr.saturating_add(6)),
        memory.read_u16_be(gamma_ptr.saturating_add(8)),
        memory.read_u16_be(gamma_ptr.saturating_add(10)),
    )
    else {
        return PPC_PARAM_ERR;
    };
    let channel_count = u32::from(channel_count);
    let data_count = u32::from(data_count);
    let data_width = u32::from(data_width);
    if version != 0
        || gamma_type != 0
        || !matches!(channel_count, 1 | 3)
        || data_width > 8
        || data_count != (1u32 << data_width)
    {
        return PPC_PARAM_ERR;
    }

    let Some(data_base) = gamma_ptr
        .checked_add(12)
        .and_then(|address| address.checked_add(u32::from(formula_size)))
    else {
        return PPC_PARAM_ERR;
    };
    let shift = 8 - data_width;
    let mut installed = [[0u8; 256]; 3];
    for (channel, output) in installed.iter_mut().enumerate() {
        let source_channel = if channel_count == 1 {
            0
        } else {
            channel as u32
        };
        let Some(source_base) = source_channel
            .checked_mul(data_count)
            .and_then(|offset| data_base.checked_add(offset))
        else {
            return PPC_PARAM_ERR;
        };
        for (input, value) in output.iter_mut().enumerate() {
            let Some(source) = source_base.checked_add((input as u32) >> shift) else {
                return PPC_PARAM_ERR;
            };
            let Some(output_value) = memory.read_u8(source) else {
                return PPC_PARAM_ERR;
            };
            *value = output_value;
        }
    }
    *device_gamma = installed;
    PPC_NO_ERR
}

pub(crate) fn ppc_driver_control(
    memory: &mut PpcSectionMem,
    current_gdevice: u32,
    screen_clut: &mut [[u16; 3]; 256],
    display_gamma: &SharedProcessDisplayGamma,
    toolbox_startup: &mut PpcToolboxStartupState,
    cs_code: i16,
    cs_param: u32,
) -> i16 {
    const CONTROL_ERR: i16 = -17;
    let clut_protected = ppc_device_clut_protected(toolbox_startup, current_gdevice);
    match cs_code {
        2 => {
            let valid = cs_param.checked_add(11).is_some_and(|end| {
                ppc_memory_can_write_bytes(memory, cs_param, end - cs_param + 1)
            });
            if !valid {
                PPC_PARAM_ERR
            } else {
                let _ = memory.write_u16_be(cs_param + 6, 0);
                let _ = memory.write_u32_be(cs_param + 8, PPC_MAIN_SCREEN_BASE);
                PPC_NO_ERR
            }
        }
        3 | 8 => {
            let request = memory.read_u32_be(cs_param);
            let fields = request.and_then(|request| {
                Some((
                    memory.read_u32_be(request)?,
                    memory.read_u16_be(request.checked_add(4)?)? as i16,
                    memory.read_u16_be(request.checked_add(6)?)? as i16,
                ))
            });
            match fields {
                Some((table, start, count))
                    if ppc_apply_set_entries(
                        memory,
                        start,
                        count,
                        table,
                        current_gdevice,
                        screen_clut,
                        &clut_protected,
                        None,
                        false,
                        toolbox_startup,
                    ) =>
                {
                    // Designing Cards and Drivers, 3rd ed. (1992), pp. 245–248:
                    // cscSetEntries supplies values directly to the video
                    // device. Preserve a guest-installed gamma table, but do
                    // not apply the Color Manager compatibility transfer to
                    // presentation-ready driver entries.
                    display_gamma.set_implicit(crate::display::linear_display_gamma());
                    PPC_NO_ERR
                }
                _ => PPC_PARAM_ERR,
            }
        }
        4 => memory
            .read_u32_be(cs_param)
            .map(|vd_gamma| {
                let mut installed = display_gamma.table();
                let result = ppc_install_device_gamma(memory, vd_gamma, &mut installed);
                if result == PPC_NO_ERR {
                    display_gamma.install(installed);
                }
                result
            })
            .unwrap_or(PPC_PARAM_ERR),
        _ => CONTROL_ERR,
    }
}

pub(crate) fn ppc_control(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    current_gdevice: u32,
    screen_clut: &mut [[u16; 3]; 256],
    display_gamma: &SharedProcessDisplayGamma,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> i16 {
    let ref_num = cpu.gpr[3] as u16 as i16;
    if ref_num != 0 {
        return -21; // badUnitErr: Systemless only exposes the main display DCE.
    }
    // Inside Macintosh: Devices (1994), pp. 1-75--1-76: Control is the
    // high-level synchronous form of PBControl and passes the driver-specific
    // csParam block directly. Both entry points share the display selectors.
    ppc_driver_control(
        memory,
        current_gdevice,
        screen_clut,
        display_gamma,
        toolbox_startup,
        cpu.gpr[4] as u16 as i16,
        cpu.gpr[5],
    )
}

pub(crate) fn ppc_pb_control(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    current_gdevice: u32,
    screen_clut: &mut [[u16; 3]; 256],
    display_gamma: &SharedProcessDisplayGamma,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> i16 {
    let parameter_block = cpu.gpr[3];
    let Some(cs_code_addr) = parameter_block.checked_add(26) else {
        return PPC_PARAM_ERR;
    };
    let Some(cs_code) = memory.read_u16_be(cs_code_addr).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let Some(cs_param) = parameter_block.checked_add(28) else {
        return PPC_PARAM_ERR;
    };

    // Inside Macintosh: Devices (1994), pp. 1-79--1-80: PBControl uses a
    // CntrlParam record with csCode at byte 26 and driver-specific csParam
    // data at byte 28. The built-in display driver selectors below follow
    // Designing Cards and Drivers, 3rd ed. (1992), pp. 235 and 245--248.
    let result = ppc_driver_control(
        memory,
        current_gdevice,
        screen_clut,
        display_gamma,
        toolbox_startup,
        cs_code,
        cs_param,
    );

    if let Some(result_addr) = parameter_block.checked_add(16) {
        let _ = memory.write_u16_be(result_addr, result as u16);
    }
    result
}

pub(crate) fn ppc_pb_status(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    const STATUS_ERR: i16 = -18;

    let parameter_block = cpu.gpr[3];
    let result = (|| {
        let ref_num = memory.read_u16_be(parameter_block.checked_add(24)?)? as i16;
        if ref_num != 0 {
            return Some(-21); // badUnitErr: no matching Device Manager driver.
        }
        let cs_code = memory.read_u16_be(parameter_block.checked_add(26)?)? as i16;
        let cs_param = parameter_block.checked_add(28)?;

        // Inside Macintosh: Devices (1994), pp. 1-65 and 1-83: PBStatus
        // uses CntrlParam.csCode/csParam, and selector 1 is handled by the
        // Device Manager itself by returning the driver's DCE handle.
        match cs_code {
            1 => memory
                .write_u32_be(cs_param, PPC_MAIN_DCE_HANDLE)
                .map(|()| PPC_NO_ERR),
            // Universal Interfaces 3.4 Video.h defines cscGetGamma (8) as a
            // status request whose csParam contains a VDGammaRecord pointer;
            // the driver stores its device-owned GammaTbl pointer there.
            8 => {
                let vd_gamma = memory.read_u32_be(cs_param)?;
                memory
                    .write_u32_be(vd_gamma, PPC_MAIN_GAMMA_TABLE)
                    .map(|()| PPC_NO_ERR)
            }
            // Inside Macintosh: Devices (1994), p. 1-47: a driver must
            // return statusErr for status selectors that it does not support.
            _ => Some(STATUS_ERR),
        }
    })()
    .unwrap_or(PPC_PARAM_ERR);

    if parameter_block != 0 {
        if let Some(result_addr) = parameter_block.checked_add(16) {
            let _ = memory.write_u16_be(result_addr, result as u16);
        }
    }
    result
}

pub(crate) fn ppc_get_ctable(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    current_gworld: u32,
    quickdraw_hilite_colors: &SharedProcessQuickDrawHiliteColors,
) -> u32 {
    let ct_id = cpu.gpr[3] as u16 as i16;
    // Imaging With QuickDraw 1994, 4-92 through 4-93: GetCTable returns a
    // newly allocated ColorTable handle copied from a 'clut' resource, and
    // recognizes the standard depth IDs 1, 2, 4, and 8.
    let data = match ct_id {
        1 | 2 | 4 | 8 => TrapDispatcher::standard_mac_indexed_clut(ct_id as u16)
            .map(|(clut, entry_count)| (clut, entry_count, ct_id as u32)),
        66 | 68 | 72 => {
            let depth = (ct_id - 64) as u16;
            let hilite = ppc_current_hilite_color(memory, current_gworld, quickdraw_hilite_colors);
            TrapDispatcher::standard_mac_enhanced_clut(
                depth,
                (hilite.red, hilite.green, hilite.blue),
            )
            .map(|(clut, entry_count)| (clut, entry_count, u32::from(depth)))
        }
        _ => None,
    }
    .map(|(clut, entry_count, seed)| {
        let mut data = Vec::with_capacity(8 + entry_count * 8);
        data.extend_from_slice(&seed.to_be_bytes());
        data.extend_from_slice(&0u16.to_be_bytes());
        data.extend_from_slice(&((entry_count - 1) as u16).to_be_bytes());
        for (index, rgb) in clut.into_iter().take(entry_count).enumerate() {
            data.extend_from_slice(&(index as u16).to_be_bytes());
            data.extend_from_slice(&rgb[0].to_be_bytes());
            data.extend_from_slice(&rgb[1].to_be_bytes());
            data.extend_from_slice(&rgb[2].to_be_bytes());
        }
        data
    })
    .or_else(|| {
        let index = ppc_vfs_resource_index(
            vfs_resources,
            current_resource_refnum,
            u32::from_be_bytes(*b"clut"),
            ct_id,
            false,
        )?;
        let data = vfs_resources[index].data.clone();
        (data.len() >= 8).then_some(data)
    });
    let Some(data) = data else {
        return 0;
    };
    let handle = ppc_process_alloc_handle_with_bytes(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        &data,
    );
    *last_mem_error = if handle == 0 {
        PPC_MEM_FULL_ERR
    } else {
        PPC_NO_ERR
    };
    handle
}

pub(crate) fn ppc_read_ctable_clut(
    memory: &mut PpcSectionMem,
    ctable_handle: u32,
    base_clut: &[[u16; 3]; 256],
) -> Option<[[u16; 3]; 256]> {
    let ctable = memory
        .read_u32_be(ctable_handle)
        .filter(|ctable| *ctable != 0)?;
    let flags = memory.read_u16_be(ctable.checked_add(4)?)?;
    let last_entry = usize::from(memory.read_u16_be(ctable.checked_add(6)?)?);
    let entry_count = last_entry.saturating_add(1).min(256);
    let mut clut = *base_clut;
    for slot in 0..entry_count {
        let entry = ctable.checked_add(8 + slot as u32 * 8)?;
        let value = usize::from(memory.read_u16_be(entry)?);
        let index = if flags & 0x8000 != 0 { slot } else { value };
        if index >= clut.len() {
            continue;
        }
        clut[index] = [
            memory.read_u16_be(entry.checked_add(2)?)?,
            memory.read_u16_be(entry.checked_add(4)?)?,
            memory.read_u16_be(entry.checked_add(6)?)?,
        ];
    }
    Some(clut)
}
