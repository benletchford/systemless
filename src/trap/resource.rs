//! Resource Manager and File Manager trap handlers.

use crate::cpu::{CpuOps, Register};
use crate::loader::CodeSegmentHeader;
use crate::machine_profile::{REFERENCE_M68K_EXECUTION_CAPABILITIES, REFERENCE_MACHINE_PROFILE};
use crate::managers::resource::ResourceFork;
use crate::memory::globals::addr;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::process_context::ProcessVfsDirectory;
use crate::process_manager::{ProcessSerialNumber, SingleProcessEnumeration};
use crate::trap::types::{decode_mac_roman, encode_mac_roman_lossy, read_fsspec_name};
use crate::{Error, Result};

use std::collections::BTreeMap;
use std::sync::OnceLock;

use super::dispatch::{
    raw_trap_route, selector_operation_route, OsRoutineVariant, SelectorOperationRoute,
};
use super::memory::{mac_roman_strip_diacriticals, mac_roman_to_upper};
static TRACE_MENU_PICT: OnceLock<bool> = OnceLock::new();
static TRACE_FSSPEC: OnceLock<bool> = OnceLock::new();
static TRACE_SOUND_RESOURCE: OnceLock<bool> = OnceLock::new();
static TRACE_GETRESOURCE: OnceLock<bool> = OnceLock::new();
static TRACE_LOADSEG: OnceLock<bool> = OnceLock::new();

const RESOURCE_DISPATCH_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_resource_dispatch_operations.rs");
const HIGH_LEVEL_FS_DISPATCH_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_high_level_fs_dispatch_operations.rs");
const HFS_DISPATCH_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_hfs_dispatch_operations.rs");
const OS_DISPATCH_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_os_dispatch_operations.rs");

fn resource_dispatch_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA822 {
        return None;
    }
    selector_operation_route(RESOURCE_DISPATCH_OPERATION_ROUTES, selector)
}

fn high_level_fs_dispatch_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xAA52 {
        return None;
    }
    selector_operation_route(HIGH_LEVEL_FS_DISPATCH_OPERATION_ROUTES, selector)
}

fn hfs_dispatch_operation_route(
    trap_word: u16,
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA260 {
        return None;
    }
    selector_operation_route(HFS_DISPATCH_OPERATION_ROUTES, selector)
}

fn os_dispatch_operation_route(
    trap_word: u16,
    selector: u16,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA88F {
        return None;
    }
    selector_operation_route(OS_DISPATCH_OPERATION_ROUTES, u32::from(selector))
}

const BOOT_VOLUME_ALLOCATION_BLOCKS: u16 = 16_384;
const BOOT_VOLUME_ALLOCATION_BLOCK_SIZE: u32 = 4 * 1024;
const BOOT_VOLUME_FREE_BLOCKS: u16 = 16_384;
const QUICKTIME_NUM_VERSION_6_0_FINAL: u32 = 0x0600_8000;
const NO_ERR: u32 = 0;
const MEM_FULL_ERR: u32 = (-108i32) as u32;
const NIL_HANDLE_ERR: u32 = (-109i32) as u32;
const MEM_WZ_ERR: u32 = (-111i32) as u32;
const RES_NOT_FOUND_ERR: i16 = -192;
const NO_OUTSTANDING_HLE_ERR: i16 = -608;
const CONNECTION_INVALID_ERR: i16 = -609;
const NO_PORT_ERR: i16 = -903;

#[derive(Clone, Debug)]
struct CatSearchEntry {
    name: String,
    vref_num: i16,
    parent_dir_id: u32,
    is_directory: bool,
    locked: bool,
    finder_info: [u8; 16],
    extended_finder_info: [u8; 16],
    data_length: u32,
    resource_length: u32,
    child_count: u16,
    created_date: u32,
    modified_date: u32,
    backup_date: u32,
}

fn trace_menu_pict_enabled() -> bool {
    *TRACE_MENU_PICT.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_MENU_PICT").is_some())
}

fn trace_fsspec_enabled() -> bool {
    *TRACE_FSSPEC.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_FSSPEC").is_some())
}

fn trace_sound_resource_enabled() -> bool {
    *TRACE_SOUND_RESOURCE.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_SOUND").is_some())
}

fn trace_getresource_enabled() -> bool {
    *TRACE_GETRESOURCE.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_GETRESOURCE").is_some())
}

fn trace_loadseg_enabled() -> bool {
    *TRACE_LOADSEG.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_LOADSEG").is_some())
}

fn signed_byte_from_stack_word(word: u16) -> i16 {
    let byte = if word & 0x00FF == 0 {
        (word >> 8) as u8
    } else {
        (word & 0x00FF) as u8
    };
    i16::from(byte as i8)
}

fn file_permission_from_stack_word(word: u16) -> i16 {
    fn is_file_permission(byte: u8) -> bool {
        matches!(byte, 0..=4)
    }

    let high = (word >> 8) as u8;
    let low = (word & 0x00FF) as u8;
    match (is_file_permission(high), is_file_permission(low)) {
        (true, false) => high as i16,
        (false, true) => low as i16,
        _ => signed_byte_from_stack_word(word),
    }
}

fn temporary_memory_free_estimate(bus: &MacMemoryBus) -> u32 {
    (bus.ram_size() / 2).clamp(8 * 1024 * 1024, 64 * 1024 * 1024)
}

fn write_temp_memory_result_code(bus: &mut MacMemoryBus, result_code_ptr: u32, result: u32) {
    if result_code_ptr != 0 {
        bus.write_word(result_code_ptr, result as u16);
    }
}

fn write_osdispatch_oseerr_result<C: CpuOps>(
    cpu: &mut C,
    bus: &mut MacMemoryBus,
    result_slot: u32,
    err: i16,
) {
    bus.write_word(result_slot, err as u16);
    cpu.write_reg(Register::D0, err as i32 as u32);
    cpu.write_reg(Register::A7, result_slot);
}

fn write_osdispatch_selector_result<C: CpuOps>(
    cpu: &mut C,
    bus: &mut MacMemoryBus,
    selector_slot: u32,
    err: i16,
) {
    bus.write_word(selector_slot, err as u16);
    cpu.write_reg(Register::D0, err as i32 as u32);
    cpu.write_reg(Register::A7, selector_slot);
}

fn scribble_temporary_allocation(bus: &mut MacMemoryBus, address: u32, size: u32) {
    for offset in 0..size {
        bus.write_byte(address.wrapping_add(offset), 0xA5);
    }
}

fn temporary_memory_handle_status(bus: &MacMemoryBus, handle: u32) -> u32 {
    if handle == 0 {
        return NIL_HANDLE_ERR;
    }
    if bus.get_alloc_size(handle).is_none() {
        return MEM_WZ_ERR;
    }
    let ptr = bus.read_long(handle);
    if ptr == 0 {
        return NIL_HANDLE_ERR;
    }
    if bus.get_alloc_size(ptr).is_none() {
        return MEM_WZ_ERR;
    }
    NO_ERR
}

fn format_ostype(res_type: [u8; 4]) -> String {
    res_type
        .into_iter()
        .map(|byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                byte as char
            } else {
                '.'
            }
        })
        .collect()
}

/// `gestaltUndefSelectorErr` (-5551). Returned by `Gestalt` /
/// `ReplaceGestalt` when the selector code is not recognised.
/// Inside Macintosh: Operating System Utilities 1994, 1-32 / 1-35.
const GESTALT_UNDEF_SELECTOR_ERR: u32 = 0xFFFF_EA51;

/// `gestaltDupSelectorErr` (-5552). Returned by `NewGestalt` when the
/// caller tries to register a selector that is already known to the
/// Gestalt Manager (built-in or previously installed).
/// Inside Macintosh: Operating System Utilities 1994, 1-34.
const GESTALT_DUP_SELECTOR_ERR: u32 = 0xFFFF_EA50;
const GESTALT_68040_LOGICAL_PAGE_SIZE_BYTES: u32 = 4096;

/// Closed list of `Gestalt` selectors recognised by Systemless's built-in
/// query handler. Kept in sync with the match arms in the `(false,
/// 0xAD)` dispatch — `_NewGestalt` and `_ReplaceGestalt` consult this
/// to decide between `gestaltDupSelectorErr` and noErr / between noErr
/// and `gestaltUndefSelectorErr`. See the Gestalt arm in
/// `dispatch_resource` for the response values and IM citations.
fn is_builtin_gestalt_selector(sel: &[u8; 4]) -> bool {
    matches!(
        sel,
        b"vers"
            | b"sysv"
            | b"sysa"
            | b"ostt"
            | b"tbtt"
            | b"evnt"
            | b"edtn"
            | b"ppc "
            | b"cput"
            | b"proc"
            | b"mach"
            | b"kbd "
            | b"qd  "
            | b"qdrw"
            | b"ram "
            | b"lram"
            | b"pgsz"
            | b"fpu "
            | b"mmu "
            | b"snd "
            | b"ttsc"
            | b"te  "
            | b"teat"
            | b"cltn"
            | b"tmgr"
            | b"thds"
            | b"dplv"
            | b"dply"
            | b"alis"
            | b"fs  "
            | b"fold"
            | b"rsrc"
            | b"scr#"
            | b"qtim"
            | b"drag"
            | b"os  "
            | b"powr"
            | b"appr"
            | b"addr"
            | b"hdwr"
            | b"sdev"
            | b"stdf"
            | b"help"
            | b"vm  "
    )
}

impl super::TrapDispatcher {
    fn restore_native_toolbox_nonvolatile<C: CpuOps>(
        cpu: &mut C,
        call: &super::dispatch::NativeTrapCallState,
    ) {
        for (reg, value) in [
            Register::D3,
            Register::D4,
            Register::D5,
            Register::D6,
            Register::D7,
        ]
        .into_iter()
        .zip(call.preserved_d_regs)
        {
            cpu.write_reg(reg, value);
        }
        for (reg, value) in [
            Register::A2,
            Register::A3,
            Register::A4,
            Register::A5,
            Register::A6,
        ]
        .into_iter()
        .zip(call.preserved_a_regs)
        {
            cpu.write_reg(reg, value);
        }
    }

    pub(crate) const RES_NOT_FOUND: i16 = -192;
    pub(crate) const RES_ATTR_ERR: i16 = -198;
    pub(crate) const RES_CHANGED_ATTR: u16 = 0x0002;
    pub(crate) const RES_SYS_REF_ATTR: u16 = 0x0080;
    pub(crate) const RES_MAP_CHANGED_ATTR: u16 = 0x0020;
    pub(crate) const RES_MAP_READ_ONLY_ATTR: u16 = 0x0080;

    fn is_available_synthetic_driver_name(filename: &str) -> bool {
        // Driver presence is a capability probe. Keep this list limited to
        // services backed by the HLE: AppleTalk's .MPP/.ATP/.XPP drivers must
        // remain absent until their control, status, and protocol paths exist.
        [".AIn", ".AOut", ".BIn", ".BOut"]
            .iter()
            .any(|driver| filename.eq_ignore_ascii_case(driver))
    }

    fn is_unavailable_appletalk_driver_name(filename: &str) -> bool {
        [".MPP", ".ATP", ".XPP"]
            .iter()
            .any(|driver| filename.eq_ignore_ascii_case(driver))
    }

    fn address_is_loaded_code(&self, address: u32) -> bool {
        let mut bases: Vec<u32> = self.segment_map.values().copied().collect();
        bases.sort_unstable();

        let Some((index, &base)) = bases
            .iter()
            .enumerate()
            .rev()
            .find(|(_, base)| address >= **base)
        else {
            return false;
        };

        bases
            .get(index + 1)
            .map(|&next| address < next)
            .unwrap_or(false)
            && address >= base
    }

    /// Minimal serialized resource fork containing an empty resource map.
    fn empty_resource_fork_bytes() -> Vec<u8> {
        let mut bytes = vec![0u8; 46];
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&16u32.to_be_bytes());
        header[4..8].copy_from_slice(&16u32.to_be_bytes());
        header[8..12].copy_from_slice(&0u32.to_be_bytes());
        header[12..16].copy_from_slice(&30u32.to_be_bytes());
        bytes[0..16].copy_from_slice(&header);
        bytes[16..32].copy_from_slice(&header);
        bytes[40..42].copy_from_slice(&28u16.to_be_bytes());
        bytes[42..44].copy_from_slice(&30u16.to_be_bytes());
        bytes[44..46].copy_from_slice(&0xFFFFu16.to_be_bytes());
        bytes
    }

    fn resolve_file_mark_position(
        pos_mode: u16,
        pos_offset: i32,
        cur_pos: usize,
        file_len: usize,
    ) -> std::result::Result<usize, i16> {
        let base = match pos_mode & 0x03 {
            0 => return Ok(cur_pos),
            1 => 0i64,
            2 => file_len as i64,
            3 => cur_pos as i64,
            _ => cur_pos as i64,
        };
        let position = base + pos_offset as i64;
        if position < 0 {
            Err(-40)
        } else {
            Ok(position as usize)
        }
    }

    fn loadseg_entry_is_loaded(bus: &MacMemoryBus, entry_addr: u32) -> bool {
        bus.read_word(entry_addr) == 0x4EF9 || bus.read_word(entry_addr + 2) == 0x4EF9
    }

    fn loadseg_entry_dispatch_pc(bus: &MacMemoryBus, entry_addr: u32) -> u32 {
        if bus.read_word(entry_addr) == 0x4EF9 {
            entry_addr
        } else {
            entry_addr + 2
        }
    }

    fn patch_loadseg_entry(
        bus: &mut MacMemoryBus,
        seg_num: i16,
        seg_addr: u32,
        header_size: u32,
        entry_addr: u32,
    ) -> u32 {
        let routine_offset = if bus.read_word(entry_addr) == 0xA9F0 {
            // Think C format: [A9F0, 0000, offset, seg#]
            bus.read_word(entry_addr + 4) as u32
        } else {
            // Standard format: [offset, 3F3C, seg#, A9F0]
            bus.read_word(entry_addr) as u32
        };
        let code_addr = seg_addr + header_size + routine_offset;
        if bus.read_word(entry_addr) == 0xA9F0 {
            // Think C format entries are called at entry+0, so the loaded
            // entry must also begin with the JMP instruction.
            bus.write_word(entry_addr, 0x4EF9); // JMP.L
            bus.write_long(entry_addr + 2, code_addr);
            bus.write_word(entry_addr + 6, 0);
        } else {
            bus.write_word(entry_addr, seg_num as u16);
            bus.write_word(entry_addr + 2, 0x4EF9); // JMP.L
            bus.write_long(entry_addr + 4, code_addr);
        }
        code_addr
    }

    fn refresh_segment_from_resource(
        &self,
        bus: &mut MacMemoryBus,
        seg_num: i16,
        seg_addr: u32,
        trace_loadseg: bool,
    ) {
        let Some(resources) = self.resources.as_ref() else {
            return;
        };

        let code_key = (*b"CODE", seg_num);
        let ptr = resources
            .files
            .get(&0)
            .and_then(|file| file.loaded.get(&code_key))
            .copied()
            .or_else(|| {
                self.resource_search_order().into_iter().find_map(|refnum| {
                    resources
                        .files
                        .get(&refnum)
                        .and_then(|file| file.loaded.get(&code_key))
                        .copied()
                })
            });
        let Some(ptr) = ptr else {
            return;
        };

        let Some(resource_size) = bus.get_alloc_size(ptr) else {
            return;
        };
        if resource_size == 0 {
            return;
        }

        let current_header =
            CodeSegmentHeader::from_words(bus.read_word(seg_addr), bus.read_word(seg_addr + 2));
        let refreshed_header =
            CodeSegmentHeader::from_words(bus.read_word(ptr), bus.read_word(ptr + 2));
        let mut preserve_header_bytes = 0u32;
        if let (Some(current_count), Some(refreshed_count)) = (
            current_header.jump_table_entry_count(),
            refreshed_header.jump_table_entry_count(),
        ) {
            let tolerated_count = current_count.saturating_mul(4).max(current_count + 256);
            if current_count > 0 && refreshed_count > tolerated_count {
                if trace_loadseg {
                    eprintln!(
                        "[TRAP] LoadSeg: preserving CODE {} resident header; resource header {:?} expands resident {:?} entry count {} -> {}",
                        seg_num, refreshed_header, current_header, current_count, refreshed_count
                    );
                }
                preserve_header_bytes = current_header.code_header_size();
            }
        }

        let loaded_size = if seg_addr >= 4 {
            bus.read_long(seg_addr - 4)
        } else {
            0
        };
        let copy_len = resource_size.min(loaded_size);
        if copy_len == 0 {
            return;
        }

        let copied_bytes = if preserve_header_bytes > 0 {
            let preserved = preserve_header_bytes.min(copy_len);
            let body_len = copy_len.saturating_sub(preserved);
            if body_len == 0 {
                Vec::new()
            } else {
                let bytes = bus.read_bytes(ptr + preserved, body_len as usize);
                bus.write_bytes(seg_addr + preserved, &bytes);
                bytes
            }
        } else {
            let bytes = bus.read_bytes(ptr, copy_len as usize);
            bus.write_bytes(seg_addr, &bytes);
            bytes
        };

        if trace_loadseg {
            let preview: Vec<String> = copied_bytes
                .iter()
                .take(16)
                .map(|byte| format!("{byte:02X}"))
                .collect();
            eprintln!(
                "[TRAP] LoadSeg: refreshed CODE {} from resource ptr=${:08X} len={} copied={} preserved_header={} preview={}",
                seg_num,
                ptr,
                resource_size,
                copy_len,
                preserve_header_bytes,
                preview.join(" ")
            );
            if resource_size != loaded_size {
                eprintln!(
                    "[TRAP] LoadSeg: CODE {} resource size {} differs from segment buffer {}",
                    seg_num, resource_size, loaded_size
                );
            }
        }
    }

    fn allocate_loadseg_getresource_trampoline(&mut self, bus: &mut MacMemoryBus) -> u32 {
        if let Some(addr) = self.loadseg_getresource_trampoline_addr {
            return addr;
        }
        let addr = bus.alloc(8);
        bus.write_word(addr, 0x303C); // MOVE.W #imm, D0
        bus.write_word(addr + 2, super::dispatch::LOADSEG_GETRESOURCE_SENTINEL);
        bus.write_word(addr + 4, 0xA816); // _Pack8
        self.loadseg_getresource_trampoline_addr = Some(addr);
        addr
    }

    fn save_d_regs<C: CpuOps>(cpu: &C) -> [u32; 8] {
        [
            cpu.read_reg(Register::D0),
            cpu.read_reg(Register::D1),
            cpu.read_reg(Register::D2),
            cpu.read_reg(Register::D3),
            cpu.read_reg(Register::D4),
            cpu.read_reg(Register::D5),
            cpu.read_reg(Register::D6),
            cpu.read_reg(Register::D7),
        ]
    }

    fn save_a_regs<C: CpuOps>(cpu: &C) -> [u32; 8] {
        [
            cpu.read_reg(Register::A0),
            cpu.read_reg(Register::A1),
            cpu.read_reg(Register::A2),
            cpu.read_reg(Register::A3),
            cpu.read_reg(Register::A4),
            cpu.read_reg(Register::A5),
            cpu.read_reg(Register::A6),
            cpu.read_reg(Register::A7),
        ]
    }

    fn restore_regs<C: CpuOps>(cpu: &mut C, d_regs: [u32; 8], a_regs: [u32; 8]) {
        for (index, value) in d_regs.into_iter().enumerate() {
            cpu.write_reg(
                match index {
                    0 => Register::D0,
                    1 => Register::D1,
                    2 => Register::D2,
                    3 => Register::D3,
                    4 => Register::D4,
                    5 => Register::D5,
                    6 => Register::D6,
                    _ => Register::D7,
                },
                value,
            );
        }
        for (index, value) in a_regs.into_iter().enumerate() {
            cpu.write_reg(
                match index {
                    0 => Register::A0,
                    1 => Register::A1,
                    2 => Register::A2,
                    3 => Register::A3,
                    4 => Register::A4,
                    5 => Register::A5,
                    6 => Register::A6,
                    _ => Register::A7,
                },
                value,
            );
        }
    }

    fn maybe_inject_native_getresource_for_loadseg<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        seg_num: i16,
        entry_addr: u32,
        trace_loadseg: bool,
    ) -> bool {
        if self.loadseg_getresource_state.is_some() {
            return false;
        }

        let Some(handler_addr) = self.native_trap_handler(bus, 0xA9A0) else {
            return false;
        };

        let trampoline = self.allocate_loadseg_getresource_trampoline(bus);
        let saved_sp = cpu.read_reg(Register::A7);
        let call_sp = saved_sp.wrapping_sub(14);

        // Native trap handlers are entered as if the trap dispatcher did a
        // JSR to them: return PC first, then the original Pascal arguments.
        bus.write_long(call_sp, trampoline);
        bus.write_word(call_sp + 4, seg_num as u16);
        bus.write_long(call_sp + 6, u32::from_be_bytes(*b"CODE"));
        bus.write_long(call_sp + 10, 0);

        self.loadseg_getresource_state = Some(super::dispatch::LoadSegGetResourceState {
            seg_num,
            entry_addr,
            result_sp: call_sp + 10,
            d_regs: Self::save_d_regs(cpu),
            a_regs: Self::save_a_regs(cpu),
        });

        cpu.write_reg(Register::A7, call_sp);
        cpu.write_reg(Register::PC, handler_addr);

        if trace_loadseg {
            eprintln!(
                "[TRAP] LoadSeg: invoking native GetResource('CODE', {}) handler=${:08X} trampoline=${:08X}",
                seg_num, handler_addr, trampoline
            );
        }

        true
    }

    pub(crate) fn resume_loadseg_after_getresource<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
    ) -> Result<()> {
        let state = self
            .loadseg_getresource_state
            .take()
            .expect("LoadSeg GetResource trampoline fired without saved state");
        let trace_loadseg = trace_loadseg_enabled();
        let handle = bus.read_long(state.result_sp);

        if trace_loadseg {
            let ptr = if handle == 0 {
                0
            } else {
                bus.read_long(handle)
            };
            eprintln!(
                "[TRAP] LoadSeg: native GetResource returned handle=${:08X} ptr=${:08X} for CODE {}",
                handle, ptr, state.seg_num
            );
        }

        Self::restore_regs(cpu, state.d_regs, state.a_regs);
        self.finish_loadseg(bus, cpu, state.seg_num, Some(state.entry_addr), true)
    }

    fn finish_loadseg<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        seg_num: i16,
        entry_addr: Option<u32>,
        refresh_from_resource: bool,
    ) -> Result<()> {
        let trace_loadseg = trace_loadseg_enabled();
        let existing_seg_addr = self.segment_map.get(&seg_num).copied();
        let seg_addr = existing_seg_addr.or_else(|| {
            let (_, ptr) = self.find_or_load_resource_any(bus, *b"CODE", seg_num)?;
            self.segment_map.insert(seg_num, ptr);
            super::dispatch::record_segment_base(seg_num, ptr);
            if trace_loadseg {
                eprintln!(
                    "[TRAP] LoadSeg: materialized CODE {} from resource chain at ${:08X}",
                    seg_num, ptr
                );
            }
            Some(ptr)
        });

        if let Some(seg_addr) = seg_addr {
            if refresh_from_resource && existing_seg_addr.is_some() {
                self.refresh_segment_from_resource(bus, seg_num, seg_addr, trace_loadseg);
            }

            let segment_header =
                CodeSegmentHeader::from_words(bus.read_word(seg_addr), bus.read_word(seg_addr + 2));
            let header_size = segment_header.code_header_size();

            // Read segment header. Standard near segments store byte offset
            // + entry count. Symantec/THINK far segments store first
            // jump-table entry index + entry count.
            if let (Some(tab_off), Some(n_entries)) = (
                segment_header.jump_table_start_offset(),
                segment_header.jump_table_entry_count(),
            ) {
                let a5 = cpu.read_reg(Register::A5);
                let cur_jt_offset = bus.read_word(addr::CUR_JT_OFFSET) as u32;
                let ptr_base = a5 + tab_off + cur_jt_offset;

                if trace_loadseg {
                    let header_desc = match segment_header {
                        CodeSegmentHeader::Near {
                            table_offset,
                            entry_count,
                        } => format!("near taboff={} n={}", table_offset, entry_count),
                        CodeSegmentHeader::ThinkFar {
                            has_relocations,
                            first_entry_index,
                            entry_count,
                        } => format!(
                            "think-far first_jt={} n={} relocs={}",
                            first_entry_index, entry_count, has_relocations
                        ),
                        CodeSegmentHeader::MpwFar => "mpw-far".to_string(),
                    };
                    eprintln!(
                        "[TRAP] LoadSeg seg={}: {} ptr=${:08X}",
                        seg_num, header_desc, ptr_base
                    );
                }

                let mut patched = 0u32;
                for i in 0..n_entries {
                    let p = ptr_base + i * 8;
                    // Skip already-loaded entries. MPW-style entries place
                    // JMP.L at +2; Think-style entries place it at +0.
                    if Self::loadseg_entry_is_loaded(bus, p) {
                        continue;
                    }
                    Self::patch_loadseg_entry(bus, seg_num, seg_addr, header_size, p);
                    patched += 1;
                }
                if trace_loadseg {
                    eprintln!(
                        "[TRAP] LoadSeg: patched {}/{} entries for seg {}",
                        patched, n_entries, seg_num
                    );
                }
            }

            // Ensure the calling entry itself was patched. In Think C format,
            // the calling entry may lie outside the segment header's taboff
            // range (entries for a given segment can be scattered across the JT).
            if let Some(entry_addr) = entry_addr {
                if !Self::loadseg_entry_is_loaded(bus, entry_addr) {
                    let code_addr =
                        Self::patch_loadseg_entry(bus, seg_num, seg_addr, header_size, entry_addr);
                    if trace_loadseg {
                        eprintln!(
                            "[TRAP] LoadSeg: patched calling entry at ${:08X} -> ${:08X}",
                            entry_addr, code_addr
                        );
                    }
                }
            }

            // Apply environment-driven byte patches now that this segment's
            // code is in RAM. Format:
            //   SYSTEMLESS_PATCH_BYTES="0xADDR=HEXBYTES,0xADDR2=BYTES2"
            // This is a diagnostics/test harness hook: callers provide both
            // the absolute address and replacement bytes explicitly.
            if let Ok(spec) = std::env::var("SYSTEMLESS_PATCH_BYTES") {
                for chunk in spec.split(',') {
                    let chunk = chunk.trim();
                    if chunk.is_empty() {
                        continue;
                    }
                    let Some((addr_str, bytes_str)) = chunk.split_once('=') else {
                        continue;
                    };
                    let addr_str = addr_str
                        .trim()
                        .trim_start_matches("0x")
                        .trim_start_matches("0X");
                    let Ok(addr) = u32::from_str_radix(addr_str, 16) else {
                        continue;
                    };
                    let bytes_str = bytes_str.trim();
                    if bytes_str.len() % 2 != 0 {
                        continue;
                    }
                    let mut written = 0u32;
                    for i in 0..(bytes_str.len() / 2) {
                        let hex = &bytes_str[i * 2..i * 2 + 2];
                        let Ok(byte) = u8::from_str_radix(hex, 16) else {
                            continue;
                        };
                        bus.write_byte(addr + written, byte);
                        written += 1;
                    }
                    eprintln!(
                        "[PATCH] Wrote {} bytes at ${:08X} (post-LoadSeg seg={})",
                        written, addr, seg_num
                    );
                }
            }

            // Re-execute the calling JT entry at the JMP instruction. MPW
            // entries dispatch at entry+2; Think C entries dispatch at entry+0.
            if let Some(entry_addr) = entry_addr {
                let jmp_addr = Self::loadseg_entry_dispatch_pc(bus, entry_addr);
                if trace_loadseg {
                    eprintln!("[TRAP] LoadSeg: re-executing JT entry at ${:08X}", jmp_addr);
                }
                cpu.write_reg(Register::PC, jmp_addr);
            }
            Ok(())
        } else {
            eprintln!("ERROR: LoadSeg unknown segment {}", seg_num);
            Err(Error::Halted)
        }
    }

    fn should_trace_extended_resource_activity(&self) -> bool {
        trace_sound_resource_enabled()
            && self
                .resources
                .as_ref()
                .is_some_and(|resources| resources.files.len() > 1)
    }

    fn add_resource_materialization_tick_cost(&mut self, bus: &MacMemoryBus, ptr: u32) {
        let Some(byte_len) = bus.get_alloc_size(ptr) else {
            return;
        };
        self.add_hle_tick_cost(Self::resource_load_tick_cost(byte_len));
    }

    fn resource_trace_chain(&self) -> String {
        self.resource_search_order()
            .into_iter()
            .map(|refnum| {
                if let Some(name) = self.resource_file_name(refnum) {
                    format!("{}:{}", refnum, name)
                } else {
                    refnum.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    // ResLoad is guest-owned state: assembly callers may write the byte
    // directly rather than call SetResLoad. Inside Macintosh I (1985), I-118;
    // More Macintosh Toolbox (1993), pp. 1-75 and 1-79.
    pub(crate) fn resource_loading_enabled(bus: &MacMemoryBus) -> bool {
        bus.read_byte(crate::memory::globals::addr::RES_LOAD) != 0
    }

    pub(crate) fn get_or_create_resource_handle_in_file(
        &mut self,
        bus: &mut MacMemoryBus,
        res_type: [u8; 4],
        res_id: i16,
        ptr: u32,
        refnum: u16,
    ) -> u32 {
        let key = (refnum, res_type, res_id);
        let recorded_resident = self.resident_resources.contains(&key);
        if let Some(handle) = self.resource_handles_by_key.get(&key).copied() {
            if let Some((mut existing_ptr, existing_type, existing_id)) =
                self.loaded_handles.get(&handle).copied()
            {
                if existing_type == res_type
                    && existing_id == res_id
                    && self.resource_handle_files.get(&handle).copied() == Some(refnum)
                {
                    // IM:More Macintosh Toolbox 1993, 1-79: SetResLoad(TRUE)
                    // restores automatic loading for resource-returning routines.
                    // If this resource was first seen while SetResLoad(FALSE) was
                    // active, the handle exists but its master pointer is NIL; a
                    // later lookup with loading enabled should fill it and restore
                    // the reverse ptr->handle ownership so RecoverHandle keeps
                    // working after the refill.
                    if existing_ptr == 0 && ptr != 0 {
                        existing_ptr = ptr;
                        self.with_resource_manager_mut(|resource_manager| {
                            resource_manager
                                .loaded_handles
                                .insert(handle, (ptr, existing_type, existing_id));
                        });
                    }
                    if bus.read_long(handle) == 0
                        && (Self::resource_loading_enabled(bus) || recorded_resident)
                    {
                        bus.write_long(handle, existing_ptr);
                        self.add_resource_materialization_tick_cost(bus, existing_ptr);
                    }
                    if bus.read_long(handle) != 0 && existing_ptr != 0 {
                        self.with_resource_manager_mut(|resource_manager| {
                            resource_manager.resident_resources.insert(key);
                        });
                        self.track_handle_ptr(existing_ptr, handle);
                    }
                    self.update_handle_state_bits(handle, |state| {
                        Some(state.unwrap_or(0x40) | 0x20)
                    });
                    return handle;
                }
            }
            self.with_resource_manager_mut(|resource_manager| {
                resource_manager.resource_handles_by_key.remove(&key);
            });
        }

        let handle = bus.alloc(4);
        // IM:More Macintosh Toolbox 1993, 1-79: SetResLoad(FALSE) returns
        // an empty handle (master pointer NIL) for resource data that is not
        // already in memory. Keep the true data pointer in loaded_handles so
        // LoadResource can populate the master pointer later.
        let materialize = Self::resource_loading_enabled(bus) || recorded_resident;
        bus.write_long(handle, if materialize { ptr } else { 0 });
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager
                .loaded_handles
                .insert(handle, (ptr, res_type, res_id));
            resource_manager.resource_handle_files.insert(handle, refnum);
        });
        self.remember_resource_handle_index(handle, key.0, key.1, key.2);
        self.update_handle_state_bits(handle, |state| Some(state.unwrap_or(0x40) | 0x20));
        if materialize && ptr != 0 {
            self.with_resource_manager_mut(|resource_manager| {
                resource_manager.resident_resources.insert(key);
            });
            self.track_handle_ptr(ptr, handle);
            self.add_resource_materialization_tick_cost(bus, ptr);
        }
        handle
    }

    pub(crate) fn get_or_create_resource_handle(
        &mut self,
        bus: &mut MacMemoryBus,
        res_type: [u8; 4],
        res_id: i16,
        ptr: u32,
    ) -> u32 {
        let refnum = self
            .resource_refnum_for_ptr(res_type, res_id, ptr)
            .unwrap_or_else(|| self.current_resource_refnum());
        self.get_or_create_resource_handle_in_file(bus, res_type, res_id, ptr, refnum)
    }

    fn resource_file_contains(&self, refnum: u16, res_type: [u8; 4], res_id: i16) -> bool {
        if self.resources.as_ref().is_some_and(|resources| {
            resources
                .files
                .get(&refnum)
                .is_some_and(|file| file.loaded.contains_key(&(res_type, res_id)))
        }) {
            return true;
        }

        if self
            .resource_backing_data
            .contains_key(&(refnum, res_type, res_id))
        {
            return true;
        }

        false
    }

    pub(crate) fn resource_file_contains_type(&self, refnum: u16, res_type: [u8; 4]) -> bool {
        self.resources.as_ref().is_some_and(|resources| {
            resources
                .files
                .get(&refnum)
                .is_some_and(|file| file.loaded.keys().any(|(kind, _)| *kind == res_type))
        }) || self
            .resource_backing_data
            .keys()
            .any(|(file, kind, _)| *file == refnum && *kind == res_type)
    }

    fn resource_ptr_referenced_elsewhere(&self, refnum: u16, ptr: u32) -> bool {
        self.resources.as_ref().is_some_and(|resources| {
            resources.files.iter().any(|(other_refnum, file)| {
                *other_refnum != refnum && file.loaded.values().any(|&p| p == ptr)
            })
        })
    }

    pub(crate) fn reload_resource_data_from_file(
        &mut self,
        bus: &mut MacMemoryBus,
        refnum: u16,
        res_type: [u8; 4],
        res_id: i16,
    ) -> Option<u32> {
        let (data, attrs, name) = if let Some(data) = self
            .resource_backing_data
            .get(&(refnum, res_type, res_id))
            .cloned()
        {
            let attrs = self
                .resources
                .as_ref()
                .and_then(|resources| resources.files.get(&refnum))
                .and_then(|file| file.attrs.get(&(res_type, res_id)).copied())
                .unwrap_or(0);
            let name = self
                .resources
                .as_ref()
                .and_then(|resources| resources.files.get(&refnum))
                .and_then(|file| file.names_by_id.get(&(res_type, res_id)).cloned());
            (data, attrs, name)
        } else {
            let file_name = self.resource_file_name(refnum)?.to_string();
            let rsrc_bytes = self.vfs_rsrc.get(&file_name)?;
            let fork = ResourceFork::parse(rsrc_bytes)?;
            let resource = fork.resources().get(&(res_type, res_id))?;
            let data = resource.data.clone();
            let attrs = resource.attrs;
            let name = resource.name.clone();
            self.remember_resource_backing_data(refnum, res_type, res_id, data.clone());
            (data, attrs, name)
        };
        let ptr = bus.alloc(data.len() as u32);
        if ptr == 0 {
            return None;
        }
        bus.write_bytes(ptr, &data);
        Self::zero_loaded_resource_padding(bus, ptr, data.len() as u32);

        self.with_resource_manager_mut(|resource_manager| {
            if let Some(file) = resource_manager
                .resources
                .as_mut()
                .and_then(|resources| resources.files.get_mut(&refnum))
            {
                file.loaded.insert((res_type, res_id), ptr);
                file.attrs.insert((res_type, res_id), attrs);
                if let Some(name) = name {
                    file.named.insert((res_type, name.clone()), (res_id, ptr));
                    file.names_by_id.insert((res_type, res_id), name);
                }
            }
        });

        Some(ptr)
    }

    fn find_or_load_resource_current(
        &mut self,
        bus: &mut MacMemoryBus,
        res_type: [u8; 4],
        res_id: i16,
    ) -> Option<(u16, u32)> {
        let res_type = super::TrapDispatcher::normalize_ostype(res_type);
        let refnum = self.current_resource_refnum();
        let live_ptr = self
            .resources
            .as_ref()
            .and_then(|resources| resources.files.get(&refnum))
            .and_then(|file| file.loaded.get(&(res_type, res_id)).copied())
            .filter(|ptr| *ptr != 0);
        if let Some(ptr) = live_ptr {
            return Some((refnum, ptr));
        }

        let known_entry = self
            .resources
            .as_ref()
            .and_then(|resources| resources.files.get(&refnum))
            .is_some_and(|file| file.loaded.contains_key(&(res_type, res_id)));
        if known_entry || self.resource_file_contains(refnum, res_type, res_id) {
            return self
                .reload_resource_data_from_file(bus, refnum, res_type, res_id)
                .map(|ptr| (refnum, ptr));
        }

        None
    }

    fn reload_named_resource_if_needed(
        &mut self,
        bus: &mut MacMemoryBus,
        refnum: u16,
        res_type: [u8; 4],
        res_id: i16,
        ptr: u32,
    ) -> Option<u32> {
        // More Macintosh Toolbox 1993, 1-79: with SetResLoad(FALSE),
        // resource-returning routines may return an empty handle; preserve
        // that contract while restoring released resources when automatic
        // loading is enabled.
        if ptr != 0 || !Self::resource_loading_enabled(bus) {
            return Some(ptr);
        }

        self.reload_resource_data_from_file(bus, refnum, res_type, res_id)
    }

    pub(crate) fn find_named_resource_current_loaded(
        &mut self,
        bus: &mut MacMemoryBus,
        res_type: [u8; 4],
        name: &str,
    ) -> Option<(u16, i16, u32)> {
        let (refnum, res_id, ptr) = self.find_named_resource_current(res_type, name)?;
        let ptr = self.reload_named_resource_if_needed(bus, refnum, res_type, res_id, ptr)?;
        Some((refnum, res_id, ptr))
    }

    pub(crate) fn find_named_resource_any_loaded(
        &mut self,
        bus: &mut MacMemoryBus,
        res_type: [u8; 4],
        name: &str,
    ) -> Option<(u16, i16, u32)> {
        let (refnum, res_id, ptr) = self.find_named_resource_any(res_type, name)?;
        let ptr = self.reload_named_resource_if_needed(bus, refnum, res_type, res_id, ptr)?;
        Some((refnum, res_id, ptr))
    }

    pub(crate) fn find_or_load_resource_any(
        &mut self,
        bus: &mut MacMemoryBus,
        res_type: [u8; 4],
        res_id: i16,
    ) -> Option<(u16, u32)> {
        let res_type = super::TrapDispatcher::normalize_ostype(res_type);
        for refnum in self.resource_search_order() {
            let live_ptr = self
                .resources
                .as_ref()
                .and_then(|resources| resources.files.get(&refnum))
                .and_then(|file| file.loaded.get(&(res_type, res_id)).copied())
                .filter(|ptr| *ptr != 0);
            if let Some(ptr) = live_ptr {
                return Some((refnum, ptr));
            }

            let known_entry = self
                .resources
                .as_ref()
                .and_then(|resources| resources.files.get(&refnum))
                .is_some_and(|file| file.loaded.contains_key(&(res_type, res_id)));
            if known_entry || self.resource_file_contains(refnum, res_type, res_id) {
                if let Some(ptr) =
                    self.reload_resource_data_from_file(bus, refnum, res_type, res_id)
                {
                    return Some((refnum, ptr));
                }
            }
        }

        None
    }

    fn restore_loaded_resource_handle(&mut self, handle: u32, ptr: u32) {
        self.track_handle_ptr(ptr, handle);
        self.update_handle_state_bits(handle, |state| Some(state.unwrap_or(0x40) | 0x20));
        self.with_resource_manager_mut(|resource_manager| {
            if let (Some((_, res_type, res_id)), Some(refnum)) = (
                resource_manager.loaded_handles.get(&handle).copied(),
                resource_manager.resource_handle_files.get(&handle).copied(),
            ) {
                resource_manager
                    .resident_resources
                    .insert((refnum, res_type, res_id));
            }
            resource_manager.detached_handles.remove(&handle);
            if let Some(refnum) = resource_manager.detached_handle_files.remove(&handle) {
                // A reload should repair any stale detached-file bookkeeping so
                // later Resource Manager queries still see a live resource.
                resource_manager.resource_handle_files.insert(handle, refnum);
            }
        });
    }

    pub(crate) fn forget_resource_residency_for_handle(&mut self, handle: u32) {
        self.with_resource_manager_mut(|resource_manager| {
            if let (Some((_, res_type, res_id)), Some(refnum)) = (
                resource_manager.loaded_handles.get(&handle).copied(),
                resource_manager.resource_handle_files.get(&handle).copied(),
            ) {
                resource_manager
                    .resident_resources
                    .remove(&(refnum, res_type, res_id));
            }
        });
    }

    /// Retain a resource handle's identity while marking its data nonresident.
    ///
    /// `EmptyHandle` releases the relocatable block and sets the master
    /// pointer to `NIL`; a later Resource Manager lookup may reload the data
    /// into the same handle. Inside Macintosh: Memory (1992), pp. 2-51--2-52,
    /// and More Macintosh Toolbox (1993), pp. 1-79--1-80.
    pub(crate) fn empty_resource_handle_residency(&mut self, handle: u32) {
        self.with_resource_manager_mut(|resource_manager| {
            let Some((_, res_type, res_id)) =
                resource_manager.loaded_handles.get(&handle).copied()
            else {
                return;
            };
            let Some(refnum) = resource_manager.resource_handle_files.get(&handle).copied() else {
                return;
            };
            resource_manager
                .resident_resources
                .remove(&(refnum, res_type, res_id));
            if let Some(entry) = resource_manager.loaded_handles.get_mut(&handle) {
                entry.0 = 0;
            }
            if let Some(file) = resource_manager
                .resources
                .as_mut()
                .and_then(|resources| resources.files.get_mut(&refnum))
            {
                file.loaded.insert((res_type, res_id), 0);
                for ((named_type, _), (named_id, ptr)) in &mut file.named {
                    if *named_type == res_type && *named_id == res_id {
                        *ptr = 0;
                    }
                }
            }
        });
    }

    pub(crate) fn live_resource_identity_for_handle(
        &self,
        handle: u32,
    ) -> Option<([u8; 4], i16, Option<u16>)> {
        let (_, res_type, res_id) = self.loaded_handles.get(&handle).copied()?;
        let refnum = self.resource_handle_files.get(&handle).copied();
        Some((res_type, res_id, refnum))
    }

    fn current_system_reference_for_handle(&self, handle: u32) -> Option<(u16, i16)> {
        let (ptr, res_type, _res_id) = self.loaded_handles.get(&handle).copied()?;
        let home_refnum = self.resource_handle_files.get(&handle).copied()?;
        if home_refnum != 0 {
            return None;
        }

        let current_refnum = self.current_resource_refnum();
        if current_refnum == 0 {
            return None;
        }

        let resources = self.resources.as_ref()?;
        let file = resources.files.get(&current_refnum)?;
        file.loaded
            .iter()
            .find_map(|((loaded_type, loaded_id), loaded_ptr)| {
                if *loaded_type == res_type
                    && *loaded_ptr == ptr
                    && file
                        .attrs
                        .get(&(*loaded_type, *loaded_id))
                        .is_some_and(|attrs| (*attrs & Self::RES_SYS_REF_ATTR as u8) != 0)
                {
                    Some((current_refnum, *loaded_id))
                } else {
                    None
                }
            })
    }

    pub(crate) fn resource_record_for_handle(&self, handle: u32) -> Option<(u16, [u8; 4], i16)> {
        let (_, res_type, res_id) = self.loaded_handles.get(&handle).copied()?;
        let home_refnum = self.resource_handle_files.get(&handle).copied()?;
        if let Some((current_refnum, current_id)) = self.current_system_reference_for_handle(handle)
        {
            return Some((current_refnum, res_type, current_id));
        }
        Some((home_refnum, res_type, res_id))
    }

    pub(crate) fn resource_attributes_for_handle(&self, handle: u32) -> Option<u16> {
        let (refnum, res_type, res_id) = self.resource_record_for_handle(handle)?;
        let attrs = self.resources.as_ref().and_then(|resources| {
            resources
                .files
                .get(&refnum)
                .and_then(|file| file.attrs.get(&(res_type, res_id)).copied())
        });
        Some(attrs.unwrap_or(0) as u16)
    }

    pub(crate) fn remove_resource_reference(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
    ) -> bool {
        const RMV_RES_FAILED: i16 = -196;

        if let Some((refnum, res_type, res_id)) = self.resource_record_for_handle(handle) {
            let current_refnum = self.current_resource_refnum();
            let attrs = self.resource_attributes_for_handle(handle).unwrap_or(0);
            let home_refnum = self.resource_handle_files.get(&handle).copied();
            let shadowed_system_ref = home_refnum == Some(0)
                && self.current_system_reference_for_handle(handle).is_some();

            // Must be in the current file and not protected.
            if (!shadowed_system_ref && refnum != current_refnum) || (attrs & 0x0008) != 0 {
                bus.write_word(0x0A60, RMV_RES_FAILED as u16);
                false
            } else if shadowed_system_ref {
                self.with_resource_manager_mut(|resource_manager| {
                    if let Some(resources) = resource_manager.resources.as_mut() {
                        if let Some(file) = resources.files.get_mut(&refnum) {
                            file.loaded.remove(&(res_type, res_id));
                            file.attrs.remove(&(res_type, res_id));
                            file.names_by_id.remove(&(res_type, res_id));
                            file.named
                                .retain(|(t, _), (id, _)| !(*t == res_type && *id == res_id));
                        }
                    }
                    if let Some(order) = resource_manager.resource_file_order.get_mut(&refnum) {
                        order.retain(|key| *key != (res_type, res_id));
                    }
                });
                self.forget_resource_backing_data(refnum, res_type, res_id);
                bus.write_word(0x0A60, 0);
                true
            } else {
                self.with_resource_manager_mut(|resource_manager| {
                    if let Some(resources) = resource_manager.resources.as_mut() {
                        if let Some(file) = resources.files.get_mut(&current_refnum) {
                            file.loaded.remove(&(res_type, res_id));
                            file.attrs.remove(&(res_type, res_id));
                            file.names_by_id.remove(&(res_type, res_id));
                            file.named
                                .retain(|(t, _), (id, _)| !(*t == res_type && *id == res_id));
                        }
                    }
                    if let Some(order) = resource_manager
                        .resource_file_order
                        .get_mut(&current_refnum)
                    {
                        order.retain(|key| *key != (res_type, res_id));
                    }
                });
                self.forget_resource_backing_data(current_refnum, res_type, res_id);
                self.forget_resource_handle_index_for_handle(handle);
                self.with_resource_manager_mut(|resource_manager| {
                    resource_manager.loaded_handles.remove(&handle);
                    resource_manager.resource_handle_files.remove(&handle);
                });
                self.update_handle_state_bits(handle, |state| Some(state.unwrap_or(0x40) & !0x20));
                bus.write_word(0x0A60, 0);
                true
            }
        } else {
            bus.write_word(0x0A60, RMV_RES_FAILED as u16);
            false
        }
    }

    pub(crate) fn add_resource_reference(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        new_id: i16,
        name_ptr: u32,
    ) -> bool {
        const ADD_REF_FAILED: i16 = -195;

        let Some((res_type, _res_id, source_refnum)) =
            self.live_resource_identity_for_handle(handle)
        else {
            bus.write_word(0x0A60, ADD_REF_FAILED as u16);
            return false;
        };
        if source_refnum != Some(0) {
            bus.write_word(0x0A60, ADD_REF_FAILED as u16);
            return false;
        }

        let current_refnum = self.current_resource_refnum();
        if current_refnum == 0 {
            bus.write_word(0x0A60, ADD_REF_FAILED as u16);
            return false;
        }

        let Some(ptr) = self.loaded_handles.get(&handle).map(|(ptr, _, _)| *ptr) else {
            bus.write_word(0x0A60, ADD_REF_FAILED as u16);
            return false;
        };

        let Some(source_attrs) = self.resource_attributes_for_handle(handle) else {
            bus.write_word(0x0A60, ADD_REF_FAILED as u16);
            return false;
        };

        let name = (name_ptr != 0)
            .then(|| bus.read_pstring(name_ptr))
            .and_then(|name| String::from_utf8(name).ok())
            .filter(|name| !name.is_empty());
        let inserted = self.with_resource_manager_mut(|resource_manager| {
            let Some(resources) = resource_manager.resources.as_mut() else {
                return false;
            };
            let Some(file) = resources.files.get_mut(&current_refnum) else {
                return false;
            };

            if file.loaded.contains_key(&(res_type, new_id))
                || file
                    .loaded
                    .values()
                    .any(|&existing_ptr| existing_ptr == ptr)
            {
                return false;
            }

            let attrs =
                (source_attrs as u8) | Self::RES_CHANGED_ATTR as u8 | Self::RES_SYS_REF_ATTR as u8;
            file.loaded.insert((res_type, new_id), ptr);
            file.attrs.insert((res_type, new_id), attrs);
            file.map_attrs |= Self::RES_MAP_CHANGED_ATTR;
            if let Some(name) = name {
                file.named.insert((res_type, name.clone()), (new_id, ptr));
                file.names_by_id.insert((res_type, new_id), name);
            }
            resource_manager
                .resource_file_order
                .entry(current_refnum)
                .or_default()
                .push((res_type, new_id));
            true
        });
        if !inserted {
            bus.write_word(0x0A60, ADD_REF_FAILED as u16);
            return false;
        }

        if let Some(size) = bus.get_alloc_size(ptr) {
            self.remember_resource_backing_data(
                current_refnum,
                res_type,
                new_id,
                bus.read_bytes(ptr, size as usize),
            );
        }

        bus.write_word(0x0A60, 0);
        true
    }

    /// Simulate writing a resource-backed handle out to disk by
    /// clearing its resChanged bit in the resource map. Returns true
    /// when the handle was a writable changed resource and the flag was
    /// cleared, false otherwise.
    pub(crate) fn write_resource_backing_if_changed(
        &mut self,
        bus: &MacMemoryBus,
        handle: u32,
    ) -> bool {
        let Some((refnum, res_type, res_id)) = self.resource_record_for_handle(handle) else {
            return false;
        };
        let attrs = self.resource_attributes_for_handle(handle).unwrap_or(0);
        if (attrs & 0x0008) != 0 || (attrs & Self::RES_CHANGED_ATTR) == 0 {
            return false;
        }

        let handle_ptr = bus.read_long(handle);
        let backing_ptr = if handle_ptr != 0 {
            handle_ptr
        } else {
            self.loaded_handles
                .get(&handle)
                .map(|(ptr, _, _)| *ptr)
                .unwrap_or(0)
        };
        if backing_ptr != 0 {
            if let Some(size) = bus.get_alloc_size(backing_ptr) {
                let data = bus.read_bytes(backing_ptr, size as usize);
                self.remember_resource_backing_data(refnum, res_type, res_id, data);
            }
        }

        self.with_resource_manager_mut(|resource_manager| {
            if let Some(resources) = resource_manager.resources.as_mut() {
                if let Some(file) = resources.files.get_mut(&refnum) {
                    if let Some(a) = file.attrs.get_mut(&(res_type, res_id)) {
                        *a &= !(Self::RES_CHANGED_ATTR as u8);
                    }
                    if !file
                        .attrs
                        .values()
                        .any(|attr| (*attr & Self::RES_CHANGED_ATTR as u8) != 0)
                    {
                        file.map_attrs &= !Self::RES_MAP_CHANGED_ATTR;
                    }
                }
            }
        });
        true
    }

    fn resource_metadata_for_handle(&self, handle: u32) -> Option<([u8; 4], i16, Option<String>)> {
        let (refnum, res_type, res_id) = self.resource_record_for_handle(handle)?;

        let name = self.resources.as_ref().and_then(|resources| {
            resources
                .files
                .get(&refnum)
                .and_then(|file| file.names_by_id.get(&(res_type, res_id)).cloned())
        });

        Some((res_type, res_id, name))
    }

    pub(crate) fn rsrc_map_entry_for_handle(&self, handle: u32) -> Option<u32> {
        let (_, res_type, res_id) = self.loaded_handles.get(&handle).copied()?;
        let refnum = self.resource_handle_files.get(&handle).copied()?;
        if let Some(file_name) = self.resource_file_name(refnum) {
            if let Some(rsrc_bytes) = self.vfs_rsrc.get(file_name) {
                if let Some(fork) = ResourceFork::parse(rsrc_bytes) {
                    if let Some(resource) = fork.resources().get(&(res_type, res_id)) {
                        return Some(resource.reference_offset as u32);
                    }
                }
            }
        }

        let resources = self.resources.as_ref()?;
        let file = resources.files.get(&refnum)?;
        let mut ordered_keys: Vec<([u8; 4], i16)> = file.loaded.keys().copied().collect();
        ordered_keys.sort_unstable_by(|(type_a, id_a), (type_b, id_b)| {
            type_a.cmp(type_b).then(id_a.cmp(id_b))
        });
        let index = ordered_keys
            .iter()
            .position(|key| *key == (res_type, res_id))? as u32;
        Some(38 + index * 12)
    }

    fn resource_file_needs_writeback(
        file: &super::dispatch::ResourceFileMap,
        changed_attr: u8,
        changed_map_attr: u16,
    ) -> bool {
        (file.map_attrs & changed_map_attr) != 0
            || file
                .attrs
                .values()
                .any(|attrs| (*attrs & changed_attr) != 0)
    }

    fn resource_data_for_writeback(
        &self,
        bus: &MacMemoryBus,
        refnum: u16,
        res_type: [u8; 4],
        res_id: i16,
        ptr: u32,
    ) -> Option<Vec<u8>> {
        let live_ptr = self
            .handle_for_ptr(ptr)
            .map(|handle| bus.read_long(handle))
            .filter(|handle_ptr| *handle_ptr != 0)
            .unwrap_or(ptr);
        if live_ptr != 0 {
            if let Some(size) = bus.get_alloc_size(live_ptr) {
                return Some(bus.read_bytes(live_ptr, size as usize));
            }
        }

        if let Some(data) = self
            .resource_backing_data
            .get(&(refnum, res_type, res_id))
            .cloned()
        {
            return Some(data);
        }

        let file_name = self.resource_file_name(refnum)?;
        let rsrc_bytes = self.vfs_rsrc.get(file_name)?;
        let fork = ResourceFork::parse(rsrc_bytes)?;
        fork.resources()
            .get(&(res_type, res_id))
            .map(|resource| resource.data.clone())
    }

    fn serialize_resource_file_map_for_writeback(
        &self,
        bus: &MacMemoryBus,
        refnum: u16,
        file: &super::dispatch::ResourceFileMap,
    ) -> Option<Vec<u8>> {
        const DATA_OFFSET: u32 = 16;

        struct WriteEntry {
            id: i16,
            data: Vec<u8>,
            attrs: u8,
            name: Option<Vec<u8>>,
            name_offset: Option<u16>,
            data_offset: u32,
        }

        let changed_attr = Self::RES_CHANGED_ATTR as u8;
        let mut type_groups: BTreeMap<[u8; 4], Vec<WriteEntry>> = BTreeMap::new();
        for (&(res_type, res_id), &ptr) in &file.loaded {
            let data = self.resource_data_for_writeback(bus, refnum, res_type, res_id, ptr)?;
            let attrs = file.attrs.get(&(res_type, res_id)).copied().unwrap_or(0) & !changed_attr;
            let name = file.names_by_id.get(&(res_type, res_id)).and_then(|name| {
                let encoded = encode_mac_roman_lossy(name);
                if encoded.is_empty() {
                    None
                } else {
                    Some(encoded.into_iter().take(255).collect())
                }
            });
            type_groups.entry(res_type).or_default().push(WriteEntry {
                id: res_id,
                data,
                attrs,
                name,
                name_offset: None,
                data_offset: 0,
            });
        }

        if type_groups.is_empty() {
            let mut bytes = Self::empty_resource_fork_bytes();
            let map_start = 16usize;
            let map_attrs = file.map_attrs & !Self::RES_MAP_CHANGED_ATTR;
            bytes[map_start + 22..map_start + 24].copy_from_slice(&map_attrs.to_be_bytes());
            return Some(bytes);
        }

        let mut data_section = Vec::new();
        for entries in type_groups.values_mut() {
            entries.sort_by_key(|entry| entry.id);
            for entry in entries {
                if data_section.len() > 0x00FF_FFFF {
                    return None;
                }
                entry.data_offset = data_section.len() as u32;
                data_section.extend_from_slice(&(entry.data.len() as u32).to_be_bytes());
                data_section.extend_from_slice(&entry.data);
            }
        }

        let type_count = type_groups.len();
        if type_count == 0 || type_count > u16::MAX as usize + 1 {
            return None;
        }
        let resource_count: usize = type_groups.values().map(Vec::len).sum();
        let ref_lists_offset = 2usize.checked_add(type_count.checked_mul(8)?)?;
        let name_list_offset = 30usize
            .checked_add(ref_lists_offset)?
            .checked_add(resource_count.checked_mul(12)?)?;
        if name_list_offset > u16::MAX as usize {
            return None;
        }

        let mut name_section = Vec::new();
        for entries in type_groups.values_mut() {
            for entry in entries {
                if let Some(name) = entry.name.as_ref() {
                    if name_section.len() > u16::MAX as usize {
                        return None;
                    }
                    entry.name_offset = Some(name_section.len() as u16);
                    name_section.push(name.len() as u8);
                    name_section.extend_from_slice(name);
                }
            }
        }

        let map_length = name_list_offset.checked_add(name_section.len())?;
        let map_offset = DATA_OFFSET.checked_add(data_section.len() as u32)?;
        let total_len = (map_offset as usize).checked_add(map_length)?;
        let mut bytes = vec![0u8; total_len];
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&DATA_OFFSET.to_be_bytes());
        header[4..8].copy_from_slice(&map_offset.to_be_bytes());
        header[8..12].copy_from_slice(&(data_section.len() as u32).to_be_bytes());
        header[12..16].copy_from_slice(&(map_length as u32).to_be_bytes());
        bytes[0..16].copy_from_slice(&header);
        bytes[DATA_OFFSET as usize..DATA_OFFSET as usize + data_section.len()]
            .copy_from_slice(&data_section);

        let map_start = map_offset as usize;
        let type_list_offset = 30u16;
        bytes[map_start..map_start + 16].copy_from_slice(&header);
        bytes[map_start + 16..map_start + 20].copy_from_slice(&0u32.to_be_bytes());
        bytes[map_start + 20..map_start + 22].copy_from_slice(&0u16.to_be_bytes());
        let map_attrs = file.map_attrs & !Self::RES_MAP_CHANGED_ATTR;
        bytes[map_start + 22..map_start + 24].copy_from_slice(&map_attrs.to_be_bytes());
        bytes[map_start + 24..map_start + 26].copy_from_slice(&type_list_offset.to_be_bytes());
        bytes[map_start + 26..map_start + 28]
            .copy_from_slice(&(name_list_offset as u16).to_be_bytes());
        bytes[map_start + 28..map_start + 30]
            .copy_from_slice(&((type_count as u16) - 1).to_be_bytes());

        let type_list_start = map_start + type_list_offset as usize;
        bytes[type_list_start..type_list_start + 2]
            .copy_from_slice(&((type_count as u16) - 1).to_be_bytes());

        let mut next_ref_list_offset = ref_lists_offset;
        for (index, (res_type, entries)) in type_groups.iter().enumerate() {
            if entries.len() > u16::MAX as usize + 1 || next_ref_list_offset > u16::MAX as usize {
                return None;
            }

            let type_entry = type_list_start + 2 + index * 8;
            bytes[type_entry..type_entry + 4].copy_from_slice(res_type);
            bytes[type_entry + 4..type_entry + 6]
                .copy_from_slice(&((entries.len() as u16) - 1).to_be_bytes());
            bytes[type_entry + 6..type_entry + 8]
                .copy_from_slice(&(next_ref_list_offset as u16).to_be_bytes());

            let ref_list_start = type_list_start + next_ref_list_offset;
            for (entry_index, entry) in entries.iter().enumerate() {
                let ref_entry = ref_list_start + entry_index * 12;
                bytes[ref_entry..ref_entry + 2].copy_from_slice(&(entry.id as u16).to_be_bytes());
                let name_offset = entry.name_offset.unwrap_or(0xFFFF);
                bytes[ref_entry + 2..ref_entry + 4].copy_from_slice(&name_offset.to_be_bytes());
                bytes[ref_entry + 4] = entry.attrs;
                let data_offset = entry.data_offset.to_be_bytes();
                bytes[ref_entry + 5..ref_entry + 8].copy_from_slice(&data_offset[1..4]);
                bytes[ref_entry + 8..ref_entry + 12].copy_from_slice(&0u32.to_be_bytes());
            }

            next_ref_list_offset += entries.len() * 12;
        }

        let name_list_start = map_start + name_list_offset;
        bytes[name_list_start..name_list_start + name_section.len()].copy_from_slice(&name_section);
        Some(bytes)
    }

    pub(crate) fn flush_resource_file_refnum(&mut self, bus: &MacMemoryBus, refnum: u16) -> bool {
        if refnum == 0 {
            return false;
        }
        let Some((file_name, bytes)) = (|| {
            let resources = self.resources.as_ref()?;
            let file = resources.files.get(&refnum)?;
            if (file.map_attrs & Self::RES_MAP_READ_ONLY_ATTR) != 0 {
                return None;
            }
            if !Self::resource_file_needs_writeback(
                file,
                Self::RES_CHANGED_ATTR as u8,
                Self::RES_MAP_CHANGED_ATTR,
            ) {
                return None;
            }
            let file_name = resources.names.get(&refnum)?.clone();
            let bytes = self.serialize_resource_file_map_for_writeback(bus, refnum, file)?;
            Some((file_name, bytes))
        })() else {
            return false;
        };

        if let Some(fork) = ResourceFork::parse(&bytes) {
            self.clear_resource_file_backing_data(refnum);
            self.remember_resource_fork_backing_data(refnum, &fork);
        }

        let len = bytes.len();
        self.vfs_rsrc.insert(file_name.clone(), bytes);
        self.touch_vfs_entry(&file_name);
        if super::dispatch::trace_resfile_enabled() {
            eprintln!(
                "[RSRC] flushed resource refnum={} name={:?} bytes={}",
                refnum, file_name, len
            );
        }
        true
    }

    pub(crate) fn dispatch_resource<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        self.read_tick_count(bus);
        Some(match (is_tool, trap_num) {
            // ServerDispatch ($A094)
            // Dispatches synchronous Macintosh File Sharing server-control calls.
            // FUNCTION SyncServerDispatch(pb: SCParamBlockPtr): OSErr;
            // AppleShare 3.0 File Server Controls, Appendix B, pp. 73; SCShutDown, pp. 58-59.
            // Systemless does not run a file server, so valid control calls observe
            // the documented absent-server result instead of a synthetic service.
            (false, 0x94) => {
                const IO_RESULT_OFFSET: u32 = 16;
                const SC_CODE_OFFSET: u32 = 26;
                const PARAM_ERR: i16 = -50;

                let pb = cpu.read_reg(Register::A0);
                let result = if pb == 0 {
                    PARAM_ERR
                } else {
                    let sc_code = bus.read_word(pb + SC_CODE_OFFSET);
                    eprintln!(
                        "[SERVER] SyncServerDispatch pb=${pb:08X} scCode={sc_code} -> paramErr (server not running)"
                    );
                    bus.write_word(pb + IO_RESULT_OFFSET, PARAM_ERR as u16);
                    PARAM_ERR
                };
                cpu.write_reg(Register::D0, result as i32 as u32);
                Ok(())
            }

            // GetResource ($A9A0)
            // Returns a handle to the resource with the given type and ID.
            // FUNCTION GetResource(theType: ResType; theID: INTEGER): Handle;
            // Inside Macintosh Volume I, I-119
            // GetResource ($A9A0): Loads from ResourceFork, allocates in guest memory
            (true, 0x1A0) => {
                let sp = cpu.read_reg(Register::A7);
                let res_id = bus.read_word(sp) as i16;
                let raw_res_type = bus.read_long(sp + 2).to_be_bytes();
                let res_type = super::TrapDispatcher::normalize_ostype(raw_res_type);
                let trace_sound_resource = trace_sound_resource_enabled() && res_type == *b"snd ";
                let trace_extended = self.should_trace_extended_resource_activity();

                if trace_extended {
                    eprintln!(
                        "[RSRC] GetResource raw='{}' norm='{}' id={} current={} chain=[{}]",
                        format_ostype(raw_res_type),
                        format_ostype(res_type),
                        res_id,
                        self.current_resource_refnum(),
                        self.resource_trace_chain()
                    );
                }

                // Wide-net GetResource trace (gate: SYSTEMLESS_TRACE_GETRESOURCE=1).
                // Logs tick + type + id for every GetResource call.
                if trace_getresource_enabled() {
                    eprintln!(
                        "[GETRESOURCE] tick={} type='{}' id={}",
                        self.current_tick(),
                        format_ostype(res_type),
                        res_id,
                    );
                }

                if trace_menu_pict_enabled() && res_type == *b"PICT" {
                    eprintln!("[RSRC] GetResource('PICT', {})", res_id);
                }
                if trace_sound_resource {
                    eprintln!("[RSRC] GetResource('snd ', {})", res_id);
                }

                if let Some((refnum, ptr)) = self.find_or_load_resource_any(bus, res_type, res_id) {
                    if trace_menu_pict_enabled() && res_type == *b"PICT" {
                        eprintln!(
                            "[RSRC] GetResource('PICT', {}) -> ${:08X} (refnum {})",
                            res_id, ptr, refnum
                        );
                    }
                    if trace_sound_resource {
                        eprintln!(
                            "[RSRC] GetResource('snd ', {}) -> ${:08X} (refnum {})",
                            res_id, ptr, refnum
                        );
                    }
                    if trace_getresource_enabled() {
                        let preview: Vec<String> = bus
                            .read_bytes(ptr, 16)
                            .iter()
                            .map(|b| format!("{:02X}", b))
                            .collect();
                        eprintln!(
                            "[GETRESOURCE]   -> ptr=${:08X} preview={}",
                            ptr,
                            preview.join(" ")
                        );
                    }
                    let handle = self
                        .get_or_create_resource_handle_in_file(bus, res_type, res_id, ptr, refnum);

                    cpu.write_reg(Register::A0, handle);
                    cpu.write_reg(Register::D0, 0);
                    bus.write_word(0x0A60, 0); // ResErr = noErr
                    bus.write_long(sp + 6, handle);
                    cpu.write_reg(Register::A7, sp + 6);
                    return Some(Ok(()));
                }

                if trace_menu_pict_enabled() && res_type == *b"PICT" {
                    eprintln!("[RSRC] GetResource('PICT', {}) -> not found", res_id);
                }
                if trace_sound_resource {
                    eprintln!("[RSRC] GetResource('snd ', {}) -> not found", res_id);
                }

                if res_type == *b"ICON" {
                    if let Some(ptr) = self.synthesize_system_icon(bus, res_id) {
                        let handle = self.get_or_create_resource_handle(bus, res_type, res_id, ptr);
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::D0, 0);
                        bus.write_word(0x0A60, 0);
                        bus.write_long(sp + 6, handle);
                        cpu.write_reg(Register::A7, sp + 6);
                        return Some(Ok(()));
                    }
                }

                if res_type == *b"STR " {
                    if let Some(ptr) = self.synthesize_system_str(bus, res_id) {
                        let handle = self.get_or_create_resource_handle(bus, res_type, res_id, ptr);
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::D0, 0);
                        bus.write_word(0x0A60, 0); // ResErr = noErr
                        bus.write_long(sp + 6, handle);
                        cpu.write_reg(Register::A7, sp + 6);
                        return Some(Ok(()));
                    }
                }

                if res_type == *b"INTL" {
                    if let Some(ptr) = self.synthesize_system_intl(bus, res_id) {
                        let handle = self
                            .get_or_create_resource_handle_in_file(bus, res_type, res_id, ptr, 0);
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::D0, 0);
                        bus.write_word(0x0A60, 0); // ResErr = noErr
                        bus.write_long(sp + 6, handle);
                        cpu.write_reg(Register::A7, sp + 6);
                        return Some(Ok(()));
                    }
                }

                if res_type == *b"PAT#" {
                    if let Some(ptr) = self.synthesize_system_pattern_list(bus, res_id) {
                        let handle = self
                            .get_or_create_resource_handle_in_file(bus, res_type, res_id, ptr, 0);
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::D0, 0);
                        bus.write_word(0x0A60, 0); // ResErr = noErr
                        bus.write_long(sp + 6, handle);
                        cpu.write_reg(Register::A7, sp + 6);
                        return Some(Ok(()));
                    }
                }

                if res_type == *b"clut" {
                    if let Some(ptr) = self.synthesize_system_clut(bus, res_id) {
                        if trace_getresource_enabled() {
                            let preview: Vec<String> = bus
                                .read_bytes(ptr, 16)
                                .iter()
                                .map(|b| format!("{:02X}", b))
                                .collect();
                            eprintln!(
                                "[GETRESOURCE]   -> synthetic ptr=${:08X} preview={}",
                                ptr,
                                preview.join(" ")
                            );
                        }
                        let handle = self.get_or_create_resource_handle(bus, res_type, res_id, ptr);
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::D0, 0);
                        bus.write_word(0x0A60, 0); // ResErr = noErr
                        bus.write_long(sp + 6, handle);
                        cpu.write_reg(Register::A7, sp + 6);
                        return Some(Ok(()));
                    }
                }

                if res_type == *b"wctb" {
                    if let Some(ptr) = self.synthesize_system_wctb(bus, res_id) {
                        let handle = self.get_or_create_resource_handle(bus, res_type, res_id, ptr);
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::D0, 0);
                        bus.write_word(0x0A60, 0); // ResErr = noErr
                        bus.write_long(sp + 6, handle);
                        cpu.write_reg(Register::A7, sp + 6);
                        return Some(Ok(()));
                    }
                }

                if res_type == *b"WDEF" {
                    if let Some(ptr) = self.synthesize_system_wdef(bus, res_id) {
                        let handle = self
                            .get_or_create_resource_handle_in_file(bus, res_type, res_id, ptr, 0);
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::D0, 0);
                        bus.write_word(0x0A60, 0); // ResErr = noErr
                        bus.write_long(sp + 6, handle);
                        cpu.write_reg(Register::A7, sp + 6);
                        return Some(Ok(()));
                    }
                }

                if res_type == *b"MDEF" {
                    if let Some(ptr) = self.synthesize_system_mdef(bus, res_id) {
                        let handle = self
                            .get_or_create_resource_handle_in_file(bus, res_type, res_id, ptr, 0);
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::D0, 0);
                        bus.write_word(0x0A60, 0); // ResErr = noErr
                        bus.write_long(sp + 6, handle);
                        cpu.write_reg(Register::A7, sp + 6);
                        return Some(Ok(()));
                    }
                }

                if res_type == *b"KCHR" {
                    if let Some(ptr) = self.synthesize_system_kchr(bus, res_id) {
                        if trace_getresource_enabled() {
                            let preview: Vec<String> = bus
                                .read_bytes(ptr, 16)
                                .iter()
                                .map(|b| format!("{:02X}", b))
                                .collect();
                            eprintln!(
                                "[GETRESOURCE]   -> synthetic ptr=${:08X} preview={}",
                                ptr,
                                preview.join(" ")
                            );
                        }
                        let handle = self.get_or_create_resource_handle(bus, res_type, res_id, ptr);
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::D0, 0);
                        bus.write_word(0x0A60, 0); // ResErr = noErr
                        bus.write_long(sp + 6, handle);
                        cpu.write_reg(Register::A7, sp + 6);
                        return Some(Ok(()));
                    }
                }

                if res_type == *b"KMAP" {
                    if let Some(ptr) = self.synthesize_system_kmap(bus, res_id) {
                        let handle = self.get_or_create_resource_handle(bus, res_type, res_id, ptr);
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::D0, 0);
                        bus.write_word(0x0A60, 0); // ResErr = noErr
                        bus.write_long(sp + 6, handle);
                        cpu.write_reg(Register::A7, sp + 6);
                        return Some(Ok(()));
                    }
                }

                cpu.write_reg(Register::A0, 0);
                cpu.write_reg(Register::D0, 0);
                bus.write_word(0x0A60, 0);
                bus.write_long(sp + 6, 0);
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // Get1Resource ($A81F)
            // FUNCTION Get1Resource(theType: ResType; theID: INTEGER): Handle;
            // Searches only the current resource file (unlike GetResource which searches all).
            // Inside Macintosh Volume I, I-119
            // Get1Resource ($A81F): Searches current resource file only; writes ResErr per IM:I I-119
            (true, 0x01F) => {
                let sp = cpu.read_reg(Register::A7);
                let id = bus.read_word(sp) as i16;
                let raw_res_type = bus.read_long(sp + 2).to_be_bytes();
                let res_type = super::TrapDispatcher::normalize_ostype(raw_res_type);
                let trace_sound_resource = trace_sound_resource_enabled() && res_type == *b"snd ";
                let trace_extended = self.should_trace_extended_resource_activity();

                if trace_extended {
                    eprintln!(
                        "[RSRC] Get1Resource raw='{}' norm='{}' id={} current={} chain=[{}]",
                        format_ostype(raw_res_type),
                        format_ostype(res_type),
                        id,
                        self.current_resource_refnum(),
                        self.resource_trace_chain()
                    );
                }

                if trace_menu_pict_enabled() && res_type == *b"PICT" {
                    eprintln!("[RSRC] Get1Resource('PICT', {})", id);
                }
                if trace_sound_resource {
                    eprintln!("[RSRC] Get1Resource('snd ', {})", id);
                }

                let handle = if let Some((refnum, ptr)) =
                    self.find_or_load_resource_current(bus, res_type, id)
                {
                    if trace_menu_pict_enabled() && res_type == *b"PICT" {
                        eprintln!(
                            "[RSRC] Get1Resource('PICT', {}) -> ${:08X} (refnum {})",
                            id, ptr, refnum
                        );
                    }
                    if trace_sound_resource {
                        eprintln!(
                            "[RSRC] Get1Resource('snd ', {}) -> ${:08X} (refnum {})",
                            id, ptr, refnum
                        );
                    }
                    self.get_or_create_resource_handle_in_file(bus, res_type, id, ptr, refnum)
                } else if res_type == *b"STR " {
                    match self.synthesize_system_str(bus, id) {
                        Some(ptr) => self.get_or_create_resource_handle(bus, res_type, id, ptr),
                        None => 0,
                    }
                } else {
                    if trace_menu_pict_enabled() && res_type == *b"PICT" {
                        eprintln!("[RSRC] Get1Resource('PICT', {}) -> not found", id);
                    }
                    if trace_sound_resource {
                        eprintln!("[RSRC] Get1Resource('snd ', {}) -> not found", id);
                    }
                    0
                };

                // Per BasiliskII/System 7.5.3: misses return NIL and leave
                // ResError at noErr. Callers must check the returned handle,
                // not infer miss from ResError.
                if handle != 0 {
                    cpu.write_reg(Register::A0, handle);
                    cpu.write_reg(Register::D0, 0);
                    bus.write_word(0x0A60, 0); // ResErr = noErr
                    bus.write_long(sp + 6, handle);
                    cpu.write_reg(Register::A7, sp + 6);
                } else {
                    cpu.write_reg(Register::A0, 0);
                    cpu.write_reg(Register::D0, 0);
                    bus.write_word(0x0A60, 0);
                    bus.write_long(sp + 6, 0);
                    cpu.write_reg(Register::A7, sp + 6);
                }
                Ok(())
            }

            // DetachResource ($A992)
            // Sets the resource's master pointer so the resource data won't be released.
            // PROCEDURE DetachResource(theResource: Handle);
            // Inside Macintosh Volume I, I-122
            // DetachResource ($A992): Clones shared resource data into a detached private handle and stops treating that handle as resource-backed
            (true, 0x192) => {
                let sp = cpu.read_reg(Register::A7);
                let handle = bus.read_long(sp);
                if let Some((ptr, res_type, res_id)) = self.loaded_handles.get(&handle).copied() {
                    let attrs = self.resource_attributes_for_handle(handle).unwrap_or(0);
                    if (attrs & Self::RES_CHANGED_ATTR) != 0 {
                        bus.write_word(0x0A60, 0);
                    } else {
                        // DetachResource keeps the current handle/data valid but removes
                        // the link from the resource map. In this HLE, GetResource hands
                        // out per-call master pointers to shared guest data, so detaching
                        // must clone the block to keep subsequent SetHandleSize and
                        // DisposeHandle calls from mutating or freeing the shared resource.
                        // Macintosh Revealed 1987, p. 306
                        if handle != 0 && ptr != 0 {
                            if let Some(size) = bus.get_alloc_size(ptr) {
                                let detached_ptr = bus.alloc(size);
                                for offset in 0..size {
                                    bus.write_byte(
                                        detached_ptr + offset,
                                        bus.read_byte(ptr + offset),
                                    );
                                }
                                bus.write_long(handle, detached_ptr);
                            }
                        }
                        // Detaching severs this handle from the resource map without
                        // deleting the resource reference. Mark its shared data as
                        // unloaded so a later lookup reloads pristine fork bytes.
                        self.unload_resource_live_map_entry_for_handle(handle);
                        self.forget_resource_residency_for_handle(handle);
                        self.forget_resource_handle_index_for_handle(handle);
                        self.with_resource_manager_mut(|resource_manager| {
                            resource_manager.loaded_handles.remove(&handle);
                            if let Some(refnum) =
                                resource_manager.resource_handle_files.remove(&handle)
                            {
                                resource_manager.detached_handle_files.insert(handle, refnum);
                            }
                            resource_manager
                                .detached_handles
                                .insert(handle, (res_type, res_id));
                        });
                        self.update_handle_state_bits(handle, |state| {
                            Some(state.unwrap_or(0x40) & !0x20)
                        });
                        bus.write_word(0x0A60, 0);
                    }
                } else {
                    // Per BasiliskII/System 7.5.3: NIL and other
                    // non-resource handles return resNotFound.
                    bus.write_word(0x0A60, Self::RES_NOT_FOUND as u16);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // LoadResource ($A9A2)
            // Reads a resource into memory. No-op if already loaded.
            // PROCEDURE LoadResource(theResource: Handle);
            // Inside Macintosh: More Macintosh Toolbox 1993, 1-80
            //
            // ReleaseResource ($A9A3)
            // Releases the memory occupied by a resource.
            // PROCEDURE ReleaseResource(theResource: Handle);
            // Inside Macintosh Volume I, I-121
            // LoadResource ($A9A2): Populates empty SetResLoad(FALSE) handles and reloads resource-backed handles whose master pointer was later emptied/purged; BasiliskII leaves plain handles untouched and reports `noErr` there.
            (true, 0x1A2) => {
                let sp = cpu.read_reg(Register::A7);
                let handle = bus.read_long(sp);
                if handle == 0 {
                    bus.write_word(0x0A60, Self::RES_NOT_FOUND as u16);
                } else if let Some((ptr, res_type, res_id)) =
                    self.loaded_handles.get(&handle).copied()
                {
                    if super::dispatch::trace_resfile_enabled() {
                        let type_str = String::from_utf8_lossy(&res_type);
                        eprintln!(
                            "[TRAP] LoadResource handle=${:08X} '{}' {} ptr=${:08X}",
                            handle, type_str, res_id, ptr
                        );
                    }
                    let reloaded_ptr = if ptr != 0 {
                        Some(ptr)
                    } else {
                        self.resource_handle_files
                            .get(&handle)
                            .copied()
                            .and_then(|refnum| {
                                self.reload_resource_data_from_file(bus, refnum, res_type, res_id)
                            })
                    };
                    if let Some(ptr) = reloaded_ptr {
                        // Successful reload must canonicalize the master
                        // pointer again even if the caller had emptied or
                        // scribbled over it first.
                        let was_empty = bus.read_long(handle) == 0;
                        bus.write_long(handle, ptr);
                        self.with_resource_manager_mut(|resource_manager| {
                            if let Some(entry) = resource_manager.loaded_handles.get_mut(&handle) {
                                entry.0 = ptr;
                            }
                        });
                        self.restore_loaded_resource_handle(handle, ptr);
                        if was_empty {
                            self.add_resource_materialization_tick_cost(bus, ptr);
                        }
                        bus.write_word(0x0A60, 0);
                    } else {
                        bus.write_word(0x0A60, MEM_FULL_ERR as u16);
                    }
                } else {
                    bus.write_word(0x0A60, 0);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // ReleaseResource ($A9A3)
            // Releases the memory occupied by a resource and invalidates the
            // caller's handle-to-resource association. The canonical resource
            // bytes remain in Systemless's resource map, so a later GetResource
            // allocates a fresh handle for the same resource data.
            // PROCEDURE ReleaseResource(theResource: Handle);
            // Inside Macintosh Volume I, I-120 to I-121
            // ReleaseResource ($A9A3): Clears a resource handle's master pointer and invalidates its Resource Manager handle identity; returns `resNotFound` for non-resource handles
            (true, 0x1A3) => {
                let sp = cpu.read_reg(Register::A7);
                let handle = bus.read_long(sp);
                if let Some((ptr, res_type, res_id)) = self.loaded_handles.get(&handle).copied() {
                    if super::dispatch::trace_resfile_enabled() {
                        let type_str = String::from_utf8_lossy(&res_type);
                        eprintln!(
                            "[TRAP] ReleaseResource handle=${:08X} '{}' {} ptr=${:08X}",
                            handle, type_str, res_id, ptr
                        );
                    }
                    let attrs = self.resource_attributes_for_handle(handle).unwrap_or(0);
                    if (attrs & Self::RES_CHANGED_ATTR) == 0 {
                        self.forget_resource_residency_for_handle(handle);
                        let resource_record = self.resource_record_for_handle(handle);
                        let can_reload = resource_record
                            .map(|(refnum, record_type, record_id)| {
                                self.resource_file_contains(refnum, record_type, record_id)
                            })
                            .unwrap_or(false);
                        if can_reload {
                            if let Some((refnum, record_type, record_id)) = resource_record {
                                if !self.resource_backing_data.contains_key(&(
                                    refnum,
                                    record_type,
                                    record_id,
                                )) {
                                    if let Some(size) = bus.get_alloc_size(ptr) {
                                        self.remember_resource_backing_data(
                                            refnum,
                                            record_type,
                                            record_id,
                                            bus.read_bytes(ptr, size as usize),
                                        );
                                    }
                                }
                            }
                        }
                        bus.write_long(handle, 0);
                        self.untrack_handle_ptr(ptr);
                        self.forget_resource_handle_index_for_handle(handle);
                        self.with_resource_manager_mut(|resource_manager| {
                            resource_manager.loaded_handles.remove(&handle);
                            resource_manager.resource_handle_files.remove(&handle);
                        });
                        self.remove_handle_state_bits(handle);
                        if can_reload {
                            if let Some((refnum, record_type, record_id)) = resource_record {
                                self.with_resource_manager_mut(|resource_manager| {
                                    if let Some(file) = resource_manager
                                        .resources
                                        .as_mut()
                                        .and_then(|resources| resources.files.get_mut(&refnum))
                                    {
                                        if file
                                            .loaded
                                            .get(&(record_type, record_id))
                                            .is_some_and(|&loaded_ptr| loaded_ptr == ptr)
                                        {
                                            file.loaded.insert((record_type, record_id), 0);
                                        }
                                        if let Some(name) = file
                                            .names_by_id
                                            .get(&(record_type, record_id))
                                            .cloned()
                                        {
                                            if file
                                                .named
                                                .get(&(record_type, name.clone()))
                                                .is_some_and(|(_, named_ptr)| *named_ptr == ptr)
                                            {
                                                file.named
                                                    .insert((record_type, name), (record_id, 0));
                                            }
                                        }
                                    }
                                });
                                if !self.resource_ptr_referenced_elsewhere(refnum, ptr) {
                                    bus.free(ptr);
                                }
                            }
                        }
                    }
                    bus.write_word(0x0A60, 0);
                } else {
                    bus.write_word(0x0A60, Self::RES_NOT_FOUND as u16);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SizeResource ($A9A5)
            // Returns the size (in bytes) of a resource in the resource file.
            // FUNCTION SizeResource(theResource: Handle): LONGINT;
            // Inside Macintosh Volume I, I-121
            //
            // Both paths must update ResErr: success → noErr, failure →
            // resNotFound. Before this fix the success path left ResErr
            // untouched, so a caller that ran a successful SizeResource
            // after an earlier failed Resource Manager call would see a
            // stale -192.
            //
            // SizeResource ($A9A5): Returns data size; clears ResErr on success per IM:I I-121
            (true, 0x1A5) => self.handle_resource_size_query(bus, cpu),

            // RGetResource ($A80C)
            // GetResource that includes ROM-resident resources in the
            // search.
            // FUNCTION RGetResource(theType: ResType; theID: INTEGER): Handle;
            // Inside Macintosh: More Macintosh Toolbox 1993, 1-78
            // (also Inside Macintosh Volume V, V-159)
            //
            // IM:MTb 1-78: "RGetResource first uses GetResource to
            // search [the chain]. [...] If GetResource doesn't find
            // the specified resource [...] RGetResource sets the
            // global variable RomMapInsert to TRUE, then calls
            // GetResource again. In response, GetResource [...]
            // looks in the resource map of the ROM-resident resources
            // before searching the resource map of the System file."
            //
            // Systemless's HLE has no ROM resource fork, so the second
            // pass would walk an empty ROM map and produce the same
            // NIL. Delegate the entire call to GetResource ($A9A0).
            // The RomMapInsert global is documented as auto-cleared
            // per call; since no other Systemless trap consumes it,
            // modelling it would be cosmetic — skip.
            //
            // Stack frame is identical to GetResource:
            //   pre:  SP -> [theID(2)][theType(4)][result-slot(4)]
            //   post: SP -> [result(4)]; pop 6, leave 4.
            //
            // Regression coverage:
            //   rgetresource_returns_handle_for_open_chain_match
            //   rgetresource_miss_returns_nil_with_resnotfound
            //   rgetresource_walks_chain_not_just_current_file
            // RGetResource ($A80C): Reuses GetResource for the resource-chain
            // search. Systemless HLE has no ROM resource fork to retry, so a
            // NIL result is the final miss and keeps RGetResource's documented
            // resNotFound result separate from plain GetResource's Basilisk
            // NIL/noErr miss behavior.
            (true, 0x00C) => {
                let sp = cpu.read_reg(Register::A7);
                match self.dispatch_resource(true, 0x1A0, cpu, bus) {
                    Some(Ok(())) => {}
                    Some(Err(err)) => return Some(Err(err)),
                    None => return None,
                }
                if bus.read_long(sp + 6) == 0 {
                    cpu.write_reg(Register::D0, Self::RES_NOT_FOUND as i32 as u32);
                    bus.write_word(0x0A60, Self::RES_NOT_FOUND as u16);
                }
                Ok(())
            }

            // GetResFileAttrs ($A9F6)
            // Returns the resource-map-level attribute INTEGER for an
            // open resource file.
            // FUNCTION GetResFileAttrs(refNum: INTEGER): INTEGER;
            // Inside Macintosh Volume I, I-126
            //
            // IM:I-126: "If there's no resource file with the given
            // reference number, GetResFileAttrs will do nothing and
            // the ResError function will return the result code
            // resFNotFound." — the result slot is left untouched on
            // the error path (caller's pre-call value preserved).
            //
            // Stack frame:
            //   pre:  SP -> [refNum(2)][result-slot(2)]
            //   post: SP -> [result(2)]; pop 2 leaves 2.
            //
            // GetResFileAttrs ($A9F6): Returns the documented resource-map
            // attribute bits (mapReadOnly/mapCompact/mapChanged) per IM:I-126;
            // resFNotFound on missing refnum, result slot untouched on error path.
            (true, 0x1F6) => {
                let sp = cpu.read_reg(Register::A7);
                let refnum = bus.read_word(sp);

                let attrs_opt = self
                    .resources
                    .as_ref()
                    .and_then(|resources| resources.files.get(&refnum))
                    .map(|file| file.map_attrs);

                match attrs_opt {
                    Some(attrs) => {
                        bus.write_word(0x0A60, 0); // ResErr = noErr
                        bus.write_word(sp + 2, attrs & 0x00E0);
                    }
                    None => {
                        bus.write_word(0x0A60, (-193i16) as u16); // resFNotFound
                    }
                }
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // SetResFileAttrs ($A9F7)
            // Sets the resource-map-level attribute INTEGER for an
            // open resource file.
            // PROCEDURE SetResFileAttrs(refNum: INTEGER; attrs: INTEGER);
            // Inside Macintosh Volume I, I-126
            //
            // IM:I-126: "If there's no resource file with the given
            // reference number, SetResFileAttrs will do nothing but
            // the ResError function will return the result code
            // noErr." — the only Resource Manager error path that
            // resolves to noErr instead of a -19x code, and the IM
            // wording is verbatim. Real-Mac BasiliskII confirms.
            //
            // Stack frame (Pascal — refNum pushed first, deepest):
            //   pre:  SP -> [attrs(2)][refNum(2)]
            //   post: SP -> (args popped, A7 = pre + 4)
            //
            // SetResFileAttrs ($A9F7): Stores only the documented
            // resource-map attribute bits (mapReadOnly/mapCompact/mapChanged)
            // per IM:I-126; missing refnum is a documented no-op that
            // returns noErr (not resFNotFound — IM verbatim).
            (true, 0x1F7) => {
                let sp = cpu.read_reg(Register::A7);
                let attrs = bus.read_word(sp);
                let refnum = bus.read_word(sp + 2);

                self.with_resource_manager_mut(|resource_manager| {
                    if let Some(file) = resource_manager
                        .resources
                        .as_mut()
                        .and_then(|resources| resources.files.get_mut(&refnum))
                    {
                        file.map_attrs = attrs & 0x00E0;
                    }
                });
                // Both branches set ResErr to noErr per IM:I-126.
                bus.write_word(0x0A60, 0);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // MaxSizeRsrc ($A821)
            // Returns the resource size from the resource map without
            // reading from disk.
            // FUNCTION MaxSizeRsrc(theResource: Handle): LONGINT;
            // Inside Macintosh Volume IV, IV-16
            //
            // IM:IV-16: "MaxSizeRsrc is similar to SizeResource except
            // that it does not cause the disk to be read; instead it
            // determines the size (in bytes) of the resource from the
            // offsets found in the resource map." In Systemless's HLE
            // every resource we know about is already resolved to a
            // guest-bus pointer (no offline-fork distinction), so the
            // disk-vs-map nuance collapses and the two traps share an
            // implementation. A future on-disk simulation would need to
            // reintroduce the disk-vs-map distinction.
            //
            // Regression coverage:
            //   maxsizersrc_returns_resource_size_for_loaded_resource_handle
            //   maxsizersrc_returns_minus_one_and_sets_resnotfound_for_non_resource_handle
            //   maxsizersrc_consumes_handle_argument_and_writes_function_result_slot
            // MaxSizeRsrc ($A821): Returns resource size from map without disk read per IM:IV-16. In our memory-resident HLE this collapses to the same data path as SizeResource ($A9A5).
            (true, 0x021) => self.handle_resource_size_query(bus, cpu),

            // ResourceDispatch ($A822) — partial-resource family
            // PROCEDURE ReadPartialResource (theResource: Handle;
            //     offset: LongInt; buffer: UNIV Ptr; count: LongInt);
            // PROCEDURE WritePartialResource(theResource: Handle;
            //     offset: LongInt; buffer: UNIV Ptr; count: LongInt);
            // PROCEDURE SetResourceSize    (theResource: Handle;
            //     newSize: LongInt);
            // Inside Macintosh: More Macintosh Toolbox 1993, 1-69 .. 1-71
            // Inside Macintosh Volume VI 1991, p. C-27 (selector table)
            //
            // IM:VI lists the D0 values as $0001/$0002/$0003. MMTB
            // prints $7001/$7002/$7003, which Universal Interfaces 3.4
            // confirms are complete MOVEQ opcodes preceding $A822, not
            // alternate D0 values. Runtime identity therefore uses 1--3.
            // The semantic handler retains its existing low-byte behavior
            // for compatibility, without attributing noncanonical values to
            // a generated operation.
            //
            // The Mac maintains a disk/memory dichotomy that makes
            // the partial-resource workflow non-trivial there: a
            // typical caller does SetResLoad(FALSE) so GetResource
            // returns an empty handle, then uses the partial routines
            // to stream data in/out of disk. Systemless's HLE has no
            // separate disk fork — every loaded resource is a real
            // guest-bus allocation — so all three traps act on the
            // in-memory copy directly. ReadPartialResource is
            // therefore byte-for-byte equivalent to a BlockMove from
            // (*handle + offset). WritePartialResource is a BlockMove
            // in the opposite direction; if the write would extend
            // past the current resource, MMTB 1-70 says the Resource
            // Manager grows the resource and returns the diagnostic
            // result code writingPastEnd (-189). SetResourceSize
            // resizes the in-memory allocation and returns noErr —
            // MMTB's documented `resourceInMemory` (-188) success
            // code is a real-Mac quirk that signals "disk size
            // updated, memory unchanged"; reporting it here would
            // break callers that treat any non-zero ResErr as a
            // failure when the operation actually succeeded.
            //
            // ResourceDispatch ($A822)
            // Reads, writes, or resizes part of a memory-resident resource.
            // D0: selector; stack: routine parameters; ResErr: result code.
            // More Macintosh Toolbox (1993), pp. 1-111 to 1-115.
            (true, 0x022) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = cpu.read_reg(Register::D0);
                let operation = resource_dispatch_operation_route(self.current_trap_word, selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                let routine = (selector & 0xFF) as u8;
                match routine {
                    // ReadPartialResource (selector 1) — 16 bytes args
                    // SP+12 theResource | SP+8 offset | SP+4 buffer | SP+0 count
                    0x01 => {
                        let count = bus.read_long(sp) as i32;
                        let buffer = bus.read_long(sp + 4);
                        let offset = bus.read_long(sp + 8) as i32;
                        let handle = bus.read_long(sp + 12);

                        let res_err =
                            self.read_partial_resource(bus, handle, offset, buffer, count);
                        bus.write_word(0x0A60, res_err as u16);
                        cpu.write_reg(Register::A7, sp + 16);
                    }
                    // WritePartialResource (selector 2) — 16 bytes args
                    // SP+12 theResource | SP+8 offset | SP+4 buffer | SP+0 count
                    0x02 => {
                        let count = bus.read_long(sp) as i32;
                        let buffer = bus.read_long(sp + 4);
                        let offset = bus.read_long(sp + 8) as i32;
                        let handle = bus.read_long(sp + 12);

                        let res_err =
                            self.write_partial_resource(bus, handle, offset, buffer, count);
                        bus.write_word(0x0A60, res_err as u16);
                        cpu.write_reg(Register::A7, sp + 16);
                    }
                    // SetResourceSize (selector 3) — 8 bytes args
                    // SP+4 theResource | SP+0 newSize
                    0x03 => {
                        let new_size = bus.read_long(sp) as i32;
                        let handle = bus.read_long(sp + 4);

                        let res_err = self.set_resource_size(bus, handle, new_size);
                        bus.write_word(0x0A60, res_err as u16);
                        cpu.write_reg(Register::A7, sp + 8);
                    }
                    _ => {
                        eprintln!(
                            "[TRAP] ResourceDispatch unknown selector ${:04X} (low byte ${:02X})",
                            cpu.read_reg(Register::D0) & 0xFFFF,
                            routine
                        );
                        // Per the unknown-selector convention used by
                        // DialogDispatch, we leave SP unchanged and set
                        // resNotFound so the caller surfaces the miss.
                        bus.write_word(0x0A60, Self::RES_NOT_FOUND as u16);
                    }
                }
                Ok(())
            }

            // ResError ($A9AF)
            // Returns the result code from the most recent Resource Manager call.
            // FUNCTION ResError: INTEGER;
            // Inside Macintosh Volume I, I-118
            // ResError ($A9AF): Returns last resource error from D0
            (true, 0x1AF) => {
                let sp = cpu.read_reg(Register::A7);
                let res_err = bus.read_word(0x0A60) as i16;
                let pc = cpu.read_reg(Register::PC);
                eprintln!("[TRAP] ResError -> {} (PC=${:08X})", res_err, pc);
                bus.write_word(sp, res_err as u16);
                Ok(())
            }

            // Get1NamedResource ($A820)
            // Returns a handle to the named resource of the given type in the current resource file.
            // FUNCTION Get1NamedResource(theType: ResType; name: Str255): Handle;
            // Inside Macintosh Volume VI, VI-13
            //
            // More Macintosh Toolbox 1993, 1-77--1-78: an absent type
            // returns NIL with noErr; a missing resource of a present type
            // returns resNotFound.
            (true, 0x020) => {
                let sp = cpu.read_reg(Register::A7);
                let name_ptr = bus.read_long(sp);
                let raw_res_type = bus.read_long(sp + 4).to_be_bytes();
                let res_type = super::TrapDispatcher::normalize_ostype(raw_res_type);
                let type_str = std::str::from_utf8(&res_type).unwrap_or("????");
                // Read Pascal string name
                let name_len = bus.read_byte(name_ptr) as usize;
                let mut name_bytes = vec![0u8; name_len];
                for (i, byte) in name_bytes.iter_mut().enumerate() {
                    *byte = bus.read_byte(name_ptr + 1 + i as u32);
                }
                let name = String::from_utf8_lossy(&name_bytes).to_string();
                eprintln!("[TRAP] Get1NamedResource('{}', \"{}\")", type_str, name);

                // Look up by (type, name) in the named resources index
                let handle = self
                    .find_named_resource_current_loaded(bus, res_type, &name)
                    .map(|(refnum, id, ptr)| {
                        self.get_or_create_resource_handle_in_file(bus, res_type, id, ptr, refnum)
                    });

                if let Some(h) = handle {
                    eprintln!("[TRAP] Get1NamedResource -> handle ${:08X}", h);
                    cpu.write_reg(Register::A0, h);
                    bus.write_long(sp + 8, h);
                    cpu.write_reg(Register::A7, sp + 8);
                    cpu.write_reg(Register::D0, 0);
                    bus.write_word(0x0A60, 0); // ResErr = noErr
                } else {
                    eprintln!("[TRAP] Get1NamedResource -> NULL (not found)");
                    cpu.write_reg(Register::A0, 0);
                    bus.write_long(sp + 8, 0);
                    cpu.write_reg(Register::A7, sp + 8);
                    cpu.write_reg(Register::D0, 0);
                    let has_type =
                        self.resource_file_contains_type(self.current_resource_refnum(), res_type);
                    bus.write_word(
                        0x0A60,
                        if has_type { RES_NOT_FOUND_ERR as u16 } else { 0 },
                    );
                }
                Ok(())
            }

            // CurResFile ($A994)
            // Returns the file reference number of the current resource file.
            // FUNCTION CurResFile: INTEGER;
            // Inside Macintosh Volume I, I-116
            // CurResFile ($A994): Returns the current resource-file refnum from the live Resource Manager search state
            (true, 0x194) => {
                let sp = cpu.read_reg(Register::A7);
                let internal_refnum = self.current_resource_refnum();
                // The initial application resource map uses internal key 0,
                // while CurApRefNum stores its guest-visible FCB refnum.
                // CurResFile reports the latter. Inside Macintosh Volume I,
                // p. I-116.
                let refnum = if internal_refnum == 0 {
                    bus.read_word(addr::CUR_APREF_NUM)
                } else {
                    internal_refnum
                };
                if super::dispatch::trace_resfile_enabled() {
                    eprintln!(
                        "[TRAP] CurResFile -> {} (PC=${:08X})",
                        refnum,
                        cpu.read_reg(Register::PC)
                    );
                }
                bus.write_word(sp, refnum);
                Ok(())
            }

            // HomeResFile ($A9A4)
            // Returns the file reference number of the resource file containing the resource.
            // FUNCTION HomeResFile(theResource: Handle): INTEGER;
            // Inside Macintosh Volume I, I-117
            //
            // Regression coverage:
            //   src/trap/resource.rs::tests::home_res_file_returns_loaded_resource_refnum
            //   src/trap/resource.rs::tests::home_res_file_returns_minus_one_for_detached_handle
            //   src/trap/resource.rs::tests::home_res_file_returns_minus_one_for_unknown_handle
            // HomeResFile ($A9A4): Returns the owning resource-file refnum for live resource handles; detached or unknown handles return `-1` and `resNotFound`
            (true, 0x1A4) => {
                let sp = cpu.read_reg(Register::A7);
                let handle = bus.read_long(sp);
                let new_sp = sp + 4;
                cpu.write_reg(Register::A7, new_sp);
                if self.loaded_handles.contains_key(&handle) {
                    let internal_refnum = self
                        .resource_handle_files
                        .get(&handle)
                        .copied()
                        .unwrap_or(0);
                    // The initial application resource map is stored under
                    // internal key 0, while the guest-visible resource fork
                    // has the FCB refnum recorded in CurApRefNum (normally 2).
                    // HomeResFile must return the actual file refnum, not the
                    // Resource Manager's internal key. Inside Macintosh
                    // Volume I, p. I-117.
                    let refnum = if internal_refnum == 0 {
                        bus.read_word(addr::CUR_APREF_NUM)
                    } else {
                        internal_refnum
                    };
                    bus.write_word(new_sp, refnum);
                    bus.write_word(0x0A60, 0);
                } else {
                    bus.write_word(new_sp, (-1i16) as u16);
                    bus.write_word(0x0A60, Self::RES_NOT_FOUND as u16);
                }
                Ok(())
            }

            // LoadSeg ($A9F0)
            // Loads a CODE segment and patches its jump table entries.
            // Inside Macintosh Volume II, II-60; Executor segment.cpp C_LoadSeg
            //
            // Two JT entry formats are supported:
            //
            // 1. Standard MPW format (Inside Macintosh Volume II, II-60):
            //    Unloaded: [offset(2), 3F3C(2), seg#(2), A9F0(2)]
            //    Loaded:   [seg#(2),   4EF9(2), address(4)       ]
            //    Calls enter at entry+2. LoadSeg seg# pushed on stack by MOVE.W.
            //
            // 2. Think C / CodeWarrior format (Processes 1994, 7-7 note):
            //    Unloaded: [A9F0(2), 0000(2), offset(2), seg#(2)]
            //    Loaded:   [seg#(2), 4EF9(2), address(4)        ]
            //    Calls enter at entry+0. No MOVE.W; seg# read from JT entry.
            //    The word popped from stack is the high word of the JSR return
            //    address, not the actual segment number.
            //
            // Regression coverage:
            //   resource::tests::loadseg_mpw_call_consumes_segment_number_word_argument
            //   resource::tests::loadseg_patches_calling_jump_table_entry_to_jmp_loaded_code
            // LoadSeg ($A9F0): Loads CODE resource, caches in segment_map
            (true, 0x1F0) => {
                let sp = cpu.read_reg(Register::A7);
                let pc = cpu.read_reg(Register::PC); // past A9F0
                let a9f0_addr = pc.wrapping_sub(2);

                let is_loadseg_trampoline = bus
                    .system_trap_gateway(0xA9F0)
                    .is_some_and(|addr| addr == a9f0_addr);

                let auto_pop_old_trap = (self.current_trap_word & 0x0400) != 0;
                let original_trap_return = self.current_trap_caller;
                let native_call = (is_loadseg_trampoline && auto_pop_old_trap)
                    .then(|| self.take_latest_native_trap_call(0xA9F0))
                    .flatten();

                // Detect format: in standard format, [A9F0-4] = 3F3C (MOVE.W).
                // In Think C format, A9F0 is at entry+0 and [A9F0+2] = 0x0000.
                //
                // Native LoadSeg handlers save the previous trap address via
                // GetTrapAddress and later jump to its auto-pop form (e.g.
                // $ADF0). The dispatcher records the original A-line return
                // PC and argument SP before entering any installed handler,
                // so recovery does not depend on that handler's stack frame.
                let old_trap_standard = auto_pop_old_trap
                    && original_trap_return.is_some_and(|return_pc| {
                        bus.read_word(return_pc.wrapping_sub(6)) == 0x3F3C
                            && (bus.read_word(return_pc.wrapping_sub(2)) & !0x0400u16) == 0xA9F0
                    });
                let old_trap_think = auto_pop_old_trap
                    && !old_trap_standard
                    && original_trap_return.is_some_and(|return_pc| {
                        let trap_addr = return_pc.wrapping_sub(2);
                        (bus.read_word(trap_addr) & !0x0400u16) == 0xA9F0
                            && bus.read_word(trap_addr + 2) == 0x0000
                    });
                let native_old_trap_standard = native_call.as_ref().is_some_and(|call| {
                    bus.read_word(call.return_pc.wrapping_sub(6)) == 0x3F3C
                        && (bus.read_word(call.return_pc.wrapping_sub(2)) & !0x0400u16) == 0xA9F0
                });
                let native_old_trap_think = native_call.as_ref().is_some_and(|call| {
                    let trap_addr = call.return_pc.wrapping_sub(2);
                    (bus.read_word(trap_addr) & !0x0400u16) == 0xA9F0
                        && bus.read_word(trap_addr + 2) == 0x0000
                });

                let word_before_seg = bus.read_word(a9f0_addr.wrapping_sub(4));
                let is_standard = old_trap_standard || word_before_seg == 0x3F3C;

                let direct_saved_gateway = is_loadseg_trampoline
                    && auto_pop_old_trap
                    && native_call.is_none()
                    && !old_trap_standard
                    && !old_trap_think;
                let saved_gateway_reentry = direct_saved_gateway
                    .then(|| bus.read_long(sp.wrapping_sub(4)).wrapping_sub(6));

                let (seg_num, entry_addr, fmt, refresh_from_resource) = if native_old_trap_standard
                {
                    let call = native_call.as_ref().unwrap();
                    let sn = bus.read_word(call.argument_sp) as i16;
                    cpu.write_reg(Register::A7, call.argument_sp.wrapping_add(2));
                    Self::restore_native_toolbox_nonvolatile(cpu, call);
                    (
                        sn,
                        Some(call.return_pc.wrapping_sub(8)),
                        "mpw-native-oldtrap",
                        true,
                    )
                } else if native_old_trap_think {
                    let call = native_call.as_ref().unwrap();
                    let entry = call.return_pc.wrapping_sub(2);
                    let sn = bus.read_word(entry + 6) as i16;
                    cpu.write_reg(Register::A7, call.argument_sp);
                    Self::restore_native_toolbox_nonvolatile(cpu, call);
                    (sn, Some(entry), "thinkc-native-oldtrap", true)
                } else if old_trap_standard {
                    let return_pc = original_trap_return.unwrap();
                    let sn = bus.read_word(sp) as i16;
                    cpu.write_reg(Register::A7, sp + 2);
                    (sn, Some(return_pc.wrapping_sub(8)), "mpw-oldtrap", true)
                } else if old_trap_think {
                    let return_pc = original_trap_return.unwrap();
                    let entry = return_pc.wrapping_sub(2);
                    let sn = bus.read_word(entry + 6) as i16;
                    (sn, Some(entry), "thinkc-oldtrap", true)
                } else if direct_saved_gateway {
                    // A caller can push a synthetic return PC and segment
                    // argument, then JMP through a saved auto-pop gateway.
                    // The six preceding bytes are the dispatch sequence to
                    // re-execute after loading. Inside Macintosh Volume II,
                    // II-60, describes the same six-byte LoadSeg re-entry.
                    let sn = bus.read_word(sp) as i16;
                    cpu.write_reg(Register::A7, sp + 2);
                    (sn, None, "saved-gateway", false)
                } else if is_standard {
                    // Standard: seg# was pushed by MOVE.W, pop it
                    let sn = bus.read_word(sp) as i16;
                    cpu.write_reg(Register::A7, sp + 2);
                    // Entry starts 6 bytes before A9F0
                    (sn, Some(a9f0_addr.wrapping_sub(6)), "mpw", false)
                } else {
                    // Think C: A9F0 at entry+0, seg# at entry+6, offset at entry+4
                    // Stack has JSR return address (don't pop segment number)
                    let entry = a9f0_addr; // A9F0 IS entry+0
                    let sn = bus.read_word(entry + 6) as i16;
                    (sn, Some(entry), "thinkc", false)
                };

                let trace_loadseg = trace_loadseg_enabled();
                if trace_loadseg {
                    if let Some(entry_addr) = entry_addr {
                        eprintln!(
                            "[TRAP] LoadSeg(seg={}, fmt={}) entry=${:08X}",
                            seg_num, fmt, entry_addr
                        );
                    } else {
                        eprintln!("[TRAP] LoadSeg(seg={}, fmt={}) entry=none", seg_num, fmt);
                    }
                }

                if refresh_from_resource {
                    self.preserve_auto_pop_pc_once = auto_pop_old_trap;
                    self.finish_loadseg(bus, cpu, seg_num, entry_addr, true)
                } else if let Some(entry_addr) = entry_addr {
                    if self.maybe_inject_native_getresource_for_loadseg(
                        bus,
                        cpu,
                        seg_num,
                        entry_addr,
                        trace_loadseg,
                    ) {
                        Ok(())
                    } else {
                        self.finish_loadseg(bus, cpu, seg_num, Some(entry_addr), false)
                    }
                } else {
                    let result = self.finish_loadseg(bus, cpu, seg_num, None, false);
                    if result.is_ok() {
                        if let Some(reentry) = saved_gateway_reentry {
                            self.preserve_auto_pop_pc_once = true;
                            cpu.write_reg(Register::PC, reentry);
                        }
                    }
                    result
                }
            }

            // GetResInfo ($A9A8)
            // Returns the resource ID, type, and name for a resource-backed handle.
            // PROCEDURE GetResInfo (theResource: Handle; VAR theID: Integer;
            //     VAR theType: ResType; VAR name: Str255);
            // More Macintosh Toolbox 1993, 1-81 to 1-82
            // GetResInfo ($A9A8): Pops 16 bytes; returns resource ID, type, and Pascal name for resource-backed handles
            (true, 0x1A8) => {
                let sp = cpu.read_reg(Register::A7);
                let name_ptr = bus.read_long(sp);
                let type_ptr = bus.read_long(sp + 4);
                let id_ptr = bus.read_long(sp + 8);
                let handle = bus.read_long(sp + 12);

                if let Some((res_type, res_id, name)) = self.resource_metadata_for_handle(handle) {
                    if id_ptr != 0 {
                        bus.write_word(id_ptr, res_id as u16);
                    }
                    if type_ptr != 0 {
                        bus.write_long(type_ptr, u32::from_be_bytes(res_type));
                    }
                    if name_ptr != 0 {
                        bus.write_pstring(name_ptr, name.as_deref().unwrap_or("").as_bytes());
                    }
                    bus.write_word(0x0A60, 0);
                } else {
                    bus.write_word(0x0A60, (-192i16) as u16);
                }

                cpu.write_reg(Register::A7, sp + 16);
                Ok(())
            }

            // GetResAttrs ($A9A6)
            // Returns the resource attributes for a live resource handle.
            // FUNCTION GetResAttrs(theResource: Handle): INTEGER;
            // Inside Macintosh Volume I, I-121
            //
            // Regression coverage:
            //   tests::get_res_attrs_returns_attribute_bits_for_live_resource_handle
            //   tests::get_res_attrs_returns_res_changed_for_unknown_handle
            // GetResAttrs ($A9A6): Returns resource-map attribute bits for live resource handles; non-resource handles report `resChanged` and set `resNotFound`
            (true, 0x1A6) => {
                let sp = cpu.read_reg(Register::A7);
                let handle = bus.read_long(sp);
                if let Some(attrs) = self.resource_attributes_for_handle(handle) {
                    bus.write_word(sp + 4, attrs);
                    bus.write_word(0x0A60, 0);
                } else {
                    bus.write_word(sp + 4, Self::RES_CHANGED_ATTR);
                    bus.write_word(0x0A60, Self::RES_NOT_FOUND as u16);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // InitPack ($A9E5)
            // PROCEDURE InitPack(packID: INTEGER);
            // Inside Macintosh Volume I, I-483
            // Prior implementation lumped this with TECut as a shared Ok(()) and
            // silently left the 2-byte packID on the stack — latent bug.
            // InitPack ($A9E5): Pops 2-byte packID and returns per IM:I I-483.
            // Per-package loading (Floating Point, Math, etc.) is not modelled;
            // Systemless keeps the call as a no-op but records the last pack ID so
            // future pack-specific heuristics have a cheap hook.
            // Note: CountTypes / Count1Types live at `$A99E` (Toolbox, `0x19E`),
            // not `$A9E5`.
            (true, 0x1E5) => {
                let sp = cpu.read_reg(Register::A7);
                self.last_init_pack_id = Some(bus.read_word(sp) as i16);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // TECut ($A9D6) is handled in dispatch_dialog because it needs
            // the TERec accessors. Leaving the arm out here lets the
            // dispatch chain fall through to dispatch_dialog.
            // NOTE: DisposePalette ($AA93, 0x293) is handled by the Palette
            // Manager in quickdraw.rs.
            // NOTE: PutScrap ($A9FE, 0x1FE) moved to toolbox.rs Scrap Manager.

            // ========== Script Manager phantom-trap arms ==========
            //
            // $AA87 GetScript / $AA88 SetScript / $AA89 GetEnvirons
            // are PHANTOM TRAP WORDS: per IM:VI Table C-1 master
            // trap dispatch table + IM:V V-291 master trap
            // dispatch table, the trap-word range $AA87..$AA89 is
            // NOT allocated to any documented Apple trap (the
            // documented range goes ... $AA52 _HighLevelFSDispatch
            // ... $AA90 _InitPalettes ... with a gap covering
            // $AA53..$AA8F that includes our three phantom arms
            // — verified via `grep -nE "AA8[7-9]\b"
            // systemless-inside-macintosh-md/` returning zero matches
            // in the master dispatch tables).
            //
            // The canonical Script Manager dispatch is via
            // $A8B5 _ScriptUtil (a selector-based dispatcher
            // already implemented at quickdraw.rs:5903 with
            // selectors 0..22 covering FontScript, IntlScript,
            // KeyScript, Font2Script, GetEnvirons, SetEnvirons,
            // GetScript, SetScript, CharByte, CharType,
            // Char2Pixel etc.) — apps using MPW Pascal call
            // `GetScript(script, selector)` which the compiler
            // expands to `_ScriptUtil` ($A8B5) with D0 set to
            // the GetScript selector (4 per IM:VI 14-23 + Text
            // 1993 6-11). There is no separate $AA87 trap-word
            // dispatch on real Mac.
            //
            // ## Why these phantom arms exist in Systemless
            //
            // Some 68k binaries (especially older System-6 or
            // System-7 .0 era apps that linked against
            // pre-Universal-Headers MPW glue) emit $AA87 / $AA88
            // / $AA89 directly as a courtesy to the kernel's
            // dispatch table (Apple kept many such word-aliases
            // allocated for binary compatibility). On real Mac
            // these would either land on the unimplemented-trap
            // handler ($A89F) or on a glue-layer hook. Systemless's
            // arms collapse them to noErr / 0 / no-op so apps
            // don't crash.
            //
            // ## Stack frames (best-effort, no IM cite)
            //
            // - $AA87 GetScript: 6 args (script INTEGER + selector
            //   INTEGER) + 2 result. Pop 6. Returns 0 INTEGER as
            //   "script not installed" sentinel per IM:VI 14-29
            //   GetScript-via-ScriptUtil "Returns 0 if the script
            //   isn't installed".
            // - $AA88 SetScript: 6 args (script INTEGER + selector
            //   INTEGER + value INTEGER), no result. Pop 6.
            // - $AA89 GetEnvirons: 4 args (selector INTEGER) + 2
            //   result. Pop 6 = 4 args + 2 result-slot? Actually
            //   the existing impl pops 2 args + 2 result writeback
            //   = 4-byte total stack window. Wait, current pop is
            //   sp+2 which is 2 bytes — `bus.write_word(sp+2, 0);
            //   cpu.write_reg(Register::A7, sp+2)`. That's pop=2
            //   on a frame that the trap-doc claims is "Pops 6
            //   bytes". Trap-doc lies; existing impl correctly
            //   pops 2 + writes 2 result. The 6 in the trap-doc
            //   is the FRAME WINDOW (2 args + 2 result + 2 slack
            //   pre-pollution?) not the pop count.
            //
            // ## Manager classification corrected
            //
            // Was "Resource Manager — Toolbox Traps" (an artifact
            // of where the arm landed in dispatch_resource).
            // Per the manager-classification audit pattern: the
            // canonical manager is whatever IM
            // chapter heading the trap is documented under, NOT
            // what dispatch arm it lands in. Per IM:VI 14-1
            // "Script Manager" chapter heading + Text 1993 6-1
            // "Script Manager" chapter, the canonical manager
            // for these phantom GetScript/SetScript/GetEnvirons
            // aliases is Script Manager (even though they're
            // phantom — the IM-documented routine names live in
            // the Script Manager chapter).

            // GetScript ($AA87)
            // PHANTOM trap-word — see Script Manager phantom-trap
            // family rationale block above. Arm matches the
            // ScriptUtil ($A8B5) selector-4 GetScript behaviour
            // exactly: returns 0 INTEGER as "script not installed"
            // sentinel per IM:VI 14-29 (the ScriptUtil-GetScript
            // selector documents this fallback).
            // GetScript ($AA87): Phantom trap word — verified absent from IM:VI Table C-1 + IM:V V-291 master dispatch tables. Stack: SP+0 selector(2), SP+2 script(2), SP+4 result(2). Pops 4 + writes 0 INTEGER to result slot (pre-pop SP+4) per IM:VI 14-29 ScriptUtil-GetScript "Returns 0 if the script isn't installed" fallback. Canonical dispatch is via $A8B5 _ScriptUtil (selector 4) — apps that emit $AA87 directly are using non-public Apple-allocated word-aliases.
            (true, 0x287) => {
                // GetScript: FUNCTION (script, selector): Int
                // SP+0 selector(2), SP+2 script(2), SP+4 result(2).
                let sp = cpu.read_reg(Register::A7);
                bus.write_word(sp + 4, 0);
                cpu.write_reg(Register::A7, sp + 4);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // SetScript ($AA88)
            // PHANTOM trap-word — see Script Manager phantom-trap
            // family rationale block above. Arm matches the
            // ScriptUtil ($A8B5) selector-6 SetScript behaviour
            // (PROCEDURE no-op since Systemless doesn't maintain
            // per-script Manager state).
            // SetScript ($AA88): Phantom trap word — verified absent from IM:VI Table C-1 + IM:V V-291 master dispatch tables. Stack: SP+0 value(2), SP+2 selector(2), SP+4 script(2). Pops 6 PROCEDURE no-result per IM:VI 14-29 ScriptUtil-SetScript sig — Systemless doesn't maintain per-script-system local-variable state so the set is a no-op. Canonical dispatch is via $A8B5 _ScriptUtil (selector 6).
            (true, 0x288) => {
                // SetScript: PROCEDURE (script, selector, value). Pops 6.
                let sp = cpu.read_reg(Register::A7);
                cpu.write_reg(Register::A7, sp + 6);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // GetEnvirons ($AA89)
            // PHANTOM trap-word — see Script Manager phantom-trap
            // family rationale block above. Arm matches the
            // ScriptUtil ($A8B5) selector-4 GetEnvirons behaviour:
            // returns 0 INTEGER as "verb not recognised" sentinel
            // per IM:VI 14-30 ScriptUtil-GetEnvirons fallback.
            // Apps that probe `GetEnvirons(smVersion)` to detect
            // Script Manager version see 0 — they should fall
            // back to Gestalt('scri') which Systemless handles in
            // its main dispatcher.
            // GetEnvirons ($AA89): Phantom trap word — verified absent from IM:VI Table C-1 + IM:V V-291 master dispatch tables. Stack: SP+0 selector(2), SP+2 result(2). Pops 2 + writes 0 INTEGER to result slot per IM:VI 14-30 ScriptUtil-GetEnvirons "Returns 0 if the script isn't installed or the verb isn't recognized". Apps that probe smVersion via GetEnvirons see 0 → they fall back to Gestalt('scri'). Canonical dispatch is via $A8B5 _ScriptUtil (selector 4 — same selector index but "GetEnvirons" not "GetScript").
            (true, 0x289) => {
                // GetEnvirons: FUNCTION (selector): Int
                // SP+0 selector(2), SP+2 result(2).
                let sp = cpu.read_reg(Register::A7);
                bus.write_word(sp + 2, 0);
                cpu.write_reg(Register::A7, sp + 2);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }
            // ChangedResource ($A9AA)
            // Marks a resource as changed so its data and map entry will be
            // written to disk when the resource file is updated.
            // PROCEDURE ChangedResource(theResource: Handle);
            // Inside Macintosh Volume IV, p. IV-16
            //
            // Regression coverage:
            //   tests::changedresource_sets_reschanged_attribute_for_unprotected_resource
            //   tests::changedresource_protected_resource_is_noop_with_resattrerr
            // ChangedResource ($A9AA): Sets resChanged attribute on resource; resProtected returns resAttrErr per IM:IV IV-16 / MTB 1-88
            (true, 0x1AA) => {
                let sp = cpu.read_reg(Register::A7);
                let handle = bus.read_long(sp);
                if let Some((refnum, res_type, res_id)) = self.resource_record_for_handle(handle) {
                    let attrs = self.resource_attributes_for_handle(handle).unwrap_or(0);
                    // resProtected (bit 3) blocks the operation with resAttrErr
                    if (attrs & 0x0008) != 0 {
                        bus.write_word(0x0A60, Self::RES_ATTR_ERR as u16);
                    } else {
                        // Set the resChanged attribute (bit 1) on the resource
                        self.with_resource_manager_mut(|resource_manager| {
                            if let Some(resources) = resource_manager.resources.as_mut() {
                                if let Some(file) = resources.files.get_mut(&refnum) {
                                    let entry = file.attrs.entry((res_type, res_id)).or_insert(0);
                                    *entry |= Self::RES_CHANGED_ATTR as u8;
                                    file.map_attrs |= Self::RES_MAP_CHANGED_ATTR;
                                }
                            }
                        });
                        bus.write_word(0x0A60, 0); // noErr
                    }
                } else {
                    bus.write_word(0x0A60, Self::RES_NOT_FOUND as u16);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SetResInfo ($A9A9)
            // Changes the resource ID and name of a resource in the resource
            // map in memory. The change is not written to disk until the file
            // is updated (after ChangedResource is called).
            // PROCEDURE SetResInfo(theResource: Handle; theID: INTEGER; name: Str255);
            // Inside Macintosh Volume IV, p. IV-16
            //
            // Regression coverage:
            //   setresinfo_protected_resource_is_noop_with_resattrerr
            // SetResInfo ($A9A9): Updates resource ID and name in memory map; resProtected returns resAttrErr per IM:IV IV-16 / MTB 1-82
            (true, 0x1A9) => {
                let sp = cpu.read_reg(Register::A7);
                // Pascal LTR push order: theResource first (deepest), theID,
                // then name (shallowest, at SP+0).
                let name_ptr = bus.read_long(sp);
                let new_id = bus.read_word(sp + 4) as i16;
                let handle = bus.read_long(sp + 6);

                if let Some((refnum, res_type, old_id)) = self.resource_record_for_handle(handle) {
                    let attrs = self.resource_attributes_for_handle(handle).unwrap_or(0);
                    // resProtected (bit 3) blocks the operation with resAttrErr
                    if (attrs & 0x0008) != 0 {
                        bus.write_word(0x0A60, Self::RES_ATTR_ERR as u16);
                    } else {
                        let home_refnum = self.resource_handle_files.get(&handle).copied();
                        let new_name = (name_ptr != 0)
                            .then(|| bus.read_pstring(name_ptr))
                            .and_then(|bytes| String::from_utf8(bytes).ok())
                            .filter(|name| !name.is_empty());
                        let moved_data = self.with_resource_manager_mut(|resource_manager| {
                            // Update the resource ID in loaded_handles for the live
                            // home record only. System-reference shadow entries live
                            // in the current file's map and must not re-home the
                            // source handle.
                            if home_refnum == Some(refnum) {
                                if let Some(entry) = resource_manager.loaded_handles.get_mut(&handle)
                                {
                                    entry.2 = new_id;
                                }
                                if old_id != new_id {
                                    resource_manager
                                        .resource_handles_by_key
                                        .remove(&(refnum, res_type, old_id));
                                    resource_manager
                                        .resource_handles_by_key
                                        .insert((refnum, res_type, new_id), handle);
                                }
                            }
                            let moved_data = if old_id != new_id {
                                let data = resource_manager
                                    .resource_backing_data
                                    .remove(&(refnum, res_type, old_id));
                                if let Some(order) =
                                    resource_manager.resource_file_order.get_mut(&refnum)
                                {
                                    if let Some(key) =
                                        order.iter_mut().find(|key| **key == (res_type, old_id))
                                    {
                                        *key = (res_type, new_id);
                                    }
                                }
                                data
                            } else {
                                None
                            };

                            if let Some(resources) = resource_manager.resources.as_mut() {
                                if let Some(file) = resources.files.get_mut(&refnum) {
                                // Move the loaded entry from old_id to new_id
                                if let Some(ptr) = file.loaded.remove(&(res_type, old_id)) {
                                    file.loaded.insert((res_type, new_id), ptr);
                                }
                                // Move the attrs entry
                                if let Some(a) = file.attrs.remove(&(res_type, old_id)) {
                                    file.attrs.insert((res_type, new_id), a);
                                }
                                let old_name = file.names_by_id.remove(&(res_type, old_id));
                                // Update name if name_ptr != 0 (assembly-language note)
                                if let Some(name_str) = new_name {
                                    // Remove old named entry for this resource
                                    file.named.retain(|(t, _), (id, _)| {
                                        !(*t == res_type && *id == old_id)
                                    });
                                    let ptr_val = file
                                        .loaded
                                        .get(&(res_type, new_id))
                                        .copied()
                                        .unwrap_or(0);
                                    file.named.insert(
                                        (res_type, name_str.clone()),
                                        (new_id, ptr_val),
                                    );
                                    file.names_by_id.insert((res_type, new_id), name_str);
                                } else {
                                    if let Some(name_str) = old_name {
                                        file.names_by_id.insert((res_type, new_id), name_str);
                                    }
                                    // name_ptr == 0: update existing named entries to new ID
                                    for ((t, _), (id, _)) in file.named.iter_mut() {
                                        if *t == res_type && *id == old_id {
                                            *id = new_id;
                                        }
                                    }
                                }
                            }
                            }
                            moved_data
                        });
                        if let Some(data) = moved_data {
                            self.remember_resource_backing_data(refnum, res_type, new_id, data);
                        }
                        bus.write_word(0x0A60, 0); // noErr
                    }
                } else {
                    bus.write_word(0x0A60, Self::RES_NOT_FOUND as u16);
                }
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // AddResource ($A9AB)
            // Adds a resource reference to the current resource file for data
            // already in memory. Sets the resChanged attribute so the data
            // will be written when the file is updated.
            // PROCEDURE AddResource(theData: Handle; theType: ResType;
            //   theID: INTEGER; name: Str255);
            // Inside Macintosh: More Macintosh Toolbox 1993, 1-90.
            //
            // Reference coverage:
            //   a9ab_addresource_reserror
            // AddResource ($A9AB): Adds data handle to current resource file with resChanged; validates NIL/existing handles per MMTB 1-90.
            (true, 0x1AB) => {
                let sp = cpu.read_reg(Register::A7);
                // Pascal pushes the last formal nearest SP:
                //   SP+0 name: Str255 pointer
                //   SP+4 theID: Integer
                //   SP+6 theType: ResType
                //   SP+10 theData: Handle
                let name_ptr = bus.read_long(sp);
                let res_id = bus.read_word(sp + 4) as i16;
                let res_type_raw = bus.read_long(sp + 6);
                let res_type = res_type_raw.to_be_bytes();
                let handle = bus.read_long(sp + 10);

                const ADD_RES_FAILED: i16 = -194;

                // Fail if handle is NIL or already a resource handle
                if handle == 0 || self.loaded_handles.contains_key(&handle) {
                    bus.write_word(0x0A60, ADD_RES_FAILED as u16);
                } else {
                    let ptr = bus.read_long(handle);
                    let refnum = self.current_resource_refnum();
                    let name = (name_ptr != 0)
                        .then(|| bus.read_pstring(name_ptr))
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                        .filter(|name| !name.is_empty());
                    self.with_resource_manager_mut(|resource_manager| {
                        // Register in loaded_handles and resource_handle_files
                        resource_manager
                            .loaded_handles
                            .insert(handle, (ptr, res_type, res_id));
                        resource_manager.resource_handle_files.insert(handle, refnum);
                        resource_manager
                            .resource_handles_by_key
                            .insert((refnum, res_type, res_id), handle);

                        let mut added_to_file = false;
                        if let Some(resources) = resource_manager.resources.as_mut() {
                            if let Some(file) = resources.files.get_mut(&refnum) {
                                // Add to loaded map
                                file.loaded.insert((res_type, res_id), ptr);
                                added_to_file = true;
                                // Set resChanged attribute
                                let entry = file.attrs.entry((res_type, res_id)).or_insert(0);
                                *entry |= Self::RES_CHANGED_ATTR as u8;
                                file.map_attrs |= Self::RES_MAP_CHANGED_ATTR;

                                if let Some(name_str) = name {
                                    file.named
                                        .insert((res_type, name_str.clone()), (res_id, ptr));
                                    file.names_by_id.insert((res_type, res_id), name_str);
                                }
                            }
                        }
                        if added_to_file {
                            resource_manager
                                .resource_file_order
                                .entry(refnum)
                                .or_default()
                                .push((res_type, res_id));
                        }
                    });
                    self.remember_resource_handle_index(handle, refnum, res_type, res_id);
                    self.update_handle_state_bits(handle, |state| {
                        Some(state.unwrap_or(0x40) | 0x20)
                    });
                    if ptr != 0 {
                        if let Some(size) = bus.get_alloc_size(ptr) {
                            self.remember_resource_backing_data(
                                refnum,
                                res_type,
                                res_id,
                                bus.read_bytes(ptr, size as usize),
                            );
                        }
                    }
                    bus.write_word(0x0A60, 0); // noErr
                }
                cpu.write_reg(Register::A7, sp + 14);
                Ok(())
            }

            // AddReference ($A9AC)
            // PROCEDURE AddReference(theResource: Handle; theID: INTEGER;
            //                         name: Str255);
            // Public MPW trap declaration: `_AddReference = $A9AC`
            // (Resource Manager trap table / Traps.h).
            // Inside Macintosh: Promotional Edition (1985), Resource
            // Manager Programmer's Guide, "Modifying System References".
            //
            // Given a handle to a system resource, AddReference adds a
            // system reference to the current resource file, gives it the
            // supplied ID and name, and sets resChanged so the map will be
            // written out on update. That current-file system reference is
            // observable through GetResInfo / GetResAttrs on the source
            // handle while HomeResFile remains the system file. The
            // documented failure cases are:
            // - the current resource file is the system file,
            // - the handle is not a handle to a system resource,
            // - the current file already contains a system reference to the
            //   same resource.
            // ResError returns addRefFailed (-195) for these branches.
            //
            // Stack layout (Pascal left-to-right push order):
            //   SP+0  name pointer (Str255)
            //   SP+4  theID (INTEGER)
            //   SP+6  theResource (Handle)
            // Pops 10 bytes. No function result.
            (true, 0x1AC) => {
                let sp = cpu.read_reg(Register::A7);
                let name_ptr = bus.read_long(sp);
                let new_id = bus.read_word(sp + 4) as i16;
                let handle = bus.read_long(sp + 6);
                let _ = self.add_resource_reference(bus, handle, new_id, name_ptr);
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // WriteResource ($A9B0)
            // Writes the resource data for the given resource to the resource
            // file on disk. Does nothing (returning noErr) when resProtected
            // is set or resChanged is not set. In Systemless's HLE, there is no
            // physical resource file to write to, so this is a contract-
            // conformant no-op that clears resChanged after "writing".
            // PROCEDURE WriteResource(theResource: Handle);
            // Inside Macintosh Volume I, I-124
            //
            // WriteResource ($A9B0): Respects resProtected/resChanged flags; clears resChanged after write per IM:I I-124
            (true, 0x1B0) => {
                let sp = cpu.read_reg(Register::A7);
                let handle = bus.read_long(sp);
                if self.live_resource_identity_for_handle(handle).is_some() {
                    let attrs = self.resource_attributes_for_handle(handle).unwrap_or(0);
                    // resProtected (bit 3): do nothing, return noErr
                    // resChanged not set (bit 1): do nothing, return noErr
                    if (attrs & 0x0008) != 0 || (attrs & Self::RES_CHANGED_ATTR) == 0 {
                        bus.write_word(0x0A60, 0);
                    } else {
                        if let Some((refnum, _, _)) = self.resource_record_for_handle(handle) {
                            let _ = self.flush_resource_file_refnum(bus, refnum);
                        }
                        let _ = self.write_resource_backing_if_changed(bus, handle);
                        bus.write_word(0x0A60, 0);
                    }
                } else {
                    bus.write_word(0x0A60, Self::RES_NOT_FOUND as u16);
                }
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // CreateResFile ($A9B1)
            // PROCEDURE CreateResFile(fileName: Str255);
            // More Macintosh Toolbox (1993), pp. 1-57 and 1-66;
            // Inside Macintosh Volume I (1985), p. I-115.
            //
            // Semantics:
            // - Creates a zero-length data fork plus empty resource fork when
            //   no matching file exists in the OpenRF search namespace.
            // - If a data-fork file exists but has no resource fork, creates
            //   the empty resource fork.
            // - If a file already has a non-empty resource fork map, does
            //   nothing and reports dupFNErr via ResErr.
            // - If the data fork exists and the resource fork is present but
            //   zero-length, creates the empty resource map in place.
            // - Pop 4 bytes (fileName pointer), no function result.
            //
            // Systemless HLE models this with `vfs` (data fork) and `vfs_rsrc`
            // (resource fork). Search-path edge cases (root/System folder)
            // are collapsed to the normalized VFS namespace.
            (true, 0x1B1) => {
                let sp = cpu.read_reg(Register::A7);
                let name_ptr = bus.read_long(sp);
                let name = if name_ptr != 0 {
                    decode_mac_roman(&bus.read_pstring(name_ptr))
                } else {
                    String::new()
                };

                let res_err: i16 = if name.is_empty() {
                    -37 // bdNamErr
                } else {
                    let vfs_key = self
                        .find_vfs_rsrc_file(&name)
                        .or_else(|| self.find_vfs_file(&name))
                        .unwrap_or_else(|| super::TrapDispatcher::normalize_vfs_path(&name));
                    let existing_rsrc_has_map = self
                        .vfs_rsrc
                        .get(&vfs_key)
                        .map(|fork| !fork.is_empty())
                        .unwrap_or(false);
                    if self.vfs_path_is_read_only(&vfs_key) {
                        -44 // wPrErr
                    } else if existing_rsrc_has_map {
                        // MTb 1993 p. 1-57: existing resource fork with a
                        // resource map -> no-op + dupFNErr.
                        -48 // dupFNErr
                    } else {
                        // If a data fork already exists, add or initialise its
                        // zero-length resource fork map. Otherwise create both
                        // forks.
                        self.vfs.ensure_empty(vfs_key.clone());
                        self.vfs_rsrc
                            .insert(vfs_key.clone(), Self::empty_resource_fork_bytes());
                        self.touch_vfs_entry(&vfs_key);
                        if let Some(ref dir) = self.output_dir {
                            let host_path = dir.join(&vfs_key);
                            if let Some(parent) = host_path.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                            let _ = std::fs::write(host_path, []);
                        }
                        0 // noErr
                    }
                };

                bus.write_word(0x0A60, res_err as u16);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // SystemEvent ($A9B2)
            // Lets the Desk Manager handle an event or returns FALSE for application handling.
            // FUNCTION SystemEvent (theEvent: EventRecord): BOOLEAN;
            // Inside Macintosh Volume I (1985), pp. I-90--I-91 and I-442.
            (true, 0x1B2) => {
                let sp = cpu.read_reg(Register::A7);
                bus.write_word(sp + 4, 0); // FALSE
                cpu.write_reg(Register::D0, 0);
                cpu.write_reg(Register::A7, sp + 4);
                Ok(())
            }

            // Gestalt family: _Gestalt ($A1AD), _NewGestalt ($A3AD),
            // _ReplaceGestalt ($A5AD). All three OS traps share the
            // low byte $AD and so reach the same dispatch arm — the
            // dispatcher only forwards bits 0..7 for OS traps. The central
            // raw route retains their operation identity before that mask.
            //
            // Trap word table per Inside Macintosh Volume VI, p. 6-308.
            // FUNCTION/Pascal signatures and register conventions per
            // Inside Macintosh: Operating System Utilities 1994,
            // 1-31..1-35.
            //
            // Gestalt ($A1AD)
            // Returns the response registered for an environment selector.
            // FUNCTION Gestalt (selector: OSType; VAR response: LongInt): OSErr;
            // Inside Macintosh: Operating System Utilities (1994), pp. 1-25--1-31.
            (false, 0xAD) => {
                let selector = cpu.read_reg(Register::D0);
                let sel = selector.to_be_bytes();
                match raw_trap_route(self.current_trap_word).os_routine_variant {
                    // _NewGestalt ($A3AD)
                    //
                    //   FUNCTION NewGestalt (selector: OSType;
                    //                        gestaltFunction:
                    //                            SelectorFunctionUUP): OSErr;
                    //
                    // Inside Macintosh: Operating System Utilities 1994,
                    // pp. 1-31..1-34 (description) and p. 1-50 (trap-word
                    // table).
                    //
                    // OS-bit FUNCTION (bit 11 clear) with register-only ABI
                    // per IM:OSUtils 1994 p. 1-32:
                    //
                    //     Registers on entry:
                    //       A0  Address of new selector function
                    //       D0  Selector code (4-char OSType)
                    //
                    //     Registers on exit:
                    //       D0  Result code
                    //
                    // Result codes:
                    //   noErr                  (0)
                    //   memFullErr           (-108)  Ran out of memory
                    //   gestaltDupSelectorErr (-5552) Selector already exists
                    //   gestaltLocationErr   (-5553) Function not in system heap
                    //
                    // MPW Universal Headers Gestalt.h:
                    //   #pragma parameter __D0 NewGestalt(__D0, __A0)
                    //   EXTERN_API(OSErr) NewGestalt(OSType selector,
                    //       SelectorFunctionUPP gestaltFunction)
                    //         ONEWORDINLINE(0xA3AD);
                    //
                    // Systemless HLE compromise: records the (selector → guest
                    // fn) tuple in `gestalt_registry`; rejects duplicates of
                    // any built-in or already-registered selector with
                    // `gestaltDupSelectorErr` (-5552). Does NOT validate
                    // system-heap residency, so stack-local function pointers
                    // that BasiliskII would reject with `gestaltLocationErr`
                    // (-5553) are accepted. Subsequent Gestalt queries of
                    // registry-only selectors still return
                    // `gestaltUndefSelectorErr` because guest function
                    // pointers are not invokable from a trap handler.
                    //
                    // BII-vs-Systemless divergence: the absolute OSErr
                    // differs (BII enforces system-heap residency
                    // with gestaltLocationErr; Systemless HLE accepts any
                    // address). Both obey the documented register-
                    // only OS-bit FUNCTION calling convention.
                    //
                    // Regression coverage:
                    //   newgestalt_register_only_calling_convention_preserves_stack
                    OsRoutineVariant::GestaltRegister => {
                        let already_known = is_builtin_gestalt_selector(&sel)
                            || self.gestalt_registry.contains_key(&selector);
                        if already_known {
                            cpu.write_reg(Register::D0, GESTALT_DUP_SELECTOR_ERR);
                        } else {
                            let fn_ptr = cpu.read_reg(Register::A0);
                            self.gestalt_registry.insert(selector, fn_ptr);
                            cpu.write_reg(Register::D0, 0);
                        }
                        return Some(Ok(()));
                    }
                    // _ReplaceGestalt ($A5AD)
                    //
                    //   FUNCTION ReplaceGestalt (selector: OSType;
                    //                            gestaltFunction:
                    //                                SelectorFunctionUUP;
                    //                            VAR oldGestaltFunction:
                    //                                SelectorFunctionUUP)
                    //                            : OSErr;
                    //
                    // Inside Macintosh: Operating System Utilities 1994,
                    // pp. 1-31..1-35.
                    //
                    // OS-bit FUNCTION (bit 11 clear) with register-only ABI
                    // per IM:OSUtils 1994 p. 1-35:
                    //
                    //     Registers on entry:
                    //       A0  Address of new selector function
                    //       D0  Selector code (4-char OSType)
                    //
                    //     Registers on exit:
                    //       A0  Address of old selector function
                    //           (undefined on error)
                    //       D0  Result code
                    //
                    // Result codes:
                    //   noErr                   (0)
                    //   gestaltUndefSelectorErr (-5551) Undefined selector
                    //   gestaltLocationErr      (-5553) Function not in
                    //                                   system heap
                    //
                    // Per IM:OSUtils 1994 p. 1-35: "If ReplaceGestalt
                    // returns an error of any type, then the value of
                    // oldGestaltFunction is undefined" — so on
                    // gestaltUndefSelectorErr we leave A0 unchanged.
                    //
                    // MPW Universal Headers Gestalt.h:
                    //   #pragma parameter __D0 ReplaceGestalt(__D0, __A0, __A1)
                    //   EXTERN_API(OSErr) ReplaceGestalt(OSType selector,
                    //       SelectorFunctionUPP gestaltFunction,
                    //       SelectorFunctionUPP *oldGestaltFunction)
                    //         FOURWORDINLINE(0x2F09, 0xA5AD, 0x225F, 0x2288);
                    //
                    // The four-word glue saves A1 around the trap and writes
                    // the post-trap A0 register through *oldGestaltFunction;
                    // the bare trap word itself is still register-only.
                    //
                    // Systemless HLE compromise: swaps a previously-registered
                    // guest selector fn and returns the old pointer in A0;
                    // for built-in selectors there is no original guest fn
                    // to return so A0=0; unknown selectors yield
                    // gestaltUndefSelectorErr (-5551) with A0 preserved.
                    //
                    // Regression coverage:
                    //   replacegestalt_register_only_calling_convention_preserves_stack
                    OsRoutineVariant::GestaltReplace => {
                        let new_fn = cpu.read_reg(Register::A0);
                        if let Some(old_fn) = self.gestalt_registry.insert(selector, new_fn) {
                            cpu.write_reg(Register::A0, old_fn);
                            cpu.write_reg(Register::D0, 0);
                        } else if is_builtin_gestalt_selector(&sel) {
                            cpu.write_reg(Register::A0, 0);
                            cpu.write_reg(Register::D0, 0);
                        } else {
                            // Selector is genuinely unknown. Per IM the swap
                            // does not happen — unwind the speculative insert
                            // before returning the error so subsequent
                            // ReplaceGestalt/NewGestalt see a clean registry.
                            self.gestalt_registry.remove(&selector);
                            cpu.write_reg(Register::D0, GESTALT_UNDEF_SELECTOR_ERR);
                        }
                        return Some(Ok(()));
                    }
                    // `_Gestalt` ($A1AD) and unresolved modifier forms fall
                    // through to the query handler below.
                    _ => {}
                }
                match &sel {
                    // gestaltVersion ('vers') -> Gestalt Manager version.
                    // Inside Macintosh: Operating System Utilities 1994,
                    // p. 1-25: current version is 1, returned as $0001 in
                    // the low-order word.
                    b"vers" => {
                        cpu.write_reg(Register::A0, 0x0001);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltCollectionMgrVersion ('cltn') returns a
                    // NumVersion-style 1.0 value. The Collection Manager
                    // chapter requires callers to gate use of `_CollectionMgr`
                    // with this selector.
                    b"cltn" => {
                        cpu.write_reg(Register::A0, 0x0100_0000);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltSystemVersion ('sysv') -> the canonical profile version.
                    b"sysv" => {
                        cpu.write_reg(
                            Register::A0,
                            REFERENCE_MACHINE_PROFILE.system_version_bcd as u32,
                        );
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltSysArchitecture ('sysa') -> native system
                    // architecture. Systemless is a 68k runtime, so report
                    // gestalt68k.
                    // Inside Macintosh: Operating System Utilities 1994,
                    // p. 1-24: gestalt68k = 1, gestaltPowerPC = 2.
                    b"sysa" => {
                        cpu.write_reg(
                            Register::A0,
                            REFERENCE_M68K_EXECUTION_CAPABILITIES
                                .system_architecture
                                .expect("68K Gestalt supports gestaltSysArchitecture"),
                        );
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltOSTable / gestaltToolboxTable return the bases
                    // of the two writable raw dispatch tables. Inside
                    // Macintosh Volume VI (1991), Gestalt Manager constants;
                    // OSUtils 1994, pp. 8-4--8-6.
                    b"ostt" => {
                        cpu.write_reg(Register::A0, super::dispatch::OS_TRAP_TABLE_BASE);
                        cpu.write_reg(Register::D0, 0);
                    }
                    b"tbtt" => {
                        cpu.write_reg(Register::A0, super::dispatch::TOOLBOX_TRAP_TABLE_BASE);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltAppleEventsAttr ('evnt') -> AppleEvents present
                    b"evnt" => {
                        cpu.write_reg(Register::A0, 0x0001);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltEditionMgrAttr ('edtn') -> Edition Manager attrs.
                    // Inside Macintosh: Operating System Utilities 1994,
                    // p. 1-16 / p. 1-24: bit 0 is
                    // gestaltEditionMgrPresent, bit 1 is
                    // gestaltEditionMgrTranslationAware. Pack11 provides
                    // the Edition Manager package bootstrap, so advertise
                    // the manager as present but not Translation Manager
                    // aware.
                    b"edtn" => {
                        cpu.write_reg(Register::A0, 1);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltPPCToolboxAttr ('ppc ') -> PPC Toolbox present.
                    // Inside Macintosh: Interapplication Communication 1993,
                    // p. 11-80 defines gestaltPPCToolboxAttr='ppc ' and
                    // gestaltPPCToolboxPresent as bit 0; Macintosh Toolbox
                    // Essentials 1992, p. 2-7 directs high-level-event apps
                    // to use this selector before relying on PPC services.
                    // Systemless implements the local/init PPC dispatch
                    // paths generically, so report present but no incoming,
                    // outgoing, or realtime networking capability bits.
                    b"ppc " => {
                        cpu.write_reg(Register::A0, 0x0001);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltNativeCPUtype ('cput') -> 68040
                    b"cput" => {
                        cpu.write_reg(
                            Register::A0,
                            REFERENCE_M68K_EXECUTION_CAPABILITIES.native_cpu_type,
                        );
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltProcessorType ('proc') -> 68040
                    b"proc" => {
                        cpu.write_reg(
                            Register::A0,
                            REFERENCE_M68K_EXECUTION_CAPABILITIES.processor_type,
                        );
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltMachineType ('mach') -> Quadra 900
                    // Inside Macintosh: Operating System Utilities 1994, 1-58
                    b"mach" => {
                        cpu.write_reg(
                            Register::A0,
                            REFERENCE_MACHINE_PROFILE.gestalt_machine_type as u32,
                        );
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltKeyboardType ('kbd ') -> Extended ADB Keyboard.
                    // Inside Macintosh: Operating System Utilities 1994,
                    // 1-8 / 1-18 defines this selector as the keyboard type
                    // code for the keyboard that produced the last keystroke;
                    // MPW Universal Headers Gestalt.h defines
                    // gestaltExtADBKbd = 4. Systemless exposes a desktop
                    // extended-keyboard virtual input profile, including
                    // keypad aliases used by Marathon-class games.
                    b"kbd " => {
                        cpu.write_reg(Register::A0, 4);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltQuickdrawVersion ('qd  ') -> System 7
                    // 32-bit Color QuickDraw v1.3 (0x0230).
                    // IM:Operating System Utilities 1994, pp. 1-22 and
                    // 1-24 define gestalt32BitQD13 = $230; IM:Imaging
                    // With QuickDraw 1994, p. 4-18 says System 7 Color
                    // QuickDraw reports that value. Blade requires at
                    // least 32-Bit QuickDraw v1.2 during startup.
                    b"qd  " => {
                        cpu.write_reg(Register::A0, 0x0230);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltQuickdrawFeatures ('qdrw') -> hasColor | hasDeepGWorlds
                    b"qdrw" => {
                        cpu.write_reg(Register::A0, 0x000F);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltPhysicalRAMSize ('ram ') -> emulated physical RAM
                    b"ram " => {
                        cpu.write_reg(Register::A0, REFERENCE_MACHINE_PROFILE.ram_size_bytes);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltLogicalRAMSize ('lram') -> logical memory.
                    // Inside Macintosh: Operating System Utilities 1994,
                    // p. 1-19: when virtual memory is not installed, this is
                    // the same value as gestaltPhysicalRAMSize.
                    b"lram" => {
                        cpu.write_reg(Register::A0, REFERENCE_MACHINE_PROFILE.ram_size_bytes);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltLogicalPageSize ('pgsz') -> logical page size.
                    // Inside Macintosh: Operating System Utilities 1994,
                    // p. 1-19: defined for MC68010/020/030/040 systems and
                    // undefined for MC68000-only machines. Systemless exposes
                    // a 68040 profile, so report the page granularity used by
                    // the profile's flat logical address space.
                    b"pgsz" => {
                        cpu.write_reg(Register::A0, GESTALT_68040_LOGICAL_PAGE_SIZE_BYTES);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltFPUType ('fpu ') -> 68040 FPU
                    b"fpu " => {
                        cpu.write_reg(Register::A0, REFERENCE_M68K_EXECUTION_CAPABILITIES.fpu_type);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltMMUType ('mmu ') -> 68040 MMU
                    b"mmu " => {
                        cpu.write_reg(Register::A0, REFERENCE_M68K_EXECUTION_CAPABILITIES.mmu_type);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltSoundAttr ('snd ') -> advertise a full late-68k
                    // color Mac sound profile:
                    //   Bit 0:  gestaltStereoCapability
                    //   Bit 1:  gestaltStereoMixing
                    //   Bit 3:  gestaltSoundIOMgrPresent
                    //   Bit 4:  gestaltBuiltInSoundInput
                    //   Bit 5:  gestaltHasSoundInputDevice
                    //   Bit 6:  gestaltPlayAndRecord
                    //   Bit 7:  gestalt16BitSoundIO
                    //   Bit 10: gestaltSndPlayDoubleBuffer
                    //   Bit 11: gestaltMultiChannels
                    //   Bit 12: gestalt16BitAudioSupport
                    // Sound 1994, 2-91; OS Utilities 1994, 1-23.
                    //
                    // Earlier 0x0C83 subset omitted bits 3..6 and bit 12.
                    // Some Sound-Manager-3-aware titles refuse to start when
                    // those capability bits are missing.
                    b"snd " => {
                        cpu.write_reg(Register::A0, 0x1CFB);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltScreenSaverAttr ('SAVR') -> After Dark absent.
                    // AfterDarkGestalt.h (Berkeley Systems, 1993) defines
                    // this selector for its optional extension. Operating
                    // System Utilities 1994, pp. 1-31 to 1-32 specifies that
                    // undefined selectors return gestaltUndefSelectorErr.
                    // Clear A0 so callers cannot mistake a stale response for
                    // the extension's enabled or asleep attribute bits.
                    b"SAVR" => {
                        cpu.write_reg(Register::A0, 0);
                        cpu.write_reg(Register::D0, GESTALT_UNDEF_SELECTOR_ERR);
                    }
                    // gestaltSpeechAttr ('ttsc') -> Speech Manager absent.
                    // Sound 1994, 1-11..1-12 defines bit 0 as
                    // gestaltSpeechMgrPresent and says callers test it
                    // before using speech services. Systemless does not
                    // implement Speech Manager traps, so report the selector
                    // as known with no capability bits rather than leaving a
                    // stale A0 after gestaltUndefSelectorErr.
                    b"ttsc" => {
                        cpu.write_reg(Register::A0, 0);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltTextEditVersion ('te  ') -> TextEdit 5.
                    // Text 1993, 2-22 lists gestaltTE5 as the System 7.0
                    // value; p. 2-97 says TE5-or-greater gates the inline
                    // input-era TextEdit features. Systemless implements the
                    // corresponding TextEdit feature-flag path through
                    // TEDispatch, so expose the System 7 version gate.
                    b"te  " => {
                        cpu.write_reg(Register::A0, 5);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltTEAttr ('teat') -> TextEdit attributes.
                    // Operating System Utilities 1994, 1-30 defines bit 0 as
                    // gestaltTEHasGetHiliteRgn. Systemless does not implement
                    // TEGetHiliteRgn, so keep the selector known but return no
                    // advertised attribute bits.
                    b"teat" => {
                        cpu.write_reg(Register::A0, 0);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltTimeMgrVersion ('tmgr') -> revised Timer Manager
                    b"tmgr" => {
                        cpu.write_reg(Register::A0, 2);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltThreadMgrAttr ('thds') -> Thread Manager
                    // present. Inside Macintosh: Operating System Utilities
                    // 1994, p. 1-25 defines bit 0 as
                    // gestaltThreadMgrPresent and bit 1 as
                    // gestaltSpecificMatchSupport. Systemless implements the
                    // public critical-section dispatch contract through
                    // _ThreadDispatch ($ABF2), so report bit 0. Leave bit 1
                    // clear because exact-match thread creation is not
                    // implemented.
                    b"thds" => {
                        cpu.write_reg(Register::A0, 1);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltDisplayMgrVers ('dplv') -> Display Manager
                    // 2.0.6, matching the BasiliskII System 7.5.3 reference.
                    // Operating System Utilities 1994 lists 'dplv' as the
                    // Display Manager version selector. Abuse probes it before
                    // walking screen devices through DisplayDispatch.
                    b"dplv" => {
                        cpu.write_reg(Register::A0, 0x0002_0006);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltDisplayMgrAttr ('dply') -> Display Manager attrs.
                    // Bit 0 is gestaltDisplayMgrPresent; BasiliskII's System
                    // 7.5.3 reference returns bits 0..2 set for this profile.
                    b"dply" => {
                        cpu.write_reg(Register::A0, 0x0000_0007);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltAliasMgrAttr ('alis') -> alias manager present
                    b"alis" => {
                        cpu.write_reg(Register::A0, 1);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltFSAttr ('fs  ') -> has FSSpec calls and extended dispatch
                    b"fs  " => {
                        cpu.write_reg(Register::A0, (1 << 0) | (1 << 1));
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltFindFolderAttr ('fold') -> FindFolder present
                    // Inside Macintosh Volume VI, 9-28
                    b"fold" => {
                        cpu.write_reg(Register::A0, 1);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltResourceMgrAttr ('rsrc')
                    // Returns information about Resource Manager
                    // capabilities; bit 0, gestaltPartialRsrcs, indicates
                    // that the partial-resource routines exist.
                    // Inside Macintosh: Operating System Utilities 1994,
                    // p. 1-23. More Macintosh Toolbox 1993, p. 1-13.
                    b"rsrc" => {
                        cpu.write_reg(Register::A0, 1);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltScriptCount ('scr#') -> number of active script systems.
                    // Inside Macintosh: Operating System Utilities 1994,
                    // p. 1-12 defines the selector and p. 1-30 defines the
                    // response as the number of currently active script
                    // systems. Systemless exposes the Roman script system.
                    b"scr#" => {
                        cpu.write_reg(Register::A0, 1);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltQuickTime ('qtim') -> QuickTime version.
                    // Inside Macintosh: QuickTime 1993, p. 2-33. The
                    // response field (A0) is the QuickTime version
                    // formatted like the numeric version part of a 'vers'
                    // resource; Inside Macintosh Volume VI, 9-23 defines
                    // the release byte as 0x80 for a final release.
                    b"qtim" => {
                        cpu.write_reg(Register::A0, QUICKTIME_NUM_VERSION_6_0_FINAL);
                        cpu.write_reg(Register::D0, 0); // noErr
                    }
                    // gestaltDragMgrAttr ('drag') -> Drag Manager attrs.
                    // Inside Macintosh: Operating System Utilities 1994,
                    // p. 1-17. Bit 0 is gestaltDragMgrPresent. Systemless
                    // does not host Drag Manager traps, so report "known but
                    // absent" instead of gestaltUndefSelectorErr; Marathon 1
                    // probes this immediately after QuickDraw features and
                    // otherwise can interpret stale A0 feature bits.
                    b"drag" => {
                        cpu.write_reg(Register::A0, 0);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltOSAttr ('os  ') -> Mac OS attributes.
                    // Inside Macintosh: Operating System Utilities 1994,
                    // page 1-22. Bit assignments:
                    //   0: gestaltSysZoneGrowable
                    //   1: gestaltLaunchCanReturn
                    //   2: gestaltLaunchFullFileSpec
                    //   3: gestaltLaunchControl
                    //   4: gestaltTempMemSupport
                    //   5: gestaltRealTempMemory
                    //   6: gestaltTempMemTracked
                    //   7: gestaltIPCSupport
                    //   8: gestaltSysDebuggerSupport
                    //
                    // Steel Fighters and similar titles probe 'os  '
                    // during init and ExitToShell via _Debugger if the
                    // selector returns gestaltUndefSelectorErr. Reporting
                    // a System 7-class capability set (bits 0-7) keeps
                    // them on the standard launch path. Bit 8
                    // (debuggerSupport) is left clear since Systemless does
                    // not host a guest-side debugger.
                    b"os  " => {
                        cpu.write_reg(Register::A0, 0xFF);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltPowerMgrAttr ('powr') -> no Power Manager.
                    // Inside Macintosh Volume VI, 31-9. Bit 0 = present.
                    // Returning 0 with noErr signals "selector recognized,
                    // not a portable Mac" — desktop-class titles probe
                    // this on launch and fall through to the desktop
                    // power path (no battery polling, no sleep hooks).
                    b"powr" => {
                        cpu.write_reg(Register::A0, 0);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltAppearanceAttr ('appr') -> Appearance Manager
                    // present. Mac Toolbox: Appearance Manager (Apple, 1997),
                    // Gestalt selectors:
                    //   Bit 0 = gestaltAppearanceExists
                    //   Bit 1 = gestaltAppearanceCompatMode
                    // Some mid-90s titles (e.g. Meteor Storm) treat absence
                    // of the Appearance Manager as a hard failure and emit a
                    // misleading "Couldn't get the sound manager version"
                    // alert before exiting. Reporting bit 0 set says
                    // "Appearance Mgr is here"; the title's NewFeaturesDialog
                    // glue path then proceeds normally even though our
                    // DialogDispatch routes back through the standard
                    // Dialog Manager (no theming).
                    b"appr" => {
                        cpu.write_reg(Register::A0, 0x0001);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltAddressingModeAttr ('addr') -> 32-bit clean.
                    // Inside Macintosh Volume VI, 28-9. Bit 0: 32-bit
                    // addressing currently active. Bit 1: 32-bit-clean
                    // system zone. Bit 2: machine is 32-bit capable.
                    // Apps like Bonkheads that hard-require 32-bit
                    // addressing read this and ExitToShell with a
                    // "needs 32-bit addressing" alert if bit 0 is clear.
                    b"addr" => {
                        cpu.write_reg(Register::A0, 0b110 | u32::from(self.mmu_mode != 0));
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltHardwareAttr ('hdwr') -> low-level hardware attrs.
                    // Inside Macintosh: Operating System Utilities 1994,
                    // p. 1-31: bits include gestaltHasVIA1(0),
                    // gestaltHasVIA2(1), gestaltHasASC(3), gestaltHasSCC(4),
                    // and gestaltHasSCSI(7). Report a desktop color 68k
                    // hardware set consistent with the Quadra-class profile.
                    b"hdwr" => {
                        cpu.write_reg(
                            Register::A0,
                            (1 << 0) | (1 << 1) | (1 << 3) | (1 << 4) | (1 << 7),
                        );
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltSoundDeviceAttr ('sdev') -> not present.
                    // Inside Macintosh Sound 1994, 1-15. Reports the
                    // attributes of a specific sound output device. With
                    // no device selected the spec-correct response is
                    // gestaltUndefSelectorErr, but several apps (e.g.
                    // Bonkheads) probe this without first selecting a
                    // device and crash on the error. Returning 0 (no
                    // attributes set, no error) lets them proceed.
                    b"sdev" => {
                        cpu.write_reg(Register::A0, 0);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltStandardFileAttr ('stdf') -> Standard File Mgr 5.8 present.
                    // Inside Macintosh: More Macintosh Toolbox 1993, 1-86.
                    // Bit 0: gestaltStandardFile58 (StandardFile Mgr ≥5.8 features
                    // — CustomGetFile / CustomPutFile / StandardFileReply).
                    // Steel Fighters probes this and ExitToShells if D0 returns
                    // gestaltUndefSelectorErr. Reporting bit 0 set keeps the
                    // game on the standard launch path.
                    b"stdf" => {
                        cpu.write_reg(Register::A0, 1);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltHelpMgrAttr ('help') -> Help Manager present.
                    // Inside Macintosh: More Macintosh Toolbox 1993, 11-22.
                    // Bit 0: gestaltHelpMgrPresent.
                    // Steel Fighters and similar mid-90s titles probe this in
                    // the same gestalt sweep that gates ExitToShell.
                    b"help" => {
                        cpu.write_reg(Register::A0, 1);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltVMAttr ('vm  ') -> Virtual Memory not present.
                    // Inside Macintosh: Memory 1992, 3-29. Bit 0:
                    // gestaltVMPresent (1 = VM in use). Systemless does not
                    // emulate VM (the entire heap is real RAM), so report
                    // 0 — but with noErr in D0 so probes don't ExitToShell.
                    b"vm  " => {
                        cpu.write_reg(Register::A0, 0);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltAUXVersion ('a/ux') -> not running under A/UX.
                    // A/UX-aware installers sometimes use MPW-style Gestalt
                    // glue that writes A0 into the response variable even
                    // when D0 reports gestaltUndefSelectorErr. Clear A0 so
                    // those probes cannot mistake a stale prior response for
                    // an A/UX version number.
                    b"a/ux" => {
                        cpu.write_reg(Register::A0, 0);
                        cpu.write_reg(Register::D0, GESTALT_UNDEF_SELECTOR_ERR);
                    }
                    _ => {
                        let s = std::str::from_utf8(&sel).unwrap_or("????");
                        eprintln!("[GESTALT] Unknown selector '{}' (${:08X})", s, selector);
                        cpu.write_reg(Register::D0, 0xFFFFEA51u32); // gestaltUndefSelectorErr
                    }
                }
                Ok(())
            }

            // ========== File Manager ==========

            // PBOpen / HOpen ($A000 / $A200)
            // Opens a file data fork with the access mode in ioPermssn.
            // FUNCTION PBOpen (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // Inside Macintosh: Files (1992), pp. 2-7 to 2-8, 2-185 to 2-186.
            (false, 0x00) => {
                let pb = cpu.read_reg(Register::A0);
                let name_ptr = bus.read_long(pb + 18);
                let filename = Self::read_pb_filename(bus, name_ptr);
                let permission = bus.read_byte(pb + 27);
                eprintln!("[TRAP] PBOpen(\"{}\", perm={})", filename, permission);

                // Clear ioRefNum upfront so the failure path produces a
                // deterministic *refNum = 0 (IM:II-92 leaves this undefined
                // on failure, but games that read *refNum after an error
                // expect 0).
                bus.write_word(pb + 24, 0);

                // HOpen supplies ioDirID and uses HFS names, where slash is
                // a legal filename character; only colon separates path
                // components. Files 1992, pp. 2-27 to 2-29, 2-185 to 2-186.
                let vfs_file = if self.current_trap_word & 0x0200 != 0 {
                    let vref = bus.read_word(pb + 22) as i16;
                    let dir_id = bus.read_long(pb + 48);
                    self.find_vfs_file_for_hfs_lookup(vref, dir_id, &filename)
                } else {
                    self.find_vfs_file(&filename)
                };
                if let Some(vfs_name) = vfs_file {
                    let read_only = self.vfs_path_is_read_only(&vfs_name);
                    let requests_write = matches!(permission, 2 | 3 | 4);
                    if requests_write && read_only {
                        // Extracted disk-image volumes are mounted read-only;
                        // fsCurPerm follows the volume permission and explicit
                        // write modes fail with wPrErr.
                        bus.write_word(pb + 16, (-44i16) as u16);
                        cpu.write_reg(Register::D0, (-44i32) as u32);
                        return Some(Ok(()));
                    }
                    let writable = matches!(permission, 0 | 2 | 3 | 4) && !read_only;
                    let refnum = match self.allocate_data_file_fcb(bus, &vfs_name, writable) {
                        Ok(refnum) => refnum,
                        Err(err) => {
                            bus.write_word(pb + 16, err as u16);
                            cpu.write_reg(Register::D0, err as i32 as u32);
                            return Some(Ok(()));
                        }
                    };
                    self.open_files.insert(refnum, vfs_name.clone());
                    // Files 1992, 2-8: fsCurPerm (0) grants read/write when
                    // write access is available; fsWrPerm (2), fsRdWrPerm
                    // (3), and fsRdWrShPerm (4) explicitly request it.
                    // PBGetFCBInfo exposes the granted mode through bit 8 of
                    // ioFCBFlags (Files 1992, 2-108).
                    if writable {
                        self.write_refnums.insert(refnum);
                    }
                    self.file_positions.insert(refnum, 0);
                    bus.write_word(pb + 24, refnum);
                    bus.write_word(pb + 16, 0); // noErr
                    cpu.write_reg(Register::D0, 0);
                    eprintln!("[TRAP] PBOpen -> refnum={} vfs=\"{}\"", refnum, vfs_name);
                } else if Self::is_available_synthetic_driver_name(&filename) {
                    let refnum = self.allocate_process_file_refnum();
                    self.synthetic_drivers.insert(refnum, filename.clone());
                    bus.write_word(pb + 24, refnum);
                    bus.write_word(pb + 16, 0);
                    cpu.write_reg(Register::D0, 0);
                    eprintln!(
                        "[TRAP] PBOpen -> synthetic driver refnum={} name=\"{}\"",
                        refnum, filename
                    );
                } else if Self::is_unavailable_appletalk_driver_name(&filename) {
                    // A missing Device Manager driver reports dInstErr, not
                    // the File Manager's fnfErr (Inside Macintosh Volume II,
                    // II-183).
                    eprintln!("[TRAP] PBOpen: driver is not installed");
                    bus.write_word(pb + 16, (-26i16) as u16); // dInstErr
                    cpu.write_reg(Register::D0, (-26i32) as u32);
                } else {
                    eprintln!("[TRAP] PBOpen: file not found in VFS");
                    bus.write_word(pb + 16, (-43i16) as u16); // fnfErr
                    cpu.write_reg(Register::D0, (-43i32) as u32);
                }
                Ok(())
            }

            // PBRead/PBReadAsync ($A002/$A402)
            // Reads bytes from an open file, including PBRead newline mode.
            // FUNCTION PBRead (paramBlock: ParmBlkPtr; async: Boolean): OSErr;
            // Files 1992, 2-84, 2-121 to 2-122, 2-238
            (false, 0x02) => {
                let pb = cpu.read_reg(Register::A0);
                let async_call = matches!(
                    raw_trap_route(self.current_trap_word).os_routine_variant,
                    OsRoutineVariant::ParameterBlockAsynchronous
                );
                let completion_addr = bus.read_long(pb + 12);
                let ref_num = bus.read_word(pb + 24);
                let buffer = bus.read_long(pb + 32);
                let request_count = bus.read_long(pb + 36) as usize;
                let pos_mode = bus.read_word(pb + 44);
                let pos_offset = bus.read_long(pb + 46) as i32;
                eprintln!(
                    "[TRAP] PBRead ref={} buf=${:08X} count={} posMode={} posOff={}",
                    ref_num, buffer, request_count, pos_mode, pos_offset
                );

                if let Some(filename) = self.open_files.get(&ref_num).cloned() {
                    if let Some(file_buf) = self.vfs.get(&filename) {
                        let file_len = file_buf.len();
                        let cur_pos =
                            usize::try_from(*self.file_positions.get(&ref_num).unwrap_or(&0))
                                .unwrap_or(usize::MAX);
                        match Self::resolve_file_mark_position(
                            pos_mode, pos_offset, cur_pos, file_len,
                        ) {
                            Err(_) => {
                                bus.write_word(pb + 16, (-40i16) as u16); // posErr
                                bus.write_long(pb + 40, 0);
                                bus.write_long(pb + 46, cur_pos as u32);
                                cpu.write_reg(Register::D0, (-40i32) as u32);
                            }
                            Ok(start) => {
                                let start = start.min(file_len);
                                let avail = file_len - start;
                                let max_read = request_count.min(avail);
                                let newline_mode = (pos_mode & 0x0080) != 0;
                                let newline_char = (pos_mode >> 8) as u8;
                                let newline_offset = if newline_mode {
                                    file_buf[start..start + max_read]
                                        .iter()
                                        .position(|&byte| byte == newline_char)
                                } else {
                                    None
                                };
                                let bytes_read =
                                    newline_offset.map(|offset| offset + 1).unwrap_or(max_read);
                                let stopped_at_newline = newline_offset.is_some();

                                bus.write_bytes(buffer, &file_buf[start..start + bytes_read]);

                                self.file_positions.insert(ref_num, start + bytes_read);
                                bus.write_long(pb + 40, bytes_read as u32);
                                bus.write_long(pb + 46, (start + bytes_read) as u32);
                                self.recent_file_read = Some(super::dispatch::RecentFileRead {
                                    ref_num,
                                    filename: filename.clone(),
                                    buffer,
                                    start,
                                    bytes_read,
                                });

                                if !stopped_at_newline && bytes_read < request_count {
                                    eprintln!(
                                        "[TRAP] PBRead -> eofErr (read {} of {} from pos {})",
                                        bytes_read, request_count, start
                                    );
                                    bus.write_word(pb + 16, (-39i16) as u16); // eofErr
                                    cpu.write_reg(Register::D0, (-39i32) as u32);
                                } else {
                                    bus.write_word(pb + 16, 0);
                                    cpu.write_reg(Register::D0, 0);
                                }
                            }
                        }
                    } else {
                        eprintln!("[TRAP] PBRead -> fnfErr (file not in VFS)");
                        self.recent_file_read = None;
                        bus.write_word(pb + 16, (-43i16) as u16); // fnfErr
                        bus.write_long(pb + 40, 0);
                        cpu.write_reg(Register::D0, (-43i32) as u32);
                    }
                } else if self.synthetic_drivers.contains_key(&ref_num) {
                    self.recent_file_read = None;
                    bus.write_word(pb + 16, 0);
                    bus.write_long(pb + 40, 0);
                    cpu.write_reg(Register::D0, 0);
                } else {
                    eprintln!("[TRAP] PBRead -> rfNumErr (refnum {} not open)", ref_num);
                    self.recent_file_read = None;
                    bus.write_word(pb + 16, (-51i16) as u16); // rfNumErr
                    bus.write_long(pb + 40, 0);
                    cpu.write_reg(Register::D0, (-51i32) as u32);
                }

                if async_call {
                    let result = cpu.read_reg(Register::D0) as i16;
                    self.pending_file_completions.push_back(
                        crate::process_context::PendingFileCompletion {
                            parameter_block: pb,
                            completion_addr,
                            result,
                        },
                    );
                    // A successfully queued asynchronous request returns noErr.
                    // ioResult remains positive until the request completes.
                    bus.write_word(pb + 16, 1);
                    cpu.write_reg(Register::D0, 0);
                } else {
                    // The File Manager clears ioCompletion for synchronous calls.
                    bus.write_long(pb + 12, 0);
                }
                Ok(())
            }

            // FSWrite ($A003)
            // FSWrite ($A003): Writes to output_dir or VFS
            (false, 0x03) => {
                let pb = cpu.read_reg(Register::A0);
                let ref_num = bus.read_word(pb + 24);
                let buffer = bus.read_long(pb + 32);
                let request_count = bus.read_long(pb + 36) as usize;
                let pos_mode = bus.read_word(pb + 44);
                let pos_offset = bus.read_long(pb + 46) as i32;
                eprintln!(
                    "[TRAP] FSWrite ref={} buf=${:08X} count={} posMode={} posOff={}",
                    ref_num as i16, buffer, request_count, pos_mode, pos_offset
                );

                let signed_ref_num = ref_num as i16;
                if signed_ref_num < 0 {
                    let result =
                        self.write_device_driver(bus, signed_ref_num, buffer, request_count);
                    let (err, actual_count) = match result {
                        Ok(actual_count) => (0i16, actual_count),
                        Err(err) => (err, 0),
                    };
                    bus.write_long(pb + 40, actual_count as u32);
                    bus.write_word(pb + 16, err as u16);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }

                let Some(filename) = self.open_files.get(&ref_num).cloned() else {
                    if self.synthetic_drivers.contains_key(&ref_num) {
                        bus.write_long(pb + 40, request_count as u32);
                        bus.write_word(pb + 16, 0);
                        cpu.write_reg(Register::D0, 0);
                        return Some(Ok(()));
                    }
                    eprintln!("[TRAP] FSWrite -> rfNumErr (refnum {} not open)", ref_num);
                    bus.write_long(pb + 40, 0);
                    bus.write_word(pb + 16, (-51i16) as u16); // rfNumErr
                    cpu.write_reg(Register::D0, (-51i32) as u32);
                    return Some(Ok(()));
                };

                if self.vfs_path_is_read_only(&filename) {
                    bus.write_long(pb + 40, 0);
                    bus.write_word(pb + 16, (-44i16) as u16); // wPrErr
                    cpu.write_reg(Register::D0, (-44i32) as u32);
                    return Some(Ok(()));
                }

                if !self.write_refnums.contains(&ref_num) {
                    bus.write_long(pb + 40, 0);
                    bus.write_word(pb + 16, (-61i16) as u16); // wrPermErr
                    cpu.write_reg(Register::D0, (-61i32) as u32);
                    return Some(Ok(()));
                }

                let file_len = self.vfs.get(&filename).map_or(0, Vec::len);
                let cur_pos = usize::try_from(*self.file_positions.get(&ref_num).unwrap_or(&0))
                    .unwrap_or(usize::MAX);
                let Ok(start) =
                    Self::resolve_file_mark_position(pos_mode, pos_offset, cur_pos, file_len)
                else {
                    bus.write_long(pb + 40, 0);
                    bus.write_long(pb + 46, cur_pos as u32);
                    bus.write_word(pb + 16, (-40i16) as u16); // posErr
                    cpu.write_reg(Register::D0, (-40i32) as u32);
                    return Some(Ok(()));
                };
                let (new_pos, host_sync_bytes) = self.vfs.with_entry_or_default_mut(
                    filename.clone(),
                    |file_buf| {
                        if start > file_buf.len() {
                            file_buf.resize(start, 0);
                        }
                        let end = start.saturating_add(request_count);
                        if end > file_buf.len() {
                            file_buf.resize(end, 0);
                        }
                        bus.read_bytes_into(buffer, &mut file_buf[start..start + request_count]);

                        let sync = if self.output_dir.is_some() {
                            Some(file_buf.clone())
                        } else {
                            None
                        };
                        (end, sync)
                    },
                );

                self.file_positions.insert(ref_num, new_pos);
                bus.write_long(pb + 40, request_count as u32);
                bus.write_long(pb + 46, new_pos as u32);

                if let (Some(dir), Some(bytes)) = (&self.output_dir, host_sync_bytes) {
                    let host_path = dir.join(&filename);
                    if let Some(parent) = host_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = std::fs::write(&host_path, bytes) {
                        eprintln!(
                            "[FSWrite] failed to sync {} to host ({}): {}",
                            filename,
                            host_path.display(),
                            e
                        );
                    }
                }
                self.touch_vfs_entry(&filename);

                // Resource forks live in self.vfs_rsrc; PBOpenRF mirrors the
                // bytes into self.vfs under "__rsrc__<name>" so FSRead/FSWrite
                // share the data-fork code path. Mirror writes back so a later
                // OpenRFPerm/PBOpenRF reads the latest data (Mars Rising's
                // installer copies its rsrc fork to a temp file then re-opens it).
                if let Some(real_name) = filename.strip_prefix("__rsrc__") {
                    if let Some(buf) = self.vfs.get(&filename).cloned() {
                        self.vfs_rsrc.insert(real_name.to_string(), buf);
                    }
                }

                bus.write_word(pb + 16, 0);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // FSClose ($A001)
            // FUNCTION FSClose(refNum: INTEGER): OSErr;
            // Inside Macintosh Volume II, II-92. Result codes:
            //   noErr (0) — file closed
            //   rfNumErr (-51) — reference number specifies a nonexistent
            //                     access path (sibling of FSRead / FSWrite
            //                     / PBGetEOF, which all validate refnum)
            // Resource Manager open calls return file reference numbers for
            // resource forks (More Macintosh Toolbox 1993, 1-58 to 1-66).
            // If that refnum is also present in the resource chain, close the
            // in-memory resource map and its loaded handles too.
            (false, 0x01) => {
                let pb = cpu.read_reg(Register::A0);
                let ref_num = bus.read_word(pb + 24);
                let closed_file = if self.open_files.remove(&ref_num).is_some() {
                    self.file_positions.remove(&ref_num);
                    self.write_refnums.remove(&ref_num);
                    Self::clear_file_fcb(bus, ref_num);
                    true
                } else {
                    false
                };
                let closed_resource_file = self.close_resource_file_refnum(bus, ref_num);
                let err: i16 = if closed_file || closed_resource_file {
                    0
                } else if self.synthetic_drivers.remove(&ref_num).is_some() {
                    0
                } else {
                    -51
                };
                bus.write_word(pb + 16, err as u16);
                cpu.write_reg(Register::D0, err as i32 as u32);
                Ok(())
            }

            // PBGetVol / PBHGetVol ($A014 / $A214)
            // Returns the default volume reference number (a WDRefNum for the app folder).
            // FUNCTION PBGetVol  (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // FUNCTION PBHGetVol (paramBlock: WDPBPtr;    async: BOOLEAN): OSErr;
            // PBHGetVol additionally returns ioWDDirID (directory ID) at offset 48.
            // Inside Macintosh Volume IV, IV-96 / Files 1992, 2-160
            // PBHGetVol subsection begins IM:Files 1992 line 8305; trap macro
            // `_HGetVol` per IM:Files line 8331 + table line 14488. The OS-trap
            // dispatcher masks `trap & 0x00FF`, so $A214 PBHGetVol and $A014
            // PBGetVol land on the same low byte and share this arm.
            //
            // trap-doc: $A014 | PBGetVol | Partial | File Manager & Gestalt — OS Traps | Fills ioVRefNum and ioNamePtr from the current volume (IM:Files 1992, 2-162)
            // PBHGetVol ($A214): HFS variant aliased onto $A014
            (false, 0x14) => {
                let pb = cpu.read_reg(Register::A0);
                let volume_ref_num = self.resolve_volume_ref_num(*self.app_wd_refnum);
                let volume_name = self
                    .vfs_volume_for_ref_num(volume_ref_num)
                    .map(|volume| volume.name.as_str())
                    .unwrap_or(super::TrapDispatcher::boot_volume_name());
                bus.write_word(pb + 22, *self.app_wd_refnum as u16);
                let name_ptr = bus.read_long(pb + 18);
                if name_ptr != 0 {
                    Self::write_pstring(bus, name_ptr, volume_name);
                }
                // HGetVol fields: ioWDProcID and ioWDVRefNum and ioWDDirID
                // Always populate these so both PBGetVol and HGetVol callers work.
                bus.write_long(pb + 28, 0); // ioWDProcID
                bus.write_word(pb + 32, volume_ref_num as u16); // ioWDVRefNum
                let dir_id = self
                    .working_directory_info(*self.app_wd_refnum)
                    .map(|wd| wd.dir_id)
                    .unwrap_or(*self.default_dir_id);
                bus.write_long(pb + 48, dir_id); // ioWDDirID
                bus.write_word(pb + 16, 0); // noErr
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // PBGetVInfo / PBHGetVInfo ($A007 / $A207)
            // FUNCTION PBGetVInfo (paramBlock: ParmBlkPtr; async: Boolean): OSErr;
            // FUNCTION PBHGetVInfo (paramBlock: HParmBlkPtr; async: Boolean): OSErr;
            // Inside Macintosh Volume II, II-104; HFS variant Files 1992,
            // 2-144 (subsection begins line 8018; trap macro `_HGetVolInfo`
            // per line 8077).
            // The OS-trap dispatcher masks `trap & 0x00FF`, so $A207
            // PBHGetVInfo and $A007 PBGetVInfo land on the same low
            // byte and share this arm.
            //
            // PBGetVInfo ($A007): Returns mounted volume info and free-space figures
            // PBHGetVInfo ($A207): HFS variant aliased onto $A007
            (false, 0x07) => {
                let pb = cpu.read_reg(Register::A0);
                let is_hfs_variant = matches!(
                    raw_trap_route(self.current_trap_word).os_routine_variant,
                    OsRoutineVariant::FileHfsSynchronous | OsRoutineVariant::FileHfsAsynchronous
                );
                let volume_index = bus.read_word(pb + 28) as i16;
                let requested_vref = bus.read_word(pb + 22) as i16;
                let requested_name = Self::read_pb_filename(bus, bus.read_long(pb + 18));
                let relative_pathname = requested_name.starts_with(':');
                let absolute_pathname = !relative_pathname && requested_name.contains(':');
                if super::dispatch::trace_resfile_enabled() {
                    eprintln!(
                        "[TRAP] PBHGetVInfo index={} vref={} name={:?}",
                        volume_index, requested_vref, requested_name
                    );
                }
                let boot_volume = || {
                    Some((
                        super::TrapDispatcher::boot_volume_name().to_string(),
                        super::TrapDispatcher::boot_volume_ref_num(),
                        0u16,
                    ))
                };
                let volume_by_name = |name: &str| {
                    // A full pathname names its volume before the first colon.
                    // Files 1992, 2-6 and 2-145.
                    let name = name.split(':').next().unwrap_or(name);
                    if name.eq_ignore_ascii_case(super::TrapDispatcher::boot_volume_name()) {
                        boot_volume()
                    } else {
                        self.vfs_volume_by_name(name)
                            .map(|volume| (volume.name.clone(), volume.ref_num, volume.attributes))
                    }
                };
                let volume_by_ref = |requested_vref: i16| {
                    let volume_ref_num = if requested_vref
                        == super::TrapDispatcher::boot_volume_ref_num()
                        || self
                            .vfs_volumes
                            .iter()
                            .any(|volume| volume.ref_num == requested_vref)
                    {
                        Some(requested_vref)
                    } else {
                        self.working_directories
                            .get(&requested_vref)
                            .map(|working_directory| working_directory.volume_ref_num)
                    }?;
                    if volume_ref_num == super::TrapDispatcher::boot_volume_ref_num() {
                        boot_volume()
                    } else {
                        self.vfs_volume_for_ref_num(volume_ref_num)
                            .map(|volume| (volume.name.clone(), volume.ref_num, volume.attributes))
                    }
                };
                let volume_info = if volume_index == 0 {
                    // Files 1992, 2-145: ioVolIndex == 0 selects a volume by
                    // ioNamePtr or ioVRefNum instead of enumerating the VCB
                    // queue. Do not silently turn an unknown vRefNum into the
                    // boot volume; callers use nsvErr to detect a bad volume.
                    if requested_vref != 0 {
                        volume_by_ref(requested_vref)
                    } else if relative_pathname {
                        volume_by_ref(*self.app_wd_refnum)
                    } else if !requested_name.is_empty() {
                        volume_by_name(&requested_name)
                    } else {
                        volume_by_ref(*self.app_wd_refnum)
                    }
                } else if volume_index > 0 {
                    let mut mounted = self
                        .vfs_volumes
                        .iter()
                        .map(|volume| (volume.name.clone(), volume.ref_num, volume.attributes))
                        .collect::<Vec<_>>();
                    mounted.sort_by_key(|(_, ref_num, _)| std::cmp::Reverse(*ref_num));
                    let mut volumes = vec![(
                        super::TrapDispatcher::boot_volume_name().to_string(),
                        super::TrapDispatcher::boot_volume_ref_num(),
                        0u16,
                    )];
                    volumes.extend(mounted);
                    volumes.get((volume_index - 1) as usize).cloned()
                } else {
                    // Files 1992, 2-145: a negative ioVolIndex selects the
                    // volume by ioNamePtr and ioVRefNum in the standard way.
                    // A full pathname's volume name takes precedence; otherwise
                    // a nonzero volume or working-directory reference is used.
                    if absolute_pathname {
                        volume_by_name(&requested_name)
                    } else if requested_vref != 0 {
                        volume_by_ref(requested_vref)
                    } else if relative_pathname {
                        volume_by_ref(*self.app_wd_refnum)
                    } else if !requested_name.is_empty() {
                        volume_by_name(&requested_name)
                    } else {
                        volume_by_ref(*self.app_wd_refnum)
                    }
                };
                let Some((volume_name, volume_ref_num, volume_attributes)) = volume_info else {
                    // Files 1992, 2-145: positive ioVolIndex values walk the
                    // mounted-volume queue, and enumeration ends with nsvErr.
                    const NSV_ERR: i16 = -35;
                    bus.write_word(pb + 16, NSV_ERR as u16);
                    cpu.write_reg(Register::D0, NSV_ERR as i32 as u32);
                    return Some(Ok(()));
                };

                let mounted_info = self.vfs_volume_for_ref_num(volume_ref_num);
                let volume_file_count = mounted_info.map_or(100, |volume| volume.file_count);
                let allocation_block_count = mounted_info
                    .map_or(BOOT_VOLUME_ALLOCATION_BLOCKS, |volume| {
                        volume.allocation_block_count
                    });
                let allocation_block_size = mounted_info
                    .map_or(BOOT_VOLUME_ALLOCATION_BLOCK_SIZE, |volume| {
                        volume.allocation_block_size
                    });
                let clump_size = mounted_info.map_or(BOOT_VOLUME_ALLOCATION_BLOCK_SIZE, |volume| {
                    volume.clump_size
                });
                let free_blocks =
                    mounted_info.map_or(BOOT_VOLUME_FREE_BLOCKS, |volume| volume.free_blocks);
                let created_date = mounted_info.map(|volume| volume.created_date).unwrap_or(0);
                let modified_date = mounted_info.map(|volume| volume.modified_date).unwrap_or(0);
                let bitmap_start = mounted_info.map(|volume| volume.bitmap_start).unwrap_or(0);
                let allocation_pointer = mounted_info
                    .map(|volume| volume.allocation_pointer)
                    .unwrap_or(0);
                let allocation_start = mounted_info
                    .map(|volume| volume.allocation_start)
                    .unwrap_or(0);
                let next_catalog_id = mounted_info
                    .map(|volume| volume.next_catalog_id)
                    .unwrap_or(0);
                let name_ptr = bus.read_long(pb + 18);
                if name_ptr != 0 {
                    Self::write_pstring(bus, name_ptr, &volume_name);
                }
                bus.write_word(pb + 22, volume_ref_num as u16);
                bus.write_long(pb + 30, created_date); // ioVCrDate
                bus.write_long(pb + 34, modified_date); // ioVLsMod
                bus.write_word(pb + 38, volume_attributes); // ioVAtrb
                bus.write_word(pb + 40, volume_file_count); // ioVNmFls (files on volume)
                bus.write_word(pb + 42, bitmap_start); // ioVBitMap
                bus.write_word(pb + 44, allocation_pointer); // ioAllocPtr

                // Files 1992, 2-46 and 2-144: callers multiply the unsigned
                // ioVFrBlk block count by ioVAlBlkSiz to determine free bytes.
                // Report a modest 64 MB boot volume so installers and demos
                // that require scratch space do not reject the system disk.
                bus.write_word(pb + 46, allocation_block_count); // ioVNmAlBlks
                bus.write_long(pb + 48, allocation_block_size); // ioVAlBlkSiz
                bus.write_long(pb + 52, clump_size); // ioVClpSiz
                bus.write_word(pb + 56, allocation_start); // ioAlBlSt
                bus.write_long(pb + 58, next_catalog_id); // ioVNxtCNID
                bus.write_word(pb + 62, free_blocks); // ioVFrBlk
                                                      // VolumeParam ends after ioVFrBlk at byte 64. Only the HFS
                                                      // HVolumeParam supplied to PBHGetVInfo has the fields that
                                                      // follow it. Files 1992, pp. 2-91--2-92 and 2-96--2-97.
                if is_hfs_variant {
                    bus.write_word(pb + 64, 0x4244); // ioVSigWord (HFS)

                    // The extracted volume is a File Manager abstraction,
                    // not a claim about the source image's physical device
                    // or driver.
                    bus.write_word(pb + 66, 0); // ioVDrvInfo
                    bus.write_word(pb + 68, 0); // ioVDRefNum
                    bus.write_word(pb + 70, 0); // ioVFSID (File Manager)
                }
                bus.write_word(pb + 16, 0); // noErr
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // PBGetEOF ($A011)
            // PBGetEOF ($A011): Returns file length from VFS
            (false, 0x11) => {
                let pb = cpu.read_reg(Register::A0);
                let ref_num = bus.read_word(pb + 24);
                if let Some(filename) = self.open_files.get(&ref_num) {
                    if let Some(file_buf) = self.vfs.get(filename) {
                        bus.write_long(pb + 28, file_buf.len() as u32); // ioMisc = logical EOF
                        bus.write_word(pb + 16, 0);
                        cpu.write_reg(Register::D0, 0);
                    } else {
                        bus.write_word(pb + 16, (-43i16) as u16);
                        cpu.write_reg(Register::D0, (-43i32) as u32);
                    }
                } else {
                    bus.write_word(pb + 16, (-51i16) as u16);
                    cpu.write_reg(Register::D0, (-51i32) as u32);
                }
                Ok(())
            }

            // PBSetFPos ($A044)
            // Sets the file mark (position) for an open file.
            // FUNCTION PBSetFPos(paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // Files 1992, 2-214
            // PBSetFPos ($A044): Sets file mark position; supports fsAtMark, fsFromStart, fsFromLEOF, fsFromMark
            (false, 0x44) => {
                let pb = cpu.read_reg(Register::A0);
                let ref_num = bus.read_word(pb + 24);
                let pos_mode = bus.read_word(pb + 44) & 0x03;
                let pos_offset = bus.read_long(pb + 46) as i32;

                if let Some(filename) = self.open_files.get(&ref_num).cloned() {
                    let file_len = self.vfs.get(&filename).map(|f| f.len()).unwrap_or(0);
                    let cur_pos = usize::try_from(*self.file_positions.get(&ref_num).unwrap_or(&0))
                        .unwrap_or(usize::MAX);
                    let Ok(requested_pos) =
                        Self::resolve_file_mark_position(pos_mode, pos_offset, cur_pos, file_len)
                    else {
                        bus.write_long(pb + 46, cur_pos as u32);
                        bus.write_word(pb + 16, (-40i16) as u16); // posErr
                        cpu.write_reg(Register::D0, (-40i32) as u32);
                        return Some(Ok(()));
                    };
                    let (new_pos, err) = if requested_pos > file_len {
                        (file_len, -39i16)
                    } else {
                        (requested_pos, 0i16)
                    };
                    self.file_positions.insert(ref_num, new_pos);
                    bus.write_long(pb + 46, new_pos as u32);
                    bus.write_word(pb + 16, err as u16);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                } else {
                    bus.write_word(pb + 16, (-51i16) as u16); // rfNumErr
                    cpu.write_reg(Register::D0, (-51i32) as u32);
                }
                Ok(())
            }

            // PBFlushFile ($A045)
            // Per IM:II II-114: "Writes the contents of the access
            // path buffer indicated by the file reference number
            // ioRefNum to the volume, and updates the file's entry
            // in the file directory."
            // FUNCTION PBFlushFile (paramBlock: ParmBlkPtr;
            //                      async: BOOLEAN): OSErr;
            // Inside Macintosh Volume II, II-114
            //
            // Register convention (OS trap): A0 = paramBlock ptr;
            // result in D0 + ioResult slot at pb+16.
            //
            // Result codes per IM:II II-114:
            //   noErr (0) — buffer flushed
            //   rfNumErr (-51) — reference number specifies a nonexistent
            //                    access path
            //
            // ## Status: Stub → Partial promotion
            //
            // Systemless's VFS has no per-file write-back buffer
            // (all FSWrite goes directly to the in-memory VFS
            // map; no deferred writes that need flushing). So the
            // "flush buffer to volume" semantic is trivially
            // satisfied. BUT the impl DOES validate the refnum
            // against self.open_files and returns the IM-correct
            // rfNumErr (-51) for a bad refnum — that's substantive
            // refnum-tracking behaviour, not a no-op stub. The
            // prior trap-doc Status was "Stub" with terse Notes
            // "Returns noErr" which lied about the refnum
            // validation. Promoted Stub -> Partial during the
            // stub-with-substantive-body audit; same status issue as
            // PlotIcon $A94B / SetItemCmd $A84F / DisposPalette $AA93 /
            // UpdtControl $A953 / Draw1Control $A96D.
            // PBFlushFile ($A045): Validates ioRefNum (pb+24) against self.open_files + returns rfNumErr (-51) for bad refnum OR noErr (0) for valid; writes result to ioResult (pb+16) AND D0 per IM:II II-114 result-code table. VFS has no per-file write-back buffer so the noErr path is a successful no-op (FSWrite goes directly to the in-memory VFS map).
            (false, 0x45) => {
                let pb = cpu.read_reg(Register::A0);
                let ref_num = bus.read_word(pb + 24);
                let err: i16 = if self.open_files.contains_key(&ref_num) {
                    0
                } else {
                    -51
                };
                bus.write_word(pb + 16, err as u16);
                cpu.write_reg(Register::D0, err as i32 as u32);
                Ok(())
            }

            // PBGetFPos ($A018)
            // FUNCTION PBGetFPos (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // Inside Macintosh Volume II, II-117. Result codes:
            //   noErr (0); rfNumErr (-51) for a bad reference number.
            // PBGetFPos ($A018): Returns current file position
            (false, 0x18) => {
                let pb = cpu.read_reg(Register::A0);
                let ref_num = bus.read_word(pb + 24);
                if let Some(pos) = self.file_positions.get(&ref_num).copied() {
                    bus.write_long(pb + 46, pos); // ioPosOffset
                    bus.write_word(pb + 44, 0); // ioPosMode
                    bus.write_word(pb + 16, 0);
                    cpu.write_reg(Register::D0, 0);
                } else {
                    bus.write_long(pb + 46, 0);
                    bus.write_word(pb + 44, 0);
                    bus.write_word(pb + 16, (-51i16) as u16);
                    cpu.write_reg(Register::D0, (-51i32) as u32);
                }
                Ok(())
            }

            // PBFlushVol ($A013)
            // Writes the contents of the volume buffer to the volume.
            // FUNCTION PBFlushVol (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // IM:Files 1992, 2-142. Result codes include noErr (0) and
            // nsvErr (-35) when no such volume exists.
            // trap-doc: $A013 | PBFlushVol | Partial | File Manager & Gestalt — OS Traps | No-op flush for known HLE volume specs; nsvErr for unrecognised vRefNum (IM:Files 1992, 2-142)
            (false, 0x13) => {
                let pb = cpu.read_reg(Register::A0);
                let name = Self::read_pb_filename(bus, bus.read_long(pb + 18));
                let vref = bus.read_word(pb + 22) as i16;
                let known_by_vref = vref == 0
                    || vref == Self::boot_volume_ref_num()
                    || self.vfs_volumes.iter().any(|volume| volume.ref_num == vref)
                    || self.working_directories.contains_key(&vref);
                let known_by_name = !name.is_empty()
                    && (name.eq_ignore_ascii_case(super::TrapDispatcher::boot_volume_name())
                        || self.vfs_volume_by_name(&name).is_some());
                let err: i16 = if known_by_vref || known_by_name {
                    0
                } else {
                    -35
                };
                bus.write_word(pb + 16, err as u16);
                cpu.write_reg(Register::D0, err as i32 as u32);
                Ok(())
            }

            // PBUnmountVol ($A00E) / PBUnmountVolImmed ($A20E)
            // Unmounts a volume.
            //   FUNCTION PBUnmountVol      (paramBlock: ParmBlkPtr): OSErr;
            //   FUNCTION PBUnmountVolImmed (paramBlock: ParmBlkPtr): OSErr;
            // Inside Macintosh: Files (1992), p. 2-148 (PBUnmountVol).
            //
            // The $A20E variant is declared in MPW Universal Headers
            // Files.h as `PBUnmountVolImmed(ParmBlkPtr)` and is not
            // separately documented in IM:Files 1992. Its MPW Traps.h
            // macro name is `_HUnmountVol` (also known historically as
            // `PBHUnmountVol`). Both trap
            // words take ParmBlkPtr — not HParmBlkPtr — and reach this
            // arm via the OS-trap dispatcher's `trap & 0x00FF` mask.
            // The "Immed" variant differs from PBUnmountVol in that it
            // does not call PBFlushVol before unmounting.
            //
            // Register convention (OS-bit FUNCTION; IM:Files 1992
            // p. 2-148 + IM:II 1985 p. II-114):
            //   A0 entry: ParmBlkPtr (paramBlock)
            //   D0 exit:  OSErr result code (also mirrored into
            //             pb.ioResult at pb+16 per the basic File
            //             Manager parameter block dispatcher
            //             convention, IM:II 1985 p. II-114)
            //
            // Parameter block fields used (IM:II 1985 p. II-178 IOParam
            // layout):
            //   →  ioCompletion (ProcPtr)  — NIL for synchronous form
            //   ←  ioResult     (OSErr)    — result code (mirrors D0)
            //   →  ioNamePtr    (StringPtr) — volume name (NIL means
            //                                  use ioVRefNum)
            //   →  ioVRefNum    (Integer)  — volume reference number
            //
            // Documented result codes (IM:Files 1992 p. 2-148):
            //   noErr     0   No error
            //   nsvErr  -35   No such volume
            //   ioErr   -36   I/O error
            //   bdNamErr -37  Bad filename
            //   fBsyErr -47   File(s) still open on the volume
            //   paramErr -50  Bad parameter
            //
            // MPW Universal Headers (Files.h):
            //   #pragma parameter __D0 PBUnmountVol(__A0)
            //   EXTERN_API(OSErr) PBUnmountVol(ParmBlkPtr paramBlock)
            //                                       ONEWORDINLINE(0xA00E);
            //   #pragma parameter __D0 PBUnmountVolImmed(__A0)
            //   EXTERN_API(OSErr) PBUnmountVolImmed(ParmBlkPtr paramBlock)
            //                                       ONEWORDINLINE(0xA20E);
            //
            // Systemless HLE compromise: the single-volume VFS has no
            // volume table to unmount from; the boot volume is
            // permanently mounted and any other vRefNum is silently
            // a no-op. The trap collapses to writing noErr (0) into
            // both D0 and pb.ioResult unconditionally. BasiliskII
            // dispatches the real ROM PB trap and is expected to
            // return nsvErr (-35) or paramErr (-50) for an ioVRefNum
            // that does not appear in the VCB queue. The absolute
            // OSErr value therefore differs between the two; the
            // behavioral invariants are (a) the dispatcher convention
            // writing the same OSErr into both D0 and ioResult, and
            // (b) the register-only OS-bit FUNCTION calling convention
            // preserving A7.
            //
            // Regression coverage:
            //   pbunmountvol_writes_same_oserr_to_d0_and_ioresult_preserving_stack
            (false, 0x0E) => {
                let pb = cpu.read_reg(Register::A0);
                bus.write_word(pb + 16, 0); // noErr
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // -----------------------------------------------------------------
            // Volume / I/O-queue family — collapse to noErr no-ops in Systemless's
            // single-volume, synchronous-I/O HLE.
            //
            // These traps mediate state that the HLE does not model:
            //   • PBKillIO    ($A006) — abort I/O on a driver refnum. There is
            //     no async I/O queue to walk.
            //   • PBMountVol  ($A00F) — mount a volume by drive number. A
            //     single VFS volume is permanently mounted; the trap rewrites
            //     `ioVRefNum` with the boot vRefNum and returns noErr.
            //   • PBAllocate  ($A010) / PBAllocContig ($A210) — preallocate
            //     blocks on a file fork. The unbounded heap-backed VFS has no
            //     fragmentation, so every request fully succeeds.
            //   • FInitQueue  ($A016) — clear the file I/O queue. There is no
            //     queue; PROCEDURE returns via D0 = noErr.
            //   • PBEject     ($A017) — eject removable media. None exists.
            //   • PBOffLine   ($A035) — take a volume offline. The boot
            //     volume cannot be brought offline.
            //   • PBSetFVers  ($A043) — set MFS-era file version number. HFS
            //     files are always version 0; the trap is a documented HFS
            //     no-op (IM:II II-117 warns nonzero versions break the
            //     Resource Manager / Segment Loader).
            //   • AddDrive    ($A04E) — append a DrvQEl to the drive queue.
            //     There is no drive queue.

            // PBKillIO ($A006)
            // Terminates the active I/O request and removes all pending I/O
            // requests for the driver whose reference number is in the
            // ioRefNum field of the parameter block.
            // FUNCTION PBKillIO (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // Inside Macintosh Volume II (1985), p. II-187 + Inside
            // Macintosh: Devices (1994), pp. 1-45 to 1-46.
            //
            // Register convention (OS-bit FUNCTION; IM:II 1985 p. II-187
            // trap macro `_KillIO`):
            //   A0 entry: ParmBlkPtr (paramBlock)
            //   D0 exit:  OSErr result code (also mirrored into pb.ioResult
            //             at pb+16 per Device Manager dispatcher convention,
            //             IM:II 1985 p. II-114; applies to the entire $A0xx
            //             PB family)
            //
            // Parameter block fields used (IM:II 1985 p. II-178 IOParam
            // layout):
            //   →  ioCompletion (ProcPtr)    — completion routine (NIL for
            //                                  the synchronous form)
            //   ←  ioResult     (OSErr)      — result code (mirrors D0)
            //   →  ioRefNum     (Integer)    — driver reference number
            //
            // Documented behavior (IM:II 1985 p. II-187): the abort applies
            // to the driver named by ioRefNum; the active call's completion
            // routine, and the completion routines of any pending calls,
            // execute with the result code abortErr (-27). The function
            // result itself is whatever the driver's KillIO routine returns.
            //
            // MPW Universal Headers (Devices.h):
            //   #pragma parameter __D0 PBKillIOSync(__A0)
            //   EXTERN_API(OSErr) PBKillIOSync(ParmBlkPtr paramBlock)
            //                                          ONEWORDINLINE(0xA006);
            //   #define PBKillIO(pb, async) ((async) ? PBKillIOAsync(pb)
            //                                        : PBKillIOSync(pb))
            //
            // Systemless HLE compromise: the synchronous-I/O HLE has no async
            // I/O queue to walk and no driver-side KillIO routines to
            // invoke, so the trap collapses to a no-op that writes noErr
            // (0) into both D0 and pb.ioResult. BasiliskII dispatches the
            // real ROM PB trap and is expected to return whatever the
            // unit-table lookup produces for a refNum that does not map
            // to an installed driver (typically badUnitErr -21 or
            // unitEmptyErr -22) or whatever the driver's KillIO routine
            // returns for a real one. The absolute return value
            // therefore differs between the two; the behavioral
            // invariants are (a) the dispatcher convention writing the
            // same OSErr into both D0 and ioResult, and (b) the
            // register-only OS-bit FUNCTION calling convention
            // preserving A7.
            //
            // Regression coverage:
            //   pbkillio_writes_same_oserr_to_d0_and_ioresult_preserving_stack
            (false, 0x06) => {
                let pb = cpu.read_reg(Register::A0);
                bus.write_word(pb + 16, 0); // noErr → pb.ioResult
                cpu.write_reg(Register::D0, 0); // noErr → D0
                Ok(())
            }

            // PBMountVol ($A00F)
            // Mounts the volume in the drive supplied via ioVRefNum (input
            // drive number) and rewrites ioVRefNum with the assigned volume
            // reference number (output).
            // FUNCTION PBMountVol (paramBlock: ParmBlkPtr): OSErr;
            // Inside Macintosh: Files (1992), p. 2-139.
            //
            // Register convention (OS-bit FUNCTION; IM:Files 1992 p. 2-139
            // trap macro `_MountVol`):
            //   A0 entry: ParmBlkPtr (paramBlock)
            //   D0 exit:  OSErr result code (also mirrored into pb.ioResult
            //             at pb+16 per File Manager dispatcher convention,
            //             IM:II 1985 p. II-114; applies to the entire $A0xx
            //             PB family)
            //
            // Parameter block fields used (IM:II 1985 p. II-178 IOParam
            // layout):
            //   →  ioCompletion (ProcPtr)    — NIL (PBMountVol is always
            //                                  synchronous per IM:Files
            //                                  1992 p. 2-139)
            //   ←  ioResult     (OSErr)      — result code (mirrors D0)
            //   ↔  ioVRefNum    (Integer)    — input: drive number;
            //                                  output: assigned vRefNum
            //
            // Documented result codes (IM:Files 1992 p. 2-140):
            //   noErr     (0)   — success
            //   ioErr     (-36) — I/O error
            //   tmfoErr   (-42) — too many files open
            //   paramErr  (-50) — bad drive number
            //
            // MPW Universal Headers (Files.h):
            //   #pragma parameter __D0 PBMountVol(__A0)
            //   EXTERN_API(OSErr) PBMountVol(ParmBlkPtr paramBlock)
            //                                          ONEWORDINLINE(0xA00F);
            //
            // Systemless HLE behavior: the single-volume VFS collapses any
            // mount request to the permanently-mounted boot volume. The
            // trap rewrites ioVRefNum with BOOT_VOLUME_REF_NUM (-1) and
            // writes noErr (0) into both D0 and pb.ioResult. BasiliskII
            // dispatches the real ROM PB trap and is expected to return
            // paramErr (-50) for a bogus drive number that does not map
            // to any installed drive queue entry. The absolute OSErr
            // value (and the rewritten ioVRefNum) therefore differ
            // between the two; the behavioral invariants are (a) the
            // dispatcher convention writing the same OSErr into both D0
            // and ioResult, and (b) the register-only OS-bit FUNCTION
            // calling convention preserving A7.
            //
            // Regression coverage:
            //   pbmountvol_writes_same_oserr_to_d0_and_ioresult_preserving_stack
            (false, 0x0F) => {
                let pb = cpu.read_reg(Register::A0);
                bus.write_word(pb + 22, super::TrapDispatcher::boot_volume_ref_num_u16());
                bus.write_word(pb + 16, 0); // noErr
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // PBAllocate ($A010) / PBAllocContig ($A210)
            // Preallocates ioReqCount bytes on the open file referenced by
            // ioRefNum. ioActCount on output is the bytes actually allocated
            // (rounded up to allocation-block size on real Mac; Systemless
            // matches the request exactly).
            // FUNCTION PBAllocate    (paramBlock: ParmBlkPtr; async: Boolean): OSErr;
            // FUNCTION PBAllocContig (paramBlock: ParmBlkPtr; async: Boolean): OSErr;
            // Inside Macintosh: Files (1992), pp. 2-130 to 2-131. The HFS
            // variant ($A210) shares this arm via the OS-trap low-byte mask;
            // both behave identically in Systemless's flat unbounded VFS where
            // contiguous-vs-best-fit is moot.
            //
            // Per IM:Files 1992 p. 2-130 the trap word $A010 is an OS-bit
            // FUNCTION (bit 11 of the trap word is clear) with a strict
            // register-only calling convention:
            //
            //   Entry: A0 = ParmBlkPtr (pointer to the I/O parameter
            //                           block; see IM:Files 1992 p. 2-130
            //                           for the field layout — ioCompletion
            //                           at +12, ioResult at +16, ioRefNum
            //                           at +24, ioReqCount at +36, ioActCount
            //                           at +40)
            //   Exit:  D0 = OSErr     (mirrored into pb.ioResult at pb+16
            //                          per the File Manager dispatcher
            //                          convention, IM:II 1985 p. II-114)
            //
            // Documented result codes (IM:Files 1992, pp. 2-130 to 2-131):
            //   noErr     0    No error
            //   ioErr    -36   I/O error
            //   dskFulErr-34   Disk full
            //   fnfErr   -43   File not found
            //   wPrErr   -44   Hardware volume lock
            //   fLckdErr -45   File is locked
            //   vLckdErr -46   Software volume lock
            //   rfNumErr -51   Bad reference number
            //
            // MPW Universal Headers `Files.h` declares the trap as:
            //   #pragma parameter __D0 PBAllocateSync(__A0)
            //   EXTERN_API(OSErr) PBAllocateSync(ParmBlkPtr paramBlock)
            //       ONEWORDINLINE(0xA010);
            //   #define PBAllocate(pb, async) \
            //       ((async) ? PBAllocateAsync(pb) : PBAllocateSync(pb))
            //
            // Systemless HLE compromise: the unbounded-VFS HLE skips refnum
            // validation and treats every PBAllocate as a success that
            // writes ioActCount = ioReqCount (the requested byte count)
            // and noErr (0) to both D0 and pb.ioResult. There is no real
            // file table or allocation-block fragmentation to validate
            // against, so any caller request succeeds. BasiliskII System
            // 7.5.3 ROM dispatches the real PB trap and is expected to
            // return rfNumErr (-51) for ioRefNum 9999 since no such open
            // file exists in the BasiliskII session.
            //
            // Behavioral invariant: regardless of the absolute OSErr value,
            // the File Manager dispatcher convention (IM:II 1985 p. II-114)
            // writes the SAME value to BOTH D0 and pb.ioResult at pb+16;
            // A7 is preserved across the call since the OS-bit FUNCTION ABI
            // takes no Pascal stack frame. The contiguous-allocation variant
            // ($A210) differs from PBAllocate only in that the real Apple ROM
            // returns dskFulErr (-34) on contiguous-fit failure rather than
            // performing a partial allocation, but both traps obey the same
            // dispatcher convention.
            //
            // Regression coverage:
            //   pballocate_nominal_call_returns_noerr_and_sets_ioactcount
            //   pballocate_writes_ioresult_noerr_when_paramblock_present
            //   pballocate_writes_same_oserr_to_d0_and_ioresult_preserving_stack
            //   pballoccontig_nominal_call_returns_noerr_and_sets_ioactcount
            //   pballoccontig_overwrites_ioresult_field_with_function_result
            //   pballoccontig_writes_same_oserr_to_d0_and_ioresult_preserving_stack
            (false, 0x10) => {
                let pb = cpu.read_reg(Register::A0);
                let req_count = bus.read_long(pb + 36); // ioReqCount
                bus.write_long(pb + 40, req_count); // ioActCount
                bus.write_word(pb + 16, 0); // noErr
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // FInitQueue ($A016)
            // Clears all queued File Manager calls except the current one.
            // PROCEDURE FInitQueue;
            // Inside Macintosh Volume II (1985), p. II-103; Inside
            // Macintosh Volume IV (1986), p. IV-128.
            //
            // Per IM:II 1985 p. II-103 FInitQueue is a parameterless
            // PROCEDURE that clears all queued File Manager calls
            // except the one currently in progress. The trap word
            // $A016 is OS-bit (bit 11 of the trap word is clear); the
            // calling convention is register-only with no inputs and
            // no Pascal FUNCTION result slot.
            //
            // MPW Universal Headers `Files.h` declares the trap as:
            //   EXTERN_API(void) FInitQueue(void) ONEWORDINLINE(0xA016);
            //
            // Systemless HLE compromise: the File Manager I/O queue is
            // permanently empty in the single-volume, synchronous-I/O
            // HLE (all File Manager calls complete inline before
            // returning to the caller), so FInitQueue has nothing to
            // clear. The HLE writes D0=0 (noErr, by Memory Manager
            // dispatcher convention for OS-bit PROCEDUREs) and
            // returns without consuming any stack bytes.
            //
            // Behavioral invariant: A7 is preserved across the call (no
            // Pascal stack frame is consumed) and the queue is left in an
            // empty state on exit.
            //
            // Regression coverage:
            //   finitqueue_has_no_parameters_and_preserves_stack_pointer
            (false, 0x16) => {
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // PBEject ($A017)
            // Flushes the specified volume, places it offline, and ejects it.
            // FUNCTION PBEject (paramBlock: ParmBlkPtr): OSErr;
            // Inside Macintosh: Files (1992), p. 2-141.
            //
            // Register convention (OS-bit FUNCTION; IM:Files 1992 p. 2-141
            // trap macro `_Eject`):
            //   A0 entry: ParmBlkPtr (paramBlock)
            //   D0 exit:  OSErr result code (also mirrored into pb.ioResult
            //             at pb+16 per File Manager basic parameter block
            //             dispatcher convention, IM:II 1985 p. II-114)
            //
            // Parameter block fields used (IM:Files 1992 p. 2-141):
            //   →  ioCompletion (ProcPtr) — pointer to a completion routine
            //   ←  ioResult     (OSErr)   — result code (mirrors D0)
            //   →  ioNamePtr    (StringPtr) — pointer to a pathname
            //   →  ioVRefNum    (Integer) — volume specification
            //
            // Always executes synchronously.
            //
            // Documented result codes (IM:Files 1992 p. 2-141):
            //   noErr     0    No error
            //   nsvErr   -35   No such volume
            //   ioErr    -36   I/O error
            //   bdNamErr -37   Bad volume name
            //   paramErr -50   No default volume
            //   nsDrvErr -56   No such drive
            //   extFSErr -58   External file system
            //
            // MPW Universal Headers Files.h:
            //   #pragma parameter __D0 PBEject(__A0)
            //   EXTERN_API(OSErr) PBEject(ParmBlkPtr paramBlock)
            //     ONEWORDINLINE(0xA017);
            //
            // Systemless HLE compromise: the single-volume HLE has no
            // removable media to eject, so this stub returns noErr (0)
            // unconditionally and writes 0 to pb.ioResult.
            //
            // Behavioral invariant:
            //   • D0 == pb.ioResult (pb+16) after the call (dispatcher
            //     convention IM:II 1985 p. II-114) regardless of the
            //     absolute OSErr value;
            //   • Register-only OS-bit FUNCTION calling convention; no
            //     Pascal stack frame consumed (A7 preserved).
            //
            // Apple-vs-BasiliskII divergence on absolute OSErr: BII
            // System 7.5.3 ROM is expected to return nsvErr (-35) or a
            // related error for a bogus vRefNum (e.g. 9999), while
            // Systemless returns noErr unconditionally. Both obey
            // the dispatcher convention (D0 == ioResult).
            //
            // Regression coverage:
            //   pbeject_returns_noerr_when_hle_has_no_removable_media
            //   pbeject_writes_same_oserr_to_d0_and_ioresult_preserving_stack
            (false, 0x17) => {
                let pb = cpu.read_reg(Register::A0);
                bus.write_word(pb + 16, 0); // noErr
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // PBOffLine ($A035)
            // Places the specified volume offline by calling PBFlushVol to
            // flush the volume and releasing all the memory used for the
            // volume except for the volume control block.
            // FUNCTION PBOffLine (paramBlock: ParmBlkPtr): OSErr;
            // Inside Macintosh: Files (1992), pp. 2-141 to 2-142.
            //
            // Register convention (OS-bit FUNCTION; IM:Files 1992 p. 2-142
            // trap macro `_OffLine`):
            //   A0 entry: ParmBlkPtr (paramBlock)
            //   D0 exit:  OSErr result code (also mirrored into pb.ioResult
            //             at pb+16 per File Manager dispatcher convention,
            //             IM:II 1985 p. II-114)
            //
            // Parameter block fields used (IM:Files 1992 p. 2-141):
            //   →  ioCompletion (ProcPtr) — pointer to a completion routine
            //   ←  ioResult     (OSErr)   — result code (mirrors D0)
            //   →  ioNamePtr    (StringPtr) — pointer to a pathname
            //   →  ioVRefNum    (Integer) — volume specification
            //
            // Always executes synchronously.
            //
            // Documented result codes (IM:Files 1992 p. 2-142):
            //   noErr     0    No error
            //   nsvErr   -35   No such volume
            //   ioErr    -36   I/O error
            //   bdNamErr -37   Bad volume name
            //   paramErr -50   No default volume
            //   nsDrvErr -56   No such drive
            //   extFSErr -58   External file system
            //
            // MPW Universal Headers (Files.h):
            //   #pragma parameter __D0 PBOffLine(__A0)
            //   EXTERN_API(OSErr) PBOffLine(ParmBlkPtr paramBlock)
            //                                          ONEWORDINLINE(0xA035);
            //
            // Systemless HLE compromise: the single-volume HLE has no offline-
            // able volume (the boot volume is permanently online in our
            // VFS), so the trap collapses to a no-op that writes noErr (0)
            // into both D0 and pb.ioResult. BasiliskII dispatches the real
            // ROM PB trap and may return a non-zero OSErr (typically
            // nsvErr -35) for a bogus vRefNum. The absolute return value
            // therefore differs between the two; the behavioral invariants
            // are (a) the dispatcher convention writing the same OSErr into
            // both D0 and ioResult, and (b) the register-only OS-bit
            // FUNCTION calling convention preserving A7.
            //
            // Regression coverage:
            //   pboffline_writes_same_oserr_to_d0_and_ioresult_preserving_stack
            (false, 0x35) => {
                let pb = cpu.read_reg(Register::A0);
                bus.write_word(pb + 16, 0); // noErr → pb.ioResult
                cpu.write_reg(Register::D0, 0); // noErr → D0
                Ok(())
            }

            // PBSetFVers / _SetFilType ($A043)
            // Changes the version number of a file (high byte of ioMisc).
            // FUNCTION PBSetFVers (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // Inside Macintosh Volume II, II-117 / Volume IV, IV-153
            //
            // Register convention (IM:II 1985 p. II-114 File Manager basic-PB
            // dispatcher; IM:II 1985 p. II-117 PBSetFVers entry):
            //   A0 (entry) = ParmBlkPtr — pointer to the parameter block.
            //   D0 (exit)  = OSErr — function result, also mirrored to
            //                pb.ioResult at pb+16 by the OS dispatcher.
            //
            // OS-bit FUNCTION ABI: no Pascal stack argument frame; A7 is
            // preserved across the call. The MPW Universal Headers Files.h
            // declaration confirms the register-only convention:
            //   #pragma parameter __D0 PBSetFVersSync(__A0)
            //   EXTERN_API(OSErr) PBSetFVersSync(ParmBlkPtr paramBlock)
            //                                   ONEWORDINLINE(0xA043);
            //
            // IOParam field map (IM:II 1985 p. II-117):
            //   → 12 ioCompletion pointer (NIL for synchronous calls)
            //   ← 16 ioResult     word    (mirrored from D0)
            //   → 18 ioNamePtr    pointer (Pascal-string file name)
            //   → 22 ioVRefNum    word    (volume reference number)
            //   → 26 ioFVersNum   byte    (current MFS version number)
            //   → 28 ioMisc       byte    (new version in high-order byte)
            //
            // Documented result codes (IM:II 1985 p. II-117):
            //   noErr     ( 0) — call succeeded (and on HFS volumes always)
            //   bdNamErr  (-37) — bad file name
            //   dupFNErr  (-48) — duplicate file name and version
            //   extFSErr  (-58) — external file system
            //   fLckdErr  (-45) — file locked
            //   fnfErr    (-43) — file not found
            //   nsvErr    (-35) — no such volume
            //   wPrErr    (-44) — hardware volume lock
            //
            // Per IM:IV 1986 p. IV-153 (HFS update): "PBSetFVers has no
            // effect on hierarchical volumes." Per IM:II 1985 p. II-117
            // verbatim warning: "The Resource Manager, the Segment Loader,
            // and the Standard File Package operate only on files with
            // version number 0; changing the version number of a file to
            // a nonzero number will prevent them from operating on it."
            //
            // Systemless HLE compromise: the host VFS is HFS-only, so every
            // file is implicitly at version 0 and PBSetFVers is a no-op.
            // We write 0 (noErr) to both pb.ioResult @ pb+16 and D0,
            // honoring the basic-PB dispatcher convention.
            //
            // Behavioral invariant:
            //   1. Dispatcher convention: D0 == pb.ioResult after the call
            //      (both registers receive the same OSErr).
            //   2. Register-only ABI: A0 is the sole input, D0 the sole
            //      output; A7 unchanged.
            //
            // BasiliskII parity note: BII System 7.5.3 ROM dispatches the
            // real PB trap against an HFS volume mounted via extfs. Per
            // IM:IV 1986 p. IV-153 PBSetFVers is a documented no-op there,
            // so the absolute OSErr is expected to be noErr (0) as well.
            // Where the absolute OSErr diverges, that value is still
            // mirrored into both D0 and ioResult.
            //
            // Regression coverage:
            //   pbsetfvers_writes_same_oserr_to_d0_and_ioresult_preserving_stack
            //
            // PBSetFVers ($A043): HFS no-op per IM:IV 1986 p. IV-153;
            // mirrors noErr to D0 and pb.ioResult per IM:II 1985 p. II-114
            // dispatcher convention; preserves A7 per the register-only
            // OS-bit FUNCTION ABI.
            (false, 0x43) => {
                let pb = cpu.read_reg(Register::A0);
                bus.write_word(pb + 16, 0); // noErr → pb.ioResult
                cpu.write_reg(Register::D0, 0); // noErr → D0
                Ok(())
            }

            // AddDrive ($A04E)
            // Adds a drive queue element to the drive queue. Register-form:
            // D0.W = drvNum, D1.W = drvrRefNum, A0 = qEl.
            // PROCEDURE AddDrive (drvrRefNum, drvNum: Integer; qEl: DrvQElPtr);
            // Inside Macintosh: Files (1992), p. 2-236; Technical Note #108
            // "AddDrive, DrvrInstall and DrvrRemove" documents the register
            // calling convention and noErr return.
            // AddDrive ($A04E): No drive queue in HLE; PROCEDURE returns via D0 = noErr.
            (false, 0x4E) => {
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // PBRename / PBHRename ($A00B / $A20B)
            // Renames a file or directory. PBHRename is the HFS variant.
            // FUNCTION PBRename  (paramBlock: ParmBlkPtr; async: Boolean): OSErr;
            // FUNCTION PBHRename (paramBlock: HParmBlkPtr; async: Boolean): OSErr;
            // Inside Macintosh: Files (1992), p. 2-118
            //
            // Register convention (IM:Files 1992 p. 2-118):
            //   A0 entry: ParmBlkPtr / HParmBlkPtr (pb)
            //   D0 exit:  OSErr (mirrored to pb.ioResult at pb+16 by the
            //             File Manager basic-PB dispatcher convention,
            //             IM:II 1985 p. II-114).
            //   No Pascal stack frame is consumed (OS-bit FUNCTION ABI).
            //
            // Parameter block fields (IM:Files 1992 p. 2-118):
            //   pb+12 ioCompletion (→) IOCompletionUPP, may be NIL on Sync
            //   pb+16 ioResult     (←) OSErr, mirror of D0
            //   pb+18 ioNamePtr    (→) StringPtr, OLD file name
            //   pb+22 ioVRefNum    (→) volume reference number
            //   pb+28 ioMisc       (→) StringPtr, NEW file name
            //   pb+48 ioDirID      (→) HFS variant only; directory ID
            //
            // Documented result codes (IM:Files 1992 pp. 2-118, 10222..10234):
            //   noErr     0    No error
            //   nsvErr   -35   No such volume
            //   ioErr    -36   I/O error
            //   bdNamErr -37   Bad name
            //   fnfErr   -43   File not found (source missing)
            //   wPrErr   -44   Diskette write-protected
            //   fLckdErr -45   File locked
            //   vLckdErr -46   Volume locked
            //   dupFNErr -48   Duplicate file name (destination exists)
            //   paramErr -50   Empty name
            //
            // MPW Universal Headers (Files.h):
            //   #pragma parameter __D0 PBRenameSync(__A0)
            //   EXTERN_API(OSErr) PBRenameSync(ParmBlkPtr paramBlock)
            //       ONEWORDINLINE(0xA00B);
            //   #pragma parameter __D0 PBHRenameSync(__A0)
            //   EXTERN_API(OSErr) PBHRenameSync(HParmBlkPtr paramBlock)
            //       ONEWORDINLINE(0xA20B);
            //   Sibling Async forms at $A40B and $A60B respectively.
            //
            // Behavioral invariant:
            //   - dispatcher convention: D0 == ioResult @ pb+16.
            //   - register-only calling convention: A7 preserved.
            //
            // Systemless HLE: maintains VFS state in self.vfs / self.vfs_rsrc /
            // self.vfs_metadata / self.locked_files / self.open_files /
            // self.output_dir; the same (false, 0x0B) arm services both
            // $A00B and $A20B since the HFS variant differs only in the
            // ioDirID field which Systemless does not branch on.
            //
            // Regression coverage:
            //   pbhrename_writes_same_oserr_to_d0_and_ioresult_preserving_stack
            //   pbhrename_path_new_name_stays_in_source_directory
            (false, 0x0B) => {
                let pb = cpu.read_reg(Register::A0);
                let old_name_ptr = bus.read_long(pb + 18);
                let new_name_ptr = bus.read_long(pb + 28);
                let old_name = Self::read_pb_filename(bus, old_name_ptr);
                let new_name = Self::read_pb_filename(bus, new_name_ptr);
                eprintln!("[TRAP] PBRename(\"{}\" -> \"{}\")", old_name, new_name);

                if old_name.is_empty() || new_name.is_empty() {
                    bus.write_word(pb + 16, (-50i16) as u16); // paramErr
                    cpu.write_reg(Register::D0, (-50i32) as u32);
                    return Some(Ok(()));
                }

                let Some(old_key) = self.find_vfs_file(&old_name) else {
                    bus.write_word(pb + 16, (-43i16) as u16); // fnfErr
                    cpu.write_reg(Register::D0, (-43i32) as u32);
                    return Some(Ok(()));
                };

                if self.vfs_path_is_read_only(&old_key) {
                    bus.write_word(pb + 16, (-44i16) as u16); // wPrErr
                    cpu.write_reg(Register::D0, (-44i32) as u32);
                    return Some(Ok(()));
                }

                // Compute new key by replacing the basename of old_key.
                // PBHRename cannot move a file to another directory
                // (Inside Macintosh: Files, 1992, p. 2-199), so even if a
                // caller passes a colon pathname in ioMisc, only its final
                // name participates in the rename.
                let normalized_new = super::TrapDispatcher::normalize_hfs_path(&new_name);
                let new_leaf = super::TrapDispatcher::vfs_basename(&normalized_new);
                if new_leaf.is_empty() {
                    bus.write_word(pb + 16, (-50i16) as u16); // paramErr
                    cpu.write_reg(Register::D0, (-50i32) as u32);
                    return Some(Ok(()));
                }
                let new_key = match old_key.rsplit_once('/') {
                    Some((parent, _)) => format!("{parent}/{new_leaf}"),
                    None => new_leaf.to_string(),
                };

                if new_key == old_key {
                    // Rename to the same name is a noErr no-op per Files
                    // 1992, 2-118 (the file is unaffected).
                    bus.write_word(pb + 16, 0);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }

                if self.vfs.contains_key(&new_key) || self.vfs_rsrc.contains_key(&new_key) {
                    bus.write_word(pb + 16, (-48i16) as u16); // dupFNErr
                    cpu.write_reg(Register::D0, (-48i32) as u32);
                    return Some(Ok(()));
                }

                if let Some(data) = self.vfs.remove(&old_key) {
                    self.vfs.insert(new_key.clone(), data);
                }
                if let Some(rsrc) = self.vfs_rsrc.remove(&old_key) {
                    self.vfs_rsrc.insert(new_key.clone(), rsrc);
                }
                if let Some(metadata) = self.vfs_metadata.remove(&old_key) {
                    self.vfs_metadata.insert(new_key.clone(), metadata);
                }
                if self.locked_files.remove(&old_key) {
                    self.locked_files.insert(new_key.clone());
                }
                self.remove_vfs_entry_from_process(&old_key);
                self.publish_vfs_entry_to_process(&new_key);
                // Open access paths must follow the rename so a
                // subsequent FSRead/FSWrite still resolves to the file.
                let open_refnums: Vec<u16> = self
                    .open_files
                    .iter()
                    .filter_map(|record| {
                        if record.path == old_key {
                            u16::try_from(record.ref_num).ok()
                        } else {
                            None
                        }
                    })
                    .collect();
                for refnum in open_refnums {
                    self.open_files.insert(refnum, new_key.clone());
                }

                if let Some(ref dir) = self.output_dir {
                    let old_path = dir.join(&old_key);
                    let new_path = dir.join(&new_key);
                    if let Some(parent) = new_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::rename(old_path, new_path);
                }

                bus.write_word(pb + 16, 0); // noErr
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // PBSetFLock / PBHSetFLock ($A041 / $A241)
            // Locks a file by setting bit 0 of ioFlAttrib (visible via
            // subsequent PBGetFInfo / PBHGetFInfo).
            // FUNCTION PBSetFLock (paramBlock: ParmBlkPtr; async: Boolean): OSErr;
            // FUNCTION PBHSetFLock (paramBlock: HParmBlkPtr; async: Boolean): OSErr;
            // Inside Macintosh: Files 1992, pp. 2-110 to 2-111
            //
            // Register convention (IM:Files 1992 p. 2-110):
            //   A0 entry: ParmBlkPtr (pb)
            //   D0 exit:  OSErr (mirrored to pb.ioResult at pb+16 by the
            //             File Manager basic-PB dispatcher convention,
            //             IM:II 1985 p. II-114).
            //   No Pascal stack frame is consumed.
            //
            // Parameter block fields (IM:Files 1992 p. 2-110):
            //   pb+12 ioCompletion (→) IOCompletionUPP, may be NIL on Sync
            //   pb+16 ioResult     (←) OSErr, mirror of D0
            //   pb+18 ioNamePtr    (→) StringPtr, file name (Pascal string)
            //   pb+22 ioVRefNum    (→) volume reference number (0 = default)
            //
            // Documented result codes (IM:Files 1992 pp. 2-110, 10141..10150):
            //   noErr     0    No error
            //   fnfErr    -43  File not found
            //   ioErr     -36  I/O error
            //   nsvErr    -35  Volume not found
            //   bdNamErr  -37  Bad filename
            //   paramErr  -50  ioVRefNum bad / ioNamePtr is NIL
            //
            // MPW Universal Headers Files.h:
            //   #pragma parameter __D0 PBSetFLockSync(__A0)
            //   EXTERN_API(OSErr) PBSetFLockSync(ParmBlkPtr paramBlock)
            //                                       ONEWORDINLINE(0xA041);
            //   #define PBSetFLock(pb,async) (async ? PBSetFLockAsync(pb) : PBSetFLockSync(pb))
            //
            // Systemless HLE: maintains lock state in self.locked_files and
            // returns -43 fnfErr for files not in the VFS, matching IM
            // verbatim.
            //
            // BasiliskII divergence note: real System 7.5.3 + extfs
            // returns noErr (0) from PBSetFLock / PBHSetFLock on a
            // missing file, not fnfErr (-43). Systemless keeps the
            // IM-documented absolute OSErr; both obey the dispatcher
            // convention writing the same value to BOTH D0 and
            // pb.ioResult @ pb+16.
            //
            // Regression coverage:
            //   pbsetflock_existing_file_returns_noerr_and_sets_ioflattrib_locked_bit
            //   pbsetflock_missing_file_returns_fnferr_in_d0_and_ioresult
            //   pbsetflock_pbrstflock_write_same_oserr_to_d0_and_ioresult_preserving_stack
            //   pbhsetflock_pbhrstflock_write_same_oserr_to_d0_and_ioresult_preserving_stack
            //   pbrstflock_existing_file_returns_noerr_and_clears_ioflattrib_locked_bit
            //   pbrstflock_missing_file_returns_fnferr_in_d0_and_ioresult
            //   pbhsetflock_existing_file_returns_noerr_and_sets_ioflattrib_locked_bit
            //   pbhrstflock_existing_file_returns_noerr_and_clears_ioflattrib_locked_bit
            (false, 0x41) => {
                let pb = cpu.read_reg(Register::A0);
                let name_ptr = bus.read_long(pb + 18);
                let filename = Self::read_pb_filename(bus, name_ptr);
                eprintln!("[TRAP] PBSetFLock(\"{}\")", filename);

                if let Some(vfs_name) = self.find_vfs_file(&filename) {
                    if self.vfs_path_is_read_only(&vfs_name) {
                        bus.write_word(pb + 16, (-44i16) as u16); // wPrErr
                        cpu.write_reg(Register::D0, (-44i32) as u32);
                    } else {
                        self.locked_files.insert(vfs_name);
                        bus.write_word(pb + 16, 0); // noErr
                        cpu.write_reg(Register::D0, 0);
                    }
                } else {
                    bus.write_word(pb + 16, (-43i16) as u16); // fnfErr
                    cpu.write_reg(Register::D0, (-43i32) as u32);
                }
                Ok(())
            }

            // PBRstFLock / PBHRstFLock ($A042 / $A242)
            // Unlocks a file by clearing bit 0 of ioFlAttrib.
            // FUNCTION PBRstFLock (paramBlock: ParmBlkPtr; async: Boolean): OSErr;
            // FUNCTION PBHRstFLock (paramBlock: HParmBlkPtr; async: Boolean): OSErr;
            // Inside Macintosh: Files 1992, pp. 2-110 to 2-111
            //
            // Register convention, parameter-block field map, result-
            // code table, MPW Universal Headers declaration, Systemless
            // HLE compromise, and BasiliskII divergence note are all
            // identical to the $A041 PBSetFLock arm above (just swap
            // PBSetFLock → PBRstFLock and ONEWORDINLINE(0xA041) →
            // ONEWORDINLINE(0xA042)).
            //
            // Regression coverage: see the $A041 arm above.
            (false, 0x42) => {
                let pb = cpu.read_reg(Register::A0);
                let name_ptr = bus.read_long(pb + 18);
                let filename = Self::read_pb_filename(bus, name_ptr);
                eprintln!("[TRAP] PBRstFLock(\"{}\")", filename);

                if let Some(vfs_name) = self.find_vfs_file(&filename) {
                    if self.vfs_path_is_read_only(&vfs_name) {
                        bus.write_word(pb + 16, (-44i16) as u16); // wPrErr
                        cpu.write_reg(Register::D0, (-44i32) as u32);
                    } else {
                        self.locked_files.remove(&vfs_name);
                        bus.write_word(pb + 16, 0); // noErr
                        cpu.write_reg(Register::D0, 0);
                    }
                } else {
                    bus.write_word(pb + 16, (-43i16) as u16); // fnfErr
                    cpu.write_reg(Register::D0, (-43i32) as u32);
                }
                Ok(())
            }

            // PBCreate / PBHCreate (0xA008 / 0xA208)
            // Creates a new file.
            // FUNCTION PBCreate (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // FUNCTION PBHCreate (paramBlock: HParmBlkPtr; async: BOOLEAN): OSErr;
            // Files 1992, 2-89 / 9279 (PBCreate) and 2-191 / 9700 (PBHCreate).
            // The OS-trap dispatcher masks `trap & 0x00FF`, so $A208
            // PBHCreate lands on the same low byte and shares this arm.
            //
            // Behavioral note: for a $A208 PBHCreate with dirID=999999,
            // BasiliskII writes dirNFErr per IM:Files 9743 while Systemless
            // writes noErr because the impl does not validate dirID.
            //
            // Regression coverage:
            //   pb_create
            // PBCreate ($A008): Creates file in VFS with metadata, writes to output_dir if set
            // PBHCreate ($A208): HFS variant aliased onto $A008
            (false, 0x08) => {
                let pb = cpu.read_reg(Register::A0);
                let name_ptr = bus.read_long(pb + 18);
                let filename = Self::read_pb_filename(bus, name_ptr);
                let v_ref = bus.read_word(pb + 22) as i16;
                let is_hfs_variant = matches!(
                    raw_trap_route(self.current_trap_word).os_routine_variant,
                    OsRoutineVariant::FileHfsSynchronous | OsRoutineVariant::FileHfsAsynchronous
                );
                let dir_id = if is_hfs_variant {
                    bus.read_long(pb + 48)
                } else {
                    0
                };
                eprintln!(
                    "[TRAP] {}(\"{}\") vref={} dirID={}",
                    if is_hfs_variant {
                        "PBHCreate"
                    } else {
                        "PBCreate"
                    },
                    filename,
                    v_ref,
                    dir_id
                );

                if filename.is_empty() {
                    bus.write_word(pb + 16, (-50i16) as u16); // paramErr
                    cpu.write_reg(Register::D0, (-50i32) as u32);
                    return Some(Ok(()));
                }
                let vfs_key = self
                    .vfs_key_for_fsspec(v_ref, dir_id, &filename)
                    .unwrap_or_else(|| super::TrapDispatcher::normalize_hfs_path(&filename));

                if self.vfs_path_is_read_only(&vfs_key) {
                    bus.write_word(pb + 16, (-44i16) as u16); // wPrErr
                    cpu.write_reg(Register::D0, (-44i32) as u32);
                    return Some(Ok(()));
                }

                // Per IM Files 1992, 2-89, PBCreate returns dupFNErr when the
                // file already exists. Some shareware/demo titles ship a
                // marker file (e.g. Meteor Storm's "MS UserKey") inside their
                // install folder yet still call HCreate at launch without an
                // intervening HDelete, treating any error as fatal. Real Mac
                // installs were typically run from a freshly-extracted copy
                // where the file did not exist, so the bug never surfaced.
                // Systemless models the .sit-extracted folder directly, so the
                // marker is always pre-existing on first launch. To keep
                // these titles bootable without breaking apps that *do*
                // handle dupFNErr, truncate-on-exists: clear both forks of
                // the existing entry and report noErr.
                let existing = self
                    .vfs
                    .keys()
                    .chain(self.vfs_rsrc.keys())
                    .find(|key| key.eq_ignore_ascii_case(&vfs_key))
                    .cloned();
                if let Some(existing) = existing {
                    // Preserve any existing resource-fork content — some
                    // shareware titles ship a key file with registration
                    // resources baked into the resource fork (the data
                    // fork is the marker, the resource fork carries the
                    // actual templates). Truncating both forks would
                    // destroy the resources the game then tries to read
                    // back via FSpOpenResFile + Get1Resource. Truncating
                    // only the data fork is enough to satisfy the
                    // "create fresh" expectation that triggers the
                    // dupFNErr fatal-launch path.
                    let rsrc_len = self.vfs_rsrc.get(&existing).map(|v| v.len()).unwrap_or(0);
                    eprintln!(
                        "[TRAP] PBCreate truncate-on-exists for \"{}\" -> noErr (rsrc preserved: {} bytes)",
                        existing, rsrc_len
                    );
                    self.vfs.insert(existing.clone(), Vec::new());
                    self.vfs_rsrc.ensure_empty(existing.clone());
                    self.touch_vfs_entry(&existing);
                    if let Some(ref dir) = self.output_dir {
                        let host_path = dir.join(&existing);
                        if let Some(parent) = host_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let _ = std::fs::write(host_path, []);
                    }
                    bus.write_word(pb + 16, 0);
                    cpu.write_reg(Register::D0, 0);
                    return Some(Ok(()));
                }

                self.vfs.insert(vfs_key.clone(), Vec::new());
                // Real Mac files always have both forks. PBCreate must seed
                // an empty resource fork too, otherwise a subsequent
                // PBOpenRF on the new file (e.g. Mars Rising's installer
                // pattern: PBCreate "Installer Temp (delete)" then
                // PBOpenRF that file's resource fork) returns fnfErr and
                // the installer halts via _ExitToShell.
                // Files 1992, 1-58 (each file has data + resource fork)
                self.vfs_rsrc.insert(vfs_key.clone(), Vec::new());
                self.touch_vfs_entry(&vfs_key);
                if let Some(ref dir) = self.output_dir {
                    let host_path = dir.join(&vfs_key);
                    if let Some(parent) = host_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(host_path, []);
                }

                bus.write_word(pb + 16, 0);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // PBDelete / PBHDelete (0xA009 / 0xA209)
            // Deletes a file.
            // FUNCTION PBDelete (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // FUNCTION PBHDelete (paramBlock: HParmBlkPtr; async: BOOLEAN): OSErr;
            // Files 1992, 2-102 / 9286 (PBDelete) and 2-189 / 9797 (PBHDelete).
            // The HFS variant lands on the same low byte after
            // `trap & 0x00FF` masking and shares this arm.
            //
            // Behavioral note: real Mac extfs (BasiliskII) returns noErr for
            // a missing file on the $A209 PBHDelete path.
            //
            // Regression coverage:
            //   pb_delete
            //   pb_delete_not_found
            // PBDelete ($A009): Deletes file from VFS, cleans up metadata and open file refs
            // PBHDelete ($A209): HFS variant aliased onto $A009
            (false, 0x09) => {
                let pb = cpu.read_reg(Register::A0);
                let name_ptr = bus.read_long(pb + 18);
                let filename = Self::read_pb_filename(bus, name_ptr);
                eprintln!("[TRAP] PBDelete(\"{}\")", filename);

                if filename.is_empty() {
                    bus.write_word(pb + 16, (-50i16) as u16); // paramErr
                    cpu.write_reg(Register::D0, (-50i32) as u32);
                    return Some(Ok(()));
                }

                if let Some(vfs_name) = self.find_vfs_file(&filename) {
                    if self.vfs_path_is_read_only(&vfs_name) {
                        bus.write_word(pb + 16, (-44i16) as u16); // wPrErr
                        cpu.write_reg(Register::D0, (-44i32) as u32);
                        return Some(Ok(()));
                    }
                    self.vfs.remove(&vfs_name);
                    self.vfs_rsrc.remove(&vfs_name);
                    self.remove_vfs_entry_metadata(&vfs_name);

                    let stale_refs: Vec<u16> = self
                        .open_files
                        .iter()
                        .filter_map(|record| {
                            if record.path == vfs_name {
                                u16::try_from(record.ref_num).ok()
                            } else {
                                None
                            }
                        })
                        .collect();
                    for refnum in stale_refs {
                        self.open_files.remove(&refnum);
                        self.file_positions.remove(&refnum);
                        self.write_refnums.remove(&refnum);
                        Self::clear_file_fcb(bus, refnum);
                    }

                    if let Some(ref dir) = self.output_dir {
                        let host_path = dir.join(&vfs_name);
                        let _ = std::fs::remove_file(host_path);
                    }

                    bus.write_word(pb + 16, 0);
                    cpu.write_reg(Register::D0, 0);
                } else {
                    bus.write_word(pb + 16, (-43i16) as u16); // fnfErr
                    cpu.write_reg(Register::D0, (-43i32) as u32);
                }
                Ok(())
            }

            // PBGetFInfo / PBHGetFInfo (0xA00C / 0xA20C)
            // Gets Finder and catalog information for a file.
            // FUNCTION PBGetFInfo (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // FUNCTION PBHGetFInfo (paramBlock: HParmBlkPtr; async: BOOLEAN): OSErr;
            // Inside Macintosh Volume IV, IV-148; Files 1992, 2-170.
            // The HFS variant ($A20C) is aliased onto this arm via the OS-trap
            // `trap & 0x00FF` mask.
            //
            // PBGetFInfo ($A00C): Returns catalog info with file type, creator, finder flags, and ioFlAttrib lock bit from VFS metadata
            // PBHGetFInfo ($A20C): HFS variant aliased onto $A00C
            (false, 0x0C) => {
                let pb = cpu.read_reg(Register::A0);
                let name_ptr = bus.read_long(pb + 18);
                let filename = Self::read_pb_filename(bus, name_ptr);
                let vref = bus.read_word(pb + 22) as i16;
                let fdir_index = bus.read_word(pb + 28) as i16;
                let dir_id = bus.read_long(pb + 48);
                eprintln!(
                    "[TRAP] PBGetFInfo(\"{}\", vRefNum={}, dirID={}, fDirIndex={})",
                    filename, vref, dir_id, fdir_index
                );

                let lookup = if fdir_index > 0 {
                    self.lookup_file_entry_for_get_finfo(vref, dir_id, &filename, fdir_index)
                        .map(|entry| entry.path)
                } else {
                    self.find_vfs_file_for_hfs_lookup(vref, dir_id, &filename)
                        .or_else(|| self.find_vfs_file(&filename))
                };

                if let Some(vfs_name) = lookup {
                    let open_refnum = self
                        .open_files
                        .iter()
                        .find(|record| record.path.eq_ignore_ascii_case(&vfs_name))
                        .and_then(|record| u16::try_from(record.ref_num).ok())
                        .or_else(|| self.refnum_for_resource_file_name(&vfs_name));
                    if let Some(metadata) = self.vfs_file_metadata(&vfs_name) {
                        self.fill_file_catalog_info(bus, pb, &vfs_name, metadata);
                        // Inside Macintosh Volume IV, pp. IV-148–149: for a
                        // name-based PBGetFInfo call, ioNamePtr is returned as
                        // the leaf name when an access path to the file is
                        // open. Closed files leave the caller's pathname in
                        // place so the documented PBGetFInfo -> PBSetFInfo
                        // sequence continues to identify the same file.
                        // Indexed calls still need the discovered leaf name.
                        if name_ptr != 0 && (fdir_index > 0 || open_refnum.is_some()) {
                            let guest_name = super::TrapDispatcher::hfs_name_from_vfs_component(
                                super::TrapDispatcher::vfs_basename(&vfs_name),
                            );
                            Self::write_pstring(bus, name_ptr, &guest_name);
                        }
                        bus.write_word(pb + 24, open_refnum.unwrap_or(0));
                    }
                    bus.write_word(pb + 16, 0); // noErr
                    cpu.write_reg(Register::D0, 0);
                } else {
                    bus.write_word(pb + 16, (-43i16) as u16); // fnfErr
                    cpu.write_reg(Register::D0, (-43i32) as u32);
                }
                Ok(())
            }

            // PBSetFInfo / PBHSetFInfo (0xA00D / 0xA20D)
            // Sets Finder information for a file.
            // FUNCTION PBSetFInfo (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // FUNCTION PBHSetFInfo (paramBlock: HParmBlkPtr; async: BOOLEAN): OSErr;
            // Files 1992, 2-205 / 9298.  The HFS variant ($A20D) is the
            // same low byte after `trap & 0x00FF` masking and shares
            // this arm.
            //
            // Per Files 1992, 2-205: returns fnfErr (-43) if the named
            // file does not exist. The previous HLE implementation
            // silently returned noErr for missing files, masking bugs in
            // games that rely on this error to detect missing saves.
            //
            //   pb_finfo               ← $A00C/$A00D fnfErr path
            //   pbh_get_set_finfo      ← $A20C/$A20D fnfErr path
            // PBSetFInfo ($A00D): Stores file type, creator, and Finder flags in VFS metadata
            // PBHSetFInfo ($A20D): HFS variant aliased onto $A00D
            (false, 0x0D) => {
                let pb = cpu.read_reg(Register::A0);
                let name_ptr = bus.read_long(pb + 18);
                let filename = Self::read_pb_filename(bus, name_ptr);
                let vref = bus.read_word(pb + 22) as i16;
                let dir_id = bus.read_long(pb + 48);
                eprintln!("[TRAP] PBSetFInfo(\"{}\")", filename);
                if let Some(vfs_name) = self
                    .find_vfs_file_for_hfs_lookup(vref, dir_id, &filename)
                    .or_else(|| self.find_vfs_file(&filename))
                {
                    if self.vfs_path_is_read_only(&vfs_name) {
                        bus.write_word(pb + 16, (-44i16) as u16); // wPrErr
                        cpu.write_reg(Register::D0, (-44i32) as u32);
                        return Some(Ok(()));
                    }
                    let file_type = bus.read_long(pb + 32);
                    let creator = bus.read_long(pb + 36);
                    let finder_flags = bus.read_word(pb + 40);
                    self.set_vfs_entry_finfo(&vfs_name, file_type, creator, finder_flags);
                    bus.write_word(pb + 16, 0); // noErr
                    cpu.write_reg(Register::D0, 0);
                } else {
                    bus.write_word(pb + 16, (-43i16) as u16); // fnfErr
                    cpu.write_reg(Register::D0, (-43i32) as u32);
                }
                Ok(())
            }

            // PBSetVol / PBHSetVol (0xA015 / 0xA215)
            // Sets the default volume and directory from a parameter block.
            // FUNCTION PBSetVol  (paramBlock: ParmBlkPtr; async: Boolean): OSErr;
            // FUNCTION PBHSetVol (paramBlock: WDPBPtr;    async: Boolean): OSErr;
            // Inside Macintosh: Files 1992, 2-151 to 2-154
            (false, 0x15) => {
                let pb = cpu.read_reg(Register::A0);
                let name_ptr = bus.read_long(pb + 18);
                let name = Self::read_pb_filename(bus, name_ptr);
                let requested_vref = bus.read_word(pb + 22) as i16;
                let is_hfs_set_vol = matches!(
                    raw_trap_route(self.current_trap_word).os_routine_variant,
                    OsRoutineVariant::FileHfsSynchronous | OsRoutineVariant::FileHfsAsynchronous
                );
                let requested_dir_id = if is_hfs_set_vol {
                    bus.read_long(pb + 48)
                } else {
                    0
                };

                let named_volume = (!name.is_empty())
                    .then(|| {
                        if name.eq_ignore_ascii_case(super::TrapDispatcher::boot_volume_name()) {
                            Some((super::TrapDispatcher::boot_volume_ref_num(), 2u32))
                        } else {
                            self.vfs_volume_by_name(&name)
                                .map(|volume| (volume.ref_num, volume.root_dir_id))
                        }
                    })
                    .flatten();
                let matching_working_directory = self
                    .working_directories
                    .get(&requested_vref)
                    .copied()
                    .filter(|working_directory| {
                        named_volume.is_none_or(|(volume_ref_num, _)| {
                            volume_ref_num == working_directory.volume_ref_num
                        })
                    });
                if requested_vref == 0 && !name.is_empty() && named_volume.is_none() {
                    let nsverr: i16 = -35;
                    bus.write_word(pb + 16, nsverr as u16);
                    cpu.write_reg(Register::D0, nsverr as u32);
                    return Some(Ok(()));
                }
                let mut target_volume_ref_num = matching_working_directory
                    .map(|working_directory| working_directory.volume_ref_num)
                    .or_else(|| named_volume.map(|(volume_ref_num, _)| volume_ref_num))
                    .unwrap_or_else(|| self.resolve_volume_ref_num(requested_vref));
                let mut target_dir_id = if let Some(working_directory) = matching_working_directory
                {
                    working_directory.dir_id
                } else if let Some((_, root_dir_id)) = named_volume {
                    root_dir_id
                } else if let Some(working_directory) =
                    self.working_directories.get(&requested_vref)
                {
                    target_volume_ref_num = working_directory.volume_ref_num;
                    working_directory.dir_id
                } else if is_hfs_set_vol
                    && requested_dir_id != 0
                    && self.directory_entry_for_id(requested_dir_id).is_some()
                {
                    requested_dir_id
                } else if requested_vref == 0 {
                    *self.default_dir_id
                } else if requested_vref == Self::boot_volume_ref_num() {
                    // ioVRefNum = -1: boot volume, use root directory (dirID 2).
                    2
                } else if let Some(volume) = self.vfs_volume_for_ref_num(requested_vref) {
                    volume.root_dir_id
                } else {
                    // Unrecognised volume reference number: nsvErr (-35).
                    // IM:Files 1992, 2-162: "nsvErr — No such volume."
                    let nsverr: i16 = -35;
                    bus.write_word(pb + 16, nsverr as u16);
                    cpu.write_reg(Register::D0, nsverr as u32);
                    return Some(Ok(()));
                };

                if named_volume.is_none() && !name.is_empty() {
                    if let Some(path) = self.find_vfs_directory_in_directory(target_dir_id, &name) {
                        if let Some(directory) = self
                            .vfs_directories
                            .iter()
                            .find(|directory| directory.path.eq_ignore_ascii_case(&path))
                        {
                            target_dir_id = directory.dir_id;
                        }
                    } else if let Some(path) = self.find_vfs_file_in_directory(target_dir_id, &name)
                    {
                        if let Some(metadata) = self.vfs_file_metadata(&path) {
                            target_dir_id = metadata.parent_dir_id;
                        }
                    }
                }

                self.default_dir_id
                    .with_mut(|default_dir_id| *default_dir_id = target_dir_id);
                let app_wd_refnum = if target_dir_id == 2 {
                    target_volume_ref_num
                } else {
                    self.open_working_directory(target_volume_ref_num, target_dir_id, 0)
                        .unwrap_or(target_volume_ref_num)
                };
                self.app_wd_refnum
                    .with_mut(|current| *current = app_wd_refnum);

                // Keep the Standard File globals aligned with the current
                // default directory and volume. Files 1992, 3-65.
                bus.write_long(addr::CUR_DIR_STORE, target_dir_id);
                bus.write_word(addr::SF_SAVE_DISK, (-target_volume_ref_num) as u16);

                eprintln!(
                    "[TRAP] PBSetVol name='{}' request_vRefNum={} request_dirID={} -> current_vRefNum={} current_dirID={}",
                    name,
                    requested_vref,
                    requested_dir_id,
                    *self.app_wd_refnum,
                    *self.default_dir_id
                );
                bus.write_word(pb + 16, 0);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // _CmpString ($A03C), the register-level EqualString operation.
            // On entry: A0 = ptr to first string's first character
            //           A1 = ptr to second string's first character
            //           D0 high word = length of first string
            //           D0 low word  = length of second string
            // On exit:  D0 = 0 if equal, 1 if not equal (long word).
            //
            // MARKS (bit 9) makes comparison diacritic-sensitive; CASE (bit
            // 10) makes it case-sensitive. The bare form ignores both. Inside
            // Macintosh: Text (1993), pp. 5-51--5-52. This later four-row table
            // supersedes the opposite MARKS annotation in Volume II (1985),
            // p. II-377. D0 is 0 for equal and 1 for unequal.
            (false, 0x3C) => {
                let a_ptr = cpu.read_reg(Register::A0);
                let b_ptr = cpu.read_reg(Register::A1);
                let d0 = cpu.read_reg(Register::D0);
                let a_len = (d0 >> 16) & 0xFFFF;
                let b_len = d0 & 0xFFFF;
                let (case_sensitive, diacritic_sensitive) = raw_trap_route(self.current_trap_word)
                    .os_routine_variant
                    .text_comparison_sensitivity()
                    .expect("CmpString trap word must have a classified comparison variant");

                let result: u32 = if a_len != b_len {
                    1
                } else {
                    let equal = (0..a_len).all(|offset| {
                        let mut a = bus.read_byte(a_ptr + offset);
                        let mut b = bus.read_byte(b_ptr + offset);
                        if !diacritic_sensitive {
                            a = mac_roman_strip_diacriticals(a);
                            b = mac_roman_strip_diacriticals(b);
                        }
                        if !case_sensitive {
                            a = mac_roman_to_upper(a, false);
                            b = mac_roman_to_upper(b, false);
                        }
                        a == b
                    });
                    u32::from(!equal)
                };
                cpu.write_reg(Register::D0, result);
                Ok(())
            }

            // PBSetEOF / HSetEOF ($A012)
            // Sets the logical end-of-file of an open file.
            // FUNCTION PBSetEOF (paramBlock: ParmBlkPtr; async: BOOLEAN): OSErr;
            // Inside Macintosh Volume II, II-113
            // PBSetEOF ($A012): Sets file length, truncates/extends VFS data
            (false, 0x12) => {
                let pb = cpu.read_reg(Register::A0);
                let ref_num = bus.read_word(pb + 24);
                let new_eof = bus.read_long(pb + 28) as usize; // ioMisc

                let Some(filename) = self.open_files.get(&ref_num).cloned() else {
                    bus.write_word(pb + 16, (-51i16) as u16); // rfNumErr
                    cpu.write_reg(Register::D0, (-51i32) as u32);
                    return Some(Ok(()));
                };

                if self.vfs_path_is_read_only(&filename) {
                    bus.write_word(pb + 16, (-44i16) as u16); // wPrErr
                    cpu.write_reg(Register::D0, (-44i32) as u32);
                    return Some(Ok(()));
                }

                if !self.write_refnums.contains(&ref_num) {
                    bus.write_word(pb + 16, (-61i16) as u16); // wrPermErr
                    cpu.write_reg(Register::D0, (-61i32) as u32);
                    return Some(Ok(()));
                }

                let host_sync_bytes = self.vfs.with_entry_or_default_mut(
                    filename.clone(),
                    |file_buf| {
                        file_buf.resize(new_eof, 0);
                        if self.output_dir.is_some() {
                            Some(file_buf.clone())
                        } else {
                            None
                        }
                    },
                );

                self.file_positions.with_position_mut(&ref_num, |pos| {
                    let new_eof = u32::try_from(new_eof).unwrap_or(u32::MAX);
                    if *pos > new_eof {
                        *pos = new_eof;
                    }
                });

                if let (Some(dir), Some(bytes)) = (&self.output_dir, host_sync_bytes) {
                    let host_path = dir.join(&filename);
                    if let Some(parent) = host_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = std::fs::write(&host_path, bytes) {
                        eprintln!(
                            "[PBSetEOF] failed to sync {} to host ({}): {}",
                            filename,
                            host_path.display(),
                            e
                        );
                    }
                }
                self.touch_vfs_entry(&filename);

                bus.write_word(pb + 16, 0); // noErr
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // OSDispatch (0xA88F)
            // Dispatches Temporary Memory, High-Level Event, and Process Manager routines selected by a stack word.
            // FUNCTION TempMaxMem(VAR grow: Size): Size;
            // Inside Macintosh: Memory (1992), pp. 2-79 to 2-80 and 2-104.
            (true, 0x08F) => {
                let sp_entry = cpu.read_reg(Register::A7);
                let stack_sel = bus.read_word(sp_entry) as u32 & 0xFFFF;
                let operation =
                    os_dispatch_operation_route(self.current_trap_word, stack_sel as u16);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                let d0_sel = cpu.read_reg(Register::D0) & 0xFFFF;
                let (selector, sp, selector_from_stack) = match stack_sel {
                    0x0015 | 0x0016 | 0x0018 | 0x001D..=0x0020 | 0x0033..=0x003D | 0x0045 => {
                        // Pop the selector word pushed by MOVE.W #sel,-(SP).
                        cpu.write_reg(Register::A7, sp_entry + 2);
                        (stack_sel, sp_entry + 2, true)
                    }
                    _ => (d0_sel, sp_entry, false),
                };
                match selector {
                    0x0015 => {
                        // TempMaxMem (0xA88F selector $0015)
                        // Returns the largest contiguous temporary-memory block.
                        // FUNCTION TempMaxMem(VAR grow: Size): Size;
                        // Inside Macintosh Volume VI, 28-36..28-39; Memory 1992, 2-79..2-80.
                        let grow_ptr = bus.read_long(sp);
                        if grow_ptr != 0 {
                            bus.write_long(grow_ptr, 0);
                        }
                        let free = temporary_memory_free_estimate(bus);
                        bus.write_long(sp + 4, free);
                        cpu.write_reg(Register::D0, free);
                        cpu.write_reg(Register::A7, sp + 4);
                        Ok(())
                    }
                    0x0016 => {
                        // TempTopMem (0xA88F selector $0016)
                        // Returns a pointer to the top of addressable RAM.
                        // FUNCTION TempTopMem: Ptr;
                        // Inside Macintosh Volume VI, 28-37 and 28-45.
                        let mem_top = bus.read_long(crate::memory::globals::addr::MEM_TOP);
                        let result = if mem_top != 0 {
                            mem_top
                        } else {
                            bus.ram_size()
                        };
                        bus.write_long(sp, result);
                        cpu.write_reg(Register::D0, result);
                        Ok(())
                    }
                    0x0018 => {
                        // TempFreeMem (0xA88F selector $0018)
                        // Returns the total amount of free temporary memory.
                        // FUNCTION TempFreeMem: LongInt;
                        // Inside Macintosh Volume VI, 28-38 and 28-45.
                        let free = temporary_memory_free_estimate(bus);
                        bus.write_long(sp, free);
                        cpu.write_reg(Register::D0, free);
                        Ok(())
                    }
                    0x001D => {
                        // TempNewHandle (0xA88F selector $001D)
                        // Allocates a relocatable block of temporary memory.
                        // FUNCTION TempNewHandle(logicalSize: Size; VAR resultCode: OSErr): Handle;
                        // Inside Macintosh Volume VI, 28-36..28-39; Memory 1992, 2-78.
                        let result_code_ptr = bus.read_long(sp);
                        let logical_size = bus.read_long(sp + 4);
                        let ptr = bus.alloc(logical_size);
                        let (handle, result_code) = if ptr == 0 && logical_size > 0 {
                            (0, MEM_FULL_ERR)
                        } else {
                            scribble_temporary_allocation(bus, ptr, logical_size);
                            let handle = bus.alloc(4);
                            if handle == 0 {
                                if ptr != 0 {
                                    bus.free(ptr);
                                }
                                (0, MEM_FULL_ERR)
                            } else {
                                bus.write_long(handle, ptr);
                                self.track_handle_ptr(ptr, handle);
                                (handle, NO_ERR)
                            }
                        };
                        write_temp_memory_result_code(bus, result_code_ptr, result_code);
                        bus.write_long(sp + 8, handle);
                        cpu.write_reg(Register::A0, handle);
                        cpu.write_reg(Register::D0, handle);
                        cpu.write_reg(Register::A7, sp + 8);
                        Ok(())
                    }
                    0x001E | 0x001F => {
                        // TempHLock / TempHUnlock (0xA88F selectors $001E / $001F)
                        // Locks or unlocks a relocatable temporary-memory block.
                        // PROCEDURE TempHLock(h: Handle; VAR resultCode: OSErr);
                        // PROCEDURE TempHUnlock(h: Handle; VAR resultCode: OSErr);
                        // Inside Macintosh Volume VI, 28-37..28-39 and 28-45.
                        let result_code_ptr = bus.read_long(sp);
                        let handle = bus.read_long(sp + 4);
                        let result_code = temporary_memory_handle_status(bus, handle);
                        if result_code == NO_ERR {
                            self.update_handle_state_bits(handle, |bits| {
                                let bits = if selector == 0x001E {
                                    bits.unwrap_or(0) | 0x80
                                } else {
                                    bits.unwrap_or(0) & !0x80
                                };
                                (bits != 0).then_some(bits)
                            });
                        }
                        write_temp_memory_result_code(bus, result_code_ptr, result_code);
                        cpu.write_reg(Register::D0, result_code);
                        cpu.write_reg(Register::A7, sp + 8);
                        Ok(())
                    }
                    0x0020 => {
                        // TempDisposeHandle (0xA88F selector $0020)
                        // Releases a relocatable temporary-memory block.
                        // PROCEDURE TempDisposeHandle(h: Handle; VAR resultCode: OSErr);
                        // Inside Macintosh Volume VI, 28-37..28-39 and 28-45.
                        let result_code_ptr = bus.read_long(sp);
                        let handle = bus.read_long(sp + 4);
                        let result_code = if handle != 0
                            && bus.get_alloc_size(handle).is_some()
                            && !self.loaded_handles.contains_key(&handle)
                        {
                            let ptr = bus.read_long(handle);
                            self.untrack_handle_ptr(ptr);
                            bus.free(ptr);
                            bus.free(handle);
                            self.remove_handle_state_bits(handle);
                            NO_ERR
                        } else {
                            MEM_WZ_ERR
                        };
                        write_temp_memory_result_code(bus, result_code_ptr, result_code);
                        cpu.write_reg(Register::D0, result_code);
                        cpu.write_reg(Register::A7, sp + 8);
                        Ok(())
                    }
                    0x0033 => {
                        // AcceptHighLevelEvent (0xA88F selector $0033)
                        // Returns additional data for the outstanding high-level event.
                        // FUNCTION AcceptHighLevelEvent(VAR sender: TargetID;
                        //   VAR msgRefcon: LongInt; msgBuff: Ptr; VAR msgLen: LongInt): OSErr;
                        // Inside Macintosh Volume VI, 5-29 and Macintosh Toolbox Essentials 1992, 2-90.
                        if self.apple_event_launch_state.accept_open_application_event() {
                            let msg_len_ptr = bus.read_long(sp);
                            let msg_refcon_ptr = bus.read_long(sp + 8);
                            let sender_ptr = bus.read_long(sp + 12);
                            if msg_len_ptr != 0 {
                                bus.write_long(msg_len_ptr, 0);
                            }
                            if msg_refcon_ptr != 0 {
                                bus.write_long(msg_refcon_ptr, 0);
                            }
                            if sender_ptr != 0 {
                                // The local launch event has no additional data.
                                // A TargetID is 252 bytes with 68K two-byte
                                // packing: sessionID, two PPCPortRecs, and a
                                // LocationNameRec. The Mac OS sender uses
                                // session -1 and the local port name "MacOS".
                                // Inside Macintosh Volume VI, 5-10 and 7-15.
                                bus.write_bytes(sender_ptr, &[0; 252]);
                                bus.write_long(sender_ptr, u32::MAX);
                                bus.write_byte(sender_ptr + 6, 5);
                                bus.write_bytes(sender_ptr + 7, b"MacOS");
                            }
                            write_osdispatch_oseerr_result(cpu, bus, sp + 16, 0);
                        } else {
                            write_osdispatch_oseerr_result(
                                cpu,
                                bus,
                                sp + 16,
                                NO_OUTSTANDING_HLE_ERR,
                            );
                        }
                        Ok(())
                    }
                    0x0034 => {
                        // PostHighLevelEvent (0xA88F selector $0034)
                        // Sends a high-level event to another application.
                        // FUNCTION PostHighLevelEvent(theEvent: EventRecord; receiverID: Ptr;
                        //   msgRefcon: LongInt; msgBuff: Ptr; msgLen: LongInt;
                        //   postingOptions: LongInt): OSErr;
                        // Inside Macintosh Volume VI, 5-30 and Macintosh Toolbox Essentials 1992, 2-101.
                        write_osdispatch_oseerr_result(cpu, bus, sp + 24, CONNECTION_INVALID_ERR);
                        Ok(())
                    }
                    0x0035 => {
                        // GetProcessSerialNumberFromPortName (0xA88F selector $0035)
                        // Maps a local PPC port name to a process serial number.
                        // FUNCTION GetProcessSerialNumberFromPortName(portName: PPCPortRec;
                        //   VAR PSN: ProcessSerialNumber): OSErr;
                        // Inside Macintosh Volume VI, 5-32 and Macintosh Toolbox Essentials 1992, 2-106.
                        write_osdispatch_oseerr_result(cpu, bus, sp + 8, NO_PORT_ERR);
                        Ok(())
                    }
                    0x0036 => {
                        // LaunchDeskAccessory (0xA88F selector $0036)
                        // Launches a desk accessory from a resource file.
                        // FUNCTION LaunchDeskAccessory(pFileSpec: FSSpecPtr;
                        //   pDAName: StringPtr): OSErr;
                        // Inside Macintosh Volume VI, 29-22..29-23; Processes 1994, 2-30.
                        write_osdispatch_oseerr_result(cpu, bus, sp + 8, RES_NOT_FOUND_ERR);
                        Ok(())
                    }
                    0x0037 => {
                        // GetCurrentProcess (0xA88F selector $0037)
                        // Returns serial number of current process.
                        // FUNCTION GetCurrentProcess(VAR PSN: ProcessSerialNumber): OSErr;
                        // Processes 1994, p. 2-21
                        let psn_ptr = bus.read_long(sp);
                        bus.write_long(psn_ptr, ProcessSerialNumber::CURRENT.high); // highLongOfPSN
                        bus.write_long(psn_ptr + 4, ProcessSerialNumber::CURRENT.low); // lowLongOfPSN = kCurrentProcess
                        bus.write_word(sp + 4, 0); // noErr
                        cpu.write_reg(Register::A7, sp + 4);
                        Ok(())
                    }
                    0x0038 => {
                        // GetNextProcess (0xA88F selector $0038)
                        // Enumerates processes; returns procNotFound at end of list.
                        // FUNCTION GetNextProcess(VAR PSN: ProcessSerialNumber): OSErr;
                        // Processes 1994, p. 2-22
                        let psn_ptr = bus.read_long(sp);
                        let psn = ProcessSerialNumber::new(
                            bus.read_long(psn_ptr),
                            bus.read_long(psn_ptr.wrapping_add(4)),
                        );
                        let err: i16 = match psn.next_single_process() {
                            SingleProcessEnumeration::Current(current) => {
                                bus.write_long(psn_ptr, current.high);
                                bus.write_long(psn_ptr.wrapping_add(4), current.low);
                                0
                            }
                            SingleProcessEnumeration::End => {
                                bus.write_long(psn_ptr, ProcessSerialNumber::CURRENT.high);
                                bus.write_long(psn_ptr.wrapping_add(4), 0);
                                -600
                            }
                            SingleProcessEnumeration::Invalid => -50,
                        };
                        bus.write_word(sp + 4, err as u16);
                        cpu.write_reg(Register::A7, sp + 4);
                        Ok(())
                    }
                    0x0039 => {
                        // GetFrontProcess (0xA88F selector $0039)
                        // Returns foreground process serial number.
                        // FUNCTION GetFrontProcess(VAR PSN: ProcessSerialNumber): OSErr;
                        // Processes 1994, pp. 2-25 to 2-26
                        //
                        // MPW glue (disassembled):
                        //   MOVEQ #-1, D0 / MOVE.L D0, -(SP)  ; 4-byte init slot ($FFFFFFFF)
                        //   PEA <psn>                          ; psn_ptr (4 bytes)
                        //   MOVE.W #$0039, -(SP)               ; selector
                        //   DC.W $A88F / MOVE.W (SP)+, D0      ; trap → result in D0
                        // After selector pop: sp+0=init(4B), sp+4=psn_ptr(4B), sp+8=result(2B).
                        // Inside Macintosh: Processes 1994, pp. 2-25 to 2-26.
                        // Write result word at sp+8; A7 = sp+8 so caller's MOVE.W (SP)+, D0
                        // consumes the result and restores the pre-call stack pointer.
                        let psn_ptr = bus.read_long(sp + 4);
                        bus.write_long(psn_ptr, ProcessSerialNumber::CURRENT.high);
                        bus.write_long(psn_ptr.wrapping_add(4), ProcessSerialNumber::CURRENT.low);
                        bus.write_word(sp + 8, 0);
                        cpu.write_reg(Register::A7, sp + 8);
                        Ok(())
                    }
                    0x003A => {
                        // GetProcessInformation (0xA88F selector $003A)
                        // Returns information about the specified process.
                        // FUNCTION GetProcessInformation(PSN: ProcessSerialNumber;
                        //   VAR info: ProcessInfoRec): OSErr;
                        // Processes 1994, pp. 2-23 to 2-24
                        if selector_from_stack && self.address_is_loaded_code(bus.read_long(sp)) {
                            // Some 68k C runtimes factor the inline selector
                            // into a leaf thunk that pushes only
                            // `MOVE.W #selector,-(SP)` above a JSR return
                            // address. In that shape the selector word is the
                            // only safe OSErr result slot; consuming the
                            // ordinary argument frame would skip the return PC.
                            write_osdispatch_selector_result(cpu, bus, sp_entry, NO_ERR as i16);
                            return Some(Ok(()));
                        }

                        let info_ptr = bus.read_long(sp);
                        let psn_ptr = bus.read_long(sp + 4);
                        let valid_psn = ProcessSerialNumber::new(
                            bus.read_long(psn_ptr),
                            bus.read_long(psn_ptr + 4),
                        )
                        .is_current();

                        if !valid_psn {
                            bus.write_word(sp + 8, (-600i16) as u16); // procNotFound
                            cpu.write_reg(Register::A7, sp + 8);
                            return Some(Ok(()));
                        }

                        let app = self.current_process_application_metadata();
                        let app_name = super::TrapDispatcher::hfs_name_from_vfs_component(
                            super::TrapDispatcher::vfs_basename(&app.path),
                        );

                        let name_ptr = bus.read_long(info_ptr + 4);
                        if name_ptr != 0 {
                            Self::write_pstring(bus, name_ptr, &app_name);
                        }

                        let app_spec_ptr = bus.read_long(info_ptr + 56);
                        if app_spec_ptr != 0 {
                            bus.write_word(
                                app_spec_ptr,
                                super::TrapDispatcher::boot_volume_ref_num_u16(),
                            );
                            bus.write_long(app_spec_ptr + 2, app.parent_dir_id);
                            Self::write_pstring(bus, app_spec_ptr + 6, &app_name);
                        }

                        bus.write_long(info_ptr + 8, ProcessSerialNumber::CURRENT.high); // processNumber.highLongOfPSN
                        bus.write_long(info_ptr + 12, ProcessSerialNumber::CURRENT.low); // processNumber.lowLongOfPSN
                        bus.write_long(info_ptr + 16, app.file_type);
                        bus.write_long(info_ptr + 20, app.creator);
                        bus.write_long(info_ptr + 24, 0); // processMode
                        let app_zone = bus.read_long(crate::memory::globals::addr::APP_L_ZONE);
                        let appl_limit = bus.read_long(crate::memory::globals::addr::APPL_LIMIT);
                        let process_location = if app_zone != 0 { app_zone } else { 0x0010_0000 };
                        let process_free_mem = if app_zone != 0 && appl_limit > app_zone {
                            crate::memory::app_heap_free_bytes(bus)
                        } else {
                            bus.ram_size() / 2
                        };
                        bus.write_long(info_ptr + 28, process_location); // processLocation
                        bus.write_long(info_ptr + 32, crate::memory::app_partition_size_bytes(bus)); // processSize
                        bus.write_long(info_ptr + 36, process_free_mem); // processFreeMem
                        bus.write_long(info_ptr + 40, 0); // processLauncher.highLongOfPSN
                        bus.write_long(info_ptr + 44, 0); // processLauncher.lowLongOfPSN
                        bus.write_long(info_ptr + 48, 0); // processLaunchDate
                        bus.write_long(info_ptr + 52, bus.read_long(0x016A)); // processActiveTime

                        bus.write_word(sp + 8, 0); // noErr
                        cpu.write_reg(Register::A7, sp + 8);
                        Ok(())
                    }
                    0x003B => {
                        // SetFrontProcess (OSDispatch $003B)
                        // FUNCTION SetFrontProcess(PSN: ProcessSerialNumber): OSErr;
                        // Processes 1994, 2-26. The HLE runs a single app
                        // and cannot switch foreground processes; report
                        // success unconditionally.
                        bus.write_word(sp + 4, 0);
                        cpu.write_reg(Register::A7, sp + 4);
                        Ok(())
                    }
                    0x003C => {
                        // WakeUpProcess (OSDispatch $003C)
                        // Makes a suspended process eligible to receive CPU time.
                        // FUNCTION WakeUpProcess(PSN: ProcessSerialNumber): OSErr;
                        // Processes 1994, 2-27
                        // PSN is passed as const pointer (4 bytes), result is OSErr (2 bytes)
                        bus.write_word(sp + 4, 0); // noErr
                        cpu.write_reg(Register::A7, sp + 4);
                        Ok(())
                    }
                    0x003D => {
                        // SameProcess (OSDispatch $003D)
                        // FUNCTION SameProcess(PSN1, PSN2: ProcessSerialNumber;
                        //   VAR result: BOOLEAN): OSErr;
                        // Processes 1994, 2-28. Compares two PSN records;
                        // sets result and returns noErr.
                        let result_ptr = bus.read_long(sp);
                        let psn2_ptr = bus.read_long(sp + 4);
                        let psn1_ptr = bus.read_long(sp + 8);
                        let psn1 = ProcessSerialNumber::new(
                            bus.read_long(psn1_ptr),
                            bus.read_long(psn1_ptr + 4),
                        );
                        let psn2 = ProcessSerialNumber::new(
                            bus.read_long(psn2_ptr),
                            bus.read_long(psn2_ptr + 4),
                        );
                        let same = psn1 == psn2;
                        if result_ptr != 0 {
                            bus.write_byte(result_ptr, if same { 1 } else { 0 });
                        }
                        bus.write_word(sp + 12, 0);
                        cpu.write_reg(Register::A7, sp + 12);
                        Ok(())
                    }
                    0x0045 => {
                        // GetSpecificHighLevelEvent (0xA88F selector $0045)
                        // Searches the application's high-level-event queue with a
                        // caller filter and returns FALSE when no event is selected.
                        // FUNCTION GetSpecificHighLevelEvent
                        //   (aFilter: GetSpecificFilterProcPtr; yourDataPtr: UNIV Ptr;
                        //    VAR err: OSErr): Boolean;
                        // Macintosh Toolbox Essentials 1992, pp. 2-92 to 2-93.
                        //
                        // MPW 3.5 EPPC.h emits THREEWORDINLINE($3F3C,$0045,$A88F).
                        // Its 68k caller frame is filter(4), context(4), err*(4),
                        // Boolean result(1). Systemless has no registered PPC/HLE
                        // port, matching BasiliskII's noPortErr branch for a direct
                        // app without the high-level-event SIZE flag.
                        let err_ptr = bus.read_long(sp + 8);
                        if err_ptr != 0 {
                            bus.write_word(err_ptr, NO_PORT_ERR as u16);
                        }
                        bus.write_byte(sp + 12, 0);
                        cpu.write_reg(Register::D0, 0);
                        cpu.write_reg(Register::A7, sp + 12);
                        Ok(())
                    }
                    _ => {
                        eprintln!(
                            "[PROCESS] OSDispatch unhandled selector ${:04X} (stack=${:04X} d0=${:04X})",
                            selector, stack_sel, d0_sel
                        );
                        return None;
                    }
                }
            }

            // HighLevelFSDispatch ($AA52)
            // Dispatches high-level File Manager FSSpec routines selected in D0.
            // FUNCTION FSMakeFSSpec(vRefNum: INTEGER; dirID: LONGINT; fileName: Str255; VAR spec: FSSpec): OSErr;
            // Inside Macintosh: Files (1992), pp. 2-154 to 2-168.
            (true, 0x252) => {
                let raw_selector = cpu.read_reg(Register::D0);
                let operation =
                    high_level_fs_dispatch_operation_route(self.current_trap_word, raw_selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                let selector = raw_selector & 0xFFFF;
                eprintln!("[TRAP] HighLevelFSDispatch Selector={}", selector);
                match selector {
                    1 => {
                        // FSMakeFSSpec ($AA52, selector 1)
                        // Fills in an FSSpec record. Returns fnfErr if file doesn't exist.
                        // FUNCTION FSMakeFSSpec(vRefNum: INTEGER; dirID: LONGINT;
                        //   fileName: Str255; VAR spec: FSSpec): OSErr;
                        // Files 1992, 2-166
                        let sp = cpu.read_reg(Register::A7);
                        let spec_ptr = bus.read_long(sp);
                        let name_ptr = bus.read_long(sp + 4);
                        let dir_id = bus.read_long(sp + 8);
                        let requested_v_ref_num = bus.read_word(sp + 12) as i16;
                        let (resolved_v_ref_num, effective_dir_id) =
                            self.resolve_volume_and_directory(requested_v_ref_num, dir_id);

                        let requested_name = if name_ptr != 0 {
                            let bytes = bus.read_pstring(name_ptr);
                            decode_mac_roman(&bytes)
                        } else {
                            String::new()
                        };

                        let (spec_v_ref_num, spec_dir_id, spec_name, target_key) = if requested_name
                            .is_empty()
                        {
                            (
                                resolved_v_ref_num,
                                effective_dir_id,
                                String::new(),
                                self.directory_path_for_id(effective_dir_id)
                                    .map(str::to_string),
                            )
                        } else if let Some((target_vref, parent_dir_id, basename, key)) = self
                            .fsspec_parts_for_hfs_path(requested_v_ref_num, dir_id, &requested_name)
                        {
                            (target_vref, parent_dir_id, basename, Some(key))
                        } else {
                            // dirNFErr when the resolved dirID or pathname parent is not
                            // a known directory. IM:Files 1992 p. 2-34 notes that
                            // FSMakeFSSpec can return File Manager errors other than
                            // noErr/fnfErr and clears the FSSpec in that case.
                            if trace_fsspec_enabled() {
                                eprintln!(
                                    "[FSSPEC] FSMakeFSSpec dirNFErr request_vRefNum={} resolved_vRefNum={} request_dirID={} resolved_dirID={} name='{}'",
                                    requested_v_ref_num,
                                    resolved_v_ref_num,
                                    dir_id,
                                    effective_dir_id,
                                    requested_name
                                );
                            }
                            bus.write_word(spec_ptr, 0);
                            bus.write_long(spec_ptr + 2, 0);
                            bus.write_byte(spec_ptr + 6, 0);
                            bus.write_word(sp + 14, (-120i16) as u16); // dirNFErr
                            cpu.write_reg(Register::A7, sp + 14);
                            return Some(Ok(()));
                        };

                        bus.write_word(spec_ptr, spec_v_ref_num as u16);
                        bus.write_long(spec_ptr + 2, spec_dir_id);
                        let spec_name_bytes = encode_mac_roman_lossy(&spec_name);
                        let n = spec_name_bytes.len().min(63);
                        bus.write_byte(spec_ptr + 6, n as u8);
                        bus.write_bytes(spec_ptr + 7, &spec_name_bytes[..n]);

                        // FSMakeFSSpec returns fnfErr if the target doesn't exist while
                        // still filling a valid FSSpec. Files 1992, 2-34 to 2-35.
                        let exists = if requested_name.is_empty() {
                            self.directory_entry_for_id(spec_dir_id).is_some()
                        } else if let Some(ref key) = target_key {
                            self.vfs.keys().any(|path| path.eq_ignore_ascii_case(key))
                                || self
                                    .vfs_rsrc
                                    .keys()
                                    .any(|path| path.eq_ignore_ascii_case(key))
                                || self
                                    .vfs_directories
                                    .iter()
                                    .any(|directory| directory.path.eq_ignore_ascii_case(key))
                        } else {
                            false
                        };
                        eprintln!(
                            "[FSSPEC] FSMakeFSSpec request_vRefNum={} resolved_vRefNum={} request_dirID={} resolved_dirID={} name='{}' specDirID={} specName='{}' exists={}",
                            requested_v_ref_num,
                            resolved_v_ref_num,
                            dir_id,
                            effective_dir_id,
                            requested_name,
                            spec_dir_id,
                            spec_name,
                            exists
                        );
                        if trace_fsspec_enabled() && requested_name.is_empty() {
                            eprintln!(
                                "[FSSPEC] FSMakeFSSpec created directory spec for dirID={}",
                                spec_dir_id
                            );
                        }
                        if exists {
                            bus.write_word(sp + 14, 0); // noErr
                        } else {
                            bus.write_word(sp + 14, (-43i16) as u16); // fnfErr
                        }
                        cpu.write_reg(Register::A7, sp + 14);
                        Ok(())
                    }
                    2 => {
                        // FSpOpenDF ($AA52, selector 2)
                        // Opens the data fork of a file specified by an FSSpec.
                        // FUNCTION FSpOpenDF(spec: FSSpec; permission: SignedByte;
                        //   VAR refNum: INTEGER): OSErr;
                        // Files 1992, 2-326 (FSpOpenDF subsection lines 2342..2380;
                        //   trap macro / routine selector at lines 2818..2823;
                        //   result codes 2369..2380 with fnfErr -43 at line 2376).
                        //
                        // Behavioral note: real BasiliskII writes to *refNum
                        // on the fnfErr (-43) failure path while Systemless
                        // leaves the VAR-out untouched.
                        let sp = cpu.read_reg(Register::A7);
                        let ref_num_ptr = bus.read_long(sp);
                        // `permission` is a SignedByte. MPW SC emits a
                        // byte push for FSpOpenDF, while A7 still reserves a
                        // stack word; some callers leave nonzero padding in
                        // the other byte.
                        let permission = file_permission_from_stack_word(bus.read_word(sp + 4));
                        let spec_ptr = bus.read_long(sp + 6);

                        let filename = read_fsspec_name(bus, spec_ptr);
                        eprintln!(
                            "[TRAP] FSpOpenDF(\"{}\", perm={}) ref_num_ptr=${:08X}",
                            filename, permission, ref_num_ptr
                        );

                        let vref = bus.read_word(spec_ptr) as i16;
                        let dir_id = bus.read_long(spec_ptr + 2);
                        if let Some(vfs_name) =
                            self.find_vfs_file_for_hfs_lookup(vref, dir_id, &filename)
                        {
                            // IM:Files 1992, 2-326: fsWrPerm=2,
                            // fsRdWrPerm=3, fsRdWrShPerm=4. Systemless keeps
                            // the BasiliskII/extfs noErr open behavior, but
                            // still records which access paths requested write
                            // permission.
                            let read_only = self.vfs_path_is_read_only(&vfs_name);
                            let wants_write = matches!(permission, 2 | 3 | 4);
                            if wants_write && read_only {
                                bus.write_word(sp + 10, (-44i16) as u16); // wPrErr
                                cpu.write_reg(Register::A7, sp + 10);
                                return Some(Ok(()));
                            }

                            let refnum = self.allocate_process_file_refnum();
                            self.open_files.insert(refnum, vfs_name.clone());
                            if wants_write && !read_only {
                                self.write_refnums.insert(refnum);
                            }
                            self.file_positions.insert(refnum, 0);
                            eprintln!("[TRAP] FSpOpenDF -> refnum={} vfs=\"{}\"", refnum, vfs_name);

                            bus.write_word(ref_num_ptr, refnum);
                            bus.write_word(sp + 10, 0); // noErr
                            cpu.write_reg(Register::A7, sp + 10);
                        } else {
                            eprintln!("[TRAP] FSpOpenDF(\"{}\") -> fnfErr", filename);
                            bus.write_word(sp + 10, (-43i16) as u16); // fnfErr
                            cpu.write_reg(Register::A7, sp + 10);
                        }
                        Ok(())
                    }
                    3 => {
                        // FSpOpenRF ($AA52, selector 3)
                        // Opens the resource fork of a file specified by an FSSpec.
                        // FUNCTION FSpOpenRF(spec: FSSpec; permission: SignedByte;
                        //   VAR refNum: INTEGER): OSErr;
                        // Files 1992, 2-327..2-329.
                        let sp = cpu.read_reg(Register::A7);
                        let ref_num_ptr = bus.read_long(sp);
                        let permission = file_permission_from_stack_word(bus.read_word(sp + 4));
                        let spec_ptr = bus.read_long(sp + 6);
                        let filename = read_fsspec_name(bus, spec_ptr);
                        let vref = bus.read_word(spec_ptr) as i16;
                        let dir_id = bus.read_long(spec_ptr + 2);
                        let wants_write = matches!(permission, 2 | 3 | 4);
                        eprintln!(
                            "[TRAP] FSpOpenRF(\"{}\", perm={}) ref_num_ptr=${:08X}",
                            filename, permission, ref_num_ptr
                        );

                        if vref != 0 && self.working_directory_info(vref).is_none() {
                            eprintln!(
                                "[TRAP] FSpOpenRF(\"{}\") -> nsvErr (vRefNum={})",
                                filename, vref
                            );
                            bus.write_word(sp + 10, (-35i16) as u16); // nsvErr
                            cpu.write_reg(Register::A7, sp + 10);
                            return Some(Ok(()));
                        }

                        let Some(vfs_key) =
                            self.find_vfs_rsrc_file_for_hfs_lookup(vref, dir_id, &filename)
                        else {
                            eprintln!("[TRAP] FSpOpenRF(\"{}\") -> fnfErr", filename);
                            bus.write_word(sp + 10, (-43i16) as u16); // fnfErr
                            cpu.write_reg(Register::A7, sp + 10);
                            return Some(Ok(()));
                        };

                        if wants_write && self.vfs_path_is_read_only(&vfs_key) {
                            bus.write_word(sp + 10, (-44i16) as u16); // wPrErr
                            cpu.write_reg(Register::A7, sp + 10);
                            return Some(Ok(()));
                        }

                        let rsrc_data = self.vfs_rsrc.get(&vfs_key).cloned().unwrap_or_default();
                        let rsrc_key = format!("__rsrc__{vfs_key}");
                        self.vfs.insert_if_absent(rsrc_key.clone(), rsrc_data);
                        let refnum = self.allocate_process_file_refnum();
                        self.open_files.insert(refnum, rsrc_key.clone());
                        if wants_write {
                            self.write_refnums.insert(refnum);
                        }
                        self.file_positions.insert(refnum, 0);
                        eprintln!("[TRAP] FSpOpenRF -> refnum={} vfs=\"{}\"", refnum, rsrc_key);
                        bus.write_word(ref_num_ptr, refnum);
                        bus.write_word(sp + 10, 0); // noErr
                        cpu.write_reg(Register::A7, sp + 10);
                        Ok(())
                    }
                    4 => {
                        // FSpCreate ($AA52, selector 4)
                        // Creates a new file (both forks) by FSSpec.
                        // FUNCTION FSpCreate(spec: FSSpec; creator: OSType;
                        //   fileType: OSType; scriptTag: ScriptCode): OSErr;
                        // Files 1992, 8480..8531 (function 8484; trap macro /
                        // selector 8510..8514; result codes 8517..8531 with
                        // dirNFErr at 8529 and dupFNErr at 8528).
                        //
                        // Returns dupFNErr (-48) if a case-insensitive
                        // match already exists in VFS, noErr otherwise.
                        // IM:Files 8528 (dupFNErr result code).
                        let sp = cpu.read_reg(Register::A7);
                        let spec_ptr = bus.read_long(sp + 10);
                        let file_type = bus.read_long(sp + 2);
                        let creator = bus.read_long(sp + 6);

                        // Phantom parID → BasiliskII returns dupFNErr (-48).
                        // IM:Files 8529 documents dirNFErr (-120) for a missing parent
                        // directory; however BasiliskII (extfs over HFS) empirically
                        // returns -48 (dupFNErr) for phantom parIDs. BasiliskII is
                        // ground truth per the reference gate.
                        // IM:Files 1992, lines 8510..8531
                        let filename = read_fsspec_name(bus, spec_ptr);
                        let vref = bus.read_word(spec_ptr) as i16;
                        let dir_id = bus.read_long(spec_ptr + 2);
                        if trace_fsspec_enabled() {
                            eprintln!(
                                "[FSSPEC] FSpCreateResFile file='{}' vref={} dir_id={}",
                                filename, vref, dir_id
                            );
                        }
                        let par_id = dir_id;
                        if par_id > 1 && self.directory_path_for_id(par_id).is_none() {
                            if trace_fsspec_enabled() {
                                eprintln!(
                                    "[FSSPEC] FSpCreateResFile rejecting par_id={} for file='{}'",
                                    par_id, filename
                                );
                            }
                            bus.write_word(sp + 14, (-48i16) as u16); // dupFNErr
                            cpu.write_reg(Register::A7, sp + 14);
                            return Some(Ok(()));
                        }

                        let vfs_key = self
                            .vfs_key_for_fsspec(vref, dir_id, &filename)
                            .unwrap_or_else(|| {
                                super::TrapDispatcher::normalize_hfs_path(&filename)
                            });
                        if trace_fsspec_enabled() {
                            eprintln!(
                                "[FSSPEC] FSpCreateResFile file='{}' vref={} dir_id={} vfs_key='{}'",
                                filename, vref, dir_id, vfs_key
                            );
                        }

                        if self.vfs_path_is_read_only(&vfs_key) {
                            bus.write_word(sp + 14, (-44i16) as u16); // wPrErr
                            cpu.write_reg(Register::A7, sp + 14);
                            return Some(Ok(()));
                        }

                        // dupFNErr (-48): file already exists in VFS.
                        // IM:Files 8528; matches BasiliskII empirical behaviour.
                        let already_exists = self.vfs.contains_key(&vfs_key)
                            || self.vfs.keys().any(|k| k.eq_ignore_ascii_case(&vfs_key));

                        if already_exists {
                            bus.write_word(sp + 14, (-48i16) as u16); // dupFNErr
                        } else {
                            self.vfs.insert(vfs_key.clone(), Vec::new());
                            self.vfs_rsrc.ensure_empty(vfs_key.clone());
                            self.set_vfs_entry_finfo(&vfs_key, file_type, creator, 0);
                            self.touch_vfs_entry(&vfs_key);

                            if let Some(ref dir) = self.output_dir {
                                let host_path = dir.join(&vfs_key);
                                let _ = std::fs::File::create(&host_path);
                            }

                            bus.write_word(sp + 14, 0); // noErr
                        }
                        cpu.write_reg(Register::A7, sp + 14);
                        Ok(())
                    }
                    5 => {
                        // FSpDirCreate ($AA52, selector 5)
                        // Creates a directory specified by an FSSpec and
                        // returns its directory ID.
                        // FUNCTION FSpDirCreate(spec: FSSpec; scriptTag: ScriptCode;
                        //   VAR createdDirID: LongInt): OSErr;
                        // Files 1992, 2-158
                        let sp = cpu.read_reg(Register::A7);
                        let created_dir_id_ptr = bus.read_long(sp);
                        let spec_ptr = bus.read_long(sp + 6);
                        let filename = read_fsspec_name(bus, spec_ptr);
                        let vref = bus.read_word(spec_ptr) as i16;
                        let dir_id = bus.read_long(spec_ptr + 2);

                        let result = if filename.is_empty() {
                            -37i16 // bdNamErr
                        } else {
                            let parent_dir_id = self.resolve_directory_id(vref, dir_id);
                            let Some(parent_path) = self
                                .directory_path_for_id(parent_dir_id)
                                .map(str::to_string)
                            else {
                                bus.write_word(sp + 10, (-120i16) as u16); // dirNFErr
                                cpu.write_reg(Register::A7, sp + 10);
                                return Some(Ok(()));
                            };
                            if self.vfs_path_is_read_only(&parent_path) {
                                bus.write_word(sp + 10, (-44i16) as u16); // wPrErr
                                cpu.write_reg(Register::A7, sp + 10);
                                return Some(Ok(()));
                            }
                            let child_name = super::TrapDispatcher::normalize_hfs_path(&filename);
                            if child_name.is_empty() {
                                -37i16 // bdNamErr
                            } else {
                                let child_path = if parent_path.is_empty() {
                                    child_name
                                } else {
                                    format!("{parent_path}/{child_name}")
                                };
                                let duplicate = self.vfs_directories.iter().any(|directory| {
                                    directory.path.eq_ignore_ascii_case(&child_path)
                                }) || self
                                    .vfs
                                    .keys()
                                    .any(|path| path.eq_ignore_ascii_case(&child_path))
                                    || self
                                        .vfs_rsrc
                                        .keys()
                                        .any(|path| path.eq_ignore_ascii_case(&child_path));
                                if duplicate {
                                    -48i16 // dupFNErr
                                } else {
                                    let new_dir_id = self.ensure_vfs_directory(&child_path);
                                    if created_dir_id_ptr != 0 {
                                        bus.write_long(created_dir_id_ptr, new_dir_id);
                                    }
                                    if let Some(ref dir) = self.output_dir {
                                        let _ = std::fs::create_dir_all(dir.join(&child_path));
                                    }
                                    eprintln!(
                                        "[TRAP] FSpDirCreate(\"{}\") parentDirID={} -> dirID={}",
                                        filename, parent_dir_id, new_dir_id
                                    );
                                    0
                                }
                            }
                        };
                        bus.write_word(sp + 10, result as u16);
                        cpu.write_reg(Register::A7, sp + 10);
                        Ok(())
                    }
                    6 => {
                        // FSpDelete ($AA52, selector 6)
                        // Removes a file or directory specified by an FSSpec.
                        // FUNCTION FSpDelete(spec: FSSpec): OSErr;
                        // Files 1992, 2-329; FSpDelete subsection at IM:Files
                        // 8576..8610 (function signature 8580; trap macro/
                        // selector 8590..8594; result codes 8597..8610 with
                        // fnfErr at 8603, fBsyErr at 8607, dirNFErr at 8608).
                        //
                        // Behavioral note: on the delete-while-open path,
                        // BasiliskII on Unix deletes the open file with noErr,
                        // and a second FSpDelete returns fnfErr -43.
                        let sp = cpu.read_reg(Register::A7);
                        let spec_ptr = bus.read_long(sp);

                        let filename = read_fsspec_name(bus, spec_ptr);
                        let vref = bus.read_word(spec_ptr) as i16;
                        let dir_id = bus.read_long(spec_ptr + 2);
                        let mut read_only = false;
                        let found_in_vfs = if let Some(vfs_name) =
                            self.find_vfs_file_for_hfs_lookup(vref, dir_id, &filename)
                        {
                            read_only = self.vfs_path_is_read_only(&vfs_name);
                            if read_only {
                                false
                            } else {
                                self.vfs.remove(&vfs_name);
                                self.vfs_rsrc.remove(&vfs_name);
                                self.remove_vfs_entry_metadata(&vfs_name);
                                true
                            }
                        } else if let Some(vfs_name) = self.find_vfs_file(&filename) {
                            read_only = self.vfs_path_is_read_only(&vfs_name);
                            if read_only {
                                false
                            } else {
                                self.vfs.remove(&vfs_name);
                                self.vfs_rsrc.remove(&vfs_name);
                                self.remove_vfs_entry_metadata(&vfs_name);
                                true
                            }
                        } else {
                            false
                        };

                        let host_vfs_key = self.vfs_key_for_fsspec(vref, dir_id, &filename);
                        let found_on_host = if !read_only {
                            if let Some(ref dir) = self.output_dir {
                                let host_path = host_vfs_key
                                    .map(|key| dir.join(key))
                                    .unwrap_or_else(|| dir.join(&filename));
                                std::fs::remove_file(&host_path).is_ok()
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        // fnfErr (-43) when file not found anywhere.
                        // IM:Files 8603: result code fnfErr.
                        let err: i16 = if read_only {
                            -44 // wPrErr
                        } else if found_in_vfs || found_on_host {
                            0
                        } else {
                            -43
                        };
                        bus.write_word(sp + 4, err as u16);
                        cpu.write_reg(Register::A7, sp + 4);
                        Ok(())
                    }
                    7 => {
                        // FSpGetFInfo ($AA52, selector 7)
                        // Returns Finder information for a file specified by an FSSpec.
                        // FUNCTION FSpGetFInfo(spec: FSSpec; VAR fndrInfo: FInfo): OSErr;
                        // Files 1992, 2-327; FSpGetFInfo subsection at IM:Files 8616..8650
                        // (function signature 8620; trap macro/selector 8630..8631; result
                        // codes 8643..8650 with fnfErr at 8645).
                        //
                        // The missing-file path returns fnfErr (-43) and the
                        // companion FSMakeFSSpec dispatch populates the spec.
                        //
                        // BasiliskII mutates the caller's `fndrInfo` buffer on the
                        // fnfErr branch instead of leaving the pre-poisoned bytes
                        // untouched. Mirror that observed mutation here so callers do
                        // not read stale poison after a missing-file probe.
                        let sp = cpu.read_reg(Register::A7);
                        let finfo_ptr = bus.read_long(sp);
                        let spec_ptr = bus.read_long(sp + 4);
                        let filename = read_fsspec_name(bus, spec_ptr);
                        eprintln!("[TRAP] FSpGetFInfo file='{}'", filename);

                        let vref = bus.read_word(spec_ptr) as i16;
                        let dir_id = bus.read_long(spec_ptr + 2);
                        let vfs_name = self
                            .find_vfs_file_for_hfs_lookup(vref, dir_id, &filename)
                            .or_else(|| {
                                self.find_vfs_rsrc_file_for_hfs_lookup(vref, dir_id, &filename)
                            });
                        let directory_name =
                            self.find_vfs_directory_for_hfs_lookup(vref, dir_id, &filename);

                        if directory_name.is_some() {
                            // Directory specs are valid for FSpGetFInfo.
                            // Finder type/creator for directories follows
                            // the same canonical values used by PBGetCatInfo.
                            Self::write_finfo(
                                bus,
                                finfo_ptr,
                                u32::from_be_bytes(*b"fold"),
                                u32::from_be_bytes(*b"MACS"),
                                0,
                            );
                            bus.write_word(sp + 8, 0); // noErr
                        } else if let Some(vfs_name) = vfs_name {
                            if let Some(metadata) = self.vfs_file_metadata(&vfs_name) {
                                Self::write_finfo(
                                    bus,
                                    finfo_ptr,
                                    metadata.file_type,
                                    metadata.creator,
                                    metadata.finder_flags,
                                );
                            }
                            bus.write_word(sp + 8, 0); // noErr
                        } else {
                            eprintln!("[TRAP] FSpGetFInfo(\"{}\") -> fnfErr", filename);
                            Self::write_finfo(bus, finfo_ptr, 0, 0, 0);
                            bus.write_word(sp + 8, (-43i16) as u16); // fnfErr
                        }
                        cpu.write_reg(Register::A7, sp + 8);
                        Ok(())
                    }
                    8 => {
                        // FSpSetFInfo (selector 8)
                        // FUNCTION FSpSetFInfo(spec: FSSpec; fndrInfo: FInfo): OSErr;
                        // Stack: [result(2)] [fndrInfo_ptr(4)] [spec_ptr(4)] — pop 8, leave 2
                        // More Macintosh Toolbox, 1-89
                        let sp = cpu.read_reg(Register::A7);
                        let finfo_ptr = bus.read_long(sp);
                        let spec_ptr = bus.read_long(sp + 4);
                        let filename = read_fsspec_name(bus, spec_ptr);
                        let vref = bus.read_word(spec_ptr) as i16;
                        let dir_id = bus.read_long(spec_ptr + 2);
                        let result = if let Some(vfs_name) =
                            self.find_vfs_file_for_hfs_lookup(vref, dir_id, &filename)
                        {
                            if self.vfs_path_is_read_only(&vfs_name) {
                                -44i16 // wPrErr
                            } else {
                                let file_type = bus.read_long(finfo_ptr);
                                let creator = bus.read_long(finfo_ptr + 4);
                                let finder_flags = bus.read_word(finfo_ptr + 8);
                                self.set_vfs_entry_finfo(
                                    &vfs_name,
                                    file_type,
                                    creator,
                                    finder_flags,
                                );
                                0
                            }
                        } else {
                            0 // Preserve the existing no-op behavior for a missing file.
                        };
                        bus.write_word(sp + 8, result as u16);
                        cpu.write_reg(Register::A7, sp + 8);
                        Ok(())
                    }
                    9 => {
                        // FSpSetFLock ($AA52, selector 9)
                        // Locks a file — new access paths become read-only.
                        // FUNCTION FSpSetFLock(spec: FSSpec): OSErr;
                        // Inside Macintosh: Files 1992, 8681..8713
                        let sp = cpu.read_reg(Register::A7);
                        let spec_ptr = bus.read_long(sp);
                        let filename = read_fsspec_name(bus, spec_ptr);
                        let vref = bus.read_word(spec_ptr) as i16;
                        let dir_id = bus.read_long(spec_ptr + 2);
                        let err: i16 = if let Some(vfs_name) =
                            self.find_vfs_file_for_hfs_lookup(vref, dir_id, &filename)
                        {
                            if self.vfs_path_is_read_only(&vfs_name) {
                                -44
                            } else {
                                self.locked_files.insert(vfs_name);
                                0
                            }
                        } else if let Some(vfs_name) = self.find_vfs_file(&filename) {
                            if self.vfs_path_is_read_only(&vfs_name) {
                                -44
                            } else {
                                self.locked_files.insert(vfs_name);
                                0
                            }
                        } else {
                            -43 // fnfErr
                        };
                        eprintln!("[TRAP] FSpSetFLock(\"{}\") -> {}", filename, err);
                        bus.write_word(sp + 4, err as u16);
                        cpu.write_reg(Register::A7, sp + 4);
                        Ok(())
                    }
                    10 => {
                        // FSpRstFLock ($AA52, selector 10)
                        // Unlocks a file.
                        // FUNCTION FSpRstFLock(spec: FSSpec): OSErr;
                        // Inside Macintosh: Files 1992, 8716..8748
                        let sp = cpu.read_reg(Register::A7);
                        let spec_ptr = bus.read_long(sp);
                        let filename = read_fsspec_name(bus, spec_ptr);
                        let vref = bus.read_word(spec_ptr) as i16;
                        let dir_id = bus.read_long(spec_ptr + 2);
                        let err: i16 = if let Some(vfs_name) =
                            self.find_vfs_file_for_hfs_lookup(vref, dir_id, &filename)
                        {
                            if self.vfs_path_is_read_only(&vfs_name) {
                                -44
                            } else {
                                self.locked_files.remove(&vfs_name);
                                0
                            }
                        } else if let Some(vfs_name) = self.find_vfs_file(&filename) {
                            if self.vfs_path_is_read_only(&vfs_name) {
                                -44
                            } else {
                                self.locked_files.remove(&vfs_name);
                                0
                            }
                        } else {
                            0
                        };
                        eprintln!("[TRAP] FSpRstFLock(\"{}\")", filename);
                        bus.write_word(sp + 4, err as u16);
                        cpu.write_reg(Register::A7, sp + 4);
                        Ok(())
                    }
                    13 => {
                        // FSpOpenResFile (selector 13 = $000D)
                        // FUNCTION FSpOpenResFile(spec: FSSpec; permission: SignedByte): INTEGER;
                        // Inside Macintosh Volume VI, 9-43; More Macintosh Toolbox, 1-58
                        // Stack (rightmost on top): [permission(2)] [spec_ptr(4)] [result(2)]
                        let sp = cpu.read_reg(Register::A7);
                        let permission = signed_byte_from_stack_word(bus.read_word(sp));
                        let wants_write = permission == 2 || permission == 3;
                        let spec_ptr = bus.read_long(sp + 2);
                        let filename = read_fsspec_name(bus, spec_ptr);
                        if super::dispatch::trace_resfile_enabled() {
                            eprintln!("[TRAP] FSpOpenResFile file='{}'", filename);
                        }

                        let vref = bus.read_word(spec_ptr) as i16;
                        let dir_id = bus.read_long(spec_ptr + 2);
                        let rsrc_key =
                            self.find_vfs_rsrc_file_for_hfs_lookup(vref, dir_id, &filename);
                        let data_key = self.find_vfs_file_for_hfs_lookup(vref, dir_id, &filename);
                        if trace_fsspec_enabled() {
                            eprintln!(
                                "[FSSPEC] FSpOpenResFile file='{}' rsrc_match={:?} data_match={:?}",
                                filename, rsrc_key, data_key
                            );
                        }

                        if let Some(vfs_key) = rsrc_key {
                            if wants_write && self.vfs_path_is_read_only(&vfs_key) {
                                bus.write_word(sp + 6, (-44i16) as u16); // wPrErr
                                cpu.write_reg(Register::D0, (-44i32) as u32);
                                cpu.write_reg(Register::A7, sp + 6);
                                return Some(Ok(()));
                            }
                            // Dedupe: re-opening the same fork must reuse
                            // the existing refnum, not re-allocate every
                            // resource, and must not change CurResFile.
                            // (See OpenRFPerm in toolbox.rs.)
                            if let Some(existing) = self.refnum_for_resource_file_name(&vfs_key) {
                                if wants_write {
                                    self.write_refnums.insert(existing);
                                }
                                if super::dispatch::trace_resfile_enabled() {
                                    eprintln!(
                                        "[TRAP] FSpOpenResFile: \"{}\" already open as refnum {}, dedup",
                                        filename, existing
                                    );
                                }
                                bus.write_word(sp + 6, existing);
                                cpu.write_reg(Register::D0, existing as u32);
                                bus.write_word(0x0A60, 0); // ResErr = noErr
                                cpu.write_reg(Register::A7, sp + 6);
                                return Some(Ok(()));
                            }
                            let refnum =
                                self.open_resource_file_from_vfs_key(bus, &vfs_key, wants_write);
                            bus.write_word(sp + 6, refnum);
                            cpu.write_reg(Register::D0, refnum as u32);
                        } else {
                            // FSpOpenResFile failure semantics:
                            // - If the file exists but has no resource fork, return
                            //   resFNotFound (-193): "resource file not found".
                            // - If the file itself is missing, return fnfErr (-43).
                            //
                            // IM:Volume VI (1991), Resource Manager 13-19..13-20:
                            // FSpOpenResFile uses HOpenResFile result codes and
                            // returns -1 on failure; those include fnfErr for
                            // missing files. Keep the existing data-only
                            // resFNotFound behavior for compatibility with the
                            // OpenResFile/HOpenResFile path.
                            bus.write_word(sp + 6, (-1i16) as u16);
                            cpu.write_reg(Register::D0, (-1i32) as u32);
                            let res_err: i16 = if data_key.is_some() { -193 } else { -43 };
                            bus.write_word(0x0A60, res_err as u16);
                        }
                        cpu.write_reg(Register::A7, sp + 6);
                        Ok(())
                    }
                    14 => {
                        // FSpCreateResFile (selector 14 = $000E)
                        // PROCEDURE FSpCreateResFile(spec: FSSpec; creator: OSType;
                        //   fileType: OSType; scriptTag: ScriptCode);
                        // Stack: [scriptTag(2)] [fileType(4)] [creator(4)] [spec_ptr(4)] — pop 14
                        // More Macintosh Toolbox, 1-56
                        let sp = cpu.read_reg(Register::A7);
                        let file_type = bus.read_long(sp + 2);
                        let creator = bus.read_long(sp + 6);
                        let spec_ptr = bus.read_long(sp + 10);
                        let filename = read_fsspec_name(bus, spec_ptr);
                        if !filename.is_empty() {
                            let vref = bus.read_word(spec_ptr) as i16;
                            let dir_id = bus.read_long(spec_ptr + 2);
                            let Some(vfs_key) = self.vfs_key_for_fsspec(vref, dir_id, &filename)
                            else {
                                bus.write_word(0x0A60, (-120i16) as u16); // dirNFErr
                                cpu.write_reg(Register::A7, sp + 14);
                                return Some(Ok(()));
                            };
                            if self.vfs_path_is_read_only(&vfs_key) {
                                bus.write_word(0x0A60, (-44i16) as u16); // wPrErr
                                cpu.write_reg(Register::A7, sp + 14);
                                return Some(Ok(()));
                            }
                            self.vfs.ensure_empty(vfs_key.clone());
                            self.vfs_rsrc.ensure_empty(vfs_key.clone());
                            self.set_vfs_entry_finfo(&vfs_key, file_type, creator, 0);
                            self.touch_vfs_entry(&vfs_key);
                            if let Some(ref dir) = self.output_dir {
                                let host_path = dir.join(&vfs_key);
                                if let Some(parent) = host_path.parent() {
                                    let _ = std::fs::create_dir_all(parent);
                                }
                                let _ = std::fs::write(host_path, []);
                            }
                        }
                        bus.write_word(0x0A60, 0); // ResErr = noErr
                        cpu.write_reg(Register::A7, sp + 14);
                        Ok(())
                    }
                    15 => {
                        // FSpExchangeFiles (selector 15 = $000F)
                        // FUNCTION FSpExchangeFiles(source: FSSpec;
                        //   dest: FSSpec): OSErr;
                        // Stack (rightmost on top): [dest_ptr(4)]
                        //   [source_ptr(4)] [result(2)]
                        //
                        // Files 1992, pp. 2-165--2-166: exchange both data
                        // and resource forks, plus the modification dates,
                        // while file identity and catalogue information
                        // (including file ID, parent, name, creation date,
                        // and Finder information) remain attached to each
                        // catalogue entry. Open FCBs must continue to refer
                        // to the same file identity after the exchange.
                        let sp = cpu.read_reg(Register::A7);
                        let dest_spec_ptr = bus.read_long(sp);
                        let source_spec_ptr = bus.read_long(sp + 4);

                        let source_name = read_fsspec_name(bus, source_spec_ptr);
                        let source_vref = bus.read_word(source_spec_ptr) as i16;
                        let source_dir_id = bus.read_long(source_spec_ptr + 2);
                        let dest_name = read_fsspec_name(bus, dest_spec_ptr);
                        let dest_vref = bus.read_word(dest_spec_ptr) as i16;
                        let dest_dir_id = bus.read_long(dest_spec_ptr + 2);

                        let (source_volume, _) =
                            self.resolve_volume_and_directory(source_vref, source_dir_id);
                        let (dest_volume, _) =
                            self.resolve_volume_and_directory(dest_vref, dest_dir_id);

                        let source_key = self
                            .find_vfs_file_for_hfs_lookup(source_vref, source_dir_id, &source_name)
                            .or_else(|| {
                                self.find_vfs_rsrc_file_for_hfs_lookup(
                                    source_vref,
                                    source_dir_id,
                                    &source_name,
                                )
                            });
                        let dest_key = self
                            .find_vfs_file_for_hfs_lookup(dest_vref, dest_dir_id, &dest_name)
                            .or_else(|| {
                                self.find_vfs_rsrc_file_for_hfs_lookup(
                                    dest_vref,
                                    dest_dir_id,
                                    &dest_name,
                                )
                            });

                        let result: i16 = if source_volume != dest_volume {
                            -1303 // diffVolErr
                        } else if source_key.is_none() || dest_key.is_none() {
                            -43 // fnfErr
                        } else {
                            let source_key = source_key.expect("checked source FSSpec");
                            let dest_key = dest_key.expect("checked destination FSSpec");

                            if source_key.eq_ignore_ascii_case(&dest_key) {
                                -5038 // afpSameObjectErr
                            } else if self.vfs_path_is_read_only(&source_key)
                                || self.vfs_path_is_read_only(&dest_key)
                            {
                                -46 // vLckdErr
                            } else if self.locked_files.contains(&source_key)
                                || self.locked_files.contains(&dest_key)
                            {
                                -45 // fLckdErr
                            } else {
                                let source_data = self.vfs.remove(&source_key);
                                let dest_data = self.vfs.remove(&dest_key);
                                if let Some(data) = dest_data {
                                    self.vfs.insert(source_key.clone(), data);
                                }
                                if let Some(data) = source_data {
                                    self.vfs.insert(dest_key.clone(), data);
                                }

                                let source_rsrc = self.vfs_rsrc.remove(&source_key);
                                let dest_rsrc = self.vfs_rsrc.remove(&dest_key);
                                if let Some(rsrc) = dest_rsrc {
                                    self.vfs_rsrc.insert(source_key.clone(), rsrc);
                                }
                                if let Some(rsrc) = source_rsrc {
                                    self.vfs_rsrc.insert(dest_key.clone(), rsrc);
                                }

                                let source_metadata = self.vfs_file_metadata(&source_key);
                                let dest_metadata = self.vfs_file_metadata(&dest_key);
                                if let (Some(source_metadata), Some(dest_metadata)) =
                                    (source_metadata, dest_metadata)
                                {
                                    self.vfs_metadata.update(&source_key, |metadata| {
                                        metadata.modified_date = dest_metadata.modified_date;
                                    });
                                    self.vfs_metadata.update(&dest_key, |metadata| {
                                        metadata.modified_date = source_metadata.modified_date;
                                    });
                                    self.publish_vfs_entry_to_process(&source_key);
                                    self.publish_vfs_entry_to_process(&dest_key);
                                }

                                if let Some(ref dir) = self.output_dir {
                                    for key in [&source_key, &dest_key] {
                                        if let Some(data) = self.vfs.get(key) {
                                            let host_path = dir.join(key);
                                            if let Some(parent) = host_path.parent() {
                                                let _ = std::fs::create_dir_all(parent);
                                            }
                                            let _ = std::fs::write(host_path, data);
                                        }
                                    }
                                }
                                0 // noErr
                            }
                        };

                        eprintln!(
                            "[TRAP] FSpExchangeFiles(\"{}\", \"{}\") -> {}",
                            source_name, dest_name, result
                        );
                        bus.write_word(sp + 8, result as u16);
                        cpu.write_reg(Register::A7, sp + 8);
                        Ok(())
                    }
                    _ => {
                        eprintln!(
                            "[TRAP] HighLevelFSDispatch: Unimplemented Selector {}",
                            selector
                        );
                        Err(Error::Halted)
                    }
                }
            }

            // HFSDispatch ($A260) / FSDispatch ($A060)
            // Dispatches hierarchical File Manager routines selected by D0.
            // A0: parameter block; D0 and ioResult: OSErr.
            // Inside Macintosh: Files (1992), pp. 2-183 to 2-238.
            (false, 0x60) => {
                let raw_selector = cpu.read_reg(Register::D0);
                let operation = hfs_dispatch_operation_route(self.current_trap_word, raw_selector);
                self.current_selector_operation = operation.map(|route| route.operation_id);
                let selector = raw_selector & 0xFFFF;
                let pb = cpu.read_reg(Register::A0);
                if selector == 0x18 {
                    // PBCatSearch ($A260, selector $0018) searches one
                    // volume's complete catalog and returns bounded FSSpec
                    // batches. Files 1992, 2-38 to 2-42 and 2-204 to 2-206.
                    let result = self.perform_cat_search(bus, pb);
                    bus.write_word(pb + 16, result as u16);
                    cpu.write_reg(Register::D0, result as i32 as u32);
                    return Some(Ok(()));
                }
                if selector == 1 {
                    // PBOpenWD ($A260, selector 1)
                    // Creates a working directory for the specified directory.
                    // FUNCTION PBOpenWD(paramBlock: WDPBPtr; async: Boolean): OSErr;
                    // Files 1992, 2-201 to 2-202
                    let name_ptr = bus.read_long(pb + 18);
                    let name = Self::read_pb_filename(bus, name_ptr);
                    let vref = bus.read_word(pb + 22) as i16;
                    let proc_id = bus.read_long(pb + 28);
                    let requested_dir_id = bus.read_long(pb + 48);
                    if vref != 0 && self.working_directory_info(vref).is_none() {
                        bus.write_word(pb + 16, (-35i16) as u16); // nsvErr
                        cpu.write_reg(Register::D0, (-35i32) as u32);
                        return Some(Ok(()));
                    }
                    let base_dir_id = self.resolve_directory_id(vref, requested_dir_id);
                    let effective_dir_id = if name.is_empty() {
                        base_dir_id
                    } else if let Some(path) =
                        self.find_vfs_directory_in_directory(base_dir_id, &name)
                    {
                        self.vfs_directories
                            .iter()
                            .find(|directory| directory.path.eq_ignore_ascii_case(&path))
                            .map(|directory| directory.dir_id)
                            .unwrap_or(base_dir_id)
                    } else {
                        eprintln!(
                            "[TRAP] FSDispatch PBOpenWD name='{}' vref={} dirID={} -> fnfErr",
                            name, vref, requested_dir_id
                        );
                        bus.write_word(pb + 16, (-43i16) as u16);
                        cpu.write_reg(Register::D0, (-43i32) as u32);
                        return Some(Ok(()));
                    };

                    if let Some(wd_ref_num) =
                        self.open_working_directory(vref, effective_dir_id, proc_id)
                    {
                        let volume_ref_num = self.resolve_volume_ref_num(vref);
                        eprintln!(
                            "[TRAP] FSDispatch PBOpenWD name='{}' vref={} dirID={} -> wdRefNum={} resolvedDirID={}",
                            name, vref, requested_dir_id, wd_ref_num, effective_dir_id
                        );
                        bus.write_word(pb + 22, wd_ref_num as u16);
                        bus.write_long(pb + 28, proc_id);
                        bus.write_word(pb + 32, volume_ref_num as u16);
                        bus.write_long(pb + 48, effective_dir_id);
                        bus.write_word(pb + 16, 0);
                        cpu.write_reg(Register::D0, 0);
                    } else {
                        bus.write_word(pb + 16, (-43i16) as u16);
                        cpu.write_reg(Register::D0, (-43i32) as u32);
                    }
                    return Some(Ok(()));
                }

                if selector == 2 {
                    // PBCloseWD ($A260, selector 2)
                    // Releases a working directory reference number.
                    // FUNCTION PBCloseWD(paramBlock: WDPBPtr; async: Boolean): OSErr;
                    // Files 1992, 2-202 to 2-203
                    let wd_ref_num = bus.read_word(pb + 22) as i16;
                    eprintln!("[TRAP] FSDispatch PBCloseWD wdRefNum={}", wd_ref_num);
                    let err = if wd_ref_num == super::TrapDispatcher::boot_volume_ref_num()
                        || self.vfs_volume_for_ref_num(wd_ref_num).is_some()
                        || self.close_working_directory(wd_ref_num)
                    {
                        0
                    } else {
                        -51i16
                    };
                    bus.write_word(pb + 16, err as u16);
                    cpu.write_reg(Register::D0, err as i32 as u32);
                    return Some(Ok(()));
                }

                if selector == 7 {
                    // PBGetWDInfo ($A260, selector 7)
                    // Converts a working directory reference number to volume/directory info.
                    // FUNCTION PBGetWDInfo(paramBlock: WDPBPtr; async: Boolean): OSErr;
                    // Files 1992, 2-203 to 2-204
                    //
                    // Behavioral note: for a phantom ioVRefNum=-999 with
                    // ioWDIndex=0, real Mac returns nsvErr (-35) — interpreting
                    // -999 as a vRefNum (outside the wdRefNum range per
                    // IM:Files 6633) — while Systemless returns rfNumErr (-51).
                    let name_ptr = bus.read_long(pb + 18);
                    let input_vref = bus.read_word(pb + 22) as i16;
                    let wd_index = bus.read_word(pb + 26) as i16;
                    let info = if wd_index > 0 {
                        self.working_directory_by_index(wd_index, input_vref)
                    } else if input_vref == 0 {
                        Some(super::dispatch::WorkingDirectory {
                            ref_num: self
                                .open_working_directory(
                                    super::TrapDispatcher::boot_volume_ref_num(),
                                    *self.default_dir_id,
                                    0,
                                )
                                .unwrap_or(super::TrapDispatcher::boot_volume_ref_num()),
                            volume_ref_num: super::TrapDispatcher::boot_volume_ref_num(),
                            dir_id: *self.default_dir_id,
                            proc_id: 0,
                        })
                    } else {
                        self.working_directory_info(input_vref)
                    };

                    if let Some(working_directory) = info {
                        let returned_vref = if wd_index > 0 {
                            working_directory.volume_ref_num
                        } else {
                            working_directory.ref_num
                        };
                        eprintln!(
                            "[TRAP] FSDispatch PBGetWDInfo input_vRefNum={} ioWDIndex={} -> returned_vRefNum={} ioWDVRefNum={} ioWDDirID={} ioWDProcID=${:08X}",
                            input_vref,
                            wd_index,
                            returned_vref,
                            working_directory.volume_ref_num,
                            working_directory.dir_id,
                            working_directory.proc_id
                        );
                        if name_ptr != 0 {
                            let volume_name = self
                                .vfs_volume_for_ref_num(working_directory.volume_ref_num)
                                .map(|volume| volume.name.as_str())
                                .unwrap_or(super::TrapDispatcher::boot_volume_name());
                            Self::write_pstring(bus, name_ptr, volume_name);
                        }
                        bus.write_word(pb + 22, returned_vref as u16);
                        bus.write_long(pb + 28, working_directory.proc_id);
                        bus.write_word(pb + 32, working_directory.volume_ref_num as u16);
                        bus.write_long(pb + 48, working_directory.dir_id);
                        bus.write_word(pb + 16, 0);
                        cpu.write_reg(Register::D0, 0);
                    } else {
                        eprintln!(
                            "[TRAP] FSDispatch PBGetWDInfo input_vRefNum={} ioWDIndex={} -> rfNumErr",
                            input_vref, wd_index
                        );
                        bus.write_word(pb + 16, (-51i16) as u16);
                        cpu.write_reg(Register::D0, (-51i32) as u32);
                    }
                    return Some(Ok(()));
                }

                let name_ptr = bus.read_long(pb + 18);
                let vref = bus.read_word(pb + 22) as i16;
                // Selector $001A names both PBOpenDF and PBHOpenDF. hfsBit
                // (bit 9) selects the larger HParamBlockRec; without it,
                // offset 48 is outside PBOpenDF's documented inputs and must
                // not participate in lookup. Inside Macintosh Volume IV,
                // Figure 6 and assembly-language note (IV-120); Inside
                // Macintosh Volume VI, pp. 25-51 to 25-52.
                let open_df_uses_hfs_pb = selector == 26 && (self.current_trap_word & 0x0200) != 0;
                let dir_id = if selector == 26 && !open_df_uses_hfs_pb {
                    0
                } else {
                    bus.read_long(pb + 48)
                };
                let filename = Self::read_pb_filename(bus, name_ptr);
                let fdir_index = bus.read_word(pb + 28) as i16;
                eprintln!(
                    "[TRAP] FSDispatch selector={} name_ptr=${:08X} filename=\"{}\" vref={} dirID={} fdirIdx={}",
                    selector, name_ptr, filename, vref, dir_id, fdir_index
                );
                let resolved_vref = self.resolve_volume_ref_num(vref);

                if selector == 6 {
                    // PBDirCreate ($A260, selector 6)
                    // Creates a new directory and returns its directory ID in ioDirID.
                    // FUNCTION PBDirCreate(paramBlock: HParmBlkPtr; async: BOOLEAN): OSErr;
                    // Inside Macintosh Volume IV, IV-146
                    if vref != 0 && self.working_directory_info(vref).is_none() {
                        bus.write_word(pb + 16, (-35i16) as u16); // nsvErr
                        cpu.write_reg(Register::D0, (-35i32) as u32);
                        return Some(Ok(()));
                    }
                    let result = if filename.is_empty() {
                        -37i16 // bdNamErr
                    } else {
                        let parent_dir_id = self.resolve_directory_id(vref, dir_id);
                        let Some(parent_path) = self
                            .directory_path_for_id(parent_dir_id)
                            .map(str::to_string)
                        else {
                            bus.write_word(pb + 16, (-120i16) as u16); // dirNFErr
                            cpu.write_reg(Register::D0, (-120i32) as u32);
                            return Some(Ok(()));
                        };
                        if self.vfs_path_is_read_only(&parent_path) {
                            bus.write_word(pb + 16, (-44i16) as u16); // wPrErr
                            cpu.write_reg(Register::D0, (-44i32) as u32);
                            return Some(Ok(()));
                        }
                        let child_name = super::TrapDispatcher::normalize_hfs_path(&filename);
                        if child_name.is_empty() {
                            -37i16 // bdNamErr
                        } else {
                            let child_path = if parent_path.is_empty() {
                                child_name
                            } else {
                                format!("{parent_path}/{child_name}")
                            };
                            let duplicate =
                                self.vfs_directories.iter().any(|directory| {
                                    directory.path.eq_ignore_ascii_case(&child_path)
                                }) || self
                                    .vfs
                                    .keys()
                                    .any(|path| path.eq_ignore_ascii_case(&child_path))
                                    || self
                                        .vfs_rsrc
                                        .keys()
                                        .any(|path| path.eq_ignore_ascii_case(&child_path));
                            if duplicate {
                                -48i16 // dupFNErr
                            } else {
                                let new_dir_id = self.ensure_vfs_directory(&child_path);
                                if let Some(ref dir) = self.output_dir {
                                    let _ = std::fs::create_dir_all(dir.join(&child_path));
                                }
                                bus.write_long(pb + 48, new_dir_id);
                                eprintln!(
                                    "[TRAP] FSDispatch PBDirCreate(\"{}\") parentDirID={} -> dirID={}",
                                    filename, parent_dir_id, new_dir_id
                                );
                                0
                            }
                        }
                    };
                    bus.write_word(pb + 22, resolved_vref as u16);
                    bus.write_word(pb + 16, result as u16);
                    cpu.write_reg(Register::D0, result as i32 as u32);
                    return Some(Ok(()));
                }

                // PBGetFCBInfo (selector 8): returns info about an open file control block.
                // Input: ioRefNum (offset 24) = file reference number; ioFCBIndx (offset 28)
                //   for indexed access if ioRefNum=0.
                // Output: ioNamePtr filled with filename, ioVRefNum, ioFCBParID, etc.
                // Files 1992, 2-241 to 2-243
                //
                // Behavioral note: for a phantom ioRefNum=-999 with
                // ioFCBIndx=0, real Mac returns rfNumErr (-51) — treating
                // -999 as a categorically-invalid refnum per IM:Files
                // 11733..11736 — while Systemless returns fnOpnErr (-38) per
                // the open_files HashMap miss. Same polymorphic-input
                // divergence class as PBGetWDInfo selector $0007
                // (rfNumErr-vs-nsvErr split).
                if selector == 8 {
                    let ref_num = bus.read_word(pb + 24);
                    let fcb_index = bus.read_word(pb + 28) as i16;
                    eprintln!(
                        "[TRAP] FSDispatch PBGetFCBInfo refNum={} fcbIndx={}",
                        ref_num, fcb_index
                    );

                    // Look up the file by reference number. refNum 0 names the
                    // current application's resource fork. PBOpenRF mirrors
                    // resource forks into open_files as "__rsrc__<path>" so
                    // FSRead/FSWrite can share the data-fork code path.
                    let resolved_fcb = if ref_num == 0 {
                        self.launched_app_path()
                            .map(str::to_owned)
                            .map(|vfs_name| (vfs_name, true))
                    } else if let Some(vfs_name) = self.open_files.get(&ref_num).cloned() {
                        let is_resource_fork = vfs_name.starts_with("__rsrc__");
                        Some((vfs_name, is_resource_fork))
                    } else {
                        self.resource_file_name(ref_num)
                            .map(|vfs_name| (vfs_name.to_string(), true))
                    };

                    if let Some((open_vfs_name, is_resource_fork)) = resolved_fcb {
                        let metadata_name = open_vfs_name
                            .strip_prefix("__rsrc__")
                            .unwrap_or(&open_vfs_name)
                            .to_string();
                        let base_name = super::TrapDispatcher::hfs_name_from_vfs_component(
                            super::TrapDispatcher::vfs_basename(&metadata_name),
                        );
                        let metadata = self.vfs_file_metadata(&metadata_name);
                        let file_len = if is_resource_fork {
                            self.vfs
                                .get(&open_vfs_name)
                                .map(|data| data.len())
                                .or_else(|| {
                                    self.vfs_rsrc.get(&metadata_name).map(|data| data.len())
                                })
                                .unwrap_or(0)
                        } else {
                            self.vfs
                                .get(&metadata_name)
                                .map(|data| data.len())
                                .unwrap_or(0)
                        };
                        let current_pos = self
                            .file_positions
                            .get(&ref_num)
                            .copied()
                            .unwrap_or(0)
                            .min(u32::try_from(file_len).unwrap_or(u32::MAX));
                        let mut flags = if is_resource_fork { 0x0200 } else { 0 };
                        if self.write_refnums.contains(&ref_num) {
                            flags |= 0x0100;
                        }
                        let file_id = metadata.map(|m| m.file_id).unwrap_or(0);
                        let parent_dir_id = metadata.map(|m| m.parent_dir_id).unwrap_or(2);
                        if name_ptr != 0 {
                            Self::write_pstring(bus, name_ptr, &base_name);
                        }
                        let file_volume_ref = self
                            .vfs_volume_for_path(&metadata_name)
                            .map(|volume| volume.ref_num)
                            .unwrap_or(super::TrapDispatcher::boot_volume_ref_num());
                        bus.write_word(pb + 22, file_volume_ref as u16); // ioVRefNum
                        bus.write_word(pb + 30, 0); // filler1
                        bus.write_long(pb + 32, file_id); // ioFCBFlNm
                        bus.write_word(pb + 36, flags); // ioFCBFlags
                        bus.write_word(pb + 38, 0); // ioFCBStBlk
                        bus.write_long(pb + 40, file_len as u32); // ioFCBEOF
                        bus.write_long(pb + 44, file_len as u32); // ioFCBPLen
                        bus.write_long(pb + 48, current_pos); // ioFCBCrPs
                        bus.write_word(pb + 52, file_volume_ref as u16); // ioFCBVRefNum
                        bus.write_long(pb + 54, 0); // ioFCBClpSiz
                        bus.write_long(pb + 58, parent_dir_id); // ioFCBParID
                        eprintln!(
                            "[TRAP] FSDispatch PBGetFCBInfo -> name=\"{}\" len={} pos={} flags=${:04X} parentDirID={}",
                            base_name, file_len, current_pos, flags, parent_dir_id
                        );
                        bus.write_word(pb + 16, 0); // noErr
                        cpu.write_reg(Register::D0, 0);
                    } else {
                        eprintln!(
                            "[TRAP] FSDispatch PBGetFCBInfo: refNum={} not found",
                            ref_num
                        );
                        bus.write_word(pb + 16, (-38i16) as u16); // fnOpnErr
                        cpu.write_reg(Register::D0, (-38i32) as u32);
                    }
                    return Some(Ok(()));
                }

                // PBGetCatInfo (selector 9): returns catalog information.
                // Supports indexed enumeration and directory IDs.
                // Files 1992, 2-190 to 2-192
                //
                // Per IM:Files 9944..9952 the documented errors are
                // noErr / nsvErr / ioErr / bdNamErr / fnfErr / paramErr /
                // dirNFErr; the impl below writes -43 fnfErr on lookup miss.
                if selector == 9 {
                    if vref != 0 && self.working_directory_info(vref).is_none() {
                        bus.write_word(pb + 16, (-35i16) as u16); // nsvErr
                        cpu.write_reg(Register::D0, (-35i32) as u32);
                        return Some(Ok(()));
                    }
                    let lookup = self
                        .lookup_catalog_entry_for_hfs_lookup(vref, dir_id, &filename, fdir_index);

                    if let Some(entry) = lookup {
                        if entry.is_directory {
                            if let Some(directory) = self
                                .vfs_directories
                                .iter()
                                .find(|directory| directory.path.eq_ignore_ascii_case(&entry.path))
                            {
                                self.fill_directory_catalog_info(bus, pb, directory);
                                eprintln!(
                                    "[TRAP] FSDispatch PBGetCatInfo dir \"{}\" -> dirID={} parentDirID={}",
                                    entry.name, directory.dir_id, directory.parent_dir_id
                                );
                            }
                        } else if let Some(metadata) = self.vfs_file_metadata(&entry.path) {
                            self.fill_file_catalog_info(bus, pb, &entry.path, metadata);
                            bus.write_long(pb + 100, metadata.parent_dir_id);
                            eprintln!(
                                "[TRAP] FSDispatch PBGetCatInfo file \"{}\" -> fileID={} parentDirID={}",
                                entry.name, metadata.file_id, metadata.parent_dir_id
                            );
                        }
                        if name_ptr != 0 {
                            Self::write_pstring(bus, name_ptr, &entry.name);
                        }
                        bus.write_word(pb + 22, resolved_vref as u16);
                        bus.write_word(pb + 16, 0);
                        cpu.write_reg(Register::D0, 0);
                    } else {
                        bus.write_word(pb + 16, (-43i16) as u16); // fnfErr
                        cpu.write_reg(Register::D0, (-43i32) as u32);
                    }
                    return Some(Ok(()));
                }

                // PBSetCatInfo (selector 10)
                // Updates a file's Finder information and catalogue dates, or
                // a directory's Finder information, using the named entry.
                // FUNCTION PBSetCatInfo(paramBlock: CInfoPBPtr; async: BOOLEAN): OSErr;
                // Inside Macintosh: Files (1992), pp. 2-193 to 2-195.
                if selector == 10 {
                    let error = if vref != 0 && self.working_directory_info(vref).is_none() {
                        -35i16 // nsvErr
                    } else if let Some(entry) =
                        self.lookup_catalog_entry_for_hfs_lookup(vref, dir_id, &filename, 0)
                    {
                        if self.vfs_path_is_read_only(&entry.path) {
                            -46 // vLckdErr
                        } else if entry.is_directory {
                            let file_type = bus.read_long(pb + 32);
                            let creator = bus.read_long(pb + 36);
                            let finder_flags = bus.read_word(pb + 40);
                            self.vfs_directories.with_mut(|directories| {
                                if let Some(directory) = directories.iter_mut().find(|directory| {
                                    directory.path.eq_ignore_ascii_case(&entry.path)
                                }) {
                                    directory.file_type = file_type;
                                    directory.creator = creator;
                                    directory.finder_flags = finder_flags;
                                }
                            });
                            0
                        } else if self.locked_files.contains(&entry.path) {
                            -45 // fLckdErr
                        } else {
                            let file_type = bus.read_long(pb + 32);
                            let creator = bus.read_long(pb + 36);
                            let finder_flags = bus.read_word(pb + 40);
                            let created_date = bus.read_long(pb + 72);
                            let modified_date = bus.read_long(pb + 76);
                            self.vfs_metadata.update(&entry.path, |metadata| {
                                metadata.created_date = created_date;
                                metadata.modified_date = modified_date;
                            });
                            self.set_vfs_entry_finfo(&entry.path, file_type, creator, finder_flags);
                            0
                        }
                    } else {
                        -43 // fnfErr
                    };
                    bus.write_word(pb + 16, error as u16);
                    cpu.write_reg(Register::D0, (error as i32) as u32);
                    return Some(Ok(()));
                }

                // Selector 26 = PBOpenDF / PBHOpenDF: open data fork by name.
                // hfsBit distinguishes the basic parameter block from the HFS
                // form that adds ioDirID. Both forms otherwise share the open,
                // permission, and result behavior below.
                //
                // Per IM:Files 9590..9598 the documented errors are
                // noErr / nsvErr / ioErr / bdNamErr / tmfoErr / fnfErr (-43) /
                // opWrErr / permErr / dirNFErr / afpAccessDenied; the impl
                // below writes -43 fnfErr on lookup miss.
                if selector == 26 {
                    let routine_name = if open_df_uses_hfs_pb {
                        "PBHOpenDF"
                    } else {
                        "PBOpenDF"
                    };
                    if vref != 0 && self.working_directory_info(vref).is_none() {
                        bus.write_word(pb + 16, (-35i16) as u16); // nsvErr
                        cpu.write_reg(Register::D0, (-35i32) as u32);
                        return Some(Ok(()));
                    }
                    // Clear ioRefNum upfront. Files 1992 leaves the output
                    // undefined on failure, but MPW's FSOpen glue writes
                    // `*refNum = pb.ioRefNum` regardless of the result
                    // code — clearing makes the failure path deterministic.
                    bus.write_word(pb + 24, 0);
                    if filename.is_empty() {
                        eprintln!("[TRAP] FSDispatch {routine_name}(\"\") -> bdNamErr");
                        bus.write_word(pb + 16, (-37i16) as u16);
                        cpu.write_reg(Register::D0, (-37i32) as u32);
                        return Some(Ok(()));
                    }
                    if let Some(vfs_name) =
                        self.find_vfs_file_for_hfs_lookup(vref, dir_id, &filename)
                    {
                        let permission = bus.read_byte(pb + 27);
                        let refnum = self.allocate_process_file_refnum();
                        self.open_files.insert(refnum, vfs_name.clone());
                        // Files 1992, pp. 2-183 to 2-184: PBHOpenDF uses the same
                        // ioPermssn values as PBOpen. Record the granted
                        // write access so later PBWrite/PBSetEOF calls honor
                        // the open mode.
                        if matches!(permission, 0 | 2 | 3 | 4) {
                            self.write_refnums.insert(refnum);
                        }
                        self.file_positions.insert(refnum, 0);
                        bus.write_word(pb + 24, refnum); // ioRefNum
                        bus.write_word(pb + 16, 0);
                        cpu.write_reg(Register::D0, 0);
                        eprintln!(
                            "[TRAP] FSDispatch {routine_name}(\"{}\") -> refnum={} vfs=\"{}\"",
                            filename, refnum, vfs_name,
                        );
                        return Some(Ok(()));
                    } else {
                        eprintln!(
                            "[TRAP] FSDispatch {routine_name}(\"{}\") -> fnfErr",
                            filename
                        );
                        bus.write_word(pb + 16, (-43i16) as u16);
                        cpu.write_reg(Register::D0, (-43i32) as u32);
                        return Some(Ok(()));
                    }
                }

                // Return noErr via PB ioResult and D0
                bus.write_word(pb + 16, 0);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // InitFS ($A06C)
            // Public A-trap constant from the Mac OS SDK trap table.
            // Systemless does not model any file-system init side effects here;
            // the caller-visible contract we keep is a no-op that returns
            // noErr and preserves the caller's stack.
            //
            // MPW Universal Headers `Traps.h` publishes `_InitFS = 0xA06C`.
            (false, 0x6C) => {
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            _ => return None,
        })
    }

    /// Shared implementation of SizeResource ($A9A5) and MaxSizeRsrc ($A821).
    ///
    /// Pascal signature is identical for both:
    ///   FUNCTION (theResource: Handle): LONGINT;
    /// Stack on entry:
    ///   SP       theResource (Handle,  4 bytes)
    ///   SP + 4   result      (LongInt, 4 bytes — caller-allocated)
    /// Pops 4 bytes of args, leaves the 4-byte result slot.
    ///
    /// IM:I-121 (SizeResource) and IM:IV-16 (MaxSizeRsrc) document a
    /// disk-vs-map distinction (SizeResource may load the resource;
    /// MaxSizeRsrc reads from the map only) which collapses in our
    /// memory-resident HLE. An empty handle returned while `ResLoad` is false
    /// has no guest allocation, so its byte count comes from the retained
    /// resource-fork backing data without materializing the resource.
    fn handle_resource_size_query<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
    ) -> Result<()> {
        let sp = cpu.read_reg(Register::A7);
        let handle = bus.read_long(sp);

        let identity = self.loaded_handles.get(&handle).copied();
        let ptr = match identity {
            Some((0, res_type, res_id)) if Self::resource_loading_enabled(bus) => self
                .resource_handle_files
                .get(&handle)
                .copied()
                .and_then(|refnum| {
                    self.reload_resource_data_from_file(bus, refnum, res_type, res_id)
                })
                .unwrap_or(0),
            Some((ptr, _, _)) => ptr,
            None => 0,
        };
        if ptr != 0 && identity.is_some_and(|(old_ptr, _, _)| old_ptr == 0) {
            self.with_resource_manager_mut(|resource_manager| {
                if let Some(entry) = resource_manager.loaded_handles.get_mut(&handle) {
                    entry.0 = ptr;
                }
            });
            bus.write_long(handle, ptr);
            self.restore_loaded_resource_handle(handle, ptr);
        }

        let size: i32 = match identity {
            Some((_, _, _)) if ptr != 0 => bus.get_alloc_size(ptr).map(|s| s as i32).unwrap_or(-1),
            Some((0, res_type, res_id)) => self
                .resource_handle_files
                .get(&handle)
                .and_then(|refnum| self.resource_backing_data.get(&(*refnum, res_type, res_id)))
                .map(|data| data.len() as i32)
                .unwrap_or(-1),
            _ => -1,
        };

        if size < 0 {
            bus.write_word(0x0A60, (-192i16) as u16); // ResErr = resNotFound
        } else {
            bus.write_word(0x0A60, 0); // ResErr = noErr
        }

        bus.write_long(sp + 4, size as u32);
        cpu.write_reg(Register::A7, sp + 4);
        Ok(())
    }

    /// Selector 1 of _ResourceDispatch ($A822). Copies `count` bytes
    /// from (*handle + offset) to `buffer`. Returns the ResErr value
    /// to write back to $0A60.
    /// More Macintosh Toolbox 1993, 1-69.
    fn read_partial_resource(
        &self,
        bus: &mut MacMemoryBus,
        handle: u32,
        offset: i32,
        buffer: u32,
        count: i32,
    ) -> i16 {
        if handle == 0 {
            return Self::RES_NOT_FOUND;
        }
        let handle_ptr = bus.read_long(handle);
        let (ptr, already_in_memory) = if handle_ptr == 0 {
            // MMTB 1993 p. 1-41 documents the normal partial-resource
            // workflow: SetResLoad(FALSE) returns an empty handle, then
            // ReadPartialResource streams bytes from the resource on disk.
            // Systemless keeps that disk backing in loaded_handles.
            let Some((backing_ptr, _, _)) = self.loaded_handles.get(&handle).copied() else {
                return Self::RES_NOT_FOUND;
            };
            (backing_ptr, false)
        } else {
            (handle_ptr, true)
        };
        if ptr == 0 {
            return Self::RES_NOT_FOUND;
        }
        let size = bus.get_alloc_size(ptr).unwrap_or(0) as i64;
        // MMTB 1-69: "If you try to read past the end of a resource
        // or the value of the offset parameter is out of bounds,
        // ResError returns the result code inputOutOfBounds (-190)."
        if offset < 0 || count < 0 {
            return -190;
        }
        let end = offset as i64 + count as i64;
        if end > size {
            return -190;
        }
        if count == 0 {
            return 0;
        }
        let bytes = bus.read_bytes(ptr + offset as u32, count as usize);
        bus.write_bytes(buffer, &bytes);
        // The classic Resource Manager reports `resourceInMemory`
        // after a partial read from a resource that is already loaded.
        if already_in_memory {
            -188
        } else {
            0
        }
    }

    /// Selector 2 of _ResourceDispatch ($A822). Copies `count` bytes
    /// from `buffer` into the resource starting at `offset`. If the
    /// write would extend past the current resource size, the
    /// allocation is grown to fit and `writingPastEnd` (-189) is
    /// returned per MMTB 1-70.
    fn write_partial_resource(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        offset: i32,
        buffer: u32,
        count: i32,
    ) -> i16 {
        if handle == 0 {
            return Self::RES_NOT_FOUND;
        }
        let handle_ptr = bus.read_long(handle);
        let handle_was_empty = handle_ptr == 0;
        let ptr = if handle_was_empty {
            // Same SetResLoad(FALSE) partial-resource workflow as
            // ReadPartialResource: the empty handle still names a
            // resource-map entry whose backing bytes Systemless tracks.
            let Some((backing_ptr, _, _)) = self.loaded_handles.get(&handle).copied() else {
                return Self::RES_NOT_FOUND;
            };
            backing_ptr
        } else {
            handle_ptr
        };
        if ptr == 0 {
            return Self::RES_NOT_FOUND;
        }
        if offset < 0 || count < 0 {
            return -190;
        }
        if count == 0 {
            // Zero-length writes are no-ops even if the offset is past
            // the current resource tail.
            return 0;
        }
        let cur_size = bus.get_alloc_size(ptr).unwrap_or(0) as i64;
        let end = offset as i64 + count as i64;
        let mut past_end = false;
        let live_ptr = if end > cur_size {
            past_end = true;
            self.resize_resource_allocation(bus, handle, ptr, end as u32)
        } else {
            ptr
        };
        if live_ptr == 0 {
            // Allocation failed when extending — surface a memory
            // error in the File Manager band per MMTB 1-70.
            return -108; // memFullErr
        }
        if count > 0 {
            let bytes = bus.read_bytes(buffer, count as usize);
            bus.write_bytes(live_ptr + offset as u32, &bytes);
        }
        if handle_was_empty {
            bus.write_long(handle, 0);
            self.untrack_handle_ptr(live_ptr);
        }
        if past_end {
            -189
        } else {
            0
        }
    }

    /// Selector 3 of _ResourceDispatch ($A822). Resizes the resource
    /// allocation to `new_size`, preserving as many bytes as fit.
    /// MMTB 1-71.
    fn set_resource_size(&mut self, bus: &mut MacMemoryBus, handle: u32, new_size: i32) -> i16 {
        if handle == 0 {
            return Self::RES_NOT_FOUND;
        }
        if new_size < 0 {
            // Out-of-band size; treat as input error. MMTB doesn't
            // document negative sizes, but the partial-resource
            // family already uses inputOutOfBounds for invalid
            // numeric arguments per IM:VI 1-69.
            return -190;
        }
        let ptr = bus.read_long(handle);
        if ptr == 0 {
            // Empty handle (e.g. SetResLoad(FALSE) + GetResource).
            // Resize the backing allocation that the Resource Manager
            // still tracks in `loaded_handles`. If we only allocate a
            // new block here, the resource map continues to point at the
            // stale pointer and a later GetResource can synthesize a
            // duplicate handle for the same (type, id) pair.
            if new_size == 0 {
                return 0;
            }
            let Some((old_ptr, _, _)) = self.loaded_handles.get(&handle).copied() else {
                return -192;
            };
            let new_ptr = self.resize_resource_allocation(bus, handle, old_ptr, new_size as u32);
            if new_ptr == 0 {
                return -108; // memFullErr
            }
            return 0;
        }
        let new_ptr = self.resize_resource_allocation(bus, handle, ptr, new_size as u32);
        if new_ptr == 0 && new_size != 0 {
            return -108; // memFullErr
        }
        0
    }

    /// Resize the master-pointer allocation backing a resource handle
    /// to at least `new_size` bytes, preserving the prefix data.
    /// Returns the new master-pointer address (which may differ from
    /// `old_ptr` if the data had to be relocated), or 0 on
    /// allocation failure when `new_size > 0`.
    ///
    /// Updates the live ptr through `*handle`, the `ptr_to_handle`
    /// map, the `loaded_handles` ptr field, and any
    /// `LoadedResources::files[refnum].loaded` / `.named` entries
    /// pointing at the old pointer so subsequent GetResource lookups
    /// see the new allocation. A null old pointer is shared by every
    /// unloaded resource, so only the resized handle's record may be updated.
    pub(crate) fn resize_resource_allocation(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        old_ptr: u32,
        new_size: u32,
    ) -> u32 {
        let resized = {
            let memory_manager = self.process_memory_manager();
            let mut memory_manager = memory_manager.borrow_mut();
            memory_manager.attach_classic_memory_bus(bus);
            memory_manager.resize_process_resource_handle(bus, handle, old_ptr, new_size)
        };
        let Ok((old_ptr, new_ptr)) = resized else {
            return 0;
        };
        self.with_resource_manager_mut(|resource_manager| {
            let resource_key =
                resource_manager
                    .loaded_handles
                    .get(&handle)
                    .and_then(|&(_, res_type, res_id)| {
                        resource_manager
                            .resource_handle_files
                            .get(&handle)
                            .copied()
                            .map(|refnum| (refnum, res_type, res_id))
                    });
            if let Some(entry) = resource_manager.loaded_handles.get_mut(&handle) {
                entry.0 = new_ptr;
            }
            if let Some(resources) = resource_manager.resources.as_mut() {
                if old_ptr == 0 {
                    if let Some((refnum, res_type, res_id)) = resource_key {
                        if let Some(file) = resources.files.get_mut(&refnum) {
                            if file.loaded.get(&(res_type, res_id)).copied() == Some(0) {
                                file.loaded.insert((res_type, res_id), new_ptr);
                            }
                            for ((named_type, _), (named_id, ptr)) in &mut file.named {
                                if *named_type == res_type && *named_id == res_id && *ptr == 0 {
                                    *ptr = new_ptr;
                                }
                            }
                        }
                    }
                } else {
                    for file in resources.files.values_mut() {
                        for v in file.loaded.values_mut() {
                            if *v == old_ptr {
                                *v = new_ptr;
                            }
                        }
                        for (_id, v) in file.named.values_mut() {
                            if *v == old_ptr {
                                *v = new_ptr;
                            }
                        }
                    }
                }
            }
        });
        new_ptr
    }

    /// Read a Pascal string filename from a parameter block's ioNamePtr.
    pub(crate) fn read_pb_filename(bus: &MacMemoryBus, name_ptr: u32) -> String {
        if name_ptr == 0 {
            return String::new();
        }
        decode_mac_roman(&bus.read_pstring(name_ptr))
    }

    pub(crate) fn write_pstring(bus: &mut MacMemoryBus, ptr: u32, value: &str) {
        if ptr == 0 {
            return;
        }
        let bytes = encode_mac_roman_lossy(value);
        let len = bytes.len().min(63);
        bus.write_byte(ptr, len as u8);
        for (idx, byte) in bytes.into_iter().take(len).enumerate() {
            bus.write_byte(ptr + 1 + idx as u32, byte);
        }
    }

    fn write_finfo(
        bus: &mut MacMemoryBus,
        finfo_ptr: u32,
        file_type: u32,
        creator: u32,
        finder_flags: u16,
    ) {
        bus.write_long(finfo_ptr, file_type);
        bus.write_long(finfo_ptr + 4, creator);
        bus.write_word(finfo_ptr + 8, finder_flags);
        bus.write_long(finfo_ptr + 10, 0); // fdLocation
        bus.write_word(finfo_ptr + 14, 0); // fdFldr
    }

    fn fill_file_catalog_info(
        &mut self,
        bus: &mut MacMemoryBus,
        pb: u32,
        key: &str,
        metadata: super::dispatch::VfsMetadata,
    ) {
        let data_len = self.vfs.get(key).map_or(0, |data| data.len() as u32);
        let rsrc_len = self.vfs_rsrc.get(key).map_or(0, |data| data.len() as u32);
        // ioFlAttrib bit 0 = file locked.  Files 1992, 2-192.  Maintained
        // by HSetFLock/HRstFLock ($A241/$A242).
        let locked_bit: u8 = if self.locked_files.contains(key) {
            0x01
        } else {
            0x00
        };

        bus.write_byte(pb + 30, locked_bit); // ioFlAttrib
        bus.write_byte(pb + 31, 0); // ioACUser / ioFlVersNum
        Self::write_finfo(
            bus,
            pb + 32,
            metadata.file_type,
            metadata.creator,
            metadata.finder_flags,
        );
        bus.write_long(pb + 48, metadata.file_id);
        bus.write_long(pb + 54, data_len);
        bus.write_long(pb + 58, data_len);
        bus.write_long(pb + 64, rsrc_len);
        bus.write_long(pb + 68, rsrc_len);
        bus.write_long(pb + 72, metadata.created_date);
        bus.write_long(pb + 76, metadata.modified_date);
    }

    fn fill_directory_catalog_info(
        &self,
        bus: &mut MacMemoryBus,
        pb: u32,
        directory: &super::dispatch::VfsDirectory,
    ) {
        bus.write_byte(pb + 30, 0x10); // kFolderBit
        bus.write_byte(pb + 31, 0);
        Self::write_finfo(
            bus,
            pb + 32,
            directory.file_type,
            directory.creator,
            directory.finder_flags,
        );
        bus.write_long(pb + 48, directory.dir_id);
        let child_directory_count = self
            .vfs_directories
            .iter()
            .filter(|child| {
                child.dir_id != directory.dir_id && child.parent_dir_id == directory.dir_id
            })
            .count();
        let child_file_count = self
            .vfs_metadata
            .values()
            .filter(|metadata| metadata.parent_dir_id == directory.dir_id)
            .count();
        let entry_count = child_directory_count
            .saturating_add(child_file_count)
            .min(u16::MAX as usize) as u16;
        bus.write_word(pb + 52, entry_count); // ioDrNmFls
        bus.write_long(pb + 72, 0);
        bus.write_long(pb + 76, 0);
        // ioDrParID: parent directory ID.
        // Files 1992, 2-192
        bus.write_long(pb + 100, directory.parent_dir_id);
    }

    fn cat_search_volume_ref_num(&self, bus: &MacMemoryBus, pb: u32) -> Option<i16> {
        let requested_name = Self::read_pb_filename(bus, bus.read_long(pb + 18));
        if !requested_name.is_empty() {
            if requested_name.eq_ignore_ascii_case(Self::boot_volume_name()) {
                return Some(Self::boot_volume_ref_num());
            }
            return self
                .vfs_volume_by_name(&requested_name)
                .map(|volume| volume.ref_num);
        }

        let requested_ref = bus.read_word(pb + 22) as i16;
        if requested_ref == 0 || requested_ref == Self::boot_volume_ref_num() {
            return Some(self.resolve_volume_ref_num(requested_ref));
        }
        if let Some(volume) = self.vfs_volume_for_ref_num(requested_ref) {
            return Some(volume.ref_num);
        }
        self.working_directory_info(requested_ref)
            .map(|working_directory| working_directory.volume_ref_num)
    }

    fn cat_search_entries(&mut self, volume_ref_num: i16) -> Vec<CatSearchEntry> {
        self.ensure_vfs_catalog();
        let mut entries = Vec::new();

        for directory in &*self.vfs_directories {
            let path = &directory.path;
            let entry_volume_ref = self
                .vfs_volume_for_path(path)
                .map(|volume| volume.ref_num)
                .unwrap_or_else(Self::boot_volume_ref_num);
            if entry_volume_ref != volume_ref_num {
                continue;
            }
            let child_count = self
                .vfs_directories
                .iter()
                .filter(|child| {
                    child.dir_id != directory.dir_id && child.parent_dir_id == directory.dir_id
                })
                .count()
                .saturating_add(
                    self.vfs_metadata
                        .values()
                        .filter(|metadata| metadata.parent_dir_id == directory.dir_id)
                        .count(),
                )
                .min(u16::MAX as usize) as u16;
            let name = if directory.parent_dir_id == 1 {
                if volume_ref_num == Self::boot_volume_ref_num() {
                    Self::boot_volume_name().to_string()
                } else {
                    self.vfs_volume_for_ref_num(volume_ref_num)
                        .map(|volume| volume.name.clone())
                        .unwrap_or_else(|| Self::vfs_basename(&directory.path).to_string())
                }
            } else {
                Self::hfs_name_from_vfs_component(Self::vfs_basename(&directory.path))
            };
            let mut finder_info = [0u8; 16];
            finder_info[..4].copy_from_slice(&directory.file_type.to_be_bytes());
            finder_info[4..8].copy_from_slice(&directory.creator.to_be_bytes());
            finder_info[8..10].copy_from_slice(&directory.finder_flags.to_be_bytes());
            entries.push(CatSearchEntry {
                name,
                vref_num: volume_ref_num,
                parent_dir_id: directory.parent_dir_id,
                is_directory: true,
                locked: false,
                finder_info,
                extended_finder_info: [0; 16],
                data_length: 0,
                resource_length: 0,
                child_count,
                created_date: 0,
                modified_date: 0,
                backup_date: 0,
            });
        }

        let mut file_paths: Vec<String> = self.vfs_metadata.keys().cloned().collect();
        file_paths.sort_unstable_by_key(|path| path.to_ascii_lowercase());
        for path in file_paths {
            let entry_volume_ref = self
                .vfs_volume_for_path(&path)
                .map(|volume| volume.ref_num)
                .unwrap_or_else(Self::boot_volume_ref_num);
            if entry_volume_ref != volume_ref_num {
                continue;
            }
            let Some(metadata) = self.vfs_metadata.get(&path).copied() else {
                continue;
            };
            let mut finder_info = [0u8; 16];
            finder_info[..4].copy_from_slice(&metadata.file_type.to_be_bytes());
            finder_info[4..8].copy_from_slice(&metadata.creator.to_be_bytes());
            finder_info[8..10].copy_from_slice(&metadata.finder_flags.to_be_bytes());
            entries.push(CatSearchEntry {
                name: Self::hfs_name_from_vfs_component(Self::vfs_basename(&path)),
                vref_num: volume_ref_num,
                parent_dir_id: metadata.parent_dir_id,
                is_directory: false,
                locked: self.locked_files.contains(&path),
                finder_info,
                extended_finder_info: [0; 16],
                data_length: self.vfs.get(&path).map_or(0, |data| data.len() as u32),
                resource_length: self.vfs_rsrc.get(&path).map_or(0, |data| data.len() as u32),
                child_count: 0,
                created_date: metadata.created_date,
                modified_date: metadata.modified_date,
                backup_date: 0,
            });
        }

        entries.sort_unstable_by_key(|entry| {
            (
                entry.parent_dir_id,
                entry.name.to_ascii_lowercase(),
                entry.is_directory,
            )
        });
        entries
    }

    fn masked_bytes_match(
        bus: &MacMemoryBus,
        actual: &[u8],
        desired_ptr: u32,
        mask_ptr: u32,
    ) -> bool {
        actual.iter().enumerate().all(|(index, actual)| {
            let desired = bus.read_byte(desired_ptr + index as u32);
            let mask = bus.read_byte(mask_ptr + index as u32);
            (actual ^ desired) & mask == 0
        })
    }

    fn cat_search_entry_matches(
        bus: &MacMemoryBus,
        entry: &CatSearchEntry,
        search_bits: u32,
        info1: u32,
        info2: u32,
    ) -> bool {
        const PARTIAL_NAME: u32 = 1;
        const FULL_NAME: u32 = 2;
        const ATTRIBUTES: u32 = 4;
        const FINDER_INFO: u32 = 8;
        const DIRECTORY_CHILD_COUNT: u32 = 16;
        const DATA_LOGICAL_LENGTH: u32 = 32;
        const DATA_PHYSICAL_LENGTH: u32 = 64;
        const RESOURCE_LOGICAL_LENGTH: u32 = 128;
        const RESOURCE_PHYSICAL_LENGTH: u32 = 256;
        const CREATED_DATE: u32 = 512;
        const MODIFIED_DATE: u32 = 1024;
        const BACKUP_DATE: u32 = 2048;
        const EXTENDED_FINDER_INFO: u32 = 4096;
        const PARENT_ID: u32 = 8192;
        const NEGATE: u32 = 16384;

        let mut matches = true;
        if search_bits & (PARTIAL_NAME | FULL_NAME) != 0 {
            let target = Self::read_pb_filename(bus, bus.read_long(info1 + 18));
            if search_bits & FULL_NAME != 0 {
                matches &= entry.name.eq_ignore_ascii_case(&target);
            } else {
                matches &= entry
                    .name
                    .to_ascii_lowercase()
                    .contains(&target.to_ascii_lowercase());
            }
        }
        if search_bits & ATTRIBUTES != 0 {
            let actual = (u8::from(entry.is_directory) << 4) | u8::from(entry.locked);
            let desired = bus.read_byte(info1 + 30);
            let mask = bus.read_byte(info2 + 30) & 0x11;
            matches &= (actual ^ desired) & mask == 0;
        }
        if search_bits & FINDER_INFO != 0 {
            matches &= Self::masked_bytes_match(bus, &entry.finder_info, info1 + 32, info2 + 32);
        }
        if search_bits & EXTENDED_FINDER_INFO != 0 {
            matches &=
                Self::masked_bytes_match(bus, &entry.extended_finder_info, info1 + 84, info2 + 84);
        }

        if entry.is_directory {
            if search_bits
                & (DATA_LOGICAL_LENGTH
                    | DATA_PHYSICAL_LENGTH
                    | RESOURCE_LOGICAL_LENGTH
                    | RESOURCE_PHYSICAL_LENGTH)
                != 0
            {
                matches = false;
            }
            if search_bits & DIRECTORY_CHILD_COUNT != 0 {
                matches &= entry.child_count >= bus.read_word(info1 + 52)
                    && entry.child_count <= bus.read_word(info2 + 52);
            }
        } else {
            if search_bits & DIRECTORY_CHILD_COUNT != 0 {
                matches = false;
            }
            let range_matches = |bit: u32, offset: u32, value: u32| {
                search_bits & bit == 0
                    || (value >= bus.read_long(info1 + offset)
                        && value <= bus.read_long(info2 + offset))
            };
            matches &= range_matches(DATA_LOGICAL_LENGTH, 54, entry.data_length);
            matches &= range_matches(DATA_PHYSICAL_LENGTH, 58, entry.data_length);
            matches &= range_matches(RESOURCE_LOGICAL_LENGTH, 64, entry.resource_length);
            matches &= range_matches(RESOURCE_PHYSICAL_LENGTH, 68, entry.resource_length);
        }

        let range_matches = |bit: u32, offset: u32, value: u32| {
            search_bits & bit == 0
                || (value >= bus.read_long(info1 + offset)
                    && value <= bus.read_long(info2 + offset))
        };
        matches &= range_matches(CREATED_DATE, 72, entry.created_date);
        matches &= range_matches(MODIFIED_DATE, 76, entry.modified_date);
        matches &= range_matches(BACKUP_DATE, 80, entry.backup_date);
        matches &= range_matches(PARENT_ID, 100, entry.parent_dir_id);

        if search_bits & NEGATE != 0 {
            !matches
        } else {
            matches
        }
    }

    fn write_cat_search_fsspec(bus: &mut MacMemoryBus, ptr: u32, entry: &CatSearchEntry) {
        bus.write_word(ptr, entry.vref_num as u16);
        bus.write_long(ptr + 2, entry.parent_dir_id);
        let bytes = encode_mac_roman_lossy(&entry.name);
        let length = bytes.len().min(63);
        bus.write_byte(ptr + 6, length as u8);
        bus.write_bytes(ptr + 7, &bytes[..length]);
        if length < 63 {
            bus.fill_bytes(ptr + 7 + length as u32, 63 - length as u32, 0);
        }
    }

    fn write_cat_search_position(bus: &mut MacMemoryBus, pb: u32, position: usize) {
        bus.write_long(pb + 52, 1);
        bus.write_long(pb + 56, position.min(u32::MAX as usize) as u32);
        bus.fill_bytes(pb + 60, 8, 0);
    }

    fn perform_cat_search(&mut self, bus: &mut MacMemoryBus, pb: u32) -> i16 {
        bus.write_long(pb + 32, 0);
        let Some(volume_ref_num) = self.cat_search_volume_ref_num(bus, pb) else {
            return -35; // nsvErr
        };
        let requested_count = bus.read_long(pb + 28) as usize;
        if requested_count == 0 {
            return 0;
        }
        let matches_ptr = bus.read_long(pb + 24);
        if matches_ptr == 0 {
            return -50; // paramErr
        }
        let search_bits = bus.read_long(pb + 36);
        let info1 = bus.read_long(pb + 40);
        let info2 = bus.read_long(pb + 44);
        if search_bits & !16384 != 0 && (info1 == 0 || info2 == 0) {
            return -50; // paramErr
        }

        let entries = self.cat_search_entries(volume_ref_num);
        let mut position = if bus.read_long(pb + 52) == 0 {
            0
        } else {
            bus.read_long(pb + 56) as usize
        }
        .min(entries.len());
        let mut actual_count = 0usize;
        while position < entries.len() {
            let entry = &entries[position];
            position += 1;
            if Self::cat_search_entry_matches(bus, entry, search_bits, info1, info2) {
                Self::write_cat_search_fsspec(bus, matches_ptr + actual_count as u32 * 70, entry);
                actual_count += 1;
                if actual_count == requested_count {
                    bus.write_long(pb + 32, actual_count as u32);
                    Self::write_cat_search_position(bus, pb, position);
                    return 0;
                }
            }
        }

        bus.write_long(pb + 32, actual_count as u32);
        Self::write_cat_search_position(bus, pb, entries.len());
        -39 // eofErr
    }

    fn lookup_catalog_entry(
        &mut self,
        dir_id: u32,
        filename: &str,
        fdir_index: i16,
    ) -> Option<super::dispatch::VfsCatalogEntry> {
        if fdir_index < 0 {
            let directory = self.directory_entry_for_id(dir_id)?;
            let path = self.directory_path_for_id(dir_id)?.to_string();
            return Some(super::dispatch::VfsCatalogEntry {
                path,
                name: super::TrapDispatcher::hfs_name_from_vfs_component(
                    &super::TrapDispatcher::vfs_directory_name(&directory.path),
                ),
                is_directory: true,
            });
        }

        if fdir_index > 0 {
            let entries = self.list_vfs_catalog_entries(dir_id);
            return entries.get(fdir_index as usize - 1).cloned();
        }

        // PBGetCatInfo with ioFDirIndex = 0 selects ioNamePtr. System 7.5.3
        // treats an empty name as the directory selected by ioDirID rather than
        // as a missing child. Files 1992, 2-190 to 2-192.
        if filename.is_empty() {
            let directory = self.directory_entry_for_id(dir_id)?;
            let path = self.directory_path_for_id(dir_id)?.to_string();
            return Some(super::dispatch::VfsCatalogEntry {
                path,
                name: super::TrapDispatcher::hfs_name_from_vfs_component(
                    &super::TrapDispatcher::vfs_directory_name(&directory.path),
                ),
                is_directory: true,
            });
        }

        if let Some(path) = self.find_vfs_directory_in_directory(dir_id, filename) {
            let directory = self
                .vfs_directories
                .iter()
                .find(|directory| directory.path.eq_ignore_ascii_case(&path))?;
            return Some(super::dispatch::VfsCatalogEntry {
                path: path.clone(),
                name: super::TrapDispatcher::hfs_name_from_vfs_component(
                    &super::TrapDispatcher::vfs_directory_name(&directory.path),
                ),
                is_directory: true,
            });
        }
        if let Some(path) = self.find_vfs_file_in_directory(dir_id, filename) {
            self.vfs_file_metadata(&path)?;
            return Some(super::dispatch::VfsCatalogEntry {
                path: path.clone(),
                name: super::TrapDispatcher::hfs_name_from_vfs_component(
                    super::TrapDispatcher::vfs_basename(&path),
                ),
                is_directory: false,
            });
        }
        if let Some(path) = self.find_vfs_rsrc_file_in_directory(dir_id, filename) {
            self.vfs_file_metadata(&path)?;
            return Some(super::dispatch::VfsCatalogEntry {
                path: path.clone(),
                name: super::TrapDispatcher::hfs_name_from_vfs_component(
                    super::TrapDispatcher::vfs_basename(&path),
                ),
                is_directory: false,
            });
        }

        None
    }

    fn list_vfs_file_catalog_entries(
        &mut self,
        dir_id: u32,
    ) -> Vec<super::dispatch::VfsCatalogEntry> {
        self.ensure_vfs_catalog();
        let Some(_) = self.directory_path_for_id(dir_id) else {
            return Vec::new();
        };

        let mut file_paths: Vec<String> = self.vfs_metadata.keys().cloned().collect();
        file_paths.sort_by_key(|path| path.to_ascii_lowercase());

        let mut entries = Vec::new();
        for path in file_paths {
            let Some(metadata) = self.vfs_metadata.get(&path).copied() else {
                continue;
            };
            if metadata.parent_dir_id != dir_id {
                continue;
            }
            entries.push(super::dispatch::VfsCatalogEntry {
                name: super::TrapDispatcher::hfs_name_from_vfs_component(
                    super::TrapDispatcher::vfs_basename(&path),
                ),
                path,
                is_directory: false,
            });
        }

        entries.sort_by_key(|entry| entry.name.to_ascii_lowercase());
        entries
    }

    fn lookup_file_entry(
        &mut self,
        dir_id: u32,
        filename: &str,
        fdir_index: i16,
    ) -> Option<super::dispatch::VfsCatalogEntry> {
        if fdir_index > 0 {
            let entries = self.list_vfs_file_catalog_entries(dir_id);
            return entries.get(fdir_index as usize - 1).cloned();
        }

        if !filename.is_empty() {
            if let Some(path) = self.find_vfs_file_in_directory(dir_id, filename) {
                self.vfs_file_metadata(&path)?;
                return Some(super::dispatch::VfsCatalogEntry {
                    path: path.clone(),
                    name: super::TrapDispatcher::hfs_name_from_vfs_component(
                        super::TrapDispatcher::vfs_basename(&path),
                    ),
                    is_directory: false,
                });
            }
        }

        None
    }

    fn lookup_file_entry_for_get_finfo(
        &mut self,
        vref: i16,
        dir_id: u32,
        filename: &str,
        fdir_index: i16,
    ) -> Option<super::dispatch::VfsCatalogEntry> {
        let mut candidate_dir_ids = self.hfs_lookup_directory_ids(vref, dir_id);
        let non_hfs_dir_id = self.resolve_directory_id(vref, 0);
        if !candidate_dir_ids.contains(&non_hfs_dir_id) {
            candidate_dir_ids.push(non_hfs_dir_id);
        }

        for candidate_dir_id in candidate_dir_ids {
            if let Some(entry) = self.lookup_file_entry(candidate_dir_id, filename, fdir_index) {
                return Some(entry);
            }
        }
        None
    }

    fn lookup_catalog_entry_for_hfs_lookup(
        &mut self,
        vref: i16,
        dir_id: u32,
        filename: &str,
        fdir_index: i16,
    ) -> Option<super::dispatch::VfsCatalogEntry> {
        for candidate_dir_id in self.hfs_lookup_directory_ids(vref, dir_id) {
            if let Some(entry) = self.lookup_catalog_entry(candidate_dir_id, filename, fdir_index) {
                return Some(entry);
            }
        }
        None
    }

    fn find_vfs_file_for_hfs_lookup(
        &mut self,
        vref: i16,
        dir_id: u32,
        filename: &str,
    ) -> Option<String> {
        for candidate_dir_id in self.hfs_lookup_directory_ids(vref, dir_id) {
            if let Some(path) = self.find_vfs_file_in_directory(candidate_dir_id, filename) {
                return Some(path);
            }
        }
        None
    }

    pub(crate) fn find_vfs_rsrc_file_for_hfs_lookup(
        &mut self,
        vref: i16,
        dir_id: u32,
        filename: &str,
    ) -> Option<String> {
        for candidate_dir_id in self.hfs_lookup_directory_ids(vref, dir_id) {
            if let Some(path) = self.find_vfs_rsrc_file_in_directory(candidate_dir_id, filename) {
                return Some(path);
            }
        }
        if let Some(path) = self.materialize_named_quilt_resource_file(filename) {
            return Some(path);
        }
        None
    }

    fn find_vfs_directory_for_hfs_lookup(
        &mut self,
        vref: i16,
        dir_id: u32,
        filename: &str,
    ) -> Option<String> {
        for candidate_dir_id in self.hfs_lookup_directory_ids(vref, dir_id) {
            if let Some(path) = self.find_vfs_directory_in_directory(candidate_dir_id, filename) {
                return Some(path);
            }
        }
        None
    }

    fn directory_id_for_vfs_path(&mut self, path: &str) -> Option<u32> {
        self.ensure_vfs_catalog();
        let normalized = super::TrapDispatcher::normalize_vfs_path(path);
        if normalized.is_empty() {
            return Some(2);
        }

        let mut sorted_directories: Vec<&ProcessVfsDirectory> =
            self.vfs_directories.iter().collect();
        sorted_directories.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        sorted_directories
            .into_iter()
            .find(|directory| directory.path.eq_ignore_ascii_case(&normalized))
            .map(|directory| directory.dir_id)
    }

    fn fsspec_parts_for_hfs_path(
        &mut self,
        vref: i16,
        dir_id: u32,
        filename: &str,
    ) -> Option<(i16, u32, String, String)> {
        let target_key = self.vfs_key_for_fsspec(vref, dir_id, filename)?;
        let parent_path = super::TrapDispatcher::vfs_parent_path(&target_key);
        let parent_dir_id = self.directory_id_for_vfs_path(parent_path)?;
        Some((
            self.resolve_volume_ref_num(vref),
            parent_dir_id,
            super::TrapDispatcher::hfs_name_from_vfs_component(
                super::TrapDispatcher::vfs_basename(&target_key),
            ),
            target_key,
        ))
    }

    pub(crate) fn vfs_key_for_fsspec(
        &mut self,
        vref: i16,
        dir_id: u32,
        filename: &str,
    ) -> Option<String> {
        let normalized = super::TrapDispatcher::normalize_hfs_lookup_path(filename);
        if normalized.is_empty() {
            return None;
        }
        if super::TrapDispatcher::is_unix_tmp_path(filename) {
            self.ensure_vfs_directory("Temporary Items");
        }

        // IM:Files 1992 pp. 2-28 and 2-34: File Manager pathname input can
        // be full or partial, but an FSSpec itself stores only vRefNum,
        // parent dirID, and the final name. If a multi-component pathname's
        // parent already exists from the volume root, prefer that canonical
        // VFS parent; otherwise resolve the pathname relative to vRefNum/dirID.
        if normalized.contains('/') {
            let absolute_parent = super::TrapDispatcher::vfs_parent_path(&normalized);
            if self.directory_id_for_vfs_path(absolute_parent).is_some() {
                return Some(normalized);
            }
        }

        let resolved_dir_id = self.resolve_directory_id(vref, dir_id);
        let dir_path = self.directory_path_for_id(resolved_dir_id)?;
        let candidate = if dir_path.is_empty() {
            normalized
        } else {
            format!("{dir_path}/{normalized}")
        };
        let candidate_parent = super::TrapDispatcher::vfs_parent_path(&candidate);
        self.directory_id_for_vfs_path(candidate_parent)?;
        Some(candidate)
    }

    /// Find a file in VFS by name, preserving explicit path components.
    ///
    /// Basename matching is retained for the classic search-path behavior of
    /// basename-only requests, but an explicit nested pathname must not fall
    /// through to an unrelated file with the same leaf name.
    pub(crate) fn find_vfs_file(&self, name: &str) -> Option<String> {
        let normalized = super::TrapDispatcher::normalize_vfs_path(name);
        let hfs_normalized = super::TrapDispatcher::normalize_hfs_path(name);
        // Sort key iteration so the first match is stable across runs.
        let mut sorted_keys: Vec<&String> = self.vfs.keys().collect();
        sorted_keys.sort_unstable();
        if let Some(found) = sorted_keys
            .iter()
            .copied()
            .find(|key| key.eq_ignore_ascii_case(&hfs_normalized))
        {
            return Some(found.clone());
        }
        if let Some(found) = sorted_keys.iter().copied().find(|key| {
            super::TrapDispatcher::normalize_vfs_path(key).eq_ignore_ascii_case(&normalized)
        }) {
            return Some(found.clone());
        }
        if let Some(found) = super::TrapDispatcher::find_case_insensitive_relative_key(
            sorted_keys.iter().copied(),
            &normalized,
        ) {
            return Some(found);
        }
        // Do not discard explicit directory components after exact and
        // relative-path matching fail. For example, a request for
        // `:Data Files:Data CD` must not open `Character Files/Data CD`.
        // Basename-only requests retain the historical search-path fallback.
        if !hfs_normalized.contains('/') && !normalized.contains('/') {
            let hfs_basename = hfs_normalized
                .rsplit('/')
                .next()
                .unwrap_or(hfs_normalized.as_str());
            for key in &sorted_keys {
                let key_base = key.rsplit('/').next().unwrap_or(key);
                if key_base.eq_ignore_ascii_case(hfs_basename) {
                    return Some((*key).clone());
                }
            }
            let basename = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
            for key in &sorted_keys {
                let key_base = key.rsplit('/').next().unwrap_or(key);
                if key_base.eq_ignore_ascii_case(basename) {
                    return Some((*key).clone());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;
