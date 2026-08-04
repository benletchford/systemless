//! Resource Manager and File Manager trap handlers.

use crate::cpu::{CpuOps, Register};
use crate::loader::CodeSegmentHeader;
use crate::machine_profile::REFERENCE_MACHINE_PROFILE;
use crate::managers::resource::ResourceFork;
use crate::memory::globals::addr;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::trap::types::{decode_mac_roman, encode_mac_roman_lossy, read_fsspec_name};
use crate::{Error, Result};

use std::collections::BTreeMap;
use std::sync::OnceLock;
static TRACE_MENU_PICT: OnceLock<bool> = OnceLock::new();
static TRACE_FSSPEC: OnceLock<bool> = OnceLock::new();
static TRACE_SOUND_RESOURCE: OnceLock<bool> = OnceLock::new();
static TRACE_GETRESOURCE: OnceLock<bool> = OnceLock::new();
static TRACE_LOADSEG: OnceLock<bool> = OnceLock::new();

const CURRENT_PROCESS_PSN_HIGH: u32 = 0;
const CURRENT_PROCESS_PSN_LOW: u32 = 2;
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
    file_type: u32,
    creator: u32,
    finder_flags: u16,
    data_length: u32,
    resource_length: u32,
    created_date: u32,
    modified_date: u32,
    backup_date: u32,
    locked: bool,
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
    pub(crate) const RES_NOT_FOUND: i16 = -192;
    pub(crate) const RES_ATTR_ERR: i16 = -198;
    pub(crate) const RES_CHANGED_ATTR: u16 = 0x0002;
    pub(crate) const RES_SYS_REF_ATTR: u16 = 0x0080;
    pub(crate) const RES_MAP_CHANGED_ATTR: u16 = 0x0020;
    pub(crate) const RES_MAP_READ_ONLY_ATTR: u16 = 0x0080;

    fn is_synthetic_driver_name(filename: &str) -> bool {
        matches!(
            filename,
            ".AIn" | ".AOut" | ".BIn" | ".BOut" | ".MPP" | ".ATP" | ".XPP"
        )
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

        let Some(handler_addr) = self.native_trap_table.get(&0xA9A0).copied() else {
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
        self.finish_loadseg(bus, cpu, state.seg_num, state.entry_addr, true)
    }

    fn finish_loadseg<C: CpuOps>(
        &mut self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
        seg_num: i16,
        entry_addr: u32,
        refresh_from_resource: bool,
    ) -> Result<()> {
        let trace_loadseg = trace_loadseg_enabled();
        if let Some(&seg_addr) = self.segment_map.get(&{ seg_num }) {
            if refresh_from_resource {
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
            let jmp_addr = Self::loadseg_entry_dispatch_pc(bus, entry_addr);
            if trace_loadseg {
                eprintln!("[TRAP] LoadSeg: re-executing JT entry at ${:08X}", jmp_addr);
            }
            cpu.write_reg(Register::PC, jmp_addr);
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
            if let Some((existing_ptr, existing_type, existing_id)) =
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
                    if bus.read_long(handle) == 0 && (self.res_load || recorded_resident) {
                        bus.write_long(handle, existing_ptr);
                        self.add_resource_materialization_tick_cost(bus, existing_ptr);
                    }
                    if bus.read_long(handle) != 0 && existing_ptr != 0 {
                        self.resident_resources.insert(key);
                        self.ptr_to_handle.insert(existing_ptr, handle);
                    }
                    return handle;
                }
            }
            self.resource_handles_by_key.remove(&key);
        }

        let handle = bus.alloc(4);
        // IM:More Macintosh Toolbox 1993, 1-79: SetResLoad(FALSE) returns
        // an empty handle (master pointer NIL) for resource data that is not
        // already in memory. Keep the true data pointer in loaded_handles so
        // LoadResource can populate the master pointer later.
        let materialize = self.res_load || recorded_resident;
        bus.write_long(handle, if materialize { ptr } else { 0 });
        self.loaded_handles.insert(handle, (ptr, res_type, res_id));
        self.resource_handle_files.insert(handle, refnum);
        self.remember_resource_handle_index(handle, key.0, key.1, key.2);
        if materialize && ptr != 0 {
            self.resident_resources.insert(key);
            self.ptr_to_handle.insert(ptr, handle);
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

    fn resource_ptr_referenced_elsewhere(&self, refnum: u16, ptr: u32) -> bool {
        self.resources.as_ref().is_some_and(|resources| {
            resources.files.iter().any(|(other_refnum, file)| {
                *other_refnum != refnum && file.loaded.values().any(|&p| p == ptr)
            })
        })
    }

    fn reload_resource_data_from_file(
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

        if let Some(file) = self
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
        if ptr != 0 || !self.res_load {
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
        self.ptr_to_handle.insert(ptr, handle);
        if let (Some((_, res_type, res_id)), Some(refnum)) = (
            self.loaded_handles.get(&handle).copied(),
            self.resource_handle_files.get(&handle).copied(),
        ) {
            self.resident_resources.insert((refnum, res_type, res_id));
        }
        self.detached_handles.remove(&handle);
        if let Some(refnum) = self.detached_handle_files.remove(&handle) {
            // A reload should repair any stale detached-file bookkeeping so
            // later Resource Manager queries still see a live resource.
            self.resource_handle_files.insert(handle, refnum);
        }
    }

    pub(crate) fn forget_resource_residency_for_handle(&mut self, handle: u32) {
        if let (Some((_, res_type, res_id)), Some(refnum)) = (
            self.loaded_handles.get(&handle).copied(),
            self.resource_handle_files.get(&handle).copied(),
        ) {
            self.resident_resources.remove(&(refnum, res_type, res_id));
        }
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
                if let Some(resources) = self.resources.as_mut() {
                    if let Some(file) = resources.files.get_mut(&refnum) {
                        file.loaded.remove(&(res_type, res_id));
                        file.attrs.remove(&(res_type, res_id));
                        file.names_by_id.remove(&(res_type, res_id));
                        file.named
                            .retain(|(t, _), (id, _)| !(*t == res_type && *id == res_id));
                    }
                }
                self.forget_resource_backing_data(refnum, res_type, res_id);
                bus.write_word(0x0A60, 0);
                true
            } else {
                if let Some(resources) = self.resources.as_mut() {
                    if let Some(file) = resources.files.get_mut(&current_refnum) {
                        file.loaded.remove(&(res_type, res_id));
                        file.attrs.remove(&(res_type, res_id));
                        file.names_by_id.remove(&(res_type, res_id));
                        file.named
                            .retain(|(t, _), (id, _)| !(*t == res_type && *id == res_id));
                    }
                }
                self.forget_resource_backing_data(current_refnum, res_type, res_id);
                self.forget_resource_handle_index_for_handle(handle);
                self.loaded_handles.remove(&handle);
                self.resource_handle_files.remove(&handle);
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

        let Some(resources) = self.resources.as_mut() else {
            bus.write_word(0x0A60, ADD_REF_FAILED as u16);
            return false;
        };
        let Some(file) = resources.files.get_mut(&current_refnum) else {
            bus.write_word(0x0A60, ADD_REF_FAILED as u16);
            return false;
        };

        if file.loaded.contains_key(&(res_type, new_id))
            || file
                .loaded
                .values()
                .any(|&existing_ptr| existing_ptr == ptr)
        {
            bus.write_word(0x0A60, ADD_REF_FAILED as u16);
            return false;
        }

        let attrs =
            (source_attrs as u8) | Self::RES_CHANGED_ATTR as u8 | Self::RES_SYS_REF_ATTR as u8;
        file.loaded.insert((res_type, new_id), ptr);
        file.attrs.insert((res_type, new_id), attrs);
        file.map_attrs |= Self::RES_MAP_CHANGED_ATTR;

        if name_ptr != 0 {
            let name_bytes = bus.read_pstring(name_ptr);
            if let Ok(name) = String::from_utf8(name_bytes) {
                if !name.is_empty() {
                    file.named.insert((res_type, name.clone()), (new_id, ptr));
                    file.names_by_id.insert((res_type, new_id), name);
                }
            }
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

        if let Some(resources) = self.resources.as_mut() {
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
            .ptr_to_handle
            .get(&ptr)
            .copied()
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
        Some(match (is_tool, trap_num) {
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
                        self.tick_count,
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

                if res_type == *b"WDEF" {
                    if let Some(ptr) = self.synthesize_system_wdef(bus, res_id) {
                        let handle = self.get_or_create_resource_handle_in_file(
                            bus, res_type, res_id, ptr, 0,
                        );
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
                        // Detaching severs the handle from the resource map. Keep the
                        // immutable backing bytes so a later GetResource can reload the
                        // original fork data, but do not let the file's live map keep
                        // pointing at this now-private buffer.
                        self.forget_resource_residency_for_handle(handle);
                        self.forget_resource_live_map_entry_for_handle(handle);
                        self.forget_resource_handle_index_for_handle(handle);
                        self.loaded_handles.remove(&handle);
                        if let Some(refnum) = self.resource_handle_files.remove(&handle) {
                            self.detached_handle_files.insert(handle, refnum);
                        }
                        self.detached_handles.insert(handle, (res_type, res_id));
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
                    // Successful reload must canonicalize the master
                    // pointer again even if the caller had emptied or
                    // scribbled over it first.
                    let was_empty = bus.read_long(handle) == 0;
                    bus.write_long(handle, ptr);
                    self.restore_loaded_resource_handle(handle, ptr);
                    if was_empty {
                        self.add_resource_materialization_tick_cost(bus, ptr);
                    }
                    bus.write_word(0x0A60, 0);
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
                        self.ptr_to_handle.remove(&ptr);
                        self.forget_resource_handle_index_for_handle(handle);
                        self.loaded_handles.remove(&handle);
                        self.resource_handle_files.remove(&handle);
                        self.handle_state_bits.remove(&handle);
                        if can_reload {
                            if let Some((refnum, record_type, record_id)) = resource_record {
                                if let Some(file) = self
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
                                    if let Some(name) =
                                        file.names_by_id.get(&(record_type, record_id)).cloned()
                                    {
                                        if file
                                            .named
                                            .get(&(record_type, name.clone()))
                                            .is_some_and(|(_, named_ptr)| *named_ptr == ptr)
                                        {
                                            file.named.insert((record_type, name), (record_id, 0));
                                        }
                                    }
                                }
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

                if let Some(file) = self
                    .resources
                    .as_mut()
                    .and_then(|resources| resources.files.get_mut(&refnum))
                {
                    file.map_attrs = attrs & 0x00E0;
                }
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
            // Selector lives in D0; the dispatcher matches the low
            // byte. IM:VI lists the selectors as $0001/$0002/$0003;
            // MMTB 1993 lists them as $7001/$7002/$7003. Both forms
            // route to the same routine because Apple's dispatcher
            // consults only the low byte. The high byte is an MPW
            // glue marker, not a parameter-bytes hint (param sizes
            // here are 16/16/8 bytes — none of which equals
            // 2 × 0x70 = 224).
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
            // ResourceDispatch ($A822): D0-low-byte selectors: 1=ReadPartialResource, 2=WritePartialResource, 3=SetResourceSize per MMTB 1-69..1-71. HLE is memory-resident so all three operate on the in-memory allocation directly; reports inputOutOfBounds/writingPastEnd/resNotFound per IM.
            (true, 0x022) => {
                let sp = cpu.read_reg(Register::A7);
                let routine = (cpu.read_reg(Register::D0) & 0xFF) as u8;
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
            // Like Get1Resource vs. GetResource, Get1NamedResource only
            // searches the current resource file, while GetNamedResource
            // walks the full resource chain. Both must write ResErr per
            // IM:I I-119.
            //
            // Get1NamedResource ($A820): Searches by Pascal name string; writes ResErr per IM:IV IV-15
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
                let handle =
                    self.find_named_resource_current_loaded(bus, res_type, &name)
                        .map(|(refnum, id, ptr)| {
                            self.get_or_create_resource_handle_in_file(
                                bus, res_type, id, ptr, refnum,
                            )
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
                    bus.write_word(0x0A60, (-192i16) as u16); // ResErr = resNotFound
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
                let refnum = self.current_resource_refnum();
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
                    let refnum = self
                        .resource_handle_files
                        .get(&handle)
                        .copied()
                        .unwrap_or(0);
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

                let is_loadseg_trampoline = self
                    .tool_trap_trampolines
                    .get(&0xA9F0)
                    .copied()
                    .is_some_and(|addr| addr == a9f0_addr);

                let auto_pop_old_trap = (self.current_trap_word & 0x0400) != 0;
                let original_trap_return = self.current_trap_caller;
                let native_call = (is_loadseg_trampoline && auto_pop_old_trap)
                    .then(|| self.pending_native_trap_calls.remove(&0xA9F0))
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

                let (seg_num, entry_addr, fmt, refresh_from_resource) =
                    if native_old_trap_standard {
                        let call = native_call.as_ref().unwrap();
                        let sn = bus.read_word(call.argument_sp) as i16;
                        (
                            sn,
                            call.return_pc.wrapping_sub(8),
                            "mpw-native-oldtrap",
                            true,
                        )
                    } else if native_old_trap_think {
                        let call = native_call.as_ref().unwrap();
                        let entry = call.return_pc.wrapping_sub(2);
                        let sn = bus.read_word(entry + 6) as i16;
                        (sn, entry, "thinkc-native-oldtrap", true)
                    } else if old_trap_standard {
                        let return_pc = original_trap_return.unwrap();
                        let sn = bus.read_word(sp) as i16;
                        cpu.write_reg(Register::A7, sp + 2);
                        (sn, return_pc.wrapping_sub(8), "mpw-oldtrap", true)
                    } else if old_trap_think {
                        let return_pc = original_trap_return.unwrap();
                        let entry = return_pc.wrapping_sub(2);
                        let sn = bus.read_word(entry + 6) as i16;
                        (sn, entry, "thinkc-oldtrap", true)
                    } else if is_standard {
                        // Standard: seg# was pushed by MOVE.W, pop it
                        let sn = bus.read_word(sp) as i16;
                        cpu.write_reg(Register::A7, sp + 2);
                        // Entry starts 6 bytes before A9F0
                        (sn, a9f0_addr.wrapping_sub(6), "mpw", false)
                    } else {
                        // Think C: A9F0 at entry+0, seg# at entry+6, offset at entry+4
                        // Stack has JSR return address (don't pop segment number)
                        let entry = a9f0_addr; // A9F0 IS entry+0
                        let sn = bus.read_word(entry + 6) as i16;
                        (sn, entry, "thinkc", false)
                    };

                let trace_loadseg = trace_loadseg_enabled();
                if trace_loadseg {
                    eprintln!(
                        "[TRAP] LoadSeg(seg={}, fmt={}) entry=${:08X}",
                        seg_num, fmt, entry_addr
                    );
                }

                if refresh_from_resource {
                    self.preserve_auto_pop_pc_once = auto_pop_old_trap;
                    self.finish_loadseg(bus, cpu, seg_num, entry_addr, true)
                } else if self.maybe_inject_native_getresource_for_loadseg(
                    bus,
                    cpu,
                    seg_num,
                    entry_addr,
                    trace_loadseg,
                ) {
                    Ok(())
                } else {
                    self.finish_loadseg(bus, cpu, seg_num, entry_addr, false)
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
                        if let Some(resources) = self.resources.as_mut() {
                            if let Some(file) = resources.files.get_mut(&refnum) {
                                let entry = file.attrs.entry((res_type, res_id)).or_insert(0);
                                *entry |= Self::RES_CHANGED_ATTR as u8;
                                file.map_attrs |= Self::RES_MAP_CHANGED_ATTR;
                            }
                        }
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

                        // Update the resource ID in loaded_handles for the live
                        // home record only. System-reference shadow entries live
                        // in the current file's map and must not re-home the
                        // source handle.
                        if home_refnum == Some(refnum) {
                            if let Some(entry) = self.loaded_handles.get_mut(&handle) {
                                entry.2 = new_id;
                            }
                            if old_id != new_id {
                                self.resource_handles_by_key
                                    .remove(&(refnum, res_type, old_id));
                                self.remember_resource_handle_index(
                                    handle, refnum, res_type, new_id,
                                );
                            }
                        }
                        if old_id != new_id {
                            if let Some(data) = self
                                .resource_backing_data
                                .remove(&(refnum, res_type, old_id))
                            {
                                self.remember_resource_backing_data(refnum, res_type, new_id, data);
                            }
                        }

                        if let Some(resources) = self.resources.as_mut() {
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
                                if name_ptr != 0 {
                                    // Remove old named entry for this resource
                                    file.named.retain(|(t, _), (id, _)| {
                                        !(*t == res_type && *id == old_id)
                                    });
                                    // Read Pascal string from name_ptr
                                    let name_bytes = bus.read_pstring(name_ptr);
                                    if let Ok(name_str) = String::from_utf8(name_bytes) {
                                        if !name_str.is_empty() {
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
                                        }
                                    }
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

                    // Register in loaded_handles and resource_handle_files
                    self.loaded_handles.insert(handle, (ptr, res_type, res_id));
                    self.resource_handle_files.insert(handle, refnum);
                    self.remember_resource_handle_index(handle, refnum, res_type, res_id);

                    if let Some(resources) = self.resources.as_mut() {
                        if let Some(file) = resources.files.get_mut(&refnum) {
                            // Add to loaded map
                            file.loaded.insert((res_type, res_id), ptr);
                            // Set resChanged attribute
                            let entry = file.attrs.entry((res_type, res_id)).or_insert(0);
                            *entry |= Self::RES_CHANGED_ATTR as u8;
                            file.map_attrs |= Self::RES_MAP_CHANGED_ATTR;

                            // Add named entry if name_ptr != 0
                            if name_ptr != 0 {
                                let name_bytes = bus.read_pstring(name_ptr);
                                if let Ok(name_str) = String::from_utf8(name_bytes) {
                                    if !name_str.is_empty() {
                                        file.named
                                            .insert((res_type, name_str.clone()), (res_id, ptr));
                                        file.names_by_id.insert((res_type, res_id), name_str);
                                    }
                                }
                            }
                        }
                    }
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
                    if existing_rsrc_has_map {
                        // MTb 1993 p. 1-57: existing resource fork with a
                        // resource map -> no-op + dupFNErr.
                        -48 // dupFNErr
                    } else {
                        // If a data fork already exists, add or initialise its
                        // zero-length resource fork map. Otherwise create both
                        // forks.
                        self.vfs.entry(vfs_key.clone()).or_default();
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
            // Per IM:I I-441: "SystemEvent is called only by the
            // Toolbox Event Manager function GetNextEvent when it
            // receives an event, to determine whether the event
            // should be handled by the application or by the system.
            // If the given event should be handled by the
            // application, SystemEvent returns FALSE; otherwise, it
            // calls the appropriate system code to handle the event
            // and returns TRUE."
            //
            // Per IM:I I-441 dispatch matrix:
            //   - null / mouse-down events: returns FALSE (lets app
            //     run FindWindow then optionally call SystemClick).
            //   - mouse-up / keyboard events: TRUE iff active window
            //     is DA-owned and DA can handle it; else FALSE.
            //   - activate / update events: TRUE iff event window is
            //     DA-owned and DA can handle it; else FALSE.
            // Systemless's HLE has no DA-owned windows so EVERY path
            // resolves to FALSE — apps correctly receive every event
            // and perform their own dispatch.
            // FUNCTION SystemEvent(theEvent: EventRecord): BOOLEAN;
            // Inside Macintosh Volume I, I-441
            //
            // Stack: SP+0..15 theEvent EventRecord by VALUE (16
            // bytes — IM:I I-251 layout: what(2) + message(4) +
            // when(4) + where Point(4) + modifiers(2)), SP+16
            // result BOOLEAN (2-byte FUNCTION result slot pre-pushed
            // by caller). Pop 16; result at post-pop A7 = pre-call
            // SP+16. Returns FALSE per IM:I I-441 "app handles event"
            // path — corpus games' GetNextEvent → SystemEvent →
            // app-event-loop sequence proceeds correctly.
            //
            // Manager classification corrected: was "Resource Manager
            // — Toolbox Traps" (an artifact of where the arm landed
            // in the dispatch chain) but per IM:I-441 chapter heading
            // "Handling Events in Desk Accessories" this trap belongs
            // to the Desk Accessory family — same family rationale
            // block at toolbox.rs $A9B6 OpenDeskAcc applies.
            // SystemEvent ($A9B2): Pops 16-byte EventRecord by VALUE per IM:I I-251 layout + writes FALSE to 2-byte BOOLEAN result slot at post-pop A7 per IM:I I-441 PROCEDURE sig; HLE has no DA-owned windows so FALSE matches the IM-documented "event should be handled by the application" path for every event class (null/mouse-down/mouse-up/keyboard/activate/update).
            (true, 0x1B2) => {
                let sp = cpu.read_reg(Register::A7);
                // EventRecord = 16 bytes by value, result = 2 bytes
                bus.write_word(sp + 16, 0); // FALSE
                cpu.write_reg(Register::D0, 0);
                cpu.write_reg(Register::A7, sp + 16);
                Ok(())
            }

            // Gestalt family: _Gestalt ($A1AD), _NewGestalt ($A3AD),
            // _ReplaceGestalt ($A5AD). All three OS traps share the
            // low byte $AD and so reach the same dispatch arm — the
            // dispatcher only forwards bits 0..7 for OS traps. We
            // distinguish them by reading the original 16-bit trap
            // word from `current_trap_word` (set by the dispatcher
            // before each handler runs).
            //
            // Trap word table per Inside Macintosh Volume VI, p. 6-308.
            // FUNCTION/Pascal signatures and register conventions per
            // Inside Macintosh: Operating System Utilities 1994,
            // 1-31..1-35.
            //
            // Gestalt ($A0AD): documented selectors including vers/sysv/evnt/cput/proc/mach/kbd/qd/qdrw/ram/fpu/mmu/snd/ttsc/te/tmgr/alis/fs/fold/qtim/drag/os/powr/appr/addr/sdev/stdf/help/vm; guest-installed selector functions (NewGestalt/ReplaceGestalt) are registered but not invokable from a trap handler
            (false, 0xAD) => {
                let selector = cpu.read_reg(Register::D0);
                let sel = selector.to_be_bytes();
                match self.current_trap_word {
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
                    0xA3AD => {
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
                    0xA5AD => {
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
                    // _Gestalt ($A0AD/$A1AD): fall through to the query
                    // handler below.
                    _ => {}
                }
                let s = std::str::from_utf8(&sel).unwrap_or("????");
                eprintln!(
                    "[GESTALT] selector='{}' (${:08X}) PC=${:08X}",
                    s,
                    selector,
                    cpu.read_reg(Register::PC)
                );
                match &sel {
                    // gestaltVersion ('vers') -> Gestalt Manager version.
                    // Inside Macintosh: Operating System Utilities 1994,
                    // p. 1-25: current version is 1, returned as $0001 in
                    // the low-order word. Marathon 1 probes this before
                    // drawing its first visible frame.
                    b"vers" => {
                        cpu.write_reg(Register::A0, 0x0001);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltSystemVersion ('sysv') -> System 7.5.3
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
                        cpu.write_reg(Register::A0, 1);
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
                            REFERENCE_MACHINE_PROFILE.gestalt_native_cpu_type,
                        );
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltProcessorType ('proc') -> 68040
                    b"proc" => {
                        cpu.write_reg(
                            Register::A0,
                            REFERENCE_MACHINE_PROFILE.gestalt_processor_type,
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
                        cpu.write_reg(Register::A0, REFERENCE_MACHINE_PROFILE.gestalt_fpu_type);
                        cpu.write_reg(Register::D0, 0);
                    }
                    // gestaltMMUType ('mmu ') -> 68040 MMU
                    b"mmu " => {
                        cpu.write_reg(Register::A0, REFERENCE_MACHINE_PROFILE.gestalt_mmu_type);
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
                    // gestaltFSAttr ('fs  ') -> has FSSpec calls
                    b"fs  " => {
                        cpu.write_reg(Register::A0, 1);
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
                        cpu.write_reg(Register::A0, 0b111);
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

                // Look up in VFS (try exact match, then basename match)
                if let Some(vfs_name) = self.find_vfs_file(&filename) {
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
                    let refnum = self.next_refnum;
                    self.next_refnum += 1;
                    self.open_files.insert(refnum, vfs_name.clone());
                    // Files 1992, 2-8: fsCurPerm (0) grants read/write when
                    // write access is available; fsWrPerm (2), fsRdWrPerm
                    // (3), and fsRdWrShPerm (4) explicitly request it.
                    // PBGetFCBInfo exposes the granted mode through bit 8 of
                    // ioFCBFlags (Files 1992, 2-108).
                    if matches!(permission, 0 | 2 | 3 | 4) && !read_only {
                        self.write_refnums.insert(refnum);
                    }
                    self.file_positions.insert(refnum, 0);
                    bus.write_word(pb + 24, refnum);
                    bus.write_word(pb + 16, 0); // noErr
                    cpu.write_reg(Register::D0, 0);
                    eprintln!("[TRAP] PBOpen -> refnum={} vfs=\"{}\"", refnum, vfs_name);
                } else if Self::is_synthetic_driver_name(&filename)
                    || self.has_vfs_driver_name(&filename)
                {
                    let refnum = self.next_refnum;
                    self.next_refnum += 1;
                    self.synthetic_drivers.insert(refnum, filename.clone());
                    bus.write_word(pb + 24, refnum);
                    bus.write_word(pb + 16, 0);
                    cpu.write_reg(Register::D0, 0);
                    eprintln!(
                        "[TRAP] PBOpen -> synthetic driver refnum={} name=\"{}\"",
                        refnum, filename
                    );
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
                let async_call = (self.current_trap_word & 0x0400) != 0;
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
                        let cur_pos = *self.file_positions.get(&ref_num).unwrap_or(&0);
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
                        super::dispatch::PendingFileCompletion {
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
                    ref_num, buffer, request_count, pos_mode, pos_offset
                );

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

                let (new_pos, host_sync_bytes) = {
                    let file_buf = self.vfs.entry(filename.clone()).or_default();
                    let file_len = file_buf.len();
                    let cur_pos = *self.file_positions.get(&ref_num).unwrap_or(&0);
                    let Ok(start) =
                        Self::resolve_file_mark_position(pos_mode, pos_offset, cur_pos, file_len)
                    else {
                        bus.write_long(pb + 40, 0);
                        bus.write_long(pb + 46, cur_pos as u32);
                        bus.write_word(pb + 16, (-40i16) as u16); // posErr
                        cpu.write_reg(Register::D0, (-40i32) as u32);
                        return Some(Ok(()));
                    };

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
                };

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
                let volume_ref_num = self.resolve_volume_ref_num(self.app_wd_refnum);
                let volume_name = self
                    .vfs_volume_for_ref_num(volume_ref_num)
                    .map(|volume| volume.name.as_str())
                    .unwrap_or(super::TrapDispatcher::boot_volume_name());
                bus.write_word(pb + 22, self.app_wd_refnum as u16);
                let name_ptr = bus.read_long(pb + 18);
                if name_ptr != 0 {
                    Self::write_pstring(bus, name_ptr, volume_name);
                }
                // HGetVol fields: ioWDProcID and ioWDVRefNum and ioWDDirID
                // Always populate these so both PBGetVol and HGetVol callers work.
                bus.write_long(pb + 28, 0); // ioWDProcID
                bus.write_word(pb + 32, volume_ref_num as u16); // ioWDVRefNum
                let dir_id = self
                    .working_directory_info(self.app_wd_refnum)
                    .map(|wd| wd.dir_id)
                    .unwrap_or(self.default_dir_id);
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
                let volume_index = bus.read_word(pb + 28) as i16;
                let requested_vref = bus.read_word(pb + 22) as i16;
                let requested_name = Self::read_pb_filename(bus, bus.read_long(pb + 18));
                let boot_volume = || {
                    Some((
                        super::TrapDispatcher::boot_volume_name().to_string(),
                        super::TrapDispatcher::boot_volume_ref_num(),
                        0u16,
                    ))
                };
                let volume_info = if volume_index == 0 {
                    // Files 1992, 2-145: ioVolIndex == 0 selects a volume by
                    // ioNamePtr or ioVRefNum instead of enumerating the VCB
                    // queue. Do not silently turn an unknown vRefNum into the
                    // boot volume; callers use nsvErr to detect a bad volume.
                    if !requested_name.is_empty() {
                        if requested_name
                            .eq_ignore_ascii_case(super::TrapDispatcher::boot_volume_name())
                        {
                            boot_volume()
                        } else {
                            self.vfs_volume_by_name(&requested_name).map(|volume| {
                                (volume.name.clone(), volume.ref_num, volume.attributes)
                            })
                        }
                    } else {
                        let volume_ref_num = if requested_vref == 0 {
                            Some(self.resolve_volume_ref_num(self.app_wd_refnum))
                        } else if requested_vref == super::TrapDispatcher::boot_volume_ref_num()
                            || self.vfs_volumes.contains_key(&requested_vref)
                        {
                            Some(requested_vref)
                        } else {
                            self.working_directories
                                .get(&requested_vref)
                                .map(|working_directory| working_directory.volume_ref_num)
                        };
                        volume_ref_num.and_then(|volume_ref_num| {
                            if volume_ref_num == super::TrapDispatcher::boot_volume_ref_num() {
                                boot_volume()
                            } else {
                                self.vfs_volume_for_ref_num(volume_ref_num).map(|volume| {
                                    (volume.name.clone(), volume.ref_num, volume.attributes)
                                })
                            }
                        })
                    }
                } else if volume_index > 0 {
                    let mut mounted = self
                        .vfs_volumes
                        .values()
                        .map(|volume| (volume.name.clone(), volume.ref_num, volume.attributes))
                        .collect::<Vec<_>>();
                    mounted.sort_by_key(|(_, ref_num, _)| std::cmp::Reverse(*ref_num));
                    let mut volumes = vec![
                        (
                            super::TrapDispatcher::boot_volume_name().to_string(),
                            super::TrapDispatcher::boot_volume_ref_num(),
                            0u16,
                        ),
                    ];
                    volumes.extend(mounted);
                    volumes.get((volume_index - 1) as usize).cloned()
                } else {
                    None
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
                let volume_file_count = mounted_info
                    .map(|volume| volume.file_count)
                    .filter(|count| *count != 0)
                    .unwrap_or(100);
                let allocation_block_count = mounted_info
                    .map(|volume| volume.allocation_block_count)
                    .filter(|count| *count != 0)
                    .unwrap_or(BOOT_VOLUME_ALLOCATION_BLOCKS);
                let allocation_block_size = mounted_info
                    .map(|volume| volume.allocation_block_size)
                    .filter(|size| *size != 0)
                    .unwrap_or(BOOT_VOLUME_ALLOCATION_BLOCK_SIZE);
                let clump_size = mounted_info
                    .map(|volume| volume.clump_size)
                    .filter(|size| *size != 0)
                    .unwrap_or(BOOT_VOLUME_ALLOCATION_BLOCK_SIZE);
                let free_blocks = mounted_info
                    .map(|volume| volume.free_blocks)
                    .filter(|count| *count != 0)
                    .unwrap_or(BOOT_VOLUME_FREE_BLOCKS);
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
                let volume_ordinal = (-volume_ref_num - 1).max(0) as u16;
                let volume_driver_ref =
                    super::TrapDispatcher::vfs_driver_ref_num(volume_ref_num);

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
                bus.write_word(pb + 64, 0x4244); // ioVSigWord (HFS)
                bus.write_word(pb + 66, volume_ordinal.saturating_add(1)); // ioVDrvInfo
                bus.write_word(pb + 68, volume_driver_ref as u16); // ioVRefNum (driver)
                bus.write_word(pb + 70, 0); // ioVFSID (File Manager)
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
                    let cur_pos = *self.file_positions.get(&ref_num).unwrap_or(&0);
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
                    bus.write_long(pb + 46, pos as u32); // ioPosOffset
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
                    || self.vfs_volumes.contains_key(&vref)
                    || self.working_directories.contains_key(&vref);
                let known_by_name = !name.is_empty()
                    && name.eq_ignore_ascii_case(super::TrapDispatcher::boot_volume_name());
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
                // Open access paths must follow the rename so a
                // subsequent FSRead/FSWrite still resolves to the file.
                let open_refnums: Vec<u16> = self
                    .open_files
                    .iter()
                    .filter_map(|(refnum, name)| {
                        if name == &old_key {
                            Some(*refnum)
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
                    self.locked_files.insert(vfs_name);
                    bus.write_word(pb + 16, 0); // noErr
                    cpu.write_reg(Register::D0, 0);
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
                    self.locked_files.remove(&vfs_name);
                    bus.write_word(pb + 16, 0); // noErr
                    cpu.write_reg(Register::D0, 0);
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
                let is_hfs_variant = (self.current_trap_word & 0x0F00) == 0x0200;
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
                    self.vfs_rsrc.entry(existing.clone()).or_default();
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
                        .filter_map(|(refnum, open_name)| {
                            if open_name == &vfs_name {
                                Some(*refnum)
                            } else {
                                None
                            }
                        })
                        .collect();
                    for refnum in stale_refs {
                        self.open_files.remove(&refnum);
                        self.file_positions.remove(&refnum);
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
                    if let Some(metadata) = self.vfs_file_metadata(&vfs_name) {
                        self.fill_file_catalog_info(bus, pb, &vfs_name, metadata);
                        if name_ptr != 0 {
                            let guest_name = super::TrapDispatcher::hfs_name_from_vfs_component(
                                super::TrapDispatcher::vfs_basename(&vfs_name),
                            );
                            Self::write_pstring(bus, name_ptr, &guest_name);
                        }
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

            // PBSetVol / PBHSetVol ($A015 / $A215)
            // Sets the default volume and optionally the default directory.
            // FUNCTION PBSetVol  (paramBlock: ParmBlkPtr; async: Boolean): OSErr;
            // FUNCTION PBHSetVol (paramBlock: WDPBPtr;    async: Boolean): OSErr;
            // Inside Macintosh: Files (1992), pp. 2-151 to 2-154.
            //
            // PBSetVol subsection: IM:Files 1992 pp. 2-151 to 2-153.
            // PBHSetVol subsection: IM:Files 1992 pp. 2-153 to 2-154 (HFS
            //   variant; trap macro `_HSetVol`, ONEWORDINLINE(0xA215)).
            // The OS-trap dispatcher masks `trap & 0x00FF`, so $A215
            // PBHSetVol and $A015 PBSetVol land on the same low byte and
            // share this arm.
            //
            // Register convention (OS-bit FUNCTION, IM:II 1985 p. II-114):
            //   On entry: A0 = ParmBlkPtr / WDPBPtr (paramBlock)
            //   On exit:  D0 = OSErr (also mirrored to pb.ioResult @ pb+16)
            //
            // Parameter block field map (basic ParamBlockRec / WDPBRec):
            //   pb+12 ioCompletion (→)  IOCompletionUPP, NIL for sync
            //   pb+16 ioResult     (←)  OSErr (also returned in D0)
            //   pb+18 ioNamePtr    (→)  StringPtr; NIL for vRefNum-only call
            //   pb+22 ioVRefNum    (→)  Vol / working-directory refnum
            //   pb+48 ioWDDirID    (→)  HFS dirID (WDPBRec only)
            //
            // Result codes (IM:Files 1992 p. 2-137 + pp. 2-153..2-154):
            //   noErr     0   No error
            //   nsvErr   -35  Volume not found
            //   bdNamErr -37  Bad filename
            //   paramErr -50  Bad ioVRefNum / ioWDDirID combination
            //
            // MPW Universal Headers Files.h:
            //   #pragma parameter __D0 PBHSetVolSync(__A0)
            //   EXTERN_API(OSErr) PBHSetVolSync(WDPBPtr paramBlock)
            //       ONEWORDINLINE(0xA215);
            //   #define PBHSetVol(pb, async) (async ? PBHSetVolAsync(pb)
            //                                       : PBHSetVolSync(pb))
            //
            // Systemless HLE behaviour (this arm):
            //   * Resolves ioVRefNum via working-directory table, then
            //     dirID lookup, then default-directory fallback, then
            //     boot-volume fallback, else returns nsvErr (-35) per
            //     IM:Files 2-137 and 2-153..2-154 documented contract.
            //   * On the success path, updates self.default_dir_id and
            //     self.app_wd_refnum, and keeps the Standard File globals
            //     (CurDirStore @ $0398, SFSaveDisk @ $0214) aligned with
            //     the new default per IM:Files 1992 p. 3-65.
            //   * Writes the OSErr to BOTH D0 AND pb.ioResult per the
            //     File Manager basic-PB dispatcher convention, regardless
            //     of whether the absolute value is noErr or nsvErr.
            //
            // Behavioral invariant:
            //   * Dispatcher convention: D0 == ioResult after the call,
            //     and ioResult overwrites any pre-call sentinel, regardless
            //     of the absolute OSErr.
            //   * Register-only OS-bit FUNCTION calling convention:
            //     A0 input, D0 output, no Pascal stack frame consumed;
            //     A7 preserved across the call.
            //
            // Absolute-OSErr divergence: Systemless returns nsvErr (-35)
            // on the unrecognised-volume path; BasiliskII may return -35
            // or another OSErr depending on its volume table state. The
            // dispatcher-convention invariant holds either way.
            //
            // Regression coverage:
            //   pbhsetvol_sets_default_directory_from_iowddirid_for_volume_refnum_calls
            //   pbhsetvol_invalid_vrefnum_returns_nsverr
            //   pbhsetvol_writes_same_oserr_to_d0_and_ioresult_preserving_stack
            //
            // trap-doc: $A015 | PBSetVol | Partial | File Manager & Gestalt — OS Traps | Updates default volume/directory; nsvErr for unrecognised vRefNum (IM:Files 1992, 2-162)
            // PBHSetVol ($A215): HFS variant aliased onto $A015
            (false, 0x15) => {
                let pb = cpu.read_reg(Register::A0);
                let name_ptr = bus.read_long(pb + 18);
                let name = Self::read_pb_filename(bus, name_ptr);
                let requested_vref = bus.read_word(pb + 22) as i16;
                let is_hfs_set_vol = (self.current_trap_word & 0x0F00) == 0x0200;
                let requested_dir_id = if is_hfs_set_vol {
                    bus.read_long(pb + 48)
                } else {
                    0
                };

                let named_volume = (!name.is_empty()).then(|| {
                    if name.eq_ignore_ascii_case(super::TrapDispatcher::boot_volume_name()) {
                        Some((
                            super::TrapDispatcher::boot_volume_ref_num(),
                            2u32,
                        ))
                    } else {
                        self.vfs_volume_by_name(&name)
                            .map(|volume| (volume.ref_num, volume.root_dir_id))
                    }
                }).flatten();
                let mut target_volume_ref_num = named_volume
                    .map(|(volume_ref_num, _)| volume_ref_num)
                    .unwrap_or_else(|| self.resolve_volume_ref_num(requested_vref));
                let mut target_dir_id = if let Some((_, root_dir_id)) = named_volume {
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
                    self.default_dir_id
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

                if !name.is_empty() {
                    if let Some(path) = self.find_vfs_directory_in_directory(target_dir_id, &name) {
                        if let Some(directory) = self.vfs_directories.get(&path) {
                            target_dir_id = directory.dir_id;
                        }
                    } else if let Some(path) = self.find_vfs_file_in_directory(target_dir_id, &name)
                    {
                        if let Some(metadata) = self.vfs_file_metadata(&path) {
                            target_dir_id = metadata.parent_dir_id;
                        }
                    }
                }

                self.default_dir_id = target_dir_id;
                self.app_wd_refnum = if target_dir_id == 2 {
                    target_volume_ref_num
                } else {
                    self.open_working_directory(target_volume_ref_num, target_dir_id, 0)
                        .unwrap_or(target_volume_ref_num)
                };

                // Keep the Standard File globals aligned with the current
                // default directory and volume. Files 1992, 3-65.
                bus.write_long(addr::CUR_DIR_STORE, target_dir_id);
                bus.write_word(addr::SF_SAVE_DISK, (-target_volume_ref_num) as u16);

                eprintln!(
                    "[TRAP] PBSetVol name='{}' request_vRefNum={} request_dirID={} -> current_vRefNum={} current_dirID={}",
                    name,
                    requested_vref,
                    requested_dir_id,
                    self.app_wd_refnum,
                    self.default_dir_id
                );
                bus.write_word(pb + 16, 0);
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // _CmpString ($A03C) — underlying trap for EqualString
            // FUNCTION EqualString (aStr, bStr: Str255; caseSens, diacSens: BOOLEAN): BOOLEAN;
            // Inside Macintosh Volume II, II-377
            //
            // On entry: A0 = ptr to first string's first character
            //           A1 = ptr to second string's first character
            //           D0 high word = length of first string
            //           D0 low word  = length of second string
            // On exit:  D0 = 0 if equal, 1 if not equal (long word).
            //
            // The default trap word $A03C is case-INSENSITIVE for ASCII
            // (per IM:II II-377 the bare _CmpString has caseSens=FALSE).
            // The MARKS / CASE trap-word bits 9 and 10 select different
            // sensitivities; Systemless currently handles only the default.
            //
            // EqualString / CmpString ($A03C): String comparison per IM:II II-377
            // Trap word bit 10 (0x0400): caseSens=TRUE → case-sensitive comparison.
            // Default $A03C (bit 10 clear): caseSens=FALSE → case-insensitive.
            // Trap word bit 9 (0x0200): diacSens flag (strip diacriticals when set).
            // D0=0 means equal; D0=1 means not equal (EqualString Pascal glue inverts).
            (false, 0x3C) => {
                let a_ptr = cpu.read_reg(Register::A0);
                let b_ptr = cpu.read_reg(Register::A1);
                let d0 = cpu.read_reg(Register::D0);
                let a_len = (d0 >> 16) & 0xFFFF;
                let b_len = d0 & 0xFFFF;
                let case_sens = (self.current_trap_word & 0x0400) != 0;

                let result: u32 = if a_len != b_len {
                    1
                } else {
                    let a_bytes = bus.read_bytes(a_ptr, a_len as usize);
                    let b_bytes = bus.read_bytes(b_ptr, b_len as usize);
                    if case_sens {
                        // Case-sensitive: exact byte comparison.
                        if a_bytes == b_bytes {
                            0
                        } else {
                            1
                        }
                    } else {
                        // Case-insensitive: fold ASCII letters before comparing.
                        if a_bytes.eq_ignore_ascii_case(&b_bytes) {
                            0
                        } else {
                            1
                        }
                    }
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

                let host_sync_bytes = {
                    let file_buf = self.vfs.entry(filename.clone()).or_default();
                    file_buf.resize(new_eof, 0);
                    if self.output_dir.is_some() {
                        Some(file_buf.clone())
                    } else {
                        None
                    }
                };

                if let Some(pos) = self.file_positions.get_mut(&ref_num) {
                    if *pos > new_eof {
                        *pos = new_eof;
                    }
                }

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
            // Dispatches Temporary Memory and Process Manager selectors.
            // FUNCTION TempMaxMem(VAR grow: Size): Size;
            // FUNCTION TempTopMem: Ptr;
            // FUNCTION TempFreeMem: LongInt;
            // FUNCTION TempNewHandle(logicalSize: Size; VAR resultCode: OSErr): Handle;
            // PROCEDURE TempHLock(h: Handle; VAR resultCode: OSErr);
            // PROCEDURE TempHUnlock(h: Handle; VAR resultCode: OSErr);
            // PROCEDURE TempDisposeHandle(h: Handle; VAR resultCode: OSErr);
            // Inside Macintosh Volume VI, 28-38 and 28-45.
            // FUNCTION AcceptHighLevelEvent(VAR sender: TargetID; VAR msgRefcon:
            //   LongInt; msgBuff: Ptr; VAR msgLen: LongInt): OSErr;
            // FUNCTION PostHighLevelEvent(theEvent: EventRecord; receiverID: Ptr;
            //   msgRefcon: LongInt; msgBuff: Ptr; msgLen: LongInt;
            //   postingOptions: LongInt): OSErr;
            // FUNCTION GetProcessSerialNumberFromPortName(portName: PPCPortRec;
            //   VAR PSN: ProcessSerialNumber): OSErr;
            // Inside Macintosh Volume VI, 5-29..5-32.
            // FUNCTION LaunchDeskAccessory(pFileSpec: FSSpecPtr;
            //   pDAName: StringPtr): OSErr;
            // Inside Macintosh Volume VI, 29-22..29-23.
            // FUNCTION GetCurrentProcess(VAR PSN: ProcessSerialNumber): OSErr;
            // Processes 1994, 2-21 to 2-24.
            //
            // MPW emits each ProcessMgr selector as
            //   MOVE.W #<sel>, -(SP)     ; push selector word on stack
            //   _OSDispatch
            // So selector is the TOP-OF-STACK word at trap entry. A few
            // pre-existing direct-asm callers instead pre-load D0 before
            // the trap -- if the top-of-stack word isn't a recognised
            // selector, fall back to D0 for compatibility.
            // MPW Universal Interfaces 3.4 MacMemory.a emits the 68k
            // temporary-memory macros as `move.w #selector,-(sp); _OSDispatch`.
            // OSDispatch ($A88F): selectors $0015 TempMaxMem, $0016
            // TempTopMem, $0018 TempFreeMem, $001D TempNewHandle,
            // $001E TempHLock, $001F TempHUnlock, and $0020
            // TempDisposeHandle (IM VI Temporary Memory table);
            // selectors $0033..$0036 are high-level event / desk accessory
            // routines (IM VI pp. 5-29..5-32 and 29-23); selectors
            // $0037..$003D are Process Manager routines (Processes 1994 p. 2-31).
            (true, 0x08F) => {
                let sp_entry = cpu.read_reg(Register::A7);
                let stack_sel = bus.read_word(sp_entry) as u32 & 0xFFFF;
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
                                self.ptr_to_handle.insert(ptr, handle);
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
                            let bits = self.handle_state_bits.entry(handle).or_insert(0);
                            if selector == 0x001E {
                                *bits |= 0x80;
                            } else {
                                *bits &= !0x80;
                                if *bits == 0 {
                                    self.handle_state_bits.remove(&handle);
                                }
                            }
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
                            self.ptr_to_handle.remove(&ptr);
                            bus.free(ptr);
                            bus.free(handle);
                            self.handle_state_bits.remove(&handle);
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
                        write_osdispatch_oseerr_result(cpu, bus, sp + 16, NO_OUTSTANDING_HLE_ERR);
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
                        bus.write_long(psn_ptr, CURRENT_PROCESS_PSN_HIGH); // highLongOfPSN
                        bus.write_long(psn_ptr + 4, CURRENT_PROCESS_PSN_LOW); // lowLongOfPSN = kCurrentProcess
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
                        let psn_high = bus.read_long(psn_ptr);
                        let psn_low = bus.read_long(psn_ptr.wrapping_add(4));
                        let err: i16 = if psn_high == 0 && psn_low == 0 {
                            bus.write_long(psn_ptr, CURRENT_PROCESS_PSN_HIGH);
                            bus.write_long(psn_ptr.wrapping_add(4), CURRENT_PROCESS_PSN_LOW);
                            0
                        } else if psn_high == CURRENT_PROCESS_PSN_HIGH
                            && psn_low == CURRENT_PROCESS_PSN_LOW
                        {
                            bus.write_long(psn_ptr, CURRENT_PROCESS_PSN_HIGH);
                            bus.write_long(psn_ptr.wrapping_add(4), 0);
                            -600
                        } else {
                            -50
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
                        // After selector pop: sp+0=init(4B), sp+4=psn_ptr(4B).
                        // Result word at sp+6; A7 = sp+6 so MOVE.W (SP)+ restores stack.
                        let psn_ptr = bus.read_long(sp + 4);
                        bus.write_long(psn_ptr, CURRENT_PROCESS_PSN_HIGH);
                        bus.write_long(psn_ptr.wrapping_add(4), CURRENT_PROCESS_PSN_LOW);
                        bus.write_word(sp + 6, 0);
                        cpu.write_reg(Register::A7, sp + 6);
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
                        let psn_high = bus.read_long(psn_ptr);
                        let psn_low = bus.read_long(psn_ptr + 4);
                        let valid_psn = psn_high == CURRENT_PROCESS_PSN_HIGH
                            && psn_low == CURRENT_PROCESS_PSN_LOW;

                        if !valid_psn {
                            bus.write_word(sp + 8, (-600i16) as u16); // procNotFound
                            cpu.write_reg(Register::A7, sp + 8);
                            return Some(Ok(()));
                        }

                        let app_path = self.launched_app_path.clone().unwrap_or_default();
                        let app_name = if app_path.is_empty() {
                            "Application".to_string()
                        } else {
                            super::TrapDispatcher::hfs_name_from_vfs_component(
                                super::TrapDispatcher::vfs_basename(&app_path),
                            )
                        };
                        let (app_type, app_creator, app_parent_dir_id) =
                            if let Some(metadata) = self.vfs_file_metadata(&app_path) {
                                (metadata.file_type, metadata.creator, metadata.parent_dir_id)
                            } else {
                                (
                                    u32::from_be_bytes(*b"APPL"),
                                    u32::from_be_bytes(*b"????"),
                                    2,
                                )
                            };

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
                            bus.write_long(app_spec_ptr + 2, app_parent_dir_id);
                            Self::write_pstring(bus, app_spec_ptr + 6, &app_name);
                        }

                        bus.write_long(info_ptr + 8, CURRENT_PROCESS_PSN_HIGH); // processNumber.highLongOfPSN
                        bus.write_long(info_ptr + 12, CURRENT_PROCESS_PSN_LOW); // processNumber.lowLongOfPSN
                        bus.write_long(info_ptr + 16, app_type);
                        bus.write_long(info_ptr + 20, app_creator);
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
                        let p1_high = bus.read_long(psn1_ptr);
                        let p1_low = bus.read_long(psn1_ptr + 4);
                        let p2_high = bus.read_long(psn2_ptr);
                        let p2_low = bus.read_long(psn2_ptr + 4);
                        let same = p1_high == p2_high && p1_low == p2_low;
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
            // HighLevelFSDispatch ($AA52): Selectors: 1=FSMakeFSSpec, 2=FSpOpenDF,
            // 4=FSpCreate, 5=FSpDirCreate, 6=FSpDelete, 9=FSpSetFLock,
            // 10=FSpRstFLock, 13=FSpOpenResFile, 14=FSpCreateResFile
            (true, 0x252) => {
                let selector = cpu.read_reg(Register::D0) & 0xFFFF;
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
                                    .keys()
                                    .any(|path| path.eq_ignore_ascii_case(key))
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
                            let refnum = self.next_refnum;
                            self.next_refnum += 1;
                            self.open_files.insert(refnum, vfs_name.clone());
                            // Files 1992, 2-17 to 2-18: fsRdWrPerm is retried
                            // as browsing access when exclusive access is not
                            // available. Preserve the successful read path,
                            // but do not grant FSWrite access to a read-only
                            // extracted volume.
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
                            self.vfs_rsrc.entry(vfs_key.clone()).or_default();
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
                                let duplicate = self
                                    .vfs_directories
                                    .keys()
                                    .any(|path| path.eq_ignore_ascii_case(&child_path))
                                    || self
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
                        let found_on_host = if let Some(ref dir) = self.output_dir {
                            let host_path = host_vfs_key
                                .map(|key| dir.join(key))
                                .unwrap_or_else(|| dir.join(&filename));
                            std::fs::remove_file(&host_path).is_ok()
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
                            -43i16 // fnfErr
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
                            self.locked_files.insert(vfs_name);
                            0
                        } else if self.find_vfs_file(&filename).is_some() {
                            let normalized = super::TrapDispatcher::normalize_hfs_path(&filename);
                            self.locked_files.insert(normalized);
                            0
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
                        if let Some(vfs_name) =
                            self.find_vfs_file_for_hfs_lookup(vref, dir_id, &filename)
                        {
                            self.locked_files.remove(&vfs_name);
                        } else if let Some(vfs_name) = self.find_vfs_file(&filename) {
                            self.locked_files.remove(&vfs_name);
                        }
                        eprintln!("[TRAP] FSpRstFLock(\"{}\")", filename);
                        bus.write_word(sp + 4, 0u16); // noErr
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
                            bus.write_word(0x0A60, 0); // ResErr = noErr
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
                            self.vfs.entry(vfs_key.clone()).or_default();
                            self.vfs_rsrc.entry(vfs_key.clone()).or_default();
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
                    _ => {
                        eprintln!(
                            "[TRAP] HighLevelFSDispatch: Unimplemented Selector {}",
                            selector
                        );
                        Err(Error::Halted)
                    }
                }
            }

            // FSDispatch ($A060) / HFSDispatch ($A260)
            // HFSDispatch / FSDispatch ($A060): Selectors 1=PBOpenWD, 2=PBCloseWD, 6=PBDirCreate, 7=PBGetWDInfo, 8=PBGetFCBInfo, 9=PBGetCatInfo, 26=PBHOpenDF; by-name lookups retry via default/WD directory
            (false, 0x60) => {
                let selector = cpu.read_reg(Register::D0) & 0xFFFF;
                let pb = cpu.read_reg(Register::A0);
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
                    let base_dir_id = self.resolve_directory_id(vref, requested_dir_id);
                    // A PBOpenWD caller may pass ":" to name the directory
                    // already identified by ioWDDirID. In a partial HFS
                    // pathname, a lone colon is the current directory, not
                    // a child directory literally named ":".
                    let effective_dir_id = if name.is_empty() || name == ":" {
                        base_dir_id
                    } else if let Some(path) =
                        self.find_vfs_directory_in_directory(base_dir_id, &name)
                    {
                        self.vfs_directories
                            .get(&path)
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
                    //
                    // Behavioral note: for a phantom ioVRefNum=-999, real
                    // Mac returns noErr (0) per IM:Files 10366 ("If you
                    // specify a volume reference number in the ioVRefNum
                    // field, PBCloseWD does nothing") — the close-side mirror
                    // of PBGetWDInfo's polymorphic-input divergence (where
                    // real Mac returned nsvErr -35 instead of rfNumErr
                    // -51). Systemless returns rfNumErr (-51) on the
                    // HashMap-miss fallback path below.
                    let wd_ref_num = bus.read_word(pb + 22) as i16;
                    eprintln!("[TRAP] FSDispatch PBCloseWD wdRefNum={}", wd_ref_num);
                    let err = if wd_ref_num == super::TrapDispatcher::boot_volume_ref_num()
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
                                    self.default_dir_id,
                                    0,
                                )
                                .unwrap_or(super::TrapDispatcher::boot_volume_ref_num()),
                            volume_ref_num: super::TrapDispatcher::boot_volume_ref_num(),
                            dir_id: self.default_dir_id,
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
                let dir_id = bus.read_long(pb + 48);
                let filename = Self::read_pb_filename(bus, name_ptr);
                let fdir_index = bus.read_word(pb + 28) as i16;
                eprintln!(
                    "[TRAP] FSDispatch selector={} name_ptr=${:08X} filename=\"{}\" vref={} dirID={} fdirIdx={}",
                    selector, name_ptr, filename, vref, dir_id, fdir_index
                );
                let resolved_vref = self.resolve_volume_ref_num(vref);

                if selector == 0x18 {
                    // PBCatSearch ($A260, selector $0018). The read-only VFS
                    // presents a deterministic catalog view and preserves
                    // ioCatPosition across bounded calls.
                    // FUNCTION PBCatSearch(paramBlock: HParmBlkPtr; async: Boolean): OSErr;
                    // Inside Macintosh: Files 1992, 2-204 to 2-206.
                    let result = self.perform_cat_search(bus, pb, resolved_vref);
                    bus.write_word(pb + 16, result as u16);
                    cpu.write_reg(Register::D0, result as i32 as u32);
                    return Some(Ok(()));
                }

                if selector == 6 {
                    // PBDirCreate ($A260, selector 6)
                    // Creates a new directory and returns its directory ID in ioDirID.
                    // FUNCTION PBDirCreate(paramBlock: HParmBlkPtr; async: BOOLEAN): OSErr;
                    // Inside Macintosh Volume IV, IV-146
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
                        let child_name = super::TrapDispatcher::normalize_hfs_path(&filename);
                        if child_name.is_empty() {
                            -37i16 // bdNamErr
                        } else {
                            let child_path = if parent_path.is_empty() {
                                child_name
                            } else {
                                format!("{parent_path}/{child_name}")
                            };
                            let duplicate = self
                                .vfs_directories
                                .keys()
                                .any(|path| path.eq_ignore_ascii_case(&child_path))
                                || self
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
                        self.launched_app_path
                            .clone()
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
                            .min(file_len);
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
                        bus.write_long(pb + 48, current_pos as u32); // ioFCBCrPs
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
                    let lookup = self
                        .lookup_catalog_entry_for_hfs_lookup(vref, dir_id, &filename, fdir_index);

                    if let Some(entry) = lookup {
                        if entry.is_directory {
                            if let Some(directory) = self.vfs_directories.get(&entry.path) {
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

                // Selector 26 = PBHOpenDF: open data fork by name
                // PBHOpenDF opens the data fork of a file specified by name, vRefNum, and dirID.
                // Files 1992, 2-271
                //
                // Per IM:Files 9590..9598 the documented errors are
                // noErr / nsvErr / ioErr / bdNamErr / tmfoErr / fnfErr (-43) /
                // opWrErr / permErr / dirNFErr / afpAccessDenied; the impl
                // below writes -43 fnfErr on lookup miss.
                if selector == 26 {
                    // Clear ioRefNum upfront. Files 1992 leaves the output
                    // undefined on failure, but MPW's FSOpen glue writes
                    // `*refNum = pb.ioRefNum` regardless of the result
                    // code — clearing makes the failure path deterministic.
                    bus.write_word(pb + 24, 0);
                    if filename.is_empty() {
                        eprintln!("[TRAP] FSDispatch PBHOpenDF(\"\") -> bdNamErr");
                        bus.write_word(pb + 16, (-37i16) as u16);
                        cpu.write_reg(Register::D0, (-37i32) as u32);
                        return Some(Ok(()));
                    }
                    if let Some(vfs_name) =
                        self.find_vfs_file_for_hfs_lookup(vref, dir_id, &filename)
                    {
                        let refnum = self.next_refnum;
                        self.next_refnum += 1;
                        self.open_files.insert(refnum, vfs_name.clone());
                        self.file_positions.insert(refnum, 0);
                        bus.write_word(pb + 24, refnum); // ioRefNum
                        bus.write_word(pb + 16, 0);
                        cpu.write_reg(Register::D0, 0);
                        eprintln!(
                            "[TRAP] FSDispatch PBHOpenDF(\"{}\") -> refnum={} vfs=\"{}\"",
                            filename, refnum, vfs_name
                        );
                        return Some(Ok(()));
                    } else {
                        eprintln!("[TRAP] FSDispatch PBHOpenDF(\"{}\") -> fnfErr", filename);
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
    /// memory-resident HLE — every handle we know about points at a
    /// real guest-bus allocation. Both traps therefore return the
    /// same byte count from `bus.get_alloc_size`.
    fn handle_resource_size_query<C: CpuOps>(
        &self,
        bus: &mut MacMemoryBus,
        cpu: &mut C,
    ) -> Result<()> {
        let sp = cpu.read_reg(Register::A7);
        let handle = bus.read_long(sp);

        let size: i32 = if let Some((ptr, _, _)) = self.loaded_handles.get(&handle).copied() {
            // IM:I I-121 keys validity on whether the handle is a Resource
            // Manager handle, not on whether the handle's master pointer is
            // currently non-NIL (SetResLoad(FALSE) can return empty handles).
            bus.get_alloc_size(ptr).map(|s| s as i32).unwrap_or(-1)
        } else {
            -1
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
            self.ptr_to_handle.remove(&live_ptr);
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
    /// see the new allocation.
    pub(crate) fn resize_resource_allocation(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        old_ptr: u32,
        new_size: u32,
    ) -> u32 {
        let old_size = bus.get_alloc_size(old_ptr).unwrap_or(0);
        let aligned_old = (old_size + 3) & !3;
        let aligned_new = (new_size + 3) & !3;
        if aligned_new <= aligned_old {
            // Fits in the existing aligned bucket; just retag the
            // logical size. Mirrors the SetHandleSize fast path.
            bus.set_alloc_size(old_ptr, new_size);
            return old_ptr;
        }
        let new_ptr = bus.alloc(new_size);
        if new_ptr == 0 {
            return 0;
        }
        let copy_len = old_size.min(new_size) as usize;
        if copy_len > 0 {
            let bytes = bus.read_bytes(old_ptr, copy_len);
            bus.write_bytes(new_ptr, &bytes);
        }
        bus.free(old_ptr);
        bus.write_long(handle, new_ptr);
        self.ptr_to_handle.remove(&old_ptr);
        self.ptr_to_handle.insert(new_ptr, handle);
        if let Some(entry) = self.loaded_handles.get_mut(&handle) {
            entry.0 = new_ptr;
        }
        if let Some(resources) = self.resources.as_mut() {
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
        new_ptr
    }

    /// Read a Pascal string filename from a parameter block's ioNamePtr.
    pub(crate) fn read_pb_filename(bus: &MacMemoryBus, name_ptr: u32) -> String {
        if name_ptr == 0 {
            return String::new();
        }
        decode_mac_roman(&bus.read_pstring(name_ptr))
    }

    fn write_pstring(bus: &mut MacMemoryBus, ptr: u32, value: &str) {
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
            u32::from_be_bytes(*b"fold"),
            u32::from_be_bytes(*b"MACS"),
            0,
        );
        bus.write_long(pb + 48, directory.dir_id);
        let child_directory_count = self
            .vfs_directories
            .values()
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

    fn cat_search_entries(&mut self) -> Vec<CatSearchEntry> {
        self.ensure_vfs_catalog();
        let mut entries = Vec::new();

        for (path, directory) in &self.vfs_directories {
            let name = if path.is_empty() {
                Self::boot_volume_name().to_string()
            } else {
                super::TrapDispatcher::hfs_name_from_vfs_component(
                    super::TrapDispatcher::vfs_basename(path),
                )
            };
            let vref_num = self
                .vfs_volume_for_path(path)
                .map(|volume| volume.ref_num)
                .unwrap_or(super::TrapDispatcher::boot_volume_ref_num());
            entries.push(CatSearchEntry {
                name,
                vref_num,
                parent_dir_id: directory.parent_dir_id,
                is_directory: true,
                file_type: u32::from_be_bytes(*b"fold"),
                creator: u32::from_be_bytes(*b"MACS"),
                finder_flags: 0,
                data_length: 0,
                resource_length: 0,
                created_date: 0,
                modified_date: 0,
                backup_date: 0,
                locked: false,
            });
        }

        for (path, metadata) in &self.vfs_metadata {
            let vref_num = self
                .vfs_volume_for_path(path)
                .map(|volume| volume.ref_num)
                .unwrap_or(super::TrapDispatcher::boot_volume_ref_num());
            entries.push(CatSearchEntry {
                name: super::TrapDispatcher::hfs_name_from_vfs_component(
                    super::TrapDispatcher::vfs_basename(path),
                ),
                vref_num,
                parent_dir_id: metadata.parent_dir_id,
                is_directory: false,
                file_type: metadata.file_type,
                creator: metadata.creator,
                finder_flags: metadata.finder_flags,
                data_length: self.vfs.get(path).map_or(0, |data| data.len() as u32),
                resource_length: self.vfs_rsrc.get(path).map_or(0, |data| data.len() as u32),
                created_date: metadata.created_date,
                modified_date: metadata.modified_date,
                backup_date: 0,
                locked: self.locked_files.contains(path),
            });
        }

        entries.sort_by_key(|entry| {
            (
                entry.vref_num,
                entry.parent_dir_id,
                entry.name.to_ascii_lowercase(),
                entry.is_directory,
            )
        });
        entries
    }

    fn cat_search_entry_matches(
        bus: &MacMemoryBus,
        entry: &CatSearchEntry,
        search_bits: u32,
        info1: u32,
        info2: u32,
    ) -> bool {
        let mut matches = true;
        let lower_name = if info1 != 0 {
            Self::read_pb_filename(bus, bus.read_long(info1 + 18)).to_ascii_lowercase()
        } else {
            String::new()
        };
        if search_bits & 1 != 0 && !lower_name.is_empty() {
            matches &= entry.name.to_ascii_lowercase().contains(&lower_name);
        }
        if search_bits & 2 != 0 && !lower_name.is_empty() {
            matches &= entry.name.eq_ignore_ascii_case(&lower_name);
        }

        if search_bits & 4 != 0 && info1 != 0 && info2 != 0 {
            let actual = u32::from((entry.is_directory as u8) << 4 | entry.locked as u8);
            let desired = u32::from(bus.read_byte(info1 + 30));
            let mask = u32::from(bus.read_byte(info2 + 30)) & 0x11;
            matches &= (actual ^ desired) & mask == 0;
        }

        if search_bits & 8 != 0 && info1 != 0 && info2 != 0 {
            let actual_type = entry.file_type;
            let expected_type = bus.read_long(info1 + 32);
            let type_mask = bus.read_long(info2 + 32);
            let actual_creator = entry.creator;
            let expected_creator = bus.read_long(info1 + 36);
            let creator_mask = bus.read_long(info2 + 36);
            let actual_flags = u32::from(entry.finder_flags);
            let expected_flags = u32::from(bus.read_word(info1 + 40));
            let flags_mask = u32::from(bus.read_word(info2 + 40));
            matches &= (actual_type ^ expected_type) & type_mask == 0;
            matches &= (actual_creator ^ expected_creator) & creator_mask == 0;
            matches &= (actual_flags ^ expected_flags) & flags_mask == 0;
        }

        let range_matches = |bit: u32, offset: u32, value: u32| {
            if search_bits & bit == 0 || info1 == 0 || info2 == 0 {
                true
            } else {
                value >= bus.read_long(info1 + offset) && value <= bus.read_long(info2 + offset)
            }
        };
        matches &= range_matches(32, 54, entry.data_length);
        matches &= range_matches(64, 58, entry.data_length);
        matches &= range_matches(128, 64, entry.resource_length);
        matches &= range_matches(256, 68, entry.resource_length);
        matches &= range_matches(512, 72, entry.created_date);
        matches &= range_matches(1024, 76, entry.modified_date);
        matches &= range_matches(2048, 80, entry.backup_date);
        matches &= range_matches(8192, 100, entry.parent_dir_id);

        if search_bits & 16384 != 0 {
            !matches
        } else {
            matches
        }
    }

    fn write_cat_search_fsspec(bus: &mut MacMemoryBus, spec_ptr: u32, entry: &CatSearchEntry) {
        bus.write_word(spec_ptr, entry.vref_num as u16);
        bus.write_long(spec_ptr + 2, entry.parent_dir_id);
        let bytes = encode_mac_roman_lossy(&entry.name);
        let len = bytes.len().min(63);
        bus.write_byte(spec_ptr + 6, len as u8);
        for (index, byte) in bytes.into_iter().take(len).enumerate() {
            bus.write_byte(spec_ptr + 7 + index as u32, byte);
        }
    }

    fn perform_cat_search(&mut self, bus: &mut MacMemoryBus, pb: u32, vref_num: i16) -> i16 {
        let match_ptr = bus.read_long(pb + 24);
        let requested_count = bus.read_long(pb + 28) as usize;
        let search_bits = bus.read_long(pb + 36);
        let info1 = bus.read_long(pb + 40);
        let info2 = bus.read_long(pb + 44);
        let start = bus.read_long(pb + 52) as usize;
        if match_ptr == 0 {
            bus.write_long(pb + 32, 0);
            return -50; // paramErr
        }

        let entries = self
            .cat_search_entries()
            .into_iter()
            .filter(|entry| entry.vref_num == vref_num)
            .collect::<Vec<_>>();
        let start = start.min(entries.len());
        let limit = requested_count.max(1);
        let mut actual_count = 0usize;
        let mut next = start;
        while next < entries.len() && actual_count < limit {
            let entry = &entries[next];
            if Self::cat_search_entry_matches(bus, entry, search_bits, info1, info2) {
                Self::write_cat_search_fsspec(bus, match_ptr + (actual_count as u32) * 70, entry);
                actual_count += 1;
            }
            next += 1;
        }

        bus.write_long(pb + 32, actual_count as u32);
        if next < entries.len() {
            bus.write_long(pb + 52, next as u32);
            0 // noErr: the caller can resume at ioCatPosition.
        } else {
            bus.write_long(pb + 52, entries.len() as u32);
            -39 // eofErr: the catalog is exhausted.
        }
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
                name: super::TrapDispatcher::hfs_name_from_vfs_component(&directory.name),
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
                name: super::TrapDispatcher::hfs_name_from_vfs_component(&directory.name),
                is_directory: true,
            });
        }

        if let Some(path) = self.find_vfs_directory_in_directory(dir_id, filename) {
            let directory = self.vfs_directories.get(&path)?;
            return Some(super::dispatch::VfsCatalogEntry {
                path: path.clone(),
                name: super::TrapDispatcher::hfs_name_from_vfs_component(&directory.name),
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

    fn find_vfs_rsrc_file_for_hfs_lookup(
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

        let mut sorted_keys: Vec<&String> = self.vfs_directories.keys().collect();
        sorted_keys.sort_unstable();
        sorted_keys
            .into_iter()
            .find(|key| key.eq_ignore_ascii_case(&normalized))
            .and_then(|key| self.vfs_directories.get(key).map(|dir| dir.dir_id))
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
        let normalized = super::TrapDispatcher::normalize_hfs_path(filename);
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
        let normalized = normalized
            .strip_prefix(&format!("{}/", super::TrapDispatcher::boot_volume_name()))
            .unwrap_or(&normalized)
            .to_string();
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

    /// Find a file in VFS by name, trying exact match then basename match.
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
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::dispatch::{LoadedResources, ResourceFileMap};
    use super::super::test_helpers::{setup, TEST_SP};
    use super::QUICKTIME_NUM_VERSION_6_0_FINAL;
    use crate::cpu::{CpuOps, Register};
    use crate::memory::globals::addr;
    use crate::memory::MemoryBus;
    use std::collections::HashMap;

    use super::super::test_helpers::MockCpu;

    // ================================================================
    // Helper: call dispatch_resource and unwrap
    // ================================================================
    fn call(
        dispatcher: &mut super::super::TrapDispatcher,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
    ) -> crate::Result<()> {
        dispatcher
            .dispatch_resource(is_tool, trap_num, cpu, bus)
            .expect("trap arm should be handled")
    }

    fn call_trap_word(
        dispatcher: &mut super::super::TrapDispatcher,
        trap_word: u16,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
    ) -> crate::Result<()> {
        dispatcher.dispatch(trap_word, cpu, bus)
    }

    #[test]
    fn initpack_pops_pack_id_and_records_last_pack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_word(sp, 0x1234);
        bus.write_long(sp + 2, 0xDEAD_BEEF);

        let sp_pre = cpu.read_reg(Register::A7);
        let result = call_trap_word(&mut disp, 0xA9E5, &mut cpu, &mut bus);
        assert!(result.is_ok(), "InitPack should return cleanly");
        assert_eq!(cpu.read_reg(Register::A7), sp_pre + 2);
        assert_eq!(disp.last_init_pack_id, Some(0x1234_i16));
        assert_eq!(bus.read_long(sp + 2), 0xDEAD_BEEF);
    }

    #[test]
    fn phantom_script_aliases_clear_d0_and_balance_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // AA87 GetScript alias: selector word + script word + result slot.
        bus.write_word(sp, 0x0001);
        bus.write_word(sp + 2, 0x1234);
        bus.write_word(sp + 4, 0xBEEF);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);
        call_trap_word(&mut disp, 0xAA87, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0), 0, "AA87 clears D0");
        assert_eq!(bus.read_word(sp + 4), 0, "AA87 writes zero result");
        assert_eq!(cpu.read_reg(Register::A7), sp + 4, "AA87 pops 4 bytes");

        // AA88 SetScript alias: value, selector, script; no result slot.
        bus.write_word(sp, 0x5678);
        bus.write_word(sp + 2, 0x0002);
        bus.write_word(sp + 4, 0x1234);
        bus.write_word(sp + 6, 0xCAFE);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);
        call_trap_word(&mut disp, 0xAA88, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0), 0, "AA88 clears D0");
        assert_eq!(
            bus.read_word(sp + 6),
            0xCAFE,
            "AA88 leaves trailing memory untouched"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 6, "AA88 pops 6 bytes");

        // AA89 GetEnvirons alias: selector word + result slot.
        bus.write_word(sp, 0x0005);
        bus.write_word(sp + 2, 0xBEEF);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);
        call_trap_word(&mut disp, 0xAA89, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0), 0, "AA89 clears D0");
        assert_eq!(bus.read_word(sp + 2), 0, "AA89 writes zero result");
        assert_eq!(cpu.read_reg(Register::A7), sp + 2, "AA89 pops 2 bytes");
    }

    /// Write a Pascal string (length-prefixed) into memory at `addr`.
    fn write_pstring(bus: &mut crate::memory::MacMemoryBus, addr: u32, s: &[u8]) {
        bus.write_byte(addr, s.len() as u8);
        for (i, &b) in s.iter().enumerate() {
            bus.write_byte(addr + 1 + i as u32, b);
        }
    }

    /// Write an FSSpec at `spec_ptr`: vRefNum(2) + dirID(4) + name(pstring, 64 bytes).
    fn write_fsspec(
        bus: &mut crate::memory::MacMemoryBus,
        spec_ptr: u32,
        vref: u16,
        dir_id: u32,
        name: &[u8],
    ) {
        bus.write_word(spec_ptr, vref);
        bus.write_long(spec_ptr + 2, dir_id);
        let len = name.len().min(63) as u8;
        bus.write_byte(spec_ptr + 6, len);
        for (i, &byte) in name.iter().take(len as usize).enumerate() {
            bus.write_byte(spec_ptr + 7 + i as u32, byte);
        }
    }

    fn make_single_resource_fork_bytes(res_type: [u8; 4], res_id: i16, data: &[u8]) -> Vec<u8> {
        let data_offset = 16u32;
        let data_length = (4 + data.len()) as u32;
        let map_offset = data_offset + data_length;
        let type_list_offset = 30u16;
        let ref_list_offset = 10u16;
        let name_list_offset = 40u16;
        let map_length = 52u32;

        let mut bytes = vec![0u8; (map_offset + map_length) as usize];
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&data_offset.to_be_bytes());
        header[4..8].copy_from_slice(&map_offset.to_be_bytes());
        header[8..12].copy_from_slice(&data_length.to_be_bytes());
        header[12..16].copy_from_slice(&map_length.to_be_bytes());
        bytes[0..16].copy_from_slice(&header);

        let data_start = data_offset as usize;
        bytes[data_start..data_start + 4].copy_from_slice(&(data.len() as u32).to_be_bytes());
        bytes[data_start + 4..data_start + 4 + data.len()].copy_from_slice(data);

        let map_start = map_offset as usize;
        bytes[map_start..map_start + 16].copy_from_slice(&header);
        bytes[map_start + 16..map_start + 20].copy_from_slice(&0u32.to_be_bytes());
        bytes[map_start + 20..map_start + 22].copy_from_slice(&0u16.to_be_bytes());
        bytes[map_start + 22..map_start + 24].copy_from_slice(&0u16.to_be_bytes());
        bytes[map_start + 24..map_start + 26].copy_from_slice(&type_list_offset.to_be_bytes());
        bytes[map_start + 26..map_start + 28].copy_from_slice(&name_list_offset.to_be_bytes());
        bytes[map_start + 28..map_start + 30].copy_from_slice(&0u16.to_be_bytes());

        let type_list_start = map_start + type_list_offset as usize;
        bytes[type_list_start..type_list_start + 2].copy_from_slice(&0u16.to_be_bytes());
        bytes[type_list_start + 2..type_list_start + 6].copy_from_slice(&res_type);
        bytes[type_list_start + 6..type_list_start + 8].copy_from_slice(&0u16.to_be_bytes());
        bytes[type_list_start + 8..type_list_start + 10]
            .copy_from_slice(&ref_list_offset.to_be_bytes());

        let ref_list_start = map_start + type_list_offset as usize + ref_list_offset as usize;
        bytes[ref_list_start..ref_list_start + 2].copy_from_slice(&(res_id as u16).to_be_bytes());
        bytes[ref_list_start + 2..ref_list_start + 4].copy_from_slice(&0xFFFFu16.to_be_bytes());
        bytes[ref_list_start + 4] = 0;
        bytes[ref_list_start + 5..ref_list_start + 8].copy_from_slice(&0u32.to_be_bytes()[1..4]);

        bytes
    }

    #[test]
    fn systemevent_mouse_down_returns_false_and_preserves_stack_and_d0() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // EventRecord (16 bytes by value): mouseDown event.
        bus.write_word(sp, 1); // what = mouseDown
        bus.write_long(sp + 2, 0x1122_3344); // message
        bus.write_long(sp + 6, 0x5566_7788); // when
        bus.write_long(sp + 10, 0x99AA_BBCC); // where
        bus.write_word(sp + 14, 0x0102); // modifiers
        bus.write_word(sp + 16, 0xBEEF); // result sentinel

        let result = disp.dispatch_resource(true, 0x1B2, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(bus.read_word(sp + 16), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    /// Set up loaded resources with one entry.
    fn setup_resources(
        dispatcher: &mut super::super::TrapDispatcher,
        bus: &mut crate::memory::MacMemoryBus,
        res_type: &[u8; 4],
        res_id: i16,
        data: &[u8],
    ) -> u32 {
        let data_ptr = bus.alloc(data.len() as u32);
        bus.write_bytes(data_ptr, data);
        let mut loaded = HashMap::new();
        loaded.insert((*res_type, res_id), data_ptr);
        let file = ResourceFileMap {
            loaded,
            named: HashMap::new(),
            names_by_id: HashMap::new(),
            attrs: HashMap::new(),
            map_attrs: 0,
        };
        dispatcher.resources = Some(LoadedResources {
            files: HashMap::from([(0, file)]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        dispatcher.remember_resource_backing_data(0, *res_type, res_id, data.to_vec());
        data_ptr
    }

    #[test]
    fn movehhi_respects_setrespurge_for_changed_resource_handles() {
        // Inside Macintosh Volume I (1985), p. I-126 and Memory 1992,
        // pp. 2-18 / 2-91: SetResPurge installs the purge hook so a
        // changed resource can be written out when the Memory Manager
        // moves a resource handle.
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"STR ", 32000, &[0x41, 0x42, 0x43]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"STR ", 32000, data_ptr);
        let changed = super::super::TrapDispatcher::RES_CHANGED_ATTR;
        let sp = TEST_SP;

        // FALSE (high byte 0) keeps the changed resource marked as changed.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x00FF);
        let result = disp.dispatch_toolbox(true, 0x193, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert!(!disp.res_purge);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, handle);
        call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap(); // ChangedResource
        assert_ne!(
            disp.resource_attributes_for_handle(handle).unwrap_or(0) & changed,
            0
        );

        cpu.write_reg(Register::A0, handle);
        let result = disp.dispatch_memory(false, 0x64, &mut cpu, &mut bus);
        assert!(result.is_some(), "MoveHHi should be handled");
        assert!(result.unwrap().is_ok(), "MoveHHi should succeed");
        assert_eq!(cpu.read_reg(Register::D0), 0, "MoveHHi should return noErr");
        assert_ne!(
            disp.resource_attributes_for_handle(handle).unwrap_or(0) & changed,
            0,
            "SetResPurge(FALSE) should keep the resource changed after MoveHHi"
        );

        // TRUE (high byte 1) clears the changed flag through the purge hook.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0100);
        let result = disp.dispatch_toolbox(true, 0x193, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert!(disp.res_purge);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::A0, handle);
        let result = disp.dispatch_memory(false, 0x64, &mut cpu, &mut bus);
        assert!(result.is_some(), "MoveHHi should be handled");
        assert!(result.unwrap().is_ok(), "MoveHHi should succeed");
        assert_eq!(cpu.read_reg(Register::D0), 0, "MoveHHi should return noErr");
        assert_eq!(
            disp.resource_attributes_for_handle(handle).unwrap_or(0) & changed,
            0,
            "SetResPurge(TRUE) should clear the changed flag through MoveHHi"
        );
    }

    #[test]
    fn find_vfs_file_prefers_relative_path_components_before_basename() {
        let mut disp = super::super::TrapDispatcher::new();
        disp.vfs
            .insert("MacPopulous/BigColourData/LOAD.PIC".to_string(), vec![1]);
        disp.vfs
            .insert("MacPopulous/ColourData/LOAD.PIC".to_string(), vec![2]);

        assert_eq!(
            disp.find_vfs_file(":ColourData:load.pic"),
            Some("MacPopulous/ColourData/LOAD.PIC".to_string())
        );
    }

    #[test]
    fn find_vfs_rsrc_file_prefers_relative_path_components_before_basename() {
        let mut disp = super::super::TrapDispatcher::new();
        disp.vfs_rsrc
            .insert("MacPopulous/BigColourData/LOAD.PIC".to_string(), vec![1]);
        disp.vfs_rsrc
            .insert("MacPopulous/ColourData/LOAD.PIC".to_string(), vec![2]);

        assert_eq!(
            disp.find_vfs_rsrc_file(":ColourData:load.pic"),
            Some("MacPopulous/ColourData/LOAD.PIC".to_string())
        );
    }

    // ================================================================
    // 0. CreateResFile (0x1B1)
    // ================================================================
    #[test]
    fn createresfile_missing_file_creates_data_and_resource_forks() {
        // IM:More Macintosh Toolbox 1993, p. 1-57: CreateResFile creates a
        // file with a zero-length data fork and an empty resource fork map
        // when no matching file exists.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x230000u32;
        write_pstring(&mut bus, name_ptr, b"Prefs.RSRC");
        bus.write_long(sp, name_ptr);

        call(&mut disp, true, 0x1B1, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            cpu.read_reg(Register::A7),
            sp + 4,
            "CreateResFile pops fileName ptr"
        );
        assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");
        assert!(
            disp.vfs.contains_key("Prefs.RSRC"),
            "missing file should create data fork"
        );
        assert!(
            disp.vfs_rsrc.contains_key("Prefs.RSRC"),
            "missing file should create resource fork"
        );
        assert_eq!(disp.vfs.get("Prefs.RSRC").unwrap(), &Vec::<u8>::new());
        assert!(
            crate::managers::resource::ResourceFork::parse(
                disp.vfs_rsrc.get("Prefs.RSRC").unwrap()
            )
            .is_some(),
            "CreateResFile should seed a structurally valid empty resource map"
        );
    }

    #[test]
    fn createresfile_existing_data_fork_adds_resource_fork_without_truncating_data() {
        // IM:More Macintosh Toolbox 1993, p. 1-57: if data fork exists with
        // zero-length/missing resource fork, CreateResFile adds the resource
        // fork map and does not destroy file data.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.vfs
            .insert("DATAONLY".to_string(), vec![0xDE, 0xAD, 0xBE, 0xEF]);

        let sp = TEST_SP;
        let name_ptr = 0x230100u32;
        write_pstring(&mut bus, name_ptr, b"DATAONLY");
        bus.write_long(sp, name_ptr);

        call(&mut disp, true, 0x1B1, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");
        assert_eq!(
            disp.vfs.get("DATAONLY").unwrap(),
            &vec![0xDE, 0xAD, 0xBE, 0xEF],
            "CreateResFile should not truncate existing data fork bytes"
        );
        assert!(
            crate::managers::resource::ResourceFork::parse(disp.vfs_rsrc.get("DATAONLY").unwrap())
                .is_some(),
            "CreateResFile should add a resource fork map for existing data file"
        );
    }

    #[test]
    fn createresfile_existing_zero_length_resource_fork_returns_noerr() {
        // IM:More Macintosh Toolbox 1993, p. 1-57: if the data fork exists
        // and the resource fork is zero-length, CreateResFile creates the
        // empty resource map instead of returning dupFNErr.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.vfs
            .insert("ZERORSRC".to_string(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
        disp.vfs_rsrc.insert("ZERORSRC".to_string(), Vec::new());

        let sp = TEST_SP;
        let name_ptr = 0x230180u32;
        write_pstring(&mut bus, name_ptr, b"ZERORSRC");
        bus.write_long(sp, name_ptr);

        call(&mut disp, true, 0x1B1, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");
        assert_eq!(
            disp.vfs.get("ZERORSRC").unwrap(),
            &vec![0xDE, 0xAD, 0xBE, 0xEF],
            "CreateResFile should preserve existing data fork bytes"
        );
        assert!(
            crate::managers::resource::ResourceFork::parse(disp.vfs_rsrc.get("ZERORSRC").unwrap())
                .is_some(),
            "CreateResFile should turn a zero-length resource fork into an empty mapped fork"
        );
    }

    #[test]
    fn createresfile_existing_resource_fork_returns_dupfnerr_and_leaves_file_unchanged() {
        // IM:More Macintosh Toolbox 1993, p. 1-57 (Special Considerations):
        // if a matching resource file is found during PBOpenRF search,
        // CreateResFile does nothing and ResError reports dupFNErr (-48).
        let (mut disp, mut cpu, mut bus) = setup();
        disp.vfs.insert("DUPLICATE".to_string(), vec![0x11, 0x22]);
        disp.vfs_rsrc
            .insert("DUPLICATE".to_string(), vec![0x33, 0x44]);

        let sp = TEST_SP;
        let name_ptr = 0x230200u32;
        write_pstring(&mut bus, name_ptr, b"DUPLICATE");
        bus.write_long(sp, name_ptr);

        call(&mut disp, true, 0x1B1, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(
            bus.read_word(0x0A60) as i16,
            -48,
            "ResErr should be dupFNErr"
        );
        assert_eq!(disp.vfs.get("DUPLICATE").unwrap(), &vec![0x11, 0x22]);
        assert_eq!(disp.vfs_rsrc.get("DUPLICATE").unwrap(), &vec![0x33, 0x44]);
    }

    // ================================================================
    // 1. GetResource (0x1A0) — found
    // ================================================================
    #[test]
    fn get_resource_found() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"STR#", 128, &[0; 10]);

        // Push params: SP+0 = id(2), SP+2 = type(4)
        let sp = TEST_SP;
        bus.write_word(sp, 128u16); // id
        bus.write_long(sp + 2, u32::from_be_bytes(*b"STR#")); // type

        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 6, "SP should advance by 6");
        let handle = bus.read_long(new_sp);
        assert_ne!(handle, 0, "handle should be non-zero");
        // Dereference handle -> ptr should equal data_ptr
        let deref = bus.read_long(handle);
        assert_eq!(deref, data_ptr);
        assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");
    }

    #[test]
    fn get_resource_synthesizes_system_clut_depth_id() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP;
        bus.write_word(sp, 4u16); // standard 4-bit System color table ID
        bus.write_long(sp + 2, u32::from_be_bytes(*b"clut"));

        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 6, "SP should advance by 6");
        let handle = bus.read_long(new_sp);
        assert_ne!(handle, 0, "synthetic clut must return a handle");
        assert_eq!(cpu.read_reg(Register::A0), handle);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");

        let ctab = bus.read_long(handle);
        assert_ne!(ctab, 0, "synthetic clut handle must be loaded");
        assert_eq!(bus.read_long(ctab), 4, "ctSeed should match the depth ID");
        assert_eq!(bus.read_word(ctab + 4), 0, "ctFlags should be zero");
        assert_eq!(bus.read_word(ctab + 6), 15, "ctSize should hold 16 entries");
        assert_eq!(bus.read_word(ctab + 8), 0, "entry 0 value");
        assert_eq!(bus.read_word(ctab + 10), 0xFFFF, "entry 0 red");
        assert_eq!(bus.read_word(ctab + 12), 0xFFFF, "entry 0 green");
        assert_eq!(bus.read_word(ctab + 14), 0xFFFF, "entry 0 blue");
        let black = ctab + 8 + 15 * 8;
        assert_eq!(bus.read_word(black), 15, "entry 15 value");
        assert_eq!(bus.read_word(black + 2), 0, "entry 255 red");
        assert_eq!(bus.read_word(black + 4), 0, "entry 255 green");
        assert_eq!(bus.read_word(black + 6), 0, "entry 255 blue");
    }

    #[test]
    fn get_resource_synthesizes_standard_wdef_zero() {
        // Inside Macintosh Volume V, V-32 lists WDEF 0 as the default
        // document-window definition function stored in the Macintosh ROM.
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP;
        bus.write_word(sp, 0u16);
        bus.write_long(sp + 2, u32::from_be_bytes(*b"WDEF"));

        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 6, "SP should advance by 6");
        let handle = bus.read_long(new_sp);
        assert_ne!(handle, 0, "synthetic WDEF 0 must return a handle");
        assert_eq!(cpu.read_reg(Register::A0), handle);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");

        let wdef = bus.read_long(handle);
        assert_ne!(wdef, 0, "synthetic WDEF handle must be loaded");
        assert_eq!(
            bus.read_word(wdef),
            0x205F,
            "standard WDEF shim should first recover the JSR return address"
        );
        assert_eq!(
            bus.read_word(wdef + 2),
            0xDEFC,
            "standard WDEF shim should discard its 12-byte Pascal parameter block"
        );
        assert_eq!(
            bus.read_word(wdef + 8),
            0x4ED0,
            "standard WDEF shim should return through the recovered address"
        );
    }

    #[test]
    fn get_resource_synthesizes_standard_roman_kchr_id_zero() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP;
        bus.write_word(sp, 0u16); // standard U.S. Roman keyboard layout
        bus.write_long(sp + 2, u32::from_be_bytes(*b"KCHR"));

        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 6, "SP should advance by 6");
        let handle = bus.read_long(new_sp);
        assert_ne!(handle, 0, "synthetic KCHR must return a handle");
        assert_eq!(cpu.read_reg(Register::A0), handle);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");

        let kchr = bus.read_long(handle);
        assert_ne!(kchr, 0, "synthetic KCHR handle must be loaded");
        assert_eq!(bus.get_alloc_size(kchr), Some(1 + 256 + 128 * 2));
        assert_eq!(bus.read_byte(kchr), 0, "KCHR version byte");
        assert_eq!(
            bus.read_byte(kchr + 1),
            0,
            "modifier byte 0 should select the unshifted table"
        );
        assert_eq!(
            bus.read_byte(kchr + 2),
            0,
            "modifier byte 1 is Command-only and should stay unshifted"
        );
        assert_eq!(
            bus.read_byte(kchr + 3),
            1,
            "modifier byte 2 should select the shifted table"
        );

        let table0 = kchr + 1 + 256;
        let table1 = table0 + 128;
        assert_eq!(bus.read_byte(table0), b'a');
        assert_eq!(bus.read_byte(table1), b'A');
        assert_eq!(bus.read_byte(table0 + 0x0C), b'q');
        assert_eq!(bus.read_byte(table1 + 0x0C), b'Q');
    }

    #[test]
    fn sethandlesize_resource_handle_keeps_resource_maps_current() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"TEST", 77, &[1, 2, 3, 4]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"TEST", 77, data_ptr);

        disp.resources
            .as_mut()
            .unwrap()
            .files
            .get_mut(&0)
            .unwrap()
            .named
            .insert((*b"TEST", "ResizeMe".to_string()), (77, data_ptr));

        cpu.write_reg(Register::A0, handle);
        cpu.write_reg(Register::D0, 64);
        let resize = disp.dispatch_memory(false, 0x24, &mut cpu, &mut bus);
        assert!(resize.is_some(), "SetHandleSize should be handled");
        assert!(resize.unwrap().is_ok(), "SetHandleSize should succeed");
        assert_eq!(cpu.read_reg(Register::D0), 0);

        let new_ptr = bus.read_long(handle);
        assert_ne!(new_ptr, 0);
        assert_ne!(new_ptr, data_ptr);
        assert_eq!(bus.read_bytes(new_ptr, 4), vec![1, 2, 3, 4]);
        assert_eq!(bus.get_alloc_size(new_ptr), Some(64));
        assert_eq!(
            disp.loaded_handles.get(&handle).map(|(ptr, _, _)| *ptr),
            Some(new_ptr)
        );
        assert_eq!(disp.ptr_to_handle.get(&data_ptr).copied(), None);
        assert_eq!(disp.ptr_to_handle.get(&new_ptr).copied(), Some(handle));

        let file = disp.resources.as_ref().unwrap().files.get(&0).unwrap();
        assert_eq!(file.loaded.get(&(*b"TEST", 77)).copied(), Some(new_ptr));
        assert_eq!(
            file.named.get(&(*b"TEST", "ResizeMe".to_string())).copied(),
            Some((77, new_ptr))
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 77u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"TEST"));
        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let returned_handle = bus.read_long(TEST_SP + 6);
        assert_eq!(
            returned_handle, handle,
            "GetResource should return the resized live handle, not a duplicate"
        );
        assert_eq!(bus.read_long(returned_handle), new_ptr);
    }

    #[test]
    fn get_resource_respects_setresload_false_until_loadresource() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"LOAD", 501, &[0xCA, 0xFE]);

        // IM:More Macintosh Toolbox 1993, 1-79 to 1-80: after
        // SetResLoad(FALSE), GetResource returns an empty handle for data
        // that is not already in memory; LoadResource later fills it.
        disp.res_load = false;

        let sp = TEST_SP;
        bus.write_word(sp, 501u16);
        bus.write_long(sp + 2, u32::from_be_bytes(*b"LOAD"));

        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let handle = bus.read_long(TEST_SP + 6);
        assert_ne!(handle, 0, "GetResource should still return a handle");
        assert_eq!(
            bus.read_long(handle),
            0,
            "master pointer should be nil while automatic loading is disabled"
        );
        assert_eq!(
            disp.take_hle_tick_cost(),
            0,
            "empty SetResLoad(FALSE) handles should not charge materialization work"
        );
        assert_eq!(bus.read_word(0x0A60), 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);

        call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(handle), data_ptr);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert!(
            disp.take_hle_tick_cost() > 0,
            "LoadResource should charge for filling an empty resource handle"
        );
    }

    #[test]
    fn get_resource_autoload_records_hle_work_once() {
        let (mut disp, mut cpu, mut bus) = setup();
        setup_resources(&mut disp, &mut bus, b"LOAD", 503, &[0x5A; 1024]);

        bus.write_word(TEST_SP, 503u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"LOAD"));

        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let handle = bus.read_long(TEST_SP + 6);
        assert_ne!(handle, 0);
        assert_ne!(bus.read_long(handle), 0);
        assert!(
            disp.take_hle_tick_cost()
                >= super::super::TrapDispatcher::resource_load_tick_cost(1024) as i32,
            "first autoloaded GetResource should charge by resource size"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 503u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"LOAD"));

        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            disp.take_hle_tick_cost(),
            0,
            "reusing an already-loaded resource handle should not charge another materialization"
        );
    }

    #[test]
    fn get_resource_autoloads_existing_empty_handle_after_setresload_true() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"LOAD", 502, &[0xBE, 0xEF]);

        disp.res_load = false;
        bus.write_word(TEST_SP, 502u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"LOAD"));
        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let empty_handle = bus.read_long(TEST_SP + 6);
        assert_ne!(empty_handle, 0);
        assert_eq!(bus.read_long(empty_handle), 0);

        // IM:More Macintosh Toolbox 1993, 1-79: SetResLoad(TRUE) returns
        // resource-returning calls to automatic loading behavior.
        disp.res_load = true;
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 502u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"LOAD"));

        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let loaded_handle = bus.read_long(TEST_SP + 6);
        assert_eq!(loaded_handle, empty_handle);
        assert_eq!(bus.read_long(loaded_handle), data_ptr);
        assert_eq!(bus.read_word(0x0A60), 0);

        cpu.write_reg(Register::A0, data_ptr);
        let recovered = disp.dispatch_memory(false, 0x28, &mut cpu, &mut bus);
        assert!(recovered.is_some(), "RecoverHandle should be handled");
        assert!(recovered.unwrap().is_ok(), "RecoverHandle should succeed");
        assert_eq!(cpu.read_reg(Register::A0), loaded_handle);
        assert_eq!(
            disp.ptr_to_handle.get(&data_ptr).copied(),
            Some(loaded_handle)
        );
    }

    // ================================================================
    // 1b. GetResource (0x1A0) — not found
    // ================================================================
    #[test]
    fn get_resource_miss_returns_nil_with_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        setup_resources(&mut disp, &mut bus, b"TEST", 128, &[0xAA, 0xBB]);

        let sp = TEST_SP;
        bus.write_word(sp, 999u16);
        bus.write_long(sp + 2, u32::from_be_bytes(*b"TEST"));

        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 6);
        let handle = bus.read_long(new_sp);
        assert_eq!(handle, 0, "handle should be NULL");
        // BasiliskII/System 7.5.3: missing ID under an existing type is NIL
        // with noErr despite IM:I I-119 documenting resNotFound.
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // More Macintosh Toolbox (1993), p. 1-78: RGetResource follows the
    // GetResource search contract (plus ROM retry on real hardware).
    #[test]
    fn rgetresource_returns_handle_for_open_chain_match() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr =
            setup_resources(&mut disp, &mut bus, b"TEST", 128, &[0x41, 0x42, 0x43, 0x44]);

        bus.write_word(TEST_SP, 128u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"TEST"));

        call(&mut disp, true, 0x00C, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        let handle = bus.read_long(TEST_SP + 6);
        assert_ne!(handle, 0);
        assert_eq!(bus.read_long(handle), data_ptr);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    // More Macintosh Toolbox (1993), p. 1-78: failed RGetResource requests
    // return NIL and surface not-found through ResError.
    #[test]
    fn rgetresource_miss_returns_nil_with_resnotfound() {
        let (mut disp, mut cpu, mut bus) = setup();

        bus.write_word(TEST_SP, 777u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"MISS"));

        call(&mut disp, true, 0x00C, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(bus.read_long(TEST_SP + 6), 0);
        assert_eq!(bus.read_word(0x0A60), (-192i16) as u16);
    }

    // More Macintosh Toolbox (1993), p. 1-78: RGetResource delegates to
    // GetResource chain semantics before any ROM retry.
    #[test]
    fn rgetresource_walks_chain_not_just_current_file() {
        let (mut disp, mut cpu, mut bus) = setup();

        let chain_ptr = bus.alloc(4);
        bus.write_bytes(chain_ptr, &[0xAA, 0xBB, 0xCC, 0xDD]);

        disp.resources = Some(LoadedResources {
            files: HashMap::from([
                (
                    100,
                    ResourceFileMap {
                        loaded: HashMap::from([((*b"PICT", 300), chain_ptr)]),
                        named: HashMap::new(),
                        names_by_id: HashMap::new(),
                        attrs: HashMap::new(),
                        map_attrs: 0,
                    },
                ),
                (
                    200,
                    ResourceFileMap {
                        loaded: HashMap::new(),
                        named: HashMap::new(),
                        names_by_id: HashMap::new(),
                        attrs: HashMap::new(),
                        map_attrs: 0,
                    },
                ),
            ]),
            names: HashMap::new(),
            search_order: vec![100, 200],
            current_file: 200,
        });

        bus.write_word(TEST_SP, 300u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PICT"));

        call(&mut disp, true, 0x00C, &mut cpu, &mut bus).unwrap();

        let handle = bus.read_long(TEST_SP + 6);
        assert_ne!(handle, 0);
        assert_eq!(bus.read_long(handle), chain_ptr);
        assert_eq!(disp.resource_handle_files.get(&handle).copied(), Some(100));
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    // ================================================================
    // 2. Get1Resource (0x01F) — found
    // ================================================================
    #[test]
    fn get1_resource_found() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 200, &[0xAB; 8]);

        let sp = TEST_SP;
        bus.write_word(sp, 200u16);
        bus.write_long(sp + 2, u32::from_be_bytes(*b"DLOG"));

        call(&mut disp, true, 0x01F, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 6);
        let handle = bus.read_long(new_sp);
        assert_ne!(handle, 0);
        let deref = bus.read_long(handle);
        assert_eq!(deref, data_ptr);
    }

    #[test]
    fn get1_resource_uses_current_resource_file() {
        let (mut disp, mut cpu, mut bus) = setup();

        let app_ptr = bus.alloc(4);
        bus.write_bytes(app_ptr, &[0xAA; 4]);
        let images_ptr = bus.alloc(4);
        bus.write_bytes(images_ptr, &[0xBB; 4]);

        disp.resources = Some(LoadedResources {
            files: HashMap::from([
                (
                    0,
                    ResourceFileMap {
                        loaded: HashMap::from([((*b"PICT", 1114), app_ptr)]),
                        named: HashMap::new(),
                        names_by_id: HashMap::new(),
                        attrs: HashMap::new(),
                        map_attrs: 0,
                    },
                ),
                (
                    101,
                    ResourceFileMap {
                        loaded: HashMap::from([((*b"PICT", 1114), images_ptr)]),
                        named: HashMap::new(),
                        names_by_id: HashMap::new(),
                        attrs: HashMap::new(),
                        map_attrs: 0,
                    },
                ),
            ]),
            names: HashMap::new(),
            search_order: vec![0, 101],
            current_file: 101,
        });

        let sp = TEST_SP;
        bus.write_word(sp, 1114u16);
        bus.write_long(sp + 2, u32::from_be_bytes(*b"PICT"));

        call(&mut disp, true, 0x01F, &mut cpu, &mut bus).unwrap();

        let handle = bus.read_long(TEST_SP + 6);
        assert_ne!(handle, 0);
        assert_eq!(bus.read_long(handle), images_ptr);
        assert_eq!(disp.resource_handle_files.get(&handle).copied(), Some(101));
    }

    // ================================================================
    // 2b. Get1Resource (0x01F) — not found
    // ================================================================
    #[test]
    fn get1_resource_miss_returns_nil_with_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        setup_resources(&mut disp, &mut bus, b"TEST", 128, &[0x11, 0x22]);

        let sp = TEST_SP;
        bus.write_word(sp, 999u16);
        bus.write_long(sp + 2, u32::from_be_bytes(*b"TEST"));

        call(&mut disp, true, 0x01F, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 6);
        let handle = bus.read_long(new_sp);
        assert_eq!(handle, 0);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // ================================================================
    // 3. DetachResource (0x192)
    // ================================================================
    #[test]
    fn detach_resource() {
        let (mut disp, mut cpu, mut bus) = setup();

        let shared_ptr = bus.alloc(8);
        for (offset, byte) in [0x10u8, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80]
            .into_iter()
            .enumerate()
        {
            bus.write_byte(shared_ptr + offset as u32, byte);
        }
        let handle = bus.alloc(4);
        bus.write_long(handle, shared_ptr);
        disp.loaded_handles
            .insert(handle, (shared_ptr, *b"TEST", 7));

        let sp = TEST_SP;
        bus.write_long(sp, handle);

        call(&mut disp, true, 0x192, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 4);

        let detached_ptr = bus.read_long(handle);
        assert_ne!(detached_ptr, shared_ptr);
        assert_eq!(
            bus.read_bytes(detached_ptr, 8),
            bus.read_bytes(shared_ptr, 8)
        );
        assert!(!disp.loaded_handles.contains_key(&handle));
        assert_eq!(bus.read_word(0x0A60), 0);

        bus.write_byte(detached_ptr + 3, 0xEE);
        assert_eq!(bus.read_byte(shared_ptr + 3), 0x40);
    }

    // Inside Macintosh Volume I (1985), p. I-122: DetachResource must not
    // detach resChanged resources, but ResError still returns noErr.
    #[test]
    fn detach_resource_reschanged_handle_returns_noerr_without_detaching() {
        let (mut disp, mut cpu, mut bus) = setup();

        let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 201, &[0xAA; 8]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 201, data_ptr);
        let original_ptr = bus.read_long(handle);

        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap(); // ChangedResource
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_ne!(
            disp.resource_attributes_for_handle(handle).unwrap_or(0)
                & super::super::TrapDispatcher::RES_CHANGED_ATTR,
            0
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x192, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(bus.read_long(handle), original_ptr);
        assert!(disp.loaded_handles.contains_key(&handle));
        assert!(disp.resource_handle_files.contains_key(&handle));
    }

    // Inside Macintosh Volume I (1985), p. I-122: DetachResource does nothing
    // and returns resNotFound when the handle is not a resource handle.
    #[test]
    fn detach_resource_non_resource_handle_returns_resnotfound() {
        let (mut disp, mut cpu, mut bus) = setup();
        let fake_ptr = bus.alloc(6);
        bus.write_bytes(fake_ptr, &[1, 2, 3, 4, 5, 6]);
        let fake_handle = bus.alloc(4);
        bus.write_long(fake_handle, fake_ptr);

        bus.write_long(TEST_SP, fake_handle);
        call(&mut disp, true, 0x192, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(fake_handle), fake_ptr);
        assert_eq!(bus.read_word(0x0A60) as i16, -192);
    }

    // Per BasiliskII/System 7.5.3: DetachResource(nil) returns
    // resNotFound, matching the non-resource non-nil handle path.
    #[test]
    fn detach_resource_nil_handle_returns_resnotfound() {
        let (mut disp, mut cpu, mut bus) = setup();

        bus.write_word(0x0A60, 0);
        bus.write_long(TEST_SP, 0);
        call(&mut disp, true, 0x192, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            bus.read_word(0x0A60) as i16,
            super::super::TrapDispatcher::RES_NOT_FOUND
        );
    }

    #[test]
    fn detach_resource_unloads_live_map_entry_but_keeps_backing_data_reloadable() {
        let (mut disp, mut cpu, mut bus) = setup();

        let original = [0x50, 0x61, 0x6B, 0x20, 0xAA, 0xBB];
        let data_ptr = disp.install_test_resource(&mut bus, *b"Pak ", 12000, &original);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"Pak ", 12000, data_ptr);

        let decoded_ptr = disp.resize_resource_allocation(&mut bus, handle, data_ptr, 10);
        bus.write_bytes(decoded_ptr, &[0x00, 0x02, 0x00, 0x0B, 1, 2, 3, 4, 5, 6]);
        assert_eq!(
            disp.resources
                .as_ref()
                .unwrap()
                .files
                .get(&0)
                .unwrap()
                .loaded
                .get(&(*b"Pak ", 12000))
                .copied(),
            Some(decoded_ptr)
        );

        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x192, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_word(0x0A60), 0);
        let detached_ptr = bus.read_long(handle);
        assert_ne!(detached_ptr, decoded_ptr);
        assert_eq!(
            disp.resources
                .as_ref()
                .unwrap()
                .files
                .get(&0)
                .unwrap()
                .loaded
                .get(&(*b"Pak ", 12000)),
            None
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 12000u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"Pak "));
        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let reloaded_handle = bus.read_long(TEST_SP + 6);
        assert_ne!(reloaded_handle, 0);
        assert_ne!(reloaded_handle, handle);
        let reloaded_ptr = bus.read_long(reloaded_handle);
        assert_ne!(reloaded_ptr, decoded_ptr);
        assert_eq!(bus.read_bytes(reloaded_ptr, original.len()), original);
    }

    // ================================================================
    // 4. LoadResource (0x1A2)
    // ================================================================
    #[test]
    fn load_resource() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 200, &[0xAB; 8]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 200, data_ptr);
        assert_eq!(
            disp.resource_handles_by_key
                .get(&(0, *b"DLOG", 200))
                .copied(),
            Some(handle)
        );

        let sp = TEST_SP;
        bus.write_long(sp, handle);

        call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn load_resource_loaded_handle_is_noop_and_recoverable() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 203, &[0xCA, 0xFE]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 203, data_ptr);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(bus.read_long(handle), data_ptr);
        assert_eq!(
            disp.resource_handle_files.get(&handle).copied(),
            Some(disp.current_resource_refnum()),
            "LoadResource should leave the resource-file binding intact"
        );

        cpu.write_reg(Register::A0, data_ptr);
        let recovered = disp.dispatch_memory(false, 0x28, &mut cpu, &mut bus);
        assert!(recovered.is_some(), "RecoverHandle should be handled");
        assert!(recovered.unwrap().is_ok(), "RecoverHandle should succeed");
        assert_eq!(cpu.read_reg(Register::A0), handle);
    }

    #[test]
    fn load_resource_rewrites_stale_master_pointer_after_reload() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 204, &[0xAB, 0xCD]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 204, data_ptr);

        bus.write_long(handle, 0x1234_5678);
        assert_ne!(bus.read_long(handle), data_ptr);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(handle), data_ptr);
        assert_eq!(
            disp.ptr_to_handle.get(&data_ptr).copied(),
            Some(handle),
            "LoadResource should restore RecoverHandle-style ownership"
        );
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn load_resource_repopulates_handle_after_emptyhandle_purge() {
        // More Macintosh Toolbox (1993), p. 1-80: LoadResource reads
        // resource data into memory for a resource handle. Memory (1992),
        // p. 2-52 explicitly directs purged resources to LoadResource.
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 202, &[0xCA, 0xFE]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 202, data_ptr);
        let refnum = disp
            .resource_handle_files
            .get(&handle)
            .copied()
            .expect("resource handle should have a file binding");

        cpu.write_reg(Register::A0, handle);
        let emptied = disp.dispatch_memory(false, 0x2B, &mut cpu, &mut bus);
        assert!(emptied.is_some(), "EmptyHandle should be handled");
        assert!(emptied.unwrap().is_ok(), "EmptyHandle should succeed");
        assert_eq!(
            bus.read_long(handle),
            0,
            "resource handle should be empty after purge"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            bus.read_long(handle),
            data_ptr,
            "LoadResource should restore the purged resource's master pointer"
        );
        assert_eq!(
            disp.ptr_to_handle.get(&data_ptr).copied(),
            Some(handle),
            "LoadResource should restore RecoverHandle-style ptr ownership"
        );
        assert!(
            !disp.detached_handles.contains_key(&handle),
            "LoadResource should clear detached handle state after reloading"
        );
        assert!(
            !disp.detached_handle_files.contains_key(&handle),
            "LoadResource should clear detached file ownership after reloading"
        );
        cpu.write_reg(Register::A0, data_ptr);
        let recovered = disp.dispatch_memory(false, 0x28, &mut cpu, &mut bus);
        assert!(recovered.is_some(), "RecoverHandle should be handled");
        assert!(recovered.unwrap().is_ok(), "RecoverHandle should succeed");
        assert_eq!(
            cpu.read_reg(Register::A0),
            handle,
            "RecoverHandle should rediscover the reloaded resource handle"
        );
        assert_eq!(
            disp.resource_handle_files.get(&handle).copied(),
            Some(refnum),
            "LoadResource should restore the resource-file binding after reloading a purged handle"
        );
        assert_eq!(bus.read_word(0x0A60), 0, "LoadResource should report noErr");
    }

    #[test]
    fn load_resource_populates_empty_handle_after_setresload_false() {
        // IM:More Macintosh Toolbox 1993, p. 1-80: SetResLoad(FALSE) can
        // return an empty handle for a present resource; LoadResource later
        // fills the master pointer back in.
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"LOAD", 205, &[0x12, 0x34]);

        disp.res_load = false;
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"LOAD", 205, data_ptr);
        assert_eq!(bus.read_long(handle), 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(handle), data_ptr);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert!(
            disp.loaded_handles.contains_key(&handle),
            "LoadResource should keep the reloaded resource registered as loaded"
        );
        assert!(
            disp.resource_handle_files.contains_key(&handle),
            "LoadResource should keep the resource-file ownership for the reloaded handle"
        );
        cpu.write_reg(Register::A0, data_ptr);
        let recovered = disp.dispatch_memory(false, 0x28, &mut cpu, &mut bus);
        assert!(recovered.is_some(), "RecoverHandle should be handled");
        assert!(recovered.unwrap().is_ok(), "RecoverHandle should succeed");
        assert_eq!(cpu.read_reg(Register::A0), handle);
    }

    #[test]
    fn load_resource_restores_stale_detached_file_binding_when_reloading() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 206, &[0xDE, 0xAD]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 206, data_ptr);
        let refnum = disp
            .resource_handle_files
            .get(&handle)
            .copied()
            .expect("resource handle should have a file binding");

        disp.resource_handle_files.remove(&handle);
        disp.detached_handle_files.insert(handle, refnum);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(handle), data_ptr);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(
            disp.resource_handle_files.get(&handle).copied(),
            Some(refnum)
        );
        assert!(!disp.detached_handle_files.contains_key(&handle));
    }

    #[test]
    fn load_resource_non_resource_handle_leaves_handle_intact_and_reserr_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let fake_handle = bus.alloc(4);
        bus.write_long(fake_handle, 0x00AB_CDEF);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, fake_handle);
        call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(fake_handle), 0x00AB_CDEF);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    // ================================================================
    // 5. ReleaseResource (0x1A3)
    // ================================================================
    #[test]
    fn release_resource_invalidates_handle_and_getresource_allocates_fresh_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 200, &[0xAB; 8]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 200, data_ptr);

        let sp = TEST_SP;
        bus.write_long(sp, handle);

        call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(handle), 0);
        assert!(
            !disp.loaded_handles.contains_key(&handle),
            "ReleaseResource should invalidate the old handle's resource identity"
        );
        assert!(
            !disp.resource_handle_files.contains_key(&handle),
            "ReleaseResource should remove the old handle's resource file binding"
        );
        assert!(
            !disp
                .resource_handles_by_key
                .contains_key(&(0, *b"DLOG", 200)),
            "ReleaseResource should remove the stale resource handle index"
        );
        assert_eq!(
            disp.ptr_to_handle.get(&data_ptr).copied(),
            None,
            "released resource data should not be recoverable until reload"
        );
        assert_eq!(bus.read_word(0x0A60), 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            bus.read_word(0x0A60) as i16,
            -192,
            "a stale released handle is no longer a resource handle"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 200u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"DLOG"));
        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        let reloaded_handle = bus.read_long(TEST_SP + 6);
        assert_ne!(reloaded_handle, 0);
        assert_ne!(
            reloaded_handle, handle,
            "GetResource after ReleaseResource should allocate a fresh handle"
        );
        assert_eq!(bus.read_long(reloaded_handle), data_ptr);
        assert_eq!(
            disp.ptr_to_handle.get(&data_ptr).copied(),
            Some(reloaded_handle),
            "GetResource should restore RecoverHandle ownership through the fresh handle"
        );
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn release_resource_unloads_vfs_backed_resource_and_getresource_reloads() {
        let (mut disp, mut cpu, mut bus) = setup();
        let bytes = [0xAB; 8];
        let rsrc_bytes = make_single_resource_fork_bytes(*b"PICT", 23002, &bytes);
        disp.vfs_rsrc.insert("Reloadable".to_string(), rsrc_bytes);
        let refnum = disp.open_resource_file_from_vfs_key(&mut bus, "Reloadable", false);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 23002u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PICT"));
        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let handle = bus.read_long(TEST_SP + 6);
        let old_ptr = bus.read_long(handle);
        assert_ne!(handle, 0);
        assert_ne!(old_ptr, 0);
        assert_eq!(bus.get_alloc_size(old_ptr), Some(bytes.len() as u32));

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(handle), 0);
        assert_eq!(bus.get_alloc_size(old_ptr), None);
        assert_eq!(
            disp.resources
                .as_ref()
                .unwrap()
                .files
                .get(&refnum)
                .unwrap()
                .loaded
                .get(&(*b"PICT", 23002))
                .copied(),
            Some(0),
            "ReleaseResource should mark VFS-backed data unloaded"
        );
        assert!(!disp.loaded_handles.contains_key(&handle));
        assert_eq!(bus.read_word(0x0A60), 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 23002u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PICT"));
        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let reloaded_handle = bus.read_long(TEST_SP + 6);
        let reloaded_ptr = bus.read_long(reloaded_handle);
        assert_ne!(reloaded_handle, 0);
        assert_ne!(reloaded_handle, handle);
        assert_ne!(reloaded_ptr, 0);
        assert_eq!(bus.get_alloc_size(reloaded_ptr), Some(bytes.len() as u32));
        assert_eq!(bus.read_bytes(reloaded_ptr, bytes.len()), bytes);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn released_resource_reloads_from_backing_cache_without_reparsing_fork() {
        let (mut disp, mut cpu, mut bus) = setup();
        let bytes = [0xCD; 8];
        let rsrc_bytes = make_single_resource_fork_bytes(*b"PICT", 23003, &bytes);
        disp.vfs_rsrc.insert("CachedReload".to_string(), rsrc_bytes);
        let refnum = disp.open_resource_file_from_vfs_key(&mut bus, "CachedReload", false);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 23003u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PICT"));
        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let handle = bus.read_long(TEST_SP + 6);
        let old_ptr = bus.read_long(handle);
        assert_ne!(handle, 0);
        assert_ne!(old_ptr, 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_long(handle), 0);
        assert_eq!(bus.get_alloc_size(old_ptr), None);
        assert_eq!(
            disp.resources
                .as_ref()
                .unwrap()
                .files
                .get(&refnum)
                .unwrap()
                .loaded
                .get(&(*b"PICT", 23003))
                .copied(),
            Some(0)
        );

        disp.vfs_rsrc.remove("CachedReload");

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 23003u16);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PICT"));
        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        let reloaded_handle = bus.read_long(TEST_SP + 6);
        let reloaded_ptr = bus.read_long(reloaded_handle);
        assert_ne!(reloaded_handle, 0);
        assert_ne!(reloaded_ptr, 0);
        assert_eq!(bus.read_bytes(reloaded_ptr, bytes.len()), bytes);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn release_resource_reschanged_handle_is_not_released_and_reserr_is_noerr() {
        // IM:I I-121: ReleaseResource does not release a resource whose
        // resChanged attribute is set, but ResError still reports noErr.
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 201, &[0xAA; 12]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 201, data_ptr);
        let original_ptr = bus.read_long(handle);

        // Mark resource changed via ChangedResource ($A9AA).
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_word(0x0A60), 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(handle), original_ptr);
        assert!(disp.loaded_handles.contains_key(&handle));
        assert!(disp.resource_handle_files.contains_key(&handle));
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn release_resource_non_resource_handle_sets_resnotfound() {
        // IM:I I-121: non-resource handles are ignored and report resNotFound.
        let (mut disp, mut cpu, mut bus) = setup();
        let fake_ptr = bus.alloc(4);
        bus.write_bytes(fake_ptr, &[0xDE, 0xAD, 0xBE, 0xEF]);
        let fake_handle = bus.alloc(4);
        bus.write_long(fake_handle, fake_ptr);

        bus.write_long(TEST_SP, fake_handle);
        call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(fake_handle), fake_ptr);
        assert!(!disp.loaded_handles.contains_key(&fake_handle));
        assert_eq!(bus.read_word(0x0A60) as i16, -192);
    }

    // ================================================================
    // 5b. SizeResource (0x1A5)
    // ================================================================
    #[test]
    fn size_resource_loaded_resource_handle_returns_byte_size() {
        // IM:I I-121: SizeResource returns the resource size in bytes.
        let (mut disp, mut cpu, mut bus) = setup();
        let bytes = [0x10u8, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70];
        let data_ptr = setup_resources(&mut disp, &mut bus, b"STR ", 77, &bytes);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"STR ", 77, data_ptr);

        bus.write_word(0x0A60, (-192i16) as u16); // prove success clears stale error
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1A5, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 4);
        assert_eq!(bus.read_long(new_sp) as i32, bytes.len() as i32);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn size_resource_empty_handle_from_setresload_false_style_still_reports_size() {
        // IM:I I-118..I-121: SetResLoad(FALSE) can yield an empty resource
        // handle, but SizeResource still reports the resource-file byte size.
        let (mut disp, mut cpu, mut bus) = setup();
        let bytes = [0x41u8, 0x42, 0x43, 0x44, 0x45];
        let data_ptr = setup_resources(&mut disp, &mut bus, b"ALRT", 90, &bytes);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"ALRT", 90, data_ptr);

        // Emulate an empty handle while retaining Resource Manager ownership.
        bus.write_long(handle, 0);

        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1A5, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 4);
        assert_eq!(bus.read_long(new_sp) as i32, bytes.len() as i32);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn size_resource_non_resource_handles_return_minus_one_and_resnotfound() {
        // IM:I I-121: SizeResource returns -1 and resNotFound for handles that
        // are not Resource Manager handles (including detached handles).
        let (mut disp, mut cpu, mut bus) = setup();

        // Unknown handle case.
        let fake_ptr = bus.alloc(3);
        bus.write_bytes(fake_ptr, &[0x01, 0x02, 0x03]);
        let fake_handle = bus.alloc(4);
        bus.write_long(fake_handle, fake_ptr);

        bus.write_long(TEST_SP, fake_handle);
        call(&mut disp, true, 0x1A5, &mut cpu, &mut bus).unwrap();
        let mut new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 4);
        assert_eq!(bus.read_long(new_sp) as i32, -1);
        assert_eq!(bus.read_word(0x0A60) as i16, -192);

        // Detached resource handle case.
        let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 301, &[0xAA; 6]);
        let res_handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 301, data_ptr);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, res_handle);
        call(&mut disp, true, 0x192, &mut cpu, &mut bus).unwrap(); // DetachResource

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, res_handle);
        call(&mut disp, true, 0x1A5, &mut cpu, &mut bus).unwrap();
        new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 4);
        assert_eq!(bus.read_long(new_sp) as i32, -1);
        assert_eq!(bus.read_word(0x0A60) as i16, -192);
    }

    // ================================================================
    // 5c. MaxSizeRsrc (0x021)
    // ================================================================
    #[test]
    fn maxsizersrc_returns_resource_size_for_loaded_resource_handle() {
        // IM:IV IV-16: MaxSizeRsrc returns resource size from the map.
        let (mut disp, mut cpu, mut bus) = setup();
        let bytes = [0xAAu8, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let data_ptr = setup_resources(&mut disp, &mut bus, b"MSIZ", 42, &bytes);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"MSIZ", 42, data_ptr);

        bus.write_word(0x0A60, (-192i16) as u16); // stale error should clear
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x021, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 4);
        assert_eq!(bus.read_long(new_sp) as i32, bytes.len() as i32);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn maxsizersrc_returns_minus_one_and_sets_resnotfound_for_non_resource_handle() {
        // IM:I I-121 + IM:IV IV-16: non-resource handles report
        // "not found" and return -1 size.
        let (mut disp, mut cpu, mut bus) = setup();
        let fake_ptr = bus.alloc(5);
        bus.write_bytes(fake_ptr, &[1, 2, 3, 4, 5]);
        let fake_handle = bus.alloc(4);
        bus.write_long(fake_handle, fake_ptr);

        bus.write_long(TEST_SP, fake_handle);
        call(&mut disp, true, 0x021, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 4);
        assert_eq!(bus.read_long(new_sp) as i32, -1);
        assert_eq!(bus.read_word(0x0A60) as i16, -192);
    }

    #[test]
    fn maxsizersrc_consumes_handle_argument_and_writes_function_result_slot() {
        // FUNCTION MaxSizeRsrc(theResource: Handle): LONGINT
        // (IM:IV IV-16): pop 4-byte arg, write 4-byte result at new SP.
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"MSZ2", 7, &[0x10, 0x20, 0x30]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"MSZ2", 7, data_ptr);

        bus.write_long(TEST_SP, handle);
        bus.write_long(TEST_SP + 4, 0xDEADBEEF); // result slot sentinel
        call(&mut disp, true, 0x021, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            bus.read_long(TEST_SP + 4),
            3,
            "MaxSizeRsrc writes result to function-result slot at post-pop SP"
        );
    }

    // ================================================================
    // 5d. ResourceDispatch (0x022) — selectors 1/2/3
    // ================================================================
    #[test]
    fn readpartialresource_selector_1_copies_requested_bytes_and_pops_16_bytes() {
        // MMTB 1993 1-69 + IM:VI C-27: selector 1 (or $7001 glue form)
        // performs ReadPartialResource and consumes 16 bytes of args.
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(
            &mut disp,
            &mut bus,
            b"PART",
            90,
            &[0x10, 0x20, 0x30, 0x40, 0x50],
        );
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"PART", 90, data_ptr);
        let buffer = bus.alloc(6);
        bus.write_bytes(buffer, &[0xEE; 6]);

        bus.write_long(TEST_SP, 3); // count
        bus.write_long(TEST_SP + 4, buffer);
        bus.write_long(TEST_SP + 8, 1); // offset
        bus.write_long(TEST_SP + 12, handle);
        cpu.write_reg(Register::D0, 0x7001); // low-byte selector form

        call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
        assert_eq!(bus.read_bytes(buffer, 3), vec![0x20, 0x30, 0x40]);
        assert_eq!(bus.read_word(0x0A60) as i16, -188);
    }

    #[test]
    fn readpartialresource_empty_handle_reads_backing_resource_and_reports_noerr() {
        // MMTB 1993 1-41: the partial-resource workflow calls
        // SetResLoad(FALSE), gets an empty resource handle, restores
        // SetResLoad(TRUE), then calls ReadPartialResource. That read
        // should stream from the resource on disk and report noErr.
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(
            &mut disp,
            &mut bus,
            b"PART",
            94,
            &[0x10, 0x20, 0x30, 0x40, 0x50],
        );
        disp.res_load = false;
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"PART", 94, data_ptr);
        disp.res_load = true;
        assert_eq!(bus.read_long(handle), 0, "handle should be empty");
        let buffer = bus.alloc(4);
        bus.write_bytes(buffer, &[0xEE; 4]);

        bus.write_long(TEST_SP, 4); // count
        bus.write_long(TEST_SP + 4, buffer);
        bus.write_long(TEST_SP + 8, 1); // offset
        bus.write_long(TEST_SP + 12, handle);
        cpu.write_reg(Register::D0, 0x0001);

        call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
        assert_eq!(bus.read_bytes(buffer, 4), vec![0x20, 0x30, 0x40, 0x50]);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(bus.read_long(handle), 0, "partial read should not load it");
    }

    #[test]
    fn writepartialresource_selector_2_extends_resource_and_sets_writingpastend() {
        // MMTB 1993 1-70: writes past end extend the resource and report
        // writingPastEnd (-189).
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"WRPT", 91, &[0xAA, 0xBB, 0xCC, 0xDD]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"WRPT", 91, data_ptr);
        let src = bus.alloc(4);
        bus.write_bytes(src, &[1, 2, 3, 4]);

        bus.write_long(TEST_SP, 4); // count
        bus.write_long(TEST_SP + 4, src);
        bus.write_long(TEST_SP + 8, 3); // offset
        bus.write_long(TEST_SP + 12, handle);
        cpu.write_reg(Register::D0, 0x0002);

        call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
        assert_eq!(bus.read_word(0x0A60) as i16, -189);

        let live_ptr = bus.read_long(handle);
        assert_eq!(bus.get_alloc_size(live_ptr).unwrap_or(0), 7);
        assert_eq!(
            bus.read_bytes(live_ptr, 7),
            vec![0xAA, 0xBB, 0xCC, 1, 2, 3, 4]
        );
    }

    #[test]
    fn writepartialresource_empty_handle_updates_backing_resource_and_reports_noerr() {
        // MMTB 1993 1-41 describes using SetResLoad(FALSE) and an empty
        // handle for the partial-resource family; writes should still use
        // the resource map entry rather than treating the handle as missing.
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"WEPT", 95, &[0xAA, 0xBB, 0xCC, 0xDD]);
        disp.res_load = false;
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"WEPT", 95, data_ptr);
        disp.res_load = true;
        assert_eq!(bus.read_long(handle), 0, "handle should be empty");
        let src = bus.alloc(2);
        bus.write_bytes(src, &[0x11, 0x22]);

        bus.write_long(TEST_SP, 2); // count
        bus.write_long(TEST_SP + 4, src);
        bus.write_long(TEST_SP + 8, 1); // offset
        bus.write_long(TEST_SP + 12, handle);
        cpu.write_reg(Register::D0, 0x0002);

        call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(bus.read_bytes(data_ptr, 4), vec![0xAA, 0x11, 0x22, 0xDD]);
        assert_eq!(bus.read_long(handle), 0, "partial write should not load it");
    }

    #[test]
    fn writepartialresource_selector_2_zero_count_does_not_resize_resource() {
        // MMTB 1993 1-70: a zero-length write is a no-op and does not
        // grow the resource or report writingPastEnd.
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"ZCNT", 93, &[0x11, 0x22, 0x33, 0x44]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"ZCNT", 93, data_ptr);
        let src = bus.alloc(4);
        bus.write_bytes(src, &[9, 8, 7, 6]);

        bus.write_long(TEST_SP, 0); // count
        bus.write_long(TEST_SP + 4, src);
        bus.write_long(TEST_SP + 8, 7); // offset past end
        bus.write_long(TEST_SP + 12, handle);
        cpu.write_reg(Register::D0, 0x0002);

        call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
        assert_eq!(bus.read_word(0x0A60), 0);

        let live_ptr = bus.read_long(handle);
        assert_eq!(bus.get_alloc_size(live_ptr).unwrap_or(0), 4);
        assert_eq!(bus.read_bytes(live_ptr, 4), vec![0x11, 0x22, 0x33, 0x44]);
    }

    #[test]
    fn setresourcesize_selector_3_updates_resource_size_visible_to_sizeresource() {
        // MMTB 1993 1-71: selector 3 updates resource size.
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"RSIZ", 92, &[0x10, 0x20, 0x30]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"RSIZ", 92, data_ptr);

        bus.write_long(TEST_SP, 9); // newSize
        bus.write_long(TEST_SP + 4, handle);
        cpu.write_reg(Register::D0, 0x0003);
        call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(0x0A60), 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1A5, &mut cpu, &mut bus).unwrap(); // SizeResource
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(TEST_SP + 4), 9);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn setresourcesize_selector_3_resizes_emptied_resource_handle_in_place() {
        // IM:VI 13-23: SetResourceSize changes the size of a resource
        // without writing data. An emptied resource handle should still
        // stay tied to the same logical resource after resizing.
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"RSZ0", 93, &[0xAA, 0xBB, 0xCC]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"RSZ0", 93, data_ptr);

        bus.write_long(handle, 0);

        bus.write_long(TEST_SP, 7);
        bus.write_long(TEST_SP + 4, handle);
        cpu.write_reg(Register::D0, 0x0003);
        call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_ne!(
            bus.read_long(handle),
            0,
            "resized empty handle should be repopulated"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 93);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"RSZ0"));
        bus.write_long(TEST_SP + 6, 0xDEADBEEF);
        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(
            bus.read_long(TEST_SP + 6),
            handle,
            "relookup should resolve to the same handle after resize"
        );
        assert_eq!(bus.read_word(0x0A60), 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1A5, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(TEST_SP + 4), 7);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    // ================================================================
    // 6. ResError (0x1AF)
    // ================================================================
    #[test]
    fn res_error() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Set ResErr to a known value
        bus.write_word(0x0A60, (-108i16) as u16);

        call(&mut disp, true, 0x1AF, &mut cpu, &mut bus).unwrap();

        let sp = cpu.read_reg(Register::A7);
        assert_eq!(sp, TEST_SP, "SP should be unchanged");
        let result = bus.read_word(sp) as i16;
        assert_eq!(result, -108);
    }

    fn addreference_setup() -> (
        super::super::TrapDispatcher,
        MockCpu,
        crate::memory::MacMemoryBus,
        u32,
        u32,
    ) {
        let (mut disp, cpu, mut bus) = setup();
        let source_ptr = bus.alloc(8);
        bus.write_bytes(
            source_ptr,
            &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
        );

        let mut file0_loaded = HashMap::new();
        file0_loaded.insert((*b"CURS", 1), source_ptr);
        let mut file0 = ResourceFileMap {
            loaded: file0_loaded,
            named: HashMap::new(),
            names_by_id: HashMap::new(),
            attrs: HashMap::from([((*b"CURS", 1), 0x0004u8)]),
            map_attrs: 0,
        };
        file0
            .named
            .insert((*b"CURS", "SystemCursor".to_string()), (1, source_ptr));

        let file1 = ResourceFileMap::default();
        disp.resources = Some(LoadedResources {
            files: HashMap::from([(0, file0), (1, file1)]),
            names: HashMap::new(),
            search_order: vec![0, 1],
            current_file: 1,
        });

        let source_handle =
            disp.get_or_create_resource_handle_in_file(&mut bus, *b"CURS", 1, source_ptr, 0);

        (disp, cpu, bus, source_handle, source_ptr)
    }

    #[test]
    fn addreference_adds_system_reference_and_marks_current_file_changed() {
        let (mut disp, mut cpu, mut bus, source_handle, source_ptr) = addreference_setup();
        let sp = TEST_SP;
        let name_ptr = 0x200000u32;
        write_pstring(&mut bus, name_ptr, b"AddedReference");
        let ref_id = 200i16;
        let current_before = disp.current_resource_refnum();

        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, ref_id as u16);
        bus.write_long(sp + 6, source_handle);

        call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(disp.current_resource_refnum(), current_before);
        assert_eq!(
            disp.resource_handle_files.get(&source_handle).copied(),
            Some(0)
        );

        let file = disp.resources.as_ref().unwrap().files.get(&1).unwrap();
        assert_eq!(
            disp.loaded_handles.get(&source_handle).copied(),
            Some((source_ptr, *b"CURS", 1))
        );
        assert_eq!(file.loaded.get(&(*b"CURS", ref_id)), Some(&source_ptr));
        assert_eq!(
            file.named.get(&(*b"CURS", "AddedReference".to_string())),
            Some(&(ref_id, source_ptr))
        );
        assert_eq!(
            file.attrs.get(&(*b"CURS", ref_id)).copied().unwrap_or(0)
                & (super::super::TrapDispatcher::RES_CHANGED_ATTR as u8
                    | super::super::TrapDispatcher::RES_SYS_REF_ATTR as u8),
            (super::super::TrapDispatcher::RES_CHANGED_ATTR as u8
                | super::super::TrapDispatcher::RES_SYS_REF_ATTR as u8)
        );
        assert_ne!(
            file.map_attrs & super::super::TrapDispatcher::RES_MAP_CHANGED_ATTR,
            0
        );
    }

    #[test]
    fn addreference_persists_current_map_on_close_without_write_refnum_marker() {
        let (mut disp, mut cpu, mut bus, source_handle, _source_ptr) = addreference_setup();
        disp.vfs_rsrc.insert(
            "Prefs".to_string(),
            super::super::TrapDispatcher::empty_resource_fork_bytes(),
        );
        let prefs_refnum = disp.open_resource_file_from_vfs_key(&mut bus, "Prefs", false);
        assert!(!disp.write_refnums.contains(&prefs_refnum));

        let sp = TEST_SP;
        let name_ptr = 0x200020u32;
        write_pstring(&mut bus, name_ptr, b"PrefsCursor");
        let ref_id = 210i16;

        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, ref_id as u16);
        bus.write_long(sp + 6, source_handle);
        call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_word(0x0A60), 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, prefs_refnum);
        let result = disp.dispatch_toolbox(true, 0x19A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert!(!disp
            .resources
            .as_ref()
            .unwrap()
            .files
            .contains_key(&prefs_refnum));

        let fork = crate::managers::resource::ResourceFork::parse(
            disp.vfs_rsrc.get("Prefs").expect("Prefs resource fork"),
        )
        .expect("serialized resource fork should parse after CloseResFile");
        let resource = fork
            .resources()
            .get(&(*b"CURS", ref_id))
            .expect("AddReference entry should survive CloseResFile");
        assert_eq!(
            resource.data,
            vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]
        );
        assert_eq!(resource.name.as_deref(), Some("PrefsCursor"));
        assert_eq!(
            resource.attrs & super::super::TrapDispatcher::RES_CHANGED_ATTR as u8,
            0
        );
        assert_ne!(
            resource.attrs & super::super::TrapDispatcher::RES_SYS_REF_ATTR as u8,
            0
        );
    }

    // Inside Macintosh Volume I (1985), p. I-124: RmvResource removes the
    // current-file reference, leaves the handle's data allocated, and keeps
    // the current resource file unchanged.
    #[test]
    fn rmvereference_removes_current_file_reference_without_disposing_handle_data() {
        let (mut disp, mut cpu, mut bus) = setup();

        let data_ptr = setup_resources(
            &mut disp,
            &mut bus,
            b"STR ",
            30000,
            &[0x10, 0x20, 0x30, 0x40],
        );
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"STR ", 30000, data_ptr);
        let current_before = disp.current_resource_refnum();

        bus.write_long(TEST_SP, handle);
        call_trap_word(&mut disp, 0xA9AE, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(disp.current_resource_refnum(), current_before);
        assert_eq!(bus.read_long(handle), data_ptr);
        assert!(!disp.loaded_handles.contains_key(&handle));
        assert!(!disp.resource_handle_files.contains_key(&handle));
        assert!(!disp
            .resources
            .as_ref()
            .unwrap()
            .files
            .get(&0)
            .unwrap()
            .loaded
            .contains_key(&(*b"STR ", 30000)));
    }

    #[test]
    fn addreference_shadows_system_handle_metadata_in_current_file() {
        let (mut disp, mut cpu, mut bus, source_handle, _source_ptr) = addreference_setup();
        let sp = TEST_SP;
        let add_name_ptr = 0x200020u32;
        let info_name_ptr = 0x200040u32;
        let info_type_ptr = 0x200060u32;
        let info_id_ptr = 0x200080u32;
        write_pstring(&mut bus, add_name_ptr, b"AddedReference");
        let ref_id = 206i16;

        bus.write_long(sp, add_name_ptr);
        bus.write_word(sp + 4, ref_id as u16);
        bus.write_long(sp + 6, source_handle);
        call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_word(0x0A60), 0);

        bus.write_long(sp, info_name_ptr);
        bus.write_long(sp + 4, info_type_ptr);
        bus.write_long(sp + 8, info_id_ptr);
        bus.write_long(sp + 12, source_handle);
        cpu.write_reg(Register::A7, sp);
        call_trap_word(&mut disp, 0xA9A8, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(
            disp.resource_handle_files.get(&source_handle).copied(),
            Some(0)
        );
        assert_eq!(bus.read_word(info_id_ptr), ref_id as u16);
        assert_eq!(bus.read_long(info_type_ptr), u32::from_be_bytes(*b"CURS"));
        assert_eq!(bus.read_pstring(info_name_ptr), b"AddedReference".to_vec());

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, source_handle);
        bus.write_word(sp + 4, 0xBEEF);
        call_trap_word(&mut disp, 0xA9A6, &mut cpu, &mut bus).unwrap();

        let attrs = bus.read_word(sp + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_ne!(
            attrs
                & (super::super::TrapDispatcher::RES_CHANGED_ATTR as u16
                    | super::super::TrapDispatcher::RES_SYS_REF_ATTR as u16),
            0
        );
        assert_ne!(
            attrs & super::super::TrapDispatcher::RES_SYS_REF_ATTR as u16,
            0
        );
        assert_ne!(
            attrs & super::super::TrapDispatcher::RES_CHANGED_ATTR as u16,
            0
        );
    }

    #[test]
    fn addreference_preserves_a5_a6_and_pops_success_frame() {
        let (mut disp, mut cpu, mut bus, source_handle, _source_ptr) = addreference_setup();
        let sp = TEST_SP;
        let name_ptr = 0x200180u32;
        write_pstring(&mut bus, name_ptr, b"Regs");
        let ref_id = 204i16;

        cpu.write_reg(Register::A5, 0x1A5A_0000);
        cpu.write_reg(Register::A6, 0x1A6A_0000);

        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, ref_id as u16);
        bus.write_long(sp + 6, source_handle);
        call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(cpu.read_reg(Register::A5), 0x1A5A_0000);
        assert_eq!(cpu.read_reg(Register::A6), 0x1A6A_0000);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn addreference_preserves_all_nonvolatile_registers_on_success() {
        let (mut disp, mut cpu, mut bus, source_handle, _source_ptr) = addreference_setup();
        let sp = TEST_SP;
        let name_ptr = 0x2001C0u32;
        write_pstring(&mut bus, name_ptr, b"AllRegs");
        let ref_id = 205i16;

        let sentinels = [
            (Register::D2, 0xD202_0202),
            (Register::D3, 0xD303_0303),
            (Register::D4, 0xD404_0404),
            (Register::D5, 0xD505_0505),
            (Register::D6, 0xD606_0606),
            (Register::D7, 0xD707_0707),
            (Register::A2, 0xA202_0202),
            (Register::A3, 0xA303_0303),
            (Register::A4, 0xA404_0404),
            (Register::A5, 0xA505_0505),
            (Register::A6, 0xA606_0606),
        ];

        for &(reg, value) in &sentinels {
            cpu.write_reg(reg, value);
        }

        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, ref_id as u16);
        bus.write_long(sp + 6, source_handle);
        call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        for &(reg, value) in &sentinels {
            assert_eq!(cpu.read_reg(reg), value, "{reg:?} should be preserved");
        }
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn addreference_duplicate_reference_sets_addreffailed() {
        let (mut disp, mut cpu, mut bus, source_handle, _source_ptr) = addreference_setup();
        let sp = TEST_SP;
        let name_ptr = 0x200200u32;
        write_pstring(&mut bus, name_ptr, b"DuplicateRef");
        let ref_id = 201i16;

        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, ref_id as u16);
        bus.write_long(sp + 6, source_handle);
        call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_word(0x0A60), 0);

        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, ref_id as u16);
        bus.write_long(sp + 6, source_handle);
        call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_word(0x0A60) as i16, -195);
        assert_eq!(
            disp.resources
                .as_ref()
                .unwrap()
                .files
                .get(&1)
                .unwrap()
                .loaded
                .len(),
            1
        );
    }

    #[test]
    fn addreference_non_system_handle_sets_addreffailed() {
        let (mut disp, mut cpu, mut bus, _source_handle, _source_ptr) = addreference_setup();
        let sp = TEST_SP;
        let name_ptr = 0x200300u32;
        write_pstring(&mut bus, name_ptr, b"RawHandle");

        let raw_ptr = bus.alloc(4);
        bus.write_long(raw_ptr, 0xCAFEBABE);
        let raw_handle = bus.alloc(4);
        bus.write_long(raw_handle, raw_ptr);

        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, 202u16);
        bus.write_long(sp + 6, raw_handle);
        call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(0x0A60) as i16, -195);
        assert!(!disp
            .resources
            .as_ref()
            .unwrap()
            .files
            .get(&1)
            .unwrap()
            .loaded
            .contains_key(&(*b"CURS", 202)));
    }

    #[test]
    fn addreference_current_system_file_sets_addreffailed() {
        let (mut disp, mut cpu, mut bus, source_handle, _source_ptr) = addreference_setup();
        let sp = TEST_SP;
        let name_ptr = 0x200400u32;
        write_pstring(&mut bus, name_ptr, b"SysFileRef");

        bus.write_word(sp, 0);
        let result = disp.dispatch_toolbox(true, 0x198, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(disp.current_resource_refnum(), 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, 203u16);
        bus.write_long(sp + 6, source_handle);
        call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(0x0A60) as i16, -195);
        assert_eq!(disp.current_resource_refnum(), 0);
    }

    // ================================================================
    // 7. Get1NamedResource (0x020) — found
    // ================================================================
    #[test]
    fn get1_named_resource_found() {
        let (mut disp, mut cpu, mut bus) = setup();

        let data_ptr = bus.alloc(16);
        bus.write_bytes(data_ptr, &[0x42; 16]);

        let mut loaded = HashMap::new();
        loaded.insert((*b"STR ", 500i16), data_ptr);
        let mut named = HashMap::new();
        named.insert((*b"STR ", "MyString".to_string()), (500i16, data_ptr));
        let mut names_by_id = HashMap::new();
        names_by_id.insert((*b"STR ", 500i16), "MyString".to_string());
        disp.resources = Some(LoadedResources {
            files: HashMap::from([(
                0,
                ResourceFileMap {
                    loaded,
                    named,
                    names_by_id,
                    attrs: HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });

        // Write Pascal string name at 0x200000
        let name_addr = 0x200000u32;
        write_pstring(&mut bus, name_addr, b"MyString");

        let sp = TEST_SP;
        bus.write_long(sp, name_addr); // name_ptr
        bus.write_long(sp + 4, u32::from_be_bytes(*b"STR ")); // type

        call(&mut disp, true, 0x020, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 8);
        let handle = bus.read_long(new_sp);
        assert_ne!(
            handle, 0,
            "handle should be non-zero for found named resource"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            handle,
            "found named resource handle should be returned in A0"
        );
    }

    #[test]
    fn get1_named_resource_miss_returns_nil_in_a0() {
        let (mut disp, mut cpu, mut bus) = setup();
        let name_addr = 0x200000u32;
        write_pstring(&mut bus, name_addr, b"Missing");

        bus.write_long(TEST_SP, name_addr);
        bus.write_long(TEST_SP + 4, u32::from_be_bytes(*b"STR "));

        call(&mut disp, true, 0x020, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(cpu.read_reg(Register::A0), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(TEST_SP + 8), 0);
    }

    #[test]
    fn get1_named_resource_reloads_after_release() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data = [0x42, 0x43, 0x44, 0x45];
        let data_ptr = setup_resources(&mut disp, &mut bus, b"TEST", 500, &data);
        disp.resources
            .as_mut()
            .unwrap()
            .files
            .get_mut(&0)
            .unwrap()
            .named
            .insert((*b"TEST", "ReloadMe".to_string()), (500, data_ptr));
        disp.resources
            .as_mut()
            .unwrap()
            .files
            .get_mut(&0)
            .unwrap()
            .names_by_id
            .insert((*b"TEST", 500), "ReloadMe".to_string());

        let name_ptr = 0x300000u32;
        write_pstring(&mut bus, name_ptr, b"ReloadMe");
        bus.write_long(TEST_SP, name_ptr);
        bus.write_long(TEST_SP + 4, u32::from_be_bytes(*b"TEST"));
        call(&mut disp, true, 0x020, &mut cpu, &mut bus).unwrap();
        let first_handle = bus.read_long(TEST_SP + 8);
        assert_eq!(bus.read_long(first_handle), data_ptr);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, first_handle);
        call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_long(first_handle), 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, name_ptr);
        bus.write_long(TEST_SP + 4, u32::from_be_bytes(*b"TEST"));
        call(&mut disp, true, 0x020, &mut cpu, &mut bus).unwrap();
        let second_handle = bus.read_long(TEST_SP + 8);
        let second_ptr = bus.read_long(second_handle);
        assert_ne!(second_handle, first_handle);
        assert_ne!(second_ptr, 0);
        assert_eq!(bus.read_bytes(second_ptr, data.len()), data);
    }

    // ================================================================
    // 7b. GetResInfo (0x1A8) — named resource metadata
    // ================================================================
    #[test]
    fn get_res_info_named_resource_returns_id_type_and_name() {
        let (mut disp, mut cpu, mut bus) = setup();

        let data_ptr = setup_resources(&mut disp, &mut bus, b"STR ", 500, &[0x42; 16]);
        disp.resources
            .as_mut()
            .unwrap()
            .files
            .get_mut(&0)
            .unwrap()
            .named
            .insert((*b"STR ", "MyString".to_string()), (500, data_ptr));
        disp.resources
            .as_mut()
            .unwrap()
            .files
            .get_mut(&0)
            .unwrap()
            .names_by_id
            .insert((*b"STR ", 500), "MyString".to_string());

        let handle = disp.get_or_create_resource_handle(&mut bus, *b"STR ", 500, data_ptr);
        let name_ptr = 0x200000u32;
        let type_ptr = 0x200100u32;
        let id_ptr = 0x200104u32;

        let sp = TEST_SP;
        bus.write_long(sp, name_ptr);
        bus.write_long(sp + 4, type_ptr);
        bus.write_long(sp + 8, id_ptr);
        bus.write_long(sp + 12, handle);

        call(&mut disp, true, 0x1A8, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
        assert_eq!(bus.read_word(id_ptr) as i16, 500);
        assert_eq!(bus.read_long(type_ptr), u32::from_be_bytes(*b"STR "));
        assert_eq!(bus.read_byte(name_ptr), 8);
        assert_eq!(bus.read_bytes(name_ptr + 1, 8), b"MyString");
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn get_res_info_preserves_duplicate_resource_names_by_id() {
        let (mut disp, mut cpu, mut bus) = setup();

        let first_ptr = setup_resources(&mut disp, &mut bus, b"m\x95sn", 128, &[0x11; 4]);
        let second_ptr = disp.install_named_test_resource_in_file(
            &mut bus,
            0,
            *b"m\x95sn",
            129,
            "Ferry Passengers to <DST>",
            &[0x22; 4],
        );
        let file = disp.resources.as_mut().unwrap().files.get_mut(&0).unwrap();
        file.named.insert(
            (*b"m\x95sn", "Ferry Passengers to <DST>".to_string()),
            (128, first_ptr),
        );
        file.names_by_id
            .insert((*b"m\x95sn", 128), "Ferry Passengers to <DST>".to_string());

        let first_handle =
            disp.get_or_create_resource_handle(&mut bus, *b"m\x95sn", 128, first_ptr);
        let second_handle =
            disp.get_or_create_resource_handle(&mut bus, *b"m\x95sn", 129, second_ptr);
        let name_ptr = 0x200000u32;
        let type_ptr = 0x200100u32;
        let id_ptr = 0x200104u32;
        let sp = TEST_SP;

        for (handle, expected_id) in [(first_handle, 128i16), (second_handle, 129i16)] {
            bus.write_byte(name_ptr, 0);
            bus.write_long(sp, name_ptr);
            bus.write_long(sp + 4, type_ptr);
            bus.write_long(sp + 8, id_ptr);
            bus.write_long(sp + 12, handle);

            call(&mut disp, true, 0x1A8, &mut cpu, &mut bus).unwrap();

            assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
            assert_eq!(bus.read_word(id_ptr) as i16, expected_id);
            assert_eq!(bus.read_long(type_ptr), u32::from_be_bytes(*b"m\x95sn"));
            let name_len = bus.read_byte(name_ptr) as usize;
            assert_eq!(
                bus.read_bytes(name_ptr + 1, name_len),
                b"Ferry Passengers to <DST>"
            );
            assert_eq!(bus.read_word(0x0A60), 0);
            cpu.write_reg(Register::A7, TEST_SP);
        }
    }

    // ================================================================
    // 7c. GetResInfo (0x1A8) — unnamed resource clears stale Str255
    // ================================================================
    #[test]
    fn get_res_info_unnamed_resource_clears_name_buffer() {
        let (mut disp, mut cpu, mut bus) = setup();

        let data_ptr = setup_resources(&mut disp, &mut bus, b"TEST", 7, &[0x10, 0x20, 0x30]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"TEST", 7, data_ptr);
        let name_ptr = 0x200200u32;
        let type_ptr = 0x200300u32;
        let id_ptr = 0x200304u32;

        bus.write_bytes(name_ptr, b"urer\0");

        let sp = TEST_SP;
        bus.write_long(sp, name_ptr);
        bus.write_long(sp + 4, type_ptr);
        bus.write_long(sp + 8, id_ptr);
        bus.write_long(sp + 12, handle);

        call(&mut disp, true, 0x1A8, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
        assert_eq!(bus.read_word(id_ptr) as i16, 7);
        assert_eq!(bus.read_long(type_ptr), u32::from_be_bytes(*b"TEST"));
        assert_eq!(bus.read_byte(name_ptr), 0);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    // More Macintosh Toolbox (1993), pp. 1-81 to 1-82: if the handle is not
    // a resource handle, GetResInfo does nothing and ResError is resNotFound.
    #[test]
    fn get_res_info_non_resource_handle_sets_resnotfound() {
        let (mut disp, mut cpu, mut bus) = setup();

        let fake_ptr = bus.alloc(4);
        bus.write_long(fake_ptr, 0xDEADBEEF);
        let fake_handle = bus.alloc(4);
        bus.write_long(fake_handle, fake_ptr);

        let name_ptr = 0x200400u32;
        let type_ptr = 0x200500u32;
        let id_ptr = 0x200504u32;
        bus.write_pstring(name_ptr, b"keepme");
        bus.write_long(type_ptr, 0x11223344);
        bus.write_word(id_ptr, 0x5566);

        bus.write_long(TEST_SP, name_ptr);
        bus.write_long(TEST_SP + 4, type_ptr);
        bus.write_long(TEST_SP + 8, id_ptr);
        bus.write_long(TEST_SP + 12, fake_handle);

        call(&mut disp, true, 0x1A8, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
        assert_eq!(bus.read_word(0x0A60), (-192i16) as u16);
        assert_eq!(bus.read_byte(name_ptr), 6);
        assert_eq!(bus.read_long(type_ptr), 0x11223344);
        assert_eq!(bus.read_word(id_ptr), 0x5566);
    }

    // ================================================================
    // 7d. SetResInfo (0x1A9)
    // ================================================================
    // Inside Macintosh Volume I (1985), p. I-122: SetResInfo takes
    // (Handle, Integer, Str255 ptr) and pops 10 bytes.
    #[test]
    fn setresinfo_consumes_handle_id_and_name_arguments() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"SINF", 7, &[0x10, 0x20]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"SINF", 7, data_ptr);

        let name_ptr = 0x200600u32;
        write_pstring(&mut bus, name_ptr, b"Renamed");

        bus.write_long(TEST_SP, name_ptr);
        bus.write_word(TEST_SP + 4, 11u16);
        bus.write_long(TEST_SP + 6, handle);
        bus.write_word(TEST_SP + 10, 0xBEEF);

        call(&mut disp, true, 0x1A9, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(bus.read_word(TEST_SP + 10), 0xBEEF);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    // Inside Macintosh Volume I (1985), p. I-122: SetResInfo updates
    // resource map metadata (ID and name) for the specified resource.
    #[test]
    fn setresinfo_updates_resource_id_and_name_for_unprotected_resource() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"NAME", 7, &[0x31, 0x32, 0x33]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"NAME", 7, data_ptr);
        disp.resources
            .as_mut()
            .unwrap()
            .files
            .get_mut(&0)
            .unwrap()
            .named
            .insert((*b"NAME", "OldName".to_string()), (7, data_ptr));

        let name_ptr = 0x200700u32;
        write_pstring(&mut bus, name_ptr, b"NewName");
        bus.write_long(TEST_SP, name_ptr);
        bus.write_word(TEST_SP + 4, 11u16);
        bus.write_long(TEST_SP + 6, handle);

        call(&mut disp, true, 0x1A9, &mut cpu, &mut bus).unwrap();

        let file = disp.resources.as_ref().unwrap().files.get(&0).unwrap();
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(
            disp.loaded_handles.get(&handle).map(|entry| entry.2),
            Some(11i16)
        );
        assert!(file.loaded.contains_key(&(*b"NAME", 11)));
        assert!(!file.loaded.contains_key(&(*b"NAME", 7)));
        assert_eq!(
            file.named.get(&(*b"NAME", "NewName".to_string())),
            Some(&(11, data_ptr))
        );
        assert!(!file.named.contains_key(&(*b"NAME", "OldName".to_string())));
    }

    // Inside Macintosh Volume I (1985), p. I-122: resource metadata APIs
    // report missing-handle failures via ResError.
    #[test]
    fn setresinfo_invalid_handle_sets_resnotfound() {
        let (mut disp, mut cpu, mut bus) = setup();
        let fake_ptr = bus.alloc(4);
        bus.write_long(fake_ptr, 0xCAFEBABE);
        let fake_handle = bus.alloc(4);
        bus.write_long(fake_handle, fake_ptr);

        let name_ptr = 0x200800u32;
        write_pstring(&mut bus, name_ptr, b"NeverUsed");
        bus.write_long(TEST_SP, name_ptr);
        bus.write_word(TEST_SP + 4, 19u16);
        bus.write_long(TEST_SP + 6, fake_handle);

        call(&mut disp, true, 0x1A9, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(bus.read_word(0x0A60) as i16, -192);
    }

    // Inside Macintosh Volume IV (1986), p. IV-16: resProtected blocks
    // metadata mutation for SetResInfo and returns resAttrErr.
    #[test]
    fn setresinfo_protected_resource_is_noop_with_resattrerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"PROT", 3, &[0x01, 0x02]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"PROT", 3, data_ptr);
        disp.resources
            .as_mut()
            .unwrap()
            .files
            .get_mut(&0)
            .unwrap()
            .named
            .insert((*b"PROT", "KeepName".to_string()), (3, data_ptr));
        disp.resources
            .as_mut()
            .unwrap()
            .files
            .get_mut(&0)
            .unwrap()
            .attrs
            .insert((*b"PROT", 3), 0x0008u8);

        let name_ptr = 0x200900u32;
        write_pstring(&mut bus, name_ptr, b"NewName");
        bus.write_long(TEST_SP, name_ptr);
        bus.write_word(TEST_SP + 4, 22u16);
        bus.write_long(TEST_SP + 6, handle);

        call(&mut disp, true, 0x1A9, &mut cpu, &mut bus).unwrap();

        let file = disp.resources.as_ref().unwrap().files.get(&0).unwrap();
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(
            bus.read_word(0x0A60) as i16,
            super::super::TrapDispatcher::RES_ATTR_ERR
        );
        assert_eq!(
            disp.loaded_handles.get(&handle).map(|entry| entry.2),
            Some(3i16)
        );
        assert!(file.loaded.contains_key(&(*b"PROT", 3)));
        assert!(!file.loaded.contains_key(&(*b"PROT", 22)));
        assert!(file.named.contains_key(&(*b"PROT", "KeepName".to_string())));
        assert!(!file.named.contains_key(&(*b"PROT", "NewName".to_string())));
    }

    // ================================================================
    // 7e. GetResFileAttrs (0x1F6) / SetResFileAttrs (0x1F7)
    // ================================================================
    // Inside Macintosh Volume I (1985), p. I-126: GetResFileAttrs returns
    // the file map attribute word for a valid resource-file refNum.
    #[test]
    fn getresfileattrs_returns_only_documented_map_attr_bits_for_open_resource_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        setup_resources(&mut disp, &mut bus, b"RFAT", 1, &[0xAA]);
        disp.resources
            .as_mut()
            .unwrap()
            .files
            .get_mut(&0)
            .unwrap()
            .map_attrs = 0x0234;

        bus.write_word(TEST_SP, 0u16);
        bus.write_word(TEST_SP + 2, 0xBEEF);

        call(&mut disp, true, 0x1F6, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
        assert_eq!(bus.read_word(TEST_SP + 2), 0x0020);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    // Inside Macintosh Volume I (1985), p. I-126: if refNum is unknown,
    // GetResFileAttrs does nothing and returns resFNotFound in ResError.
    #[test]
    fn getresfileattrs_missing_refnum_sets_resfnotfound_and_preserves_result_slot() {
        let (mut disp, mut cpu, mut bus) = setup();
        setup_resources(&mut disp, &mut bus, b"RFAT", 1, &[0xAA]);

        bus.write_word(TEST_SP, 99u16);
        bus.write_word(TEST_SP + 2, 0xBEEF);

        call(&mut disp, true, 0x1F6, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
        assert_eq!(bus.read_word(TEST_SP + 2), 0xBEEF);
        assert_eq!(bus.read_word(0x0A60) as i16, -193);
    }

    // Inside Macintosh Volume I (1985), p. I-126: GetResFileAttrs is a
    // Pascal FUNCTION that consumes one INTEGER refNum and writes one
    // INTEGER result slot.
    #[test]
    fn getresfileattrs_consumes_refnum_and_result_slot() {
        let (mut disp, mut cpu, mut bus) = setup();
        setup_resources(&mut disp, &mut bus, b"RFAT", 1, &[0xAA]);
        disp.resources
            .as_mut()
            .unwrap()
            .files
            .get_mut(&0)
            .unwrap()
            .map_attrs = 0x0234;

        bus.write_word(TEST_SP, 0u16);
        bus.write_word(TEST_SP + 2, 0xBEEF);

        let sp_pre = cpu.read_reg(Register::A7);
        call(&mut disp, true, 0x1F6, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp_pre + 2);
        assert_eq!(bus.read_word(TEST_SP + 2), 0x0020);
    }

    // Inside Macintosh Volume I (1985), p. I-126: SetResFileAttrs takes
    // refNum + attrs and mutates the target file map attributes.
    #[test]
    fn setresfileattrs_consumes_refnum_and_attrs_arguments() {
        let (mut disp, mut cpu, mut bus) = setup();
        setup_resources(&mut disp, &mut bus, b"SRAF", 2, &[0xBB]);

        bus.write_word(TEST_SP, 0x01A5u16);
        bus.write_word(TEST_SP + 2, 0u16);
        bus.write_word(TEST_SP + 4, 0xBEEF);

        call(&mut disp, true, 0x1F7, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(TEST_SP + 4), 0xBEEF);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(
            disp.resources
                .as_ref()
                .unwrap()
                .files
                .get(&0)
                .unwrap()
                .map_attrs,
            0x00A0
        );
    }

    // Inside Macintosh Volume I (1985), p. I-126 lists only the
    // mapReadOnly/mapCompact/mapChanged bits as public resource-file
    // attributes; undefined bits are ignored by SetResFileAttrs and not
    // surfaced by GetResFileAttrs.
    #[test]
    fn setresfileattrs_getresfileattrs_roundtrip_only_documented_bits() {
        let (mut disp, mut cpu, mut bus) = setup();
        setup_resources(&mut disp, &mut bus, b"MASK", 3, &[0xCC]);

        bus.write_word(TEST_SP, 0xFFFFu16);
        bus.write_word(TEST_SP + 2, 0u16);
        call(&mut disp, true, 0x1F7, &mut cpu, &mut bus).unwrap();

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0u16);
        bus.write_word(TEST_SP + 2, 0xBEEFu16);
        call(&mut disp, true, 0x1F6, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            disp.resources
                .as_ref()
                .unwrap()
                .files
                .get(&0)
                .unwrap()
                .map_attrs,
            0x00E0
        );
        assert_eq!(bus.read_word(TEST_SP + 2), 0x00E0);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    // Inside Macintosh Volume I (1985), p. I-126: for unknown refNum,
    // SetResFileAttrs does nothing and still returns noErr.
    #[test]
    fn setresfileattrs_missing_refnum_is_noop_with_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        setup_resources(&mut disp, &mut bus, b"SRAF", 2, &[0xBB]);
        disp.resources
            .as_mut()
            .unwrap()
            .files
            .get_mut(&0)
            .unwrap()
            .map_attrs = 0x0033;

        bus.write_word(TEST_SP, 0x0F0Fu16);
        bus.write_word(TEST_SP + 2, 77u16);

        call(&mut disp, true, 0x1F7, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(
            disp.resources
                .as_ref()
                .unwrap()
                .files
                .get(&0)
                .unwrap()
                .map_attrs,
            0x0033
        );
    }

    fn add_resource_for_writeback_test(
        disp: &mut super::super::TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut crate::memory::MacMemoryBus,
        res_type: [u8; 4],
        res_id: i16,
        data: &[u8],
        name: &[u8],
    ) -> u32 {
        let data_ptr = bus.alloc(data.len() as u32);
        bus.write_bytes(data_ptr, data);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);
        let name_ptr = 0x250000u32;
        write_pstring(bus, name_ptr, name);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, name_ptr);
        bus.write_word(TEST_SP + 4, res_id as u16);
        bus.write_long(TEST_SP + 6, u32::from_be_bytes(res_type));
        bus.write_long(TEST_SP + 10, handle);
        call(disp, true, 0x1AB, cpu, bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 14);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_ne!(
            disp.resource_attributes_for_handle(handle).unwrap_or(0)
                & super::super::TrapDispatcher::RES_CHANGED_ATTR,
            0
        );
        handle
    }

    // Inside Macintosh Volume I (1985), p. I-125: WriteResource clears
    // resChanged on successful writes for changed resources.
    #[test]
    fn writeresource_changed_resource_clears_reschanged_attribute() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"WRIT", 77, &[0x10, 0x20, 0x30]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"WRIT", 77, data_ptr);

        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap(); // ChangedResource
        assert_ne!(
            disp.resource_attributes_for_handle(handle).unwrap_or(0)
                & super::super::TrapDispatcher::RES_CHANGED_ATTR,
            0
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1B0, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(
            disp.resource_attributes_for_handle(handle).unwrap_or(0)
                & super::super::TrapDispatcher::RES_CHANGED_ATTR,
            0
        );
    }

    #[test]
    fn writeresource_persists_added_resource_to_writable_vfs_resource_fork() {
        // IM:I I-124..I-125: WriteResource writes changed resource data to
        // the resource file and clears resChanged. Systemless must mirror the
        // write into vfs_rsrc so a later OpenRFPerm/Get1Resource sees it.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.vfs_rsrc.insert(
            "Prefs".to_string(),
            super::super::TrapDispatcher::empty_resource_fork_bytes(),
        );
        let _refnum = disp.open_resource_file_from_vfs_key(&mut bus, "Prefs", true);
        let handle = add_resource_for_writeback_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            *b"CAsp",
            20000,
            &[0x10, 0x20, 0x30, 0x40],
            b"Config",
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        call(&mut disp, true, 0x1B0, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(
            disp.resource_attributes_for_handle(handle).unwrap_or(0)
                & super::super::TrapDispatcher::RES_CHANGED_ATTR,
            0
        );

        let fork = crate::managers::resource::ResourceFork::parse(
            disp.vfs_rsrc.get("Prefs").expect("Prefs resource fork"),
        )
        .expect("serialized resource fork should parse");
        let resource = fork
            .resources()
            .get(&(*b"CAsp", 20000))
            .expect("added resource should be serialized");
        assert_eq!(resource.data, vec![0x10, 0x20, 0x30, 0x40]);
        assert_eq!(resource.name.as_deref(), Some("Config"));
        assert_eq!(
            resource.attrs & super::super::TrapDispatcher::RES_CHANGED_ATTR as u8,
            0
        );
    }

    #[test]
    fn closeresfile_persists_added_resource_before_closing_writable_map() {
        // CloseResFile performs UpdateResFile before removing the map from
        // the open-resource chain; the VFS mirror must be updated before the
        // in-memory file map is freed.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.vfs_rsrc.insert(
            "Prefs".to_string(),
            super::super::TrapDispatcher::empty_resource_fork_bytes(),
        );
        let refnum = disp.open_resource_file_from_vfs_key(&mut bus, "Prefs", true);
        let _handle = add_resource_for_writeback_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            *b"KMpt",
            20000,
            &[0x55, 0x66, 0x77],
            b"Keys",
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, refnum);
        let result = disp.dispatch_toolbox(true, 0x19A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert!(!disp.resources.as_ref().unwrap().files.contains_key(&refnum));

        let fork = crate::managers::resource::ResourceFork::parse(
            disp.vfs_rsrc.get("Prefs").expect("Prefs resource fork"),
        )
        .expect("serialized resource fork should parse after CloseResFile");
        let resource = fork
            .resources()
            .get(&(*b"KMpt", 20000))
            .expect("added resource should survive CloseResFile");
        assert_eq!(resource.data, vec![0x55, 0x66, 0x77]);
        assert_eq!(resource.name.as_deref(), Some("Keys"));
    }

    #[test]
    fn updateresfile_persists_added_resource_to_writable_vfs_resource_fork() {
        // UpdateResFile writes changed resources and the resource map for the
        // specified refNum. This is the normal batching path used by apps that
        // add several preference resources before closing the file.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.vfs_rsrc.insert(
            "Prefs".to_string(),
            super::super::TrapDispatcher::empty_resource_fork_bytes(),
        );
        let refnum = disp.open_resource_file_from_vfs_key(&mut bus, "Prefs", true);
        let handle = add_resource_for_writeback_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            *b"MDta",
            128,
            &[0xA1, 0xB2, 0xC3, 0xD4],
            b"Meta",
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, refnum);
        let result = disp.dispatch_toolbox(true, 0x199, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(
            disp.resource_attributes_for_handle(handle).unwrap_or(0)
                & super::super::TrapDispatcher::RES_CHANGED_ATTR,
            0
        );

        let fork = crate::managers::resource::ResourceFork::parse(
            disp.vfs_rsrc.get("Prefs").expect("Prefs resource fork"),
        )
        .expect("serialized resource fork should parse after UpdateResFile");
        let resource = fork
            .resources()
            .get(&(*b"MDta", 128))
            .expect("added resource should survive UpdateResFile");
        assert_eq!(resource.data, vec![0xA1, 0xB2, 0xC3, 0xD4]);
        assert_eq!(resource.name.as_deref(), Some("Meta"));
    }

    // Inside Macintosh Volume I (1985), p. I-125: WriteResource is a no-op
    // with noErr for protected resources and for resources not marked changed.
    #[test]
    fn writeresource_protected_or_unchanged_resource_is_noop_with_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let changed = super::super::TrapDispatcher::RES_CHANGED_ATTR;

        let prot_ptr = setup_resources(&mut disp, &mut bus, b"PROT", 1, &[0xAA; 4]);
        let prot_handle = disp.get_or_create_resource_handle(&mut bus, *b"PROT", 1, prot_ptr);
        bus.write_long(TEST_SP, prot_handle);
        call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap(); // ChangedResource
        {
            let attrs = disp
                .resources
                .as_mut()
                .unwrap()
                .files
                .get_mut(&0)
                .unwrap()
                .attrs
                .entry((*b"PROT", 1))
                .or_insert(0);
            *attrs |= 0x0008u8; // resProtected
        }

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, prot_handle);
        call(&mut disp, true, 0x1B0, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_ne!(
            disp.resource_attributes_for_handle(prot_handle)
                .unwrap_or(0)
                & changed,
            0
        );

        let plain_ptr = setup_resources(&mut disp, &mut bus, b"PLAI", 2, &[0xBB; 4]);
        let plain_handle = disp.get_or_create_resource_handle(&mut bus, *b"PLAI", 2, plain_ptr);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, plain_handle);
        call(&mut disp, true, 0x1B0, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(
            disp.resource_attributes_for_handle(plain_handle)
                .unwrap_or(0)
                & changed,
            0
        );
    }

    // Inside Macintosh Volume I (1985), p. I-125: non-resource handles cause
    // WriteResource to do nothing and return resNotFound.
    #[test]
    fn writeresource_non_resource_handle_returns_resnotfound() {
        let (mut disp, mut cpu, mut bus) = setup();
        let fake_ptr = bus.alloc(8);
        bus.write_bytes(fake_ptr, &[0xDE, 0xAD, 0xBE, 0xEF, 1, 2, 3, 4]);
        let fake_handle = bus.alloc(4);
        bus.write_long(fake_handle, fake_ptr);

        bus.write_long(TEST_SP, fake_handle);
        call(&mut disp, true, 0x1B0, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(0x0A60) as i16, -192);
        assert_eq!(bus.read_long(fake_handle), fake_ptr);
    }

    // ================================================================
    // 8. CurResFile (0x194)
    // ================================================================
    #[test]
    fn cur_res_file() {
        let (mut disp, mut cpu, mut bus) = setup();

        call(&mut disp, true, 0x194, &mut cpu, &mut bus).unwrap();

        let sp = cpu.read_reg(Register::A7);
        assert_eq!(sp, TEST_SP, "SP should be unchanged");
        let refnum = bus.read_word(sp);
        assert_eq!(refnum, 0);
    }

    // ================================================================
    // 9. HomeResFile (0x1A4)
    // ================================================================
    #[test]
    fn home_res_file_returns_loaded_resource_refnum() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = 0x1234u32;
        disp.loaded_handles.insert(handle, (0x200000, *b"STR ", 1));
        disp.resource_handle_files.insert(handle, 128);

        let sp = TEST_SP;
        bus.write_long(sp, handle);

        call(&mut disp, true, 0x1A4, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 4);
        let refnum = bus.read_word(new_sp);
        assert_eq!(refnum, 128);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn home_res_file_returns_minus_one_for_detached_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = 0x5678u32;
        disp.detached_handles.insert(handle, (*b"STR ", 2));
        disp.detached_handle_files.insert(handle, 202);

        let sp = TEST_SP;
        bus.write_long(sp, handle);

        call(&mut disp, true, 0x1A4, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 4);
        let refnum = bus.read_word(new_sp) as i16;
        assert_eq!(refnum, -1);
        assert_eq!(bus.read_word(0x0A60) as i16, -192);
    }

    #[test]
    fn home_res_file_returns_minus_one_for_unknown_handle() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP;
        bus.write_long(sp, 0x9ABCu32);

        call(&mut disp, true, 0x1A4, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 4);
        let refnum = bus.read_word(new_sp) as i16;
        assert_eq!(refnum, -1);
        assert_eq!(bus.read_word(0x0A60) as i16, -192);
    }

    // ================================================================
    // 10. LoadSeg (0x1F0)
    // ================================================================
    #[test]
    fn loadseg_patches_calling_jump_table_entry_to_jmp_loaded_code() {
        // Inside Macintosh Volume II (1985), p. II-61: LoadSeg rewrites an
        // unloaded jump-table entry to the loaded JMP form.
        //
        // Use the documented MPW entry layout:
        //   [offset, MOVE.W #segNum,-(SP), segNum, _LoadSeg]
        // so the trap can consume segNum from A7 and patch the entry.
        let (mut disp, mut cpu, mut bus) = setup();

        // Set up segment 1 at address 0x210000
        let seg_addr = 0x210000u32;
        // Write a non-0xFFFF header word so header_size = 4
        // Segment header: taboff=0, nentries=0 (no JT entries to patch)
        bus.write_word(seg_addr, 0x0000); // taboff
        bus.write_word(seg_addr + 2, 0x0000); // nentries
        let mut seg_map = HashMap::new();
        seg_map.insert(1i16, seg_addr);
        disp.register_segments(seg_map);

        // Set up a standard MPW jump table entry at 0x200000:
        //   +0: 0000  routine offset within segment
        //   +2: 3F3C  MOVE.W #imm, -(SP)
        //   +4: 0001  segment number
        //   +6: A9F0  _LoadSeg trap
        // After trap executes, the entry is patched to:
        //   +0: seg#
        //   +2: 4EF9  JMP.L
        //   +4: code_addr
        // Inside Macintosh Volume II, II-61
        let island_addr = 0x200000u32;
        bus.write_word(island_addr, 0x0000); // routine offset
        bus.write_word(island_addr + 2, 0x3F3C); // MOVE.W
        bus.write_word(island_addr + 4, 0x0001); // seg#
        bus.write_word(island_addr + 6, 0xA9F0); // _LoadSeg

        // PC points past the A9F0 instruction (as if CPU just executed it)
        cpu.write_reg(Register::PC, island_addr + 8);

        // Standard format: segment number is pushed on stack by MOVE.W
        let sp = TEST_SP;
        bus.write_word(sp, 1u16); // segment number

        call(&mut disp, true, 0x1F0, &mut cpu, &mut bus).unwrap();

        // Standard MPW format pops the segment number pushed by MOVE.W.
        let sp_after = cpu.read_reg(Register::A7);
        assert_eq!(sp_after, TEST_SP + 2);

        // Handler patches the island and sets PC to island+2 (the JMP.L instruction).
        // The JMP.L target is seg_addr + header_size(4) + routine_offset(0) = seg_addr + 4.
        let pc = cpu.read_reg(Register::PC);
        assert_eq!(pc, island_addr + 2, "PC should point to patched JMP.L");

        // Verify the island was patched correctly
        assert_eq!(
            bus.read_word(island_addr),
            1,
            "island[0] should be seg number"
        );
        assert_eq!(
            bus.read_word(island_addr + 2),
            0x4EF9,
            "island[2] should be JMP.L"
        );
        assert_eq!(
            bus.read_long(island_addr + 4),
            seg_addr + 4,
            "JMP target should be code entry"
        );
    }

    #[test]
    fn loadseg_mpw_call_consumes_segment_number_word_argument() {
        // Inside Macintosh Volume II (1985), p. II-60:
        // MPW jump-table callers push segNum with MOVE.W #segNum, -(SP)
        // before executing _LoadSeg.
        let (mut disp, mut cpu, mut bus) = setup();
        let seg_addr = 0x220000u32;
        bus.write_word(seg_addr, 0x0000);
        bus.write_word(seg_addr + 2, 0x0000);
        disp.register_segments(HashMap::from([(1i16, seg_addr)]));

        let island_addr = 0x210000u32;
        bus.write_word(island_addr, 0x0000);
        bus.write_word(island_addr + 2, 0x3F3C);
        bus.write_word(island_addr + 4, 0x0001);
        bus.write_word(island_addr + 6, 0xA9F0);

        cpu.write_reg(Register::PC, island_addr + 8);
        bus.write_word(TEST_SP, 1u16);
        bus.write_word(TEST_SP + 2, 0xBEEF);

        call(&mut disp, true, 0x1F0, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
        assert_eq!(bus.read_word(TEST_SP + 2), 0xBEEF);
    }

    #[test]
    fn loadseg_auto_pop_old_trap_uses_original_mpw_caller_entry() {
        let (mut disp, mut cpu, mut bus) = setup();

        let seg_addr = 0x220000u32;
        bus.write_word(seg_addr, 0x0000);
        bus.write_word(seg_addr + 2, 0x0000);
        disp.register_segments(HashMap::from([(2i16, seg_addr)]));
        disp.native_trap_table.insert(0xA9A0, 0x310000);

        let entry_addr = 0x240000u32;
        bus.write_word(entry_addr, 0x0010);
        bus.write_word(entry_addr + 2, 0x3F3C);
        bus.write_word(entry_addr + 4, 0x0002);
        bus.write_word(entry_addr + 6, 0xA9F0);

        let old_trap_stub = 0x300000u32;
        bus.write_word(old_trap_stub, 0xADF0);
        bus.write_word(old_trap_stub + 2, 0x0000);
        bus.write_word(old_trap_stub + 4, 0x4EB9);
        bus.write_long(old_trap_stub + 6, 0x00011111);

        bus.write_long(TEST_SP, entry_addr + 8);
        bus.write_word(TEST_SP + 4, 2);
        bus.write_word(TEST_SP + 6, 0xBEEF);
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::PC, old_trap_stub + 2);

        call_trap_word(&mut disp, 0xADF0, &mut cpu, &mut bus).unwrap();

        assert!(
            disp.loadseg_getresource_state.is_none(),
            "native LoadSeg old-trap fallback must not recursively invoke native GetResource"
        );
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(
            cpu.read_reg(Register::PC),
            entry_addr + 2,
            "LoadSeg should re-enter the patched original jump-table entry"
        );
        assert_eq!(bus.read_word(TEST_SP + 6), 0xBEEF);

        assert_eq!(bus.read_word(entry_addr), 2);
        assert_eq!(bus.read_word(entry_addr + 2), 0x4EF9);
        assert_eq!(bus.read_long(entry_addr + 4), seg_addr + 4 + 0x0010);

        assert_eq!(bus.read_word(old_trap_stub), 0xADF0);
        assert_eq!(bus.read_word(old_trap_stub + 2), 0x0000);
        assert_eq!(bus.read_word(old_trap_stub + 4), 0x4EB9);
    }

    #[test]
    fn loadseg_native_old_trap_recovers_the_recorded_original_call() {
        let (mut disp, mut cpu, mut bus) = setup();
        let seg_addr = 0x220000u32;
        bus.write_word(seg_addr, 0x0000);
        bus.write_word(seg_addr + 2, 0x0000);
        disp.register_segments(HashMap::from([(2i16, seg_addr)]));

        let entry_addr = 0x240000u32;
        bus.write_word(entry_addr, 0x0010);
        bus.write_word(entry_addr + 2, 0x3F3C);
        bus.write_word(entry_addr + 4, 0x0002);
        bus.write_word(entry_addr + 6, 0xA9F0);

        // SetTrapAddress accepts an arbitrary routine entry. Its first
        // instruction and stack-frame convention are deliberately irrelevant
        // to recovery of the original A-line call.
        let handler = 0x300000u32;
        bus.write_word(handler, 0x4E71); // NOP
        disp.native_trap_table.insert(0xA9F0, handler);
        cpu.write_reg(Register::PC, entry_addr + 8);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 2);

        disp.dispatch(0xA9F0, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::PC), handler);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 4);

        let trampoline = disp.get_or_create_tool_trap_trampoline(&mut bus, 0xA9F0);
        let handler_sp = TEST_SP - 0x100;
        bus.write_long(handler_sp, 0x310000);
        cpu.write_reg(Register::A7, handler_sp);
        cpu.write_reg(Register::PC, trampoline + 2);

        call_trap_word(&mut disp, 0xADF0, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::PC), entry_addr + 2);
        assert_eq!(cpu.read_reg(Register::A7), handler_sp + 4);
        assert_eq!(bus.read_word(entry_addr), 2);
        assert_eq!(bus.read_word(entry_addr + 2), 0x4EF9);
        assert_eq!(bus.read_long(entry_addr + 4), seg_addr + 4 + 0x0010);
        assert!(!disp.pending_native_trap_calls.contains_key(&0xA9F0));
    }

    #[test]
    fn loadseg_patches_all_entries_for_loaded_segment() {
        // Inside Macintosh Volume II (1985), pp. II-60 to II-61:
        // LoadSeg patches every unloaded jump-table entry for the segment
        // so future cross-segment calls can run directly.
        let (mut disp, mut cpu, mut bus) = setup();

        let seg_addr = 0x230000u32;
        bus.write_word(seg_addr, 0x0000);
        bus.write_word(seg_addr + 2, 0x0002);
        disp.register_segments(HashMap::from([(1i16, seg_addr)]));

        let island_addr = 0x240000u32;
        bus.write_word(island_addr, 0x0000); // routine offset 0
        bus.write_word(island_addr + 2, 0x3F3C);
        bus.write_word(island_addr + 4, 0x0001);
        bus.write_word(island_addr + 6, 0xA9F0);

        bus.write_word(island_addr + 8, 0x0004); // routine offset 4
        bus.write_word(island_addr + 10, 0x3F3C);
        bus.write_word(island_addr + 12, 0x0001);
        bus.write_word(island_addr + 14, 0xA9F0);

        cpu.write_reg(Register::PC, island_addr + 8);
        cpu.write_reg(Register::A5, island_addr);
        bus.write_word(TEST_SP, 1u16);

        call(&mut disp, true, 0x1F0, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);

        assert_eq!(bus.read_word(island_addr), 1);
        assert_eq!(bus.read_word(island_addr + 2), 0x4EF9);
        assert_eq!(bus.read_long(island_addr + 4), seg_addr + 4);

        assert_eq!(bus.read_word(island_addr + 8), 1);
        assert_eq!(bus.read_word(island_addr + 10), 0x4EF9);
        assert_eq!(bus.read_long(island_addr + 12), seg_addr + 8);
    }

    #[test]
    fn loadseg_patches_think_far_header_entry_range() {
        // Symantec/THINK far CODE reuses the first four bytes as
        // first-JT-entry index and entry count, with flag bits set. LoadSeg
        // must not treat the flagged count word as a raw near-model count.
        let (mut disp, mut cpu, mut bus) = setup();

        let seg_addr = 0x230000u32;
        bus.write_word(seg_addr, 0x8003); // reloc flag + first JT entry index
        bus.write_word(seg_addr + 2, 0x4002); // far flag + 2 entries
        disp.register_segments(HashMap::from([(1i16, seg_addr)]));

        let jt_base = 0x240000u32;
        bus.write_word(addr::CUR_JT_OFFSET, 0x20);
        cpu.write_reg(Register::A5, jt_base - 0x20);

        let entry_addr = jt_base + 3 * 8;
        bus.write_word(entry_addr, 0xA9F0);
        bus.write_word(entry_addr + 2, 0x0000);
        bus.write_word(entry_addr + 4, 0x0000); // routine offset
        bus.write_word(entry_addr + 6, 0x0001); // segment number

        let next_entry_addr = entry_addr + 8;
        bus.write_word(next_entry_addr, 0xA9F0);
        bus.write_word(next_entry_addr + 2, 0x0000);
        bus.write_word(next_entry_addr + 4, 0x0008); // routine offset
        bus.write_word(next_entry_addr + 6, 0x0001); // segment number

        bus.write_word(next_entry_addr + 8, 0xDEAD);

        cpu.write_reg(Register::PC, entry_addr + 2);

        call(&mut disp, true, 0x1F0, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP,
            "THINK-style LoadSeg entry does not pop a segment word"
        );
        assert_eq!(cpu.read_reg(Register::PC), entry_addr);

        assert_eq!(bus.read_word(entry_addr), 0x4EF9);
        assert_eq!(bus.read_long(entry_addr + 2), seg_addr + 4);
        assert_eq!(bus.read_word(entry_addr + 6), 0);

        assert_eq!(bus.read_word(next_entry_addr), 0x4EF9);
        assert_eq!(bus.read_long(next_entry_addr + 2), seg_addr + 12);
        assert_eq!(bus.read_word(next_entry_addr + 6), 0);
        assert_eq!(
            bus.read_word(next_entry_addr + 8),
            0xDEAD,
            "flagged THINK entry count should be masked before patching"
        );
    }

    #[test]
    fn loadseg_defers_to_native_getresource_hook_when_installed() {
        let (mut disp, mut cpu, mut bus) = setup();

        let seg_addr = 0x230000u32;
        bus.write_word(seg_addr, 0x0000);
        bus.write_word(seg_addr + 2, 0x0000);
        disp.register_segments(HashMap::from([(1i16, seg_addr)]));
        disp.native_trap_table.insert(0xA9A0, 0x300000);

        let island_addr = 0x240000u32;
        bus.write_word(island_addr, 0x0000);
        bus.write_word(island_addr + 2, 0x3F3C);
        bus.write_word(island_addr + 4, 0x0001);
        bus.write_word(island_addr + 6, 0xA9F0);

        cpu.write_reg(Register::PC, island_addr + 8);
        cpu.write_reg(Register::D3, 0xD3D3_D3D3);
        cpu.write_reg(Register::A4, 0xA4A4_A4A4);
        bus.write_word(TEST_SP, 1u16);

        call(&mut disp, true, 0x1F0, &mut cpu, &mut bus).unwrap();

        let state = disp
            .loadseg_getresource_state
            .as_ref()
            .expect("LoadSeg should be waiting for native GetResource");
        let call_sp = state.result_sp - 10;
        assert_eq!(state.seg_num, 1);
        assert_eq!(state.entry_addr, island_addr);
        assert_eq!(state.d_regs[3], 0xD3D3_D3D3);
        assert_eq!(state.a_regs[4], 0xA4A4_A4A4);
        assert_eq!(state.a_regs[7], TEST_SP + 2);

        assert_eq!(cpu.read_reg(Register::PC), 0x300000);
        assert_eq!(cpu.read_reg(Register::A7), call_sp);
        assert_eq!(
            bus.read_long(call_sp),
            disp.loadseg_getresource_trampoline_addr.unwrap()
        );
        assert_eq!(bus.read_word(call_sp + 4), 1);
        assert_eq!(bus.read_long(call_sp + 6), u32::from_be_bytes(*b"CODE"));
        assert_eq!(bus.read_long(state.result_sp), 0);

        assert_eq!(bus.read_word(island_addr), 0x0000);
        assert_eq!(bus.read_word(island_addr + 2), 0x3F3C);
    }

    #[test]
    fn loadseg_getresource_continuation_refreshes_decoded_code_resource() {
        let (mut disp, mut cpu, mut bus) = setup();

        let seg_addr = 0x230000u32;
        bus.write_long(seg_addr - 4, 8);
        bus.write_word(seg_addr, 0x0000);
        bus.write_word(seg_addr + 2, 0x4002); // encoded/undecoded header
        disp.register_segments(HashMap::from([(1i16, seg_addr)]));

        let decoded = [
            0x00, 0x00, 0x00, 0x01, // near header: one entry
            0x60, 0x00, 0x00, 0x02, // decoded code bytes
        ];
        let ptr = setup_resources(&mut disp, &mut bus, b"CODE", 1, &decoded);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"CODE", 1, ptr);

        let island_addr = 0x240000u32;
        bus.write_word(island_addr, 0x0000);
        bus.write_word(island_addr + 2, 0x3F3C);
        bus.write_word(island_addr + 4, 0x0001);
        bus.write_word(island_addr + 6, 0xA9F0);
        cpu.write_reg(Register::PC, 0x29CF00);
        cpu.write_reg(Register::A5, island_addr);
        cpu.write_reg(Register::A4, 0xA4A4_A4A4);

        disp.loadseg_getresource_state = Some(super::super::dispatch::LoadSegGetResourceState {
            seg_num: 1,
            entry_addr: island_addr,
            result_sp: TEST_SP - 4,
            d_regs: [0x1010_1010; 8],
            a_regs: [
                0xA0A0_A0A0,
                0xA1A1_A1A1,
                0xA2A2_A2A2,
                0xA3A3_A3A3,
                0xA4A4_A4A4,
                island_addr,
                0xA6A6_A6A6,
                TEST_SP,
            ],
        });
        bus.write_long(TEST_SP - 4, handle);

        disp.resume_loadseg_after_getresource(&mut bus, &mut cpu)
            .unwrap();

        assert_eq!(bus.read_word(seg_addr), 0x0000);
        assert_eq!(bus.read_word(seg_addr + 2), 0x0001);
        assert_eq!(bus.read_long(seg_addr + 4), 0x6000_0002);
        assert_eq!(cpu.read_reg(Register::A4), 0xA4A4_A4A4);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(cpu.read_reg(Register::PC), island_addr + 2);
        assert_eq!(bus.read_word(island_addr), 1);
        assert_eq!(bus.read_word(island_addr + 2), 0x4EF9);
        assert_eq!(bus.read_long(island_addr + 4), seg_addr + 4);
    }

    #[test]
    fn loadseg_getresource_continuation_preserves_header_for_implausibly_expanded_code_header() {
        let (mut disp, mut cpu, mut bus) = setup();

        let seg_addr = 0x230000u32;
        bus.write_long(seg_addr - 4, 8);
        bus.write_word(seg_addr, 0x0000);
        bus.write_word(seg_addr + 2, 0x0002);
        bus.write_long(seg_addr + 4, 0x4E56_0000);
        disp.register_segments(HashMap::from([(1i16, seg_addr)]));

        let corrupted = [
            0x06, 0x79, 0x69, 0x25, // would decode as THINK far with 10533 entries
            0x60, 0x00, 0x00, 0x02,
        ];
        let ptr = setup_resources(&mut disp, &mut bus, b"CODE", 1, &corrupted);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"CODE", 1, ptr);

        let jt_base = 0x240000u32;
        bus.write_word(addr::CUR_JT_OFFSET, 0);
        cpu.write_reg(Register::A5, jt_base);
        for i in 0..2 {
            let entry = jt_base + i * 8;
            bus.write_word(entry, 0);
            bus.write_word(entry + 2, 0x3F3C);
            bus.write_word(entry + 4, 1);
            bus.write_word(entry + 6, 0xA9F0);
        }

        disp.loadseg_getresource_state = Some(super::super::dispatch::LoadSegGetResourceState {
            seg_num: 1,
            entry_addr: jt_base,
            result_sp: TEST_SP - 4,
            d_regs: [0x1010_1010; 8],
            a_regs: [
                0xA0A0_A0A0,
                0xA1A1_A1A1,
                0xA2A2_A2A2,
                0xA3A3_A3A3,
                0xA4A4_A4A4,
                jt_base,
                0xA6A6_A6A6,
                TEST_SP,
            ],
        });
        bus.write_long(TEST_SP - 4, handle);

        disp.resume_loadseg_after_getresource(&mut bus, &mut cpu)
            .unwrap();

        assert_eq!(bus.read_word(seg_addr), 0x0000);
        assert_eq!(bus.read_word(seg_addr + 2), 0x0002);
        assert_eq!(bus.read_long(seg_addr + 4), 0x6000_0002);
        assert_eq!(bus.read_word(jt_base), 1);
        assert_eq!(bus.read_word(jt_base + 2), 0x4EF9);
        assert_eq!(bus.read_long(jt_base + 4), seg_addr + 4);
        assert_eq!(bus.read_word(jt_base + 8), 1);
        assert_eq!(bus.read_word(jt_base + 10), 0x4EF9);
        assert_eq!(bus.read_long(jt_base + 12), seg_addr + 4);
    }

    #[test]
    fn tool_trampoline_bypasses_later_native_trap_handler() {
        let (mut disp, mut cpu, mut bus) = setup();
        setup_resources(&mut disp, &mut bus, b"TEST", 7, &[0xAA, 0xBB]);

        let trampoline = disp.get_or_create_tool_trap_trampoline(&mut bus, 0xA9A0);
        disp.native_trap_table.insert(0xA9A0, 0x300000);

        let return_pc = 0x12345678;
        bus.write_long(TEST_SP, return_pc);
        bus.write_word(TEST_SP + 4, 7);
        bus.write_long(TEST_SP + 6, u32::from_be_bytes(*b"TEST"));
        bus.write_long(TEST_SP + 10, 0);
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::PC, trampoline + 2);

        call_trap_word(&mut disp, 0xADA0, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            cpu.read_reg(Register::PC),
            return_pc,
            "saved Systemless trampoline should call the original HLE trap"
        );
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_ne!(cpu.read_reg(Register::A0), 0);
        assert_eq!(bus.read_long(TEST_SP + 10), cpu.read_reg(Register::A0));
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn guest_auto_pop_old_trap_stub_bypasses_native_trap_handler() {
        let (mut disp, mut cpu, mut bus) = setup();
        setup_resources(&mut disp, &mut bus, b"TEST", 7, &[0xAA, 0xBB]);
        disp.native_trap_table.insert(0xA9A0, 0x300000);

        let return_pc = 0x12345678;
        bus.write_long(TEST_SP, return_pc);
        bus.write_word(TEST_SP + 4, 7);
        bus.write_long(TEST_SP + 6, u32::from_be_bytes(*b"TEST"));
        bus.write_long(TEST_SP + 10, 0);
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::PC, 0x0029D256);

        call_trap_word(&mut disp, 0xADA0, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::PC), return_pc);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_ne!(cpu.read_reg(Register::A0), 0);
        assert_eq!(bus.read_long(TEST_SP + 10), cpu.read_reg(Register::A0));
    }

    // ================================================================
    // 11. GetResAttrs (0x1A6)
    // ================================================================
    // Inside Macintosh Volume I (1985), p. I-121: GetResAttrs returns
    // the map attribute word for a live resource handle.
    #[test]
    fn get_res_attrs_returns_attribute_bits_for_live_resource_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"ATTR", 42, &[0x11, 0x22, 0x33]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"ATTR", 42, data_ptr);

        disp.resources
            .as_mut()
            .unwrap()
            .files
            .get_mut(&0)
            .unwrap()
            .attrs
            .insert((*b"ATTR", 42), 0x0026u8);

        let sp = TEST_SP;
        bus.write_long(sp, handle);
        bus.write_word(sp + 4, 0xBEEF);

        call(&mut disp, true, 0x1A6, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(TEST_SP + 4), 0x0026);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    // BasiliskII returns resChanged in the result slot and sets ResError to
    // resNotFound when the handle is not a resource handle.
    #[test]
    fn get_res_attrs_returns_res_changed_for_unknown_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x00DEAD00);
        bus.write_word(sp + 4, 0xBEEF);

        call(&mut disp, true, 0x1A6, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(TEST_SP + 4), 0x0002);
        assert_eq!(bus.read_word(0x0A60) as i16, -192);
    }

    // Inside Macintosh Volume I (1985), p. I-121 and p. I-123:
    // GetResAttrs observes the resChanged bit set by AddResource on a live
    // resource handle.
    #[test]
    fn get_res_attrs_after_add_resource_returns_res_changed() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(3);
        bus.write_bytes(data_ptr, &[0x11, 0x22, 0x33]);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);
        disp.resources = Some(LoadedResources {
            files: HashMap::from([(0, ResourceFileMap::default())]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });

        bus.write_long(TEST_SP, 0);
        bus.write_word(TEST_SP + 4, 43u16);
        bus.write_long(TEST_SP + 6, u32::from_be_bytes(*b"ATTR"));
        bus.write_long(TEST_SP + 10, handle);
        call(&mut disp, true, 0x1AB, &mut cpu, &mut bus).unwrap(); // AddResource
        assert_eq!(bus.read_word(0x0A60), 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, handle);
        bus.write_word(TEST_SP + 4, 0xBEEF);
        call(&mut disp, true, 0x1A6, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(TEST_SP + 4), 0x0002);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    // Inside Macintosh Volume IV (1986), p. IV-16 / More Macintosh Toolbox
    // 1993, p. 1-120: RsrcMapEntry returns the resource-reference offset from
    // the start of the resource map for live resource handles.
    #[test]
    #[ignore]
    fn rsrcmapentry_returns_reference_offset_for_live_resource_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let rsrc_bytes = make_single_resource_fork_bytes(*b"TEST", 43, &[0x11, 0x22]);
        disp.vfs_rsrc
            .insert("RsrcMapEntryTest".to_string(), rsrc_bytes);
        disp.open_resource_file_from_vfs_key(&mut bus, "RsrcMapEntryTest", false);

        let handle = disp
            .loaded_handles
            .iter()
            .find_map(|(handle, (_, res_type, res_id))| {
                (*res_type == *b"TEST" && *res_id == 43).then_some(*handle)
            })
            .expect("resource handle should be loaded");

        bus.write_long(TEST_SP, handle);
        bus.write_long(TEST_SP + 4, 0xBEEF_BEEF);

        call(&mut disp, true, 0x1C5, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(TEST_SP + 4), 40);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    // Nil handles are tolerated by leaving the result slot untouched while
    // setting ResErr to resNotFound.
    #[test]
    #[ignore]
    fn rsrcmapentry_nil_handle_sets_resnotfound_and_leaves_result_slot() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_long(TEST_SP, 0);
        bus.write_long(TEST_SP + 4, 0xBEEF_BEEF);

        call(&mut disp, true, 0x1C5, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(TEST_SP + 4), 0xBEEF_BEEF);
        assert_eq!(bus.read_word(0x0A60) as i16, -192);
    }

    // Inside Macintosh Volume IV (1986), p. IV-16 / More Macintosh Toolbox
    // 1993, p. 1-120: non-resource handles report resNotFound and leave the
    // caller's result slot unchanged.
    #[test]
    #[ignore]
    fn rsrcmapentry_unknown_handle_sets_resnotfound_and_leaves_result_slot() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_long(TEST_SP, 0x00DE_AD00);
        bus.write_long(TEST_SP + 4, 0xBEEF_BEEF);

        call(&mut disp, true, 0x1C5, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(TEST_SP + 4), 0xBEEF_BEEF);
        assert_eq!(bus.read_word(0x0A60) as i16, -192);
    }

    // ================================================================
    // 12. ChangedResource (0x1AA)
    // ================================================================
    // Inside Macintosh Volume I (1985), p. I-123: ChangedResource marks
    // a resource as changed so UpdateResFile/WriteResource will flush it.
    #[test]
    fn changedresource_sets_reschanged_attribute_for_unprotected_resource() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"CHNG", 9, &[0xAA, 0xBB]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"CHNG", 9, data_ptr);

        bus.write_long(TEST_SP, handle);
        bus.write_word(0x0A60, 0xBEEF);
        call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_ne!(
            disp.resource_attributes_for_handle(handle).unwrap_or(0)
                & u16::from(super::super::TrapDispatcher::RES_CHANGED_ATTR),
            0
        );
    }

    // Inside Macintosh Volume IV (1986), p. IV-16: ChangedResource does not
    // modify protected resources (resProtected bit set) but returns resAttrErr.
    #[test]
    fn changedresource_protected_resource_is_noop_with_resattrerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = setup_resources(&mut disp, &mut bus, b"PROT", 3, &[0x10, 0x20]);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"PROT", 3, data_ptr);

        disp.resources
            .as_mut()
            .unwrap()
            .files
            .get_mut(&0)
            .unwrap()
            .attrs
            .insert((*b"PROT", 3), 0x0008u8);

        bus.write_long(TEST_SP, handle);
        bus.write_word(0x0A60, 0xBEEF);
        call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap();

        let attrs_after = disp.resource_attributes_for_handle(handle).unwrap_or(0);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            bus.read_word(0x0A60) as i16,
            super::super::TrapDispatcher::RES_ATTR_ERR
        );
        assert_eq!(
            attrs_after & u16::from(super::super::TrapDispatcher::RES_CHANGED_ATTR),
            0
        );
        assert_ne!(attrs_after & 0x0008, 0);
    }

    // ================================================================
    // 13a. HighLevelFSDispatch (0x252) selector 1 — FSMakeFSSpec
    // ================================================================
    #[test]
    fn hlfs_dispatch_fsmakefsspec() {
        let (mut disp, mut cpu, mut bus) = setup();

        let spec_ptr = 0x300000u32;
        let name_ptr = 0x300100u32;
        write_pstring(&mut bus, name_ptr, b"MyFile.txt");

        let sp = TEST_SP;
        bus.write_long(sp, spec_ptr); // spec_ptr
        bus.write_long(sp + 4, name_ptr); // name_ptr
        bus.write_long(sp + 8, 2); // dirID
        bus.write_word(sp + 12, 1); // vRefNum

        cpu.write_reg(Register::D0, 1); // selector = FSMakeFSSpec

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 14);
        // Check FSSpec was filled: vRefNum at spec+0, dirID at spec+2, name at spec+6
        assert_eq!(
            bus.read_word(spec_ptr),
            super::super::dispatch::BOOT_VOLUME_REF_NUM as u16
        );
        assert_eq!(bus.read_long(spec_ptr + 2), 2);
        assert_eq!(bus.read_byte(spec_ptr + 6), 10); // length of "MyFile.txt"
                                                     // File doesn't exist in VFS — result should be fnfErr (-43)
                                                     // Files 1992, 2-166
        assert_eq!(bus.read_word(new_sp) as i16, -43);
    }

    // ================================================================
    // 13a2. FSMakeFSSpec returns noErr for existing file
    // ================================================================
    #[test]
    fn hlfs_dispatch_fsmakefsspec_exists() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.vfs.insert("MyFile.txt".to_string(), vec![]);

        let spec_ptr = 0x300000u32;
        let name_ptr = 0x300100u32;
        write_pstring(&mut bus, name_ptr, b"MyFile.txt");

        let sp = TEST_SP;
        bus.write_long(sp, spec_ptr);
        bus.write_long(sp + 4, name_ptr);
        bus.write_long(sp + 8, 2);
        bus.write_word(sp + 12, 1);
        cpu.write_reg(Register::D0, 1);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 14);
        assert_eq!(bus.read_word(new_sp), 0); // noErr
    }

    // ================================================================
    // 13a3. FSMakeFSSpec returns noErr for existing directory
    // ================================================================
    #[test]
    fn hlfs_dispatch_fsmakefsspec_dir() {
        let (mut disp, mut cpu, mut bus) = setup();
        // Add a file with directory component
        disp.vfs.insert("MyDir/SomeFile.txt".to_string(), vec![]);

        let spec_ptr = 0x300000u32;
        let name_ptr = 0x300100u32;
        write_pstring(&mut bus, name_ptr, b":MyDir");

        let sp = TEST_SP;
        bus.write_long(sp, spec_ptr);
        bus.write_long(sp + 4, name_ptr);
        bus.write_long(sp + 8, 2);
        bus.write_word(sp + 12, 1);
        cpu.write_reg(Register::D0, 1);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 14);
        assert_eq!(bus.read_word(new_sp), 0); // noErr — directory exists
    }

    #[test]
    fn fsmakefsspec_decomposes_partial_pathname_to_parent_dir_and_basename() {
        // Files 1992, 2-28 and 2-34: callers may pass full or partial
        // pathnames to FSMakeFSSpec, but the resulting FSSpec stores the
        // parent directory ID and final object name, not the whole pathname.
        let (mut disp, mut cpu, mut bus) = setup();
        let pref_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");
        disp.vfs.insert(
            "System Folder/Preferences/Existing Prefs".to_string(),
            vec![1, 2, 3],
        );
        disp.vfs.insert("App/App".to_string(), vec![]);
        disp.set_launched_app_path("App/App");

        let spec_ptr = 0x300000u32;
        let name_ptr = 0x300100u32;
        write_pstring(
            &mut bus,
            name_ptr,
            b":System Folder:Preferences:Existing Prefs",
        );

        let sp = TEST_SP;
        bus.write_long(sp, spec_ptr);
        bus.write_long(sp + 4, name_ptr);
        bus.write_long(sp + 8, 0);
        bus.write_word(sp + 12, 0);
        cpu.write_reg(Register::D0, 1);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 14);
        assert_eq!(bus.read_word(new_sp) as i16, 0);
        assert_eq!(
            bus.read_word(spec_ptr),
            super::super::dispatch::BOOT_VOLUME_REF_NUM as u16
        );
        assert_eq!(bus.read_long(spec_ptr + 2), pref_dir_id);
        assert_eq!(
            crate::trap::types::read_fsspec_name(&bus, spec_ptr),
            "Existing Prefs"
        );
    }

    #[test]
    fn fsmakefsspec_pathname_missing_target_does_not_match_same_basename_elsewhere() {
        // Files 1992, 2-34 to 2-35: if the parent directory exists but the
        // target object does not, FSMakeFSSpec still fills a valid FSSpec
        // and returns fnfErr. A pathname must not degrade to a basename-only
        // search in another directory.
        let (mut disp, mut cpu, mut bus) = setup();
        let pref_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");
        disp.vfs.insert("App/App".to_string(), vec![]);
        disp.vfs
            .insert("App/Shared Preferences".to_string(), vec![0x42]);
        disp.set_launched_app_path("App/App");

        let spec_ptr = 0x300000u32;
        let name_ptr = 0x300100u32;
        write_pstring(
            &mut bus,
            name_ptr,
            b":System Folder:Preferences:Shared Preferences",
        );

        let sp = TEST_SP;
        bus.write_long(sp, spec_ptr);
        bus.write_long(sp + 4, name_ptr);
        bus.write_long(sp + 8, 0);
        bus.write_word(sp + 12, 0);
        cpu.write_reg(Register::D0, 1);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 14);
        assert_eq!(bus.read_word(new_sp) as i16, -43);
        assert_eq!(
            bus.read_word(spec_ptr),
            super::super::dispatch::BOOT_VOLUME_REF_NUM as u16
        );
        assert_eq!(bus.read_long(spec_ptr + 2), pref_dir_id);
        assert_eq!(
            crate::trap::types::read_fsspec_name(&bus, spec_ptr),
            "Shared Preferences"
        );
    }

    #[test]
    fn fsmakefsspec_maps_unix_tmp_path_to_temporary_items_parent() {
        // Some ports pass POSIX temp pathnames through FSMakeFSSpec. Treat
        // /tmp as the standard temporary folder rather than a missing HFS
        // directory named "tmp".
        let (mut disp, mut cpu, mut bus) = setup();

        let spec_ptr = 0x300000u32;
        let name_ptr = 0x300100u32;
        write_pstring(&mut bus, name_ptr, b"/tmp/lcache00.tmp");

        let sp = TEST_SP;
        bus.write_long(sp, spec_ptr);
        bus.write_long(sp + 4, name_ptr);
        bus.write_long(sp + 8, 0);
        bus.write_word(sp + 12, 0);
        cpu.write_reg(Register::D0, 1);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 14);
        assert_eq!(bus.read_word(new_sp) as i16, -43);
        assert_eq!(
            bus.read_word(spec_ptr),
            super::super::dispatch::BOOT_VOLUME_REF_NUM as u16
        );
        let temp_dir_id = disp
            .directory_id_for_vfs_path("Temporary Items")
            .expect("temp folder should be present");
        assert_eq!(bus.read_long(spec_ptr + 2), temp_dir_id);
        assert_eq!(
            crate::trap::types::read_fsspec_name(&bus, spec_ptr),
            "lcache00.tmp"
        );
    }

    // ================================================================
    // 13b. HighLevelFSDispatch (0x252) selector 2 — FSpOpenDF
    // ================================================================
    #[test]
    fn hlfs_dispatch_fspopendf() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Add file to VFS so FSpOpenDF can find it
        disp.vfs.insert("OpenMe.txt".to_string(), vec![1, 2, 3]);

        let spec_ptr = 0x300000u32;
        write_fsspec(&mut bus, spec_ptr, 1, 2, b"OpenMe.txt");

        let ref_num_ptr = 0x300200u32;

        let sp = TEST_SP;
        bus.write_long(sp, ref_num_ptr); // refNumPtr
        bus.write_word(sp + 4, 1); // permission (fsRdPerm)
        bus.write_long(sp + 6, spec_ptr); // spec_ptr

        cpu.write_reg(Register::D0, 2); // selector = FSpOpenDF

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 10);
        // A refnum should have been written
        let refnum = bus.read_word(ref_num_ptr);
        assert!(
            refnum >= 100,
            "refnum should be >= 100 (initial next_refnum)"
        );
        // Result at new SP should be 0
        assert_eq!(bus.read_word(new_sp), 0);
    }

    #[test]
    fn hlfs_dispatch_fspopendf_packed_signedbyte_permission_marks_write_path() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs
            .insert("Prefs".to_string(), b"existing prefs".to_vec());

        let spec_ptr = 0x300000u32;
        write_fsspec(&mut bus, spec_ptr, 1, 2, b"Prefs");

        let ref_num_ptr = 0x300200u32;
        let sp = TEST_SP;
        bus.write_long(sp, ref_num_ptr);
        bus.write_word(sp + 4, 0x0233); // fsWrPerm in high byte, nonzero padding low byte.
        bus.write_long(sp + 6, spec_ptr);

        cpu.write_reg(Register::D0, 2);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        let refnum = bus.read_word(ref_num_ptr);
        assert_eq!(new_sp, TEST_SP + 10);
        assert_eq!(bus.read_word(new_sp), 0);
        assert!(
            disp.write_refnums.contains(&refnum),
            "packed fsWrPerm must be tracked as a write-open access path"
        );
    }

    #[test]
    fn hlfs_dispatch_fspopendf_double_exclusive_write_open_returns_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("Save.dat".to_string(), vec![1, 2, 3]);

        let spec_ptr = 0x300000u32;
        write_fsspec(&mut bus, spec_ptr, 1, 2, b"Save.dat");

        let first_ref_ptr = 0x300200u32;
        let sp = TEST_SP;
        bus.write_long(sp, first_ref_ptr);
        bus.write_word(sp + 4, 3); // fsRdWrPerm
        bus.write_long(sp + 6, spec_ptr);
        cpu.write_reg(Register::D0, 2);
        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_word(cpu.read_reg(Register::A7)), 0);

        let second_ref_ptr = 0x300220u32;
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(sp, second_ref_ptr);
        bus.write_word(sp + 4, 0x0333); // fsRdWrPerm in high byte, non-permission padding low byte.
        bus.write_long(sp + 6, spec_ptr);
        bus.write_word(second_ref_ptr, 0x7FFF);
        cpu.write_reg(Register::D0, 2);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 10);
        assert_eq!(bus.read_word(new_sp), 0);
        let second_refnum = bus.read_word(second_ref_ptr);
        assert_ne!(
            second_refnum, 0x7FFF,
            "BasiliskII/extfs-compatible second FSpOpenDF publishes a refnum"
        );
        assert!(
            disp.write_refnums.contains(&second_refnum),
            "packed fsRdWrPerm must be tracked on the second write-open path"
        );
    }

    #[test]
    fn hlfs_dispatch_fspopendf_locked_file_write_remains_openable() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("Locked.dat".to_string(), vec![1, 2, 3]);
        disp.locked_files.insert("Locked.dat".to_string());

        let spec_ptr = 0x300000u32;
        write_fsspec(&mut bus, spec_ptr, 1, 2, b"Locked.dat");

        let ref_num_ptr = 0x300200u32;
        let sp = TEST_SP;
        bus.write_long(sp, ref_num_ptr);
        bus.write_word(sp + 4, 0x0233); // fsWrPerm in high byte, non-permission padding low byte.
        bus.write_long(sp + 6, spec_ptr);
        bus.write_word(ref_num_ptr, 0x7FFF);
        cpu.write_reg(Register::D0, 2);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 10);
        assert_eq!(bus.read_word(new_sp), 0);
        let refnum = bus.read_word(ref_num_ptr);
        assert_ne!(
            refnum, 0x7FFF,
            "BasiliskII/extfs-compatible locked FSpOpenDF publishes a refnum"
        );
        assert!(
            disp.write_refnums.contains(&refnum),
            "packed fsWrPerm must still be tracked as write-open"
        );
    }

    #[test]
    fn hlfs_dispatch_fspopendf_read_only_volume_retries_as_browsing_access() {
        let (mut disp, mut cpu, mut bus) = setup();
        let volume_ref = disp.mount_vfs_volume_with_driver_info(
            "Legend CD",
            ".AppleCD",
            0x0080,
            1,
            1024,
            512,
            512,
            900,
            0,
            0,
            0,
            0,
            0,
            0,
        );
        let file_name = "Legend CD/Lode Runner Puzzles";
        disp.vfs.insert(file_name.to_string(), vec![1, 2, 3]);
        disp.ensure_vfs_file_metadata(file_name);
        let root_dir_id = disp
            .vfs_volume_for_ref_num(volume_ref)
            .expect("mounted volume")
            .root_dir_id;

        let spec_ptr = 0x300000u32;
        write_fsspec(
            &mut bus,
            spec_ptr,
            volume_ref as u16,
            root_dir_id,
            b"Lode Runner Puzzles",
        );
        let ref_num_ptr = 0x300200u32;
        let sp = TEST_SP;
        bus.write_long(sp, ref_num_ptr);
        bus.write_word(sp + 4, 3); // fsRdWrPerm
        bus.write_long(sp + 6, spec_ptr);
        cpu.write_reg(Register::D0, 2);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        let refnum = bus.read_word(ref_num_ptr);
        assert_eq!(new_sp, TEST_SP + 10);
        assert_eq!(bus.read_word(new_sp), 0);
        assert_eq!(disp.open_files.get(&refnum), Some(&file_name.to_string()));
        assert!(
            !disp.write_refnums.contains(&refnum),
            "browsing fallback must remain read-only"
        );
    }

    // ================================================================
    // 13b2. FSpOpenDF returns fnfErr for missing file
    // ================================================================
    #[test]
    fn hlfs_dispatch_fspopendf_fnferr() {
        let (mut disp, mut cpu, mut bus) = setup();

        let spec_ptr = 0x300000u32;
        write_fsspec(&mut bus, spec_ptr, 1, 2, b"Missing.txt");

        let ref_num_ptr = 0x300200u32;

        let sp = TEST_SP;
        bus.write_long(sp, ref_num_ptr);
        bus.write_word(sp + 4, 1);
        bus.write_long(sp + 6, spec_ptr);

        cpu.write_reg(Register::D0, 2);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 10);
        // Result should be fnfErr (-43)
        assert_eq!(bus.read_word(new_sp) as i16, -43);
    }

    // ================================================================
    // 13b3. HighLevelFSDispatch (0x252) selector 7 — FSpGetFInfo (file)
    // ================================================================
    #[test]
    fn hlfs_dispatch_fspgetfinfo_file() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("InfoFile.bin".to_string(), vec![0xAA]);
        disp.set_vfs_entry_finfo(
            "InfoFile.bin",
            u32::from_be_bytes(*b"APPL"),
            u32::from_be_bytes(*b"RSED"),
            0x1234,
        );

        let spec_ptr = 0x300000u32;
        let finfo_ptr = 0x300100u32;
        write_fsspec(&mut bus, spec_ptr, 1, 2, b"InfoFile.bin");

        let sp = TEST_SP;
        bus.write_long(sp, finfo_ptr); // fndrInfo pointer
        bus.write_long(sp + 4, spec_ptr); // spec pointer
        cpu.write_reg(Register::D0, 7);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 8);
        assert_eq!(bus.read_word(new_sp), 0); // noErr
        assert_eq!(bus.read_long(finfo_ptr), u32::from_be_bytes(*b"APPL"));
        assert_eq!(bus.read_long(finfo_ptr + 4), u32::from_be_bytes(*b"RSED"));
        assert_eq!(bus.read_word(finfo_ptr + 8), 0x1234);
    }

    // ================================================================
    // 13b4. HighLevelFSDispatch (0x252) selector 7 — FSpGetFInfo (directory)
    // ================================================================
    #[test]
    fn hlfs_dispatch_fspgetfinfo_directory() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Seed a child file so the parent directory exists in the VFS tree.
        disp.vfs
            .insert("EV Override 1.0.1/EV Override".to_string(), vec![0x00]);

        let spec_ptr = 0x300000u32;
        let finfo_ptr = 0x300100u32;
        write_fsspec(&mut bus, spec_ptr, 1, 2, b"EV Override 1.0.1");

        let sp = TEST_SP;
        bus.write_long(sp, finfo_ptr); // fndrInfo pointer
        bus.write_long(sp + 4, spec_ptr); // spec pointer
        cpu.write_reg(Register::D0, 7);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 8);
        assert_eq!(bus.read_word(new_sp), 0); // noErr
        assert_eq!(bus.read_long(finfo_ptr), u32::from_be_bytes(*b"fold"));
        assert_eq!(bus.read_long(finfo_ptr + 4), u32::from_be_bytes(*b"MACS"));
    }

    // ================================================================
    // 13c. HighLevelFSDispatch (0x252) selector 4 — FSpCreate
    // ================================================================
    #[test]
    fn hlfs_dispatch_fspcreate() {
        let (mut disp, mut cpu, mut bus) = setup();

        let spec_ptr = 0x300000u32;
        write_fsspec(&mut bus, spec_ptr, 1, 2, b"NewFile.dat");

        let sp = TEST_SP;
        // FSpCreate stack: SP+0..SP+9 = other params, SP+10 = spec_ptr
        // Zero out the lower bytes
        for i in 0..10u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 10, spec_ptr);

        cpu.write_reg(Register::D0, 4); // selector = FSpCreate

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 14);
        assert!(
            disp.vfs.contains_key("NewFile.dat"),
            "file should be added to VFS"
        );
    }

    // ================================================================
    // 13c2. HighLevelFSDispatch (0x252) selector 5 — FSpDirCreate
    // ================================================================
    #[test]
    fn hlfs_dispatch_fspdircreate_creates_child_directory_and_returns_dirid() {
        let (mut disp, mut cpu, mut bus) = setup();

        let parent_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");
        let spec_ptr = 0x300000u32;
        let created_dir_id_ptr = 0x300100u32;
        write_fsspec(
            &mut bus,
            spec_ptr,
            super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
            parent_dir_id,
            b"Humongous Entertainment",
        );
        bus.write_long(created_dir_id_ptr, 0xDEAD_BEEF);

        let sp = TEST_SP;
        bus.write_long(sp, created_dir_id_ptr);
        bus.write_word(sp + 4, 0); // scriptTag
        bus.write_long(sp + 6, spec_ptr);
        bus.write_word(sp + 10, 0xBEEF); // result poison
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 5);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(bus.read_word(TEST_SP + 10) as i16, 0);
        let created_dir_id = bus.read_long(created_dir_id_ptr);
        assert_ne!(created_dir_id, parent_dir_id);
        assert_eq!(
            disp.directory_path_for_id(created_dir_id),
            Some("System Folder/Preferences/Humongous Entertainment")
        );
    }

    #[test]
    fn hlfs_dispatch_fspdircreate_duplicate_returns_dupfneerr() {
        let (mut disp, mut cpu, mut bus) = setup();

        let parent_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");
        disp.ensure_vfs_directory("System Folder/Preferences/Humongous Entertainment");
        let spec_ptr = 0x300000u32;
        let created_dir_id_ptr = 0x300100u32;
        write_fsspec(
            &mut bus,
            spec_ptr,
            super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
            parent_dir_id,
            b"Humongous Entertainment",
        );
        bus.write_long(created_dir_id_ptr, 0xDEAD_BEEF);

        let sp = TEST_SP;
        bus.write_long(sp, created_dir_id_ptr);
        bus.write_word(sp + 4, 0);
        bus.write_long(sp + 6, spec_ptr);
        bus.write_word(sp + 10, 0xBEEF);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 5);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(bus.read_word(TEST_SP + 10) as i16, -48);
        assert_eq!(
            bus.read_long(created_dir_id_ptr),
            0xDEAD_BEEF,
            "duplicate failure should not overwrite createdDirID"
        );
    }

    // ================================================================
    // 13d. HighLevelFSDispatch (0x252) selector 14/13 — FSpCreateResFile/FSpOpenResFile
    // ================================================================
    #[test]
    fn hlfs_dispatch_fspcreateresfile_and_openresfile() {
        let (mut disp, mut cpu, mut bus) = setup();

        let spec_ptr = 0x300000u32;
        write_fsspec(&mut bus, spec_ptr, 1, 2, b"Escape Velocity Prefs");

        // FSpCreateResFile stack: [scriptTag(2)] [fileType(4)] [creator(4)] [spec_ptr(4)]
        let sp = TEST_SP;
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, 0);
        bus.write_long(sp + 6, 0);
        bus.write_long(sp + 10, spec_ptr);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 14);
        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 14);
        assert!(disp.vfs.contains_key("Escape Velocity Prefs"));
        assert!(disp.vfs_rsrc.contains_key("Escape Velocity Prefs"));
        assert_eq!(bus.read_word(0x0A60), 0);

        // FSpOpenResFile stack: [permission(2)] [spec_ptr(4)] [result(2)]
        bus.write_word(sp, 1);
        bus.write_long(sp + 2, spec_ptr);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 13);
        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 6);
        let refnum = bus.read_word(new_sp);
        assert!(refnum >= 100);
        assert_eq!(cpu.read_reg(Register::D0), refnum as u32);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(bus.read_word(0x0A5A), refnum);
    }

    #[test]
    fn hlfs_dispatch_fspcreateresfile_honors_parent_dir_id() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pilots_dir_id = disp.ensure_vfs_directory("EV Override 1.0.1/Pilots");
        let spec_ptr = 0x300000u32;
        write_fsspec(
            &mut bus,
            spec_ptr,
            super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
            pilots_dir_id,
            b"Rick Hardslab",
        );

        let sp = TEST_SP;
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, u32::from_be_bytes(*b"PIL "));
        bus.write_long(sp + 6, u32::from_be_bytes(*b"EVO!"));
        bus.write_long(sp + 10, spec_ptr);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 14);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 14);
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
        assert!(!disp.vfs.contains_key("Rick Hardslab"));
        assert!(!disp.vfs_rsrc.contains_key("Rick Hardslab"));
        assert!(disp
            .vfs
            .contains_key("EV Override 1.0.1/Pilots/Rick Hardslab"));
        assert!(disp
            .vfs_rsrc
            .contains_key("EV Override 1.0.1/Pilots/Rick Hardslab"));
        let metadata = disp
            .vfs_file_metadata("EV Override 1.0.1/Pilots/Rick Hardslab")
            .expect("created pilot metadata");
        assert_eq!(metadata.parent_dir_id, pilots_dir_id);
        assert_eq!(metadata.file_type, u32::from_be_bytes(*b"PIL "));
        assert_eq!(metadata.creator, u32::from_be_bytes(*b"EVO!"));
    }

    #[test]
    fn hlfs_dispatch_fspopenresfile_dedup_keeps_current_resource_file() {
        // More Macintosh Toolbox 1993, p. 1-63: reopening an already-open
        // resource fork returns the existing refnum without making that file
        // current. This regression catches the dedup path mutating CurResFile.
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs_rsrc.insert("First".to_string(), vec![]);
        disp.vfs_rsrc.insert("Second".to_string(), vec![]);

        let first_spec = 0x300080u32;
        let second_spec = 0x3000A0u32;
        write_fsspec(&mut bus, first_spec, 1, 2, b"First");
        write_fsspec(&mut bus, second_spec, 1, 2, b"Second");

        let sp = TEST_SP;
        bus.write_word(sp, 1);
        bus.write_long(sp + 2, first_spec);
        bus.write_word(sp + 6, 0xBEEF);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 13);
        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();
        let first_ref = bus.read_word(TEST_SP + 6);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(bus.read_word(0x0A5A), first_ref);

        bus.write_word(sp, 1);
        bus.write_long(sp + 2, second_spec);
        bus.write_word(sp + 6, 0xBEEF);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 13);
        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();
        let second_ref = bus.read_word(TEST_SP + 6);
        assert_ne!(second_ref, first_ref);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(bus.read_word(0x0A5A), second_ref);

        bus.write_word(sp, 1);
        bus.write_long(sp + 2, first_spec);
        bus.write_word(sp + 6, 0xBEEF);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 13);
        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_word(TEST_SP + 6), first_ref);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(bus.read_word(0x0A5A), second_ref);
    }

    #[test]
    fn openresfile_dedup_keeps_current_resource_file() {
        // IM:Volume I p. I-115: OpenResFile reuses an already-open resource
        // fork without switching the current resource file.
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs_rsrc.insert("First".to_string(), vec![]);
        disp.vfs_rsrc.insert("Second".to_string(), vec![]);

        let first_name = 0x3000C0u32;
        let second_name = 0x3000E0u32;
        write_pstring(&mut bus, first_name, b"First");
        write_pstring(&mut bus, second_name, b"Second");

        let sp = TEST_SP;

        bus.write_long(sp, first_name);
        bus.write_word(sp + 4, 0xBEEF);
        cpu.write_reg(Register::A7, sp);
        call_trap_word(&mut disp, 0xA997, &mut cpu, &mut bus).unwrap();
        let first_ref = bus.read_word(TEST_SP + 4);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(bus.read_word(0x0A5A), first_ref);

        bus.write_long(sp, second_name);
        bus.write_word(sp + 4, 0xBEEF);
        cpu.write_reg(Register::A7, sp);
        call_trap_word(&mut disp, 0xA997, &mut cpu, &mut bus).unwrap();
        let second_ref = bus.read_word(TEST_SP + 4);
        assert_ne!(second_ref, first_ref);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(bus.read_word(0x0A5A), second_ref);

        bus.write_long(sp, first_name);
        bus.write_word(sp + 4, 0xBEEF);
        cpu.write_reg(Register::A7, sp);
        call_trap_word(&mut disp, 0xA997, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_word(TEST_SP + 4), first_ref);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(bus.read_word(0x0A5A), second_ref);
    }

    #[test]
    fn hlfs_dispatch_fspopenresfile_data_only_file_returns_resfnotfound() {
        // More Macintosh Toolbox 1993, p. 1-58: FSpOpenResFile opens an
        // existing resource fork. If the data fork exists but no resource fork
        // has been created, Systemless should not synthesize one on open; the
        // current selector contract reports failure via -1 plus resFNotFound.
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("DataOnly".to_string(), b"DATA".to_vec());

        let spec_ptr = 0x300040u32;
        write_fsspec(&mut bus, spec_ptr, 0, 0, b"DataOnly");

        let sp = TEST_SP;
        bus.write_word(sp, 1); // fsRdPerm
        bus.write_long(sp + 2, spec_ptr);
        bus.write_word(sp + 6, 0xBEEF);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 13);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(bus.read_word(TEST_SP + 6), (-1i16) as u16);
        assert_eq!(cpu.read_reg(Register::D0), (-1i32) as u32);
        assert_eq!(bus.read_word(0x0A60), (-193i16) as u16);
        assert!(
            !disp.vfs_rsrc.contains_key("DataOnly"),
            "missing resource fork should not be synthesized by FSpOpenResFile"
        );
    }

    #[test]
    fn hlfs_dispatch_fspopenresfile_missing_file_returns_fnferr() {
        // IM:Volume VI 13-20: FSpOpenResFile shares HOpenResFile result
        // codes, including fnfErr (-43) when the file itself is missing.
        let (mut disp, mut cpu, mut bus) = setup();

        let spec_ptr = 0x300060u32;
        write_fsspec(&mut bus, spec_ptr, 0, 0, b"TotallyMissing");

        let sp = TEST_SP;
        bus.write_word(sp, 1); // fsRdPerm
        bus.write_long(sp + 2, spec_ptr);
        bus.write_word(sp + 6, 0xBEEF);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 13);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(bus.read_word(TEST_SP + 6), (-1i16) as u16);
        assert_eq!(cpu.read_reg(Register::D0), (-1i32) as u32);
        assert_eq!(bus.read_word(0x0A60), (-43i16) as u16);
    }

    // ================================================================
    // 13e. HighLevelFSDispatch (0x252) selector 6 — FSpDelete
    // ================================================================
    #[test]
    fn hlfs_dispatch_fspdelete() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Pre-populate VFS with the file
        disp.vfs.insert("DelMe.txt".to_string(), vec![1, 2, 3]);
        disp.vfs_rsrc.insert("DelMe.txt".to_string(), vec![4, 5, 6]);

        let spec_ptr = 0x300000u32;
        write_fsspec(&mut bus, spec_ptr, 1, 2, b"DelMe.txt");

        let sp = TEST_SP;
        bus.write_long(sp, spec_ptr);

        cpu.write_reg(Register::D0, 6); // selector = FSpDelete

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        let new_sp = cpu.read_reg(Register::A7);
        assert_eq!(new_sp, TEST_SP + 4);
        assert!(
            !disp.vfs.contains_key("DelMe.txt"),
            "file should be removed from VFS"
        );
        assert!(
            !disp.vfs_rsrc.contains_key("DelMe.txt"),
            "resource fork should be removed from VFS"
        );
    }

    // ================================================================
    // 14a. Gestalt (0xAD) — sysv
    // ================================================================
    #[test]
    fn gestalt_vers() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"vers"));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A0), 1);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn gestalt_sysv() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"sysv"));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            cpu.read_reg(Register::A0),
            crate::machine_profile::REFERENCE_MACHINE_PROFILE.system_version_bcd as u32
        );
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn gestalt_sys_architecture_reports_68k() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"sysa"));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A0), 1);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // ================================================================
    // 14b. Gestalt (0xAD) — cput
    // Per IM:Operating System Utilities 1994 line 1439 / 2299:
    //   gestaltCPU68040 = $004 under the 'cput' selector.
    // ================================================================
    #[test]
    fn gestalt_cput() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"cput"));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A0), 4);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn gestalt_logical_memory_selectors_match_68040_profile() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A0, 0xBEEF);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"lram"));
        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            cpu.read_reg(Register::A0),
            crate::machine_profile::REFERENCE_MACHINE_PROFILE.ram_size_bytes
        );
        assert_eq!(cpu.read_reg(Register::D0), 0);

        cpu.write_reg(Register::A0, 0xBEEF);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"pgsz"));
        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), 4096);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn gestalt_display_manager_selectors_match_basilisk753() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"dplv"));
        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), 0x0002_0006);
        assert_eq!(cpu.read_reg(Register::D0), 0);

        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"dply"));
        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), 0x0000_0007);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn gestalt_quicktime_reports_final_numversion() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A0, 0xBEEF);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"qtim"));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A0), QUICKTIME_NUM_VERSION_6_0_FINAL);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!((cpu.read_reg(Register::A0) >> 8) & 0xFF, 0x80);
    }

    #[test]
    fn gestalt_drag_manager_absent_without_error() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A0, 0x000F);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"drag"));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A0), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn gestalt_speech_manager_absent_without_error() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A0, 0xBEEF);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"ttsc"));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A0), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn gestalt_textedit_selectors_are_known_classic_system7_values() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A0, 0xBEEF);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"te  "));
        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), 5);
        assert_eq!(cpu.read_reg(Register::D0), 0);

        cpu.write_reg(Register::A0, 0xBEEF);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"teat"));
        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn gestalt_thread_manager_reports_supported_critical_sections() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A0, 0xBEEF);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"thds"));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            cpu.read_reg(Register::A0),
            1,
            "bit 0 should report Thread Manager present"
        );
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn newgestalt_rejects_builtin_thread_manager_selector() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A0, 0x0040_0000);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"thds"));

        call_trap_word(&mut disp, 0xA3AD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0xFFFF_EA50);
    }

    #[test]
    fn gestalt_resource_manager_reports_partial_resource_support() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"rsrc"));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A0), 1);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn gestalt_ppc_toolbox_reports_present() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A0, 0xBEEF);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"ppc "));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A0), 1);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn gestalt_keyboard_type_reports_extended_adb_keyboard() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A0, 0xBEEF);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"kbd "));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A0), 4);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn gestalt_common_classic_environment_selectors_do_not_error() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A0, 0xBEEF);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"edtn"));
        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), 1);
        assert_eq!(cpu.read_reg(Register::D0), 0);

        cpu.write_reg(Register::A0, 0xBEEF);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"scr#"));
        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), 1);
        assert_eq!(cpu.read_reg(Register::D0), 0);

        cpu.write_reg(Register::A0, 0xBEEF);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"hdwr"));
        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            cpu.read_reg(Register::A0) & ((1 << 0) | (1 << 1) | (1 << 3) | (1 << 4) | (1 << 7)),
            0x9B
        );
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn gestalt_quickdraw_version_reports_system7_32bit_qd13() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"qd  "));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A0), 0x0230);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // ================================================================
    // 14c. Gestalt (0xAD) — unknown selector
    // ================================================================
    #[test]
    fn gestalt_unknown() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"zzzz"));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0xFFFFEA51u32);
    }

    #[test]
    fn gestalt_aux_absent_clears_response_register() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A0, 0x0753);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"a/ux"));

        call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A0), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0xFFFFEA51u32);
    }

    #[test]
    fn newgestalt_rejects_builtin_keyboard_selector() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A0, 0x0040_0000);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*b"kbd "));

        call_trap_word(&mut disp, 0xA3AD, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0xFFFF_EA50);
    }

    /// Dispatches _NewGestalt with five distinct fictional selectors via
    /// `call_trap_word(0xA3AD)` and witnesses that A7 is preserved across
    /// each call. Per IM:OSUtils 1994 p. 1-32 NewGestalt is an OS-bit
    /// FUNCTION with A0+D0 entry registers and D0 exit register; no Pascal
    /// stack frame is consumed.
    #[test]
    fn newgestalt_register_only_calling_convention_preserves_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let fictional_fn_ptr = 0x0040_0000u32; // outside built-in zone

        for &selector_bytes in &[b"kxN1", b"kxN2", b"kxN3", b"kxN4", b"kxN5"] {
            cpu.write_reg(Register::A0, fictional_fn_ptr);
            cpu.write_reg(Register::D0, u32::from_be_bytes(*selector_bytes));
            call_trap_word(&mut disp, 0xA3AD, &mut cpu, &mut bus).unwrap();
            assert_eq!(
                cpu.read_reg(Register::A7),
                sp_before,
                "A7 preserved across NewGestalt({:?})",
                std::str::from_utf8(selector_bytes).unwrap()
            );
        }
    }

    /// Dispatches _ReplaceGestalt with five distinct fictional unknown
    /// selectors via `call_trap_word(0xA5AD)` (all hit the
    /// gestaltUndefSelectorErr path per IM:OSUtils 1994 p. 1-35) and
    /// witnesses that A7 is preserved across each call. The bare trap
    /// word is register-only ABI; the MPW FOURWORDINLINE glue's A1
    /// push/pop is balanced and is not part of the trap itself.
    #[test]
    fn replacegestalt_register_only_calling_convention_preserves_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let fictional_fn_ptr = 0x0040_0000u32;

        for &selector_bytes in &[b"kxR1", b"kxR2", b"kxR3", b"kxR4", b"kxR5"] {
            cpu.write_reg(Register::A0, fictional_fn_ptr);
            cpu.write_reg(Register::D0, u32::from_be_bytes(*selector_bytes));
            call_trap_word(&mut disp, 0xA5AD, &mut cpu, &mut bus).unwrap();
            assert_eq!(
                cpu.read_reg(Register::A7),
                sp_before,
                "A7 preserved across ReplaceGestalt({:?})",
                std::str::from_utf8(selector_bytes).unwrap()
            );
        }
    }

    // ================================================================
    // Helper: set up a param block at pb_addr with a name pointer
    // ================================================================
    fn setup_param_block(
        bus: &mut crate::memory::MacMemoryBus,
        cpu: &mut impl CpuOps,
        pb_addr: u32,
        filename: &[u8],
    ) -> u32 {
        let name_addr = pb_addr + 0x100; // put name just past pb
        write_pstring(bus, name_addr, filename);
        bus.write_long(pb_addr + 18, name_addr); // ioNamePtr
        cpu.write_reg(Register::A0, pb_addr);
        name_addr
    }

    // ================================================================
    // 15. PBOpen (0x00)
    // ================================================================
    #[test]
    fn pb_open() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("TestFile".to_string(), vec![1, 2, 3, 4, 5]);

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"TestFile");

        call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0, "D0 should be noErr");
        let refnum = bus.read_word(pb + 24);
        assert!(refnum >= 100);
        assert!(disp.open_files.contains_key(&refnum));
        assert!(
            disp.write_refnums.contains(&refnum),
            "fsCurPerm should grant write access when it is available"
        );
    }

    #[test]
    fn pbopen_permission_is_visible_through_pbgetfcbinfo_write_flag() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs
            .insert("Writable Stack".to_string(), vec![1, 2, 3, 4]);

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Writable Stack");
        bus.write_byte(pb + 27, 3); // fsRdWrPerm

        call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();

        let refnum = bus.read_word(pb + 24);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert!(disp.write_refnums.contains(&refnum));

        bus.write_word(pb + 24, refnum);
        bus.write_word(pb + 28, 0);
        cpu.write_reg(Register::D0, 8); // PBGetFCBInfo
        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            bus.read_word(pb + 36) & 0x0100,
            0x0100,
            "ioFCBFlags bit 8 reports that the data fork is writable"
        );
    }

    #[test]
    fn pbopen_read_permission_keeps_pbgetfcbinfo_write_flag_clear() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs
            .insert("Read Only Stack".to_string(), vec![1, 2, 3, 4]);

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Read Only Stack");
        bus.write_byte(pb + 27, 1); // fsRdPerm

        call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();

        let refnum = bus.read_word(pb + 24);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert!(!disp.write_refnums.contains(&refnum));

        bus.write_word(pb + 24, refnum);
        bus.write_word(pb + 28, 0);
        cpu.write_reg(Register::D0, 8); // PBGetFCBInfo
        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            bus.read_word(pb + 36) & 0x0100,
            0,
            "ioFCBFlags bit 8 stays clear for a read-only access path"
        );
    }

    #[test]
    fn pbopen_synthetic_driver_refnum_supports_read_write_close() {
        let (mut disp, mut cpu, mut bus) = setup();
        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b".AIn");

        call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        let refnum = bus.read_word(pb + 24);
        assert!(disp.synthetic_drivers.contains_key(&refnum));

        let buffer = 0x310000u32;
        bus.write_word(pb + 24, refnum);
        bus.write_long(pb + 32, buffer);
        bus.write_long(pb + 36, 8);
        call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(pb + 40), 0);

        bus.write_long(pb + 36, 4);
        call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(pb + 40), 4);

        call(&mut disp, false, 0x01, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert!(!disp.synthetic_drivers.contains_key(&refnum));
    }

    // ================================================================
    // 15b. PBOpen — file not found
    // ================================================================
    #[test]
    fn pb_open_not_found() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"NoSuchFile");

        call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            cpu.read_reg(Register::D0),
            (-43i32) as u32,
            "D0 should be fnfErr"
        );
    }

    // ================================================================
    // 16. FSRead (0x02)
    // ================================================================
    #[test]
    fn fs_read() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Insert file and open it
        disp.vfs
            .insert("ReadMe".to_string(), vec![10, 20, 30, 40, 50]);
        disp.open_files.insert(100, "ReadMe".to_string());
        disp.file_positions.insert(100, 0);

        let pb = 0x300000u32;
        let read_buf = 0x310000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100); // ioRefNum
        bus.write_long(pb + 32, read_buf); // ioBuffer
        bus.write_long(pb + 36, 3); // ioReqCount = 3
        bus.write_word(pb + 44, 0); // ioPosMode = fsAtMark
        bus.write_long(pb + 46, 0); // ioPosOffset

        call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(pb + 40), 3, "ioActCount should be 3");
        assert_eq!(bus.read_byte(read_buf), 10);
        assert_eq!(bus.read_byte(read_buf + 1), 20);
        assert_eq!(bus.read_byte(read_buf + 2), 30);
    }

    #[test]
    fn pb_read_async_queues_completion_and_leaves_ioresult_in_progress() {
        let (mut disp, mut cpu, mut bus) = setup();
        let pb = 0x300000u32;
        let read_buf = 0x310000u32;
        let completion_addr = 0x320000u32;

        disp.vfs
            .insert("AsyncData".to_string(), vec![10, 20, 30, 40]);
        disp.open_files.insert(100, "AsyncData".to_string());
        disp.file_positions.insert(100, 0);
        cpu.write_reg(Register::A0, pb);
        bus.write_long(pb + 12, completion_addr);
        bus.write_word(pb + 24, 100);
        bus.write_long(pb + 32, read_buf);
        bus.write_long(pb + 36, 4);
        bus.write_word(pb + 44, 0);
        bus.write_long(pb + 46, 0);

        call_trap_word(&mut disp, 0xA402, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0, "queueing returns noErr");
        assert_eq!(bus.read_word(pb + 16), 1, "ioResult stays in progress");
        assert_eq!(bus.read_long(pb + 40), 4);
        assert_eq!(bus.read_bytes(read_buf, 4), vec![10, 20, 30, 40]);
        assert_eq!(
            disp.pending_file_completions.pop_front(),
            Some(super::super::dispatch::PendingFileCompletion {
                parameter_block: pb,
                completion_addr,
                result: 0,
            })
        );
    }

    #[test]
    fn pb_read_sync_clears_completion_and_returns_final_result_directly() {
        let (mut disp, mut cpu, mut bus) = setup();
        let pb = 0x300000u32;

        disp.vfs.insert("Empty".to_string(), Vec::new());
        disp.open_files.insert(100, "Empty".to_string());
        disp.file_positions.insert(100, 0);
        cpu.write_reg(Register::A0, pb);
        bus.write_long(pb + 12, 0x320000);
        bus.write_word(pb + 24, 100);
        bus.write_long(pb + 32, 0x310000);
        bus.write_long(pb + 36, 1);
        bus.write_word(pb + 44, 0);
        bus.write_long(pb + 46, 0);

        call_trap_word(&mut disp, 0xA002, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -39);
        assert_eq!(bus.read_word(pb + 16) as i16, -39);
        assert_eq!(bus.read_long(pb + 12), 0);
        assert!(disp.pending_file_completions.is_empty());
    }

    #[test]
    fn fs_read_before_start_returns_poserr_and_keeps_mark() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs
            .insert("ReadMe".to_string(), vec![10, 20, 30, 40, 50]);
        disp.open_files.insert(100, "ReadMe".to_string());
        disp.file_positions.insert(100, 2);

        let pb = 0x300000u32;
        let read_buf = 0x310000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100);
        bus.write_long(pb + 32, read_buf);
        bus.write_long(pb + 36, 1);
        bus.write_long(pb + 40, 0xDEADBEEF);
        bus.write_word(pb + 44, 1); // fsFromStart
        bus.write_long(pb + 46, (-1i32) as u32);
        bus.write_byte(read_buf, 0xCC);

        call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -40);
        assert_eq!(bus.read_word(pb + 16) as i16, -40);
        assert_eq!(bus.read_long(pb + 40), 0);
        assert_eq!(bus.read_long(pb + 46), 2);
        assert_eq!(*disp.file_positions.get(&100).unwrap(), 2);
        assert_eq!(bus.read_byte(read_buf), 0xCC);
    }

    #[test]
    fn pb_read_newline_mode_returns_delimiter_and_preserves_following_data() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs
            .insert("Lines".to_string(), b"first\rsecond\r".to_vec());
        disp.open_files.insert(100, "Lines".to_string());
        disp.file_positions.insert(100, 0);

        let pb = 0x300000u32;
        let read_buf = 0x310000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100);
        bus.write_long(pb + 32, read_buf);
        bus.write_long(pb + 36, 32);
        bus.write_word(pb + 44, 0x0D80); // Return-delimited newline mode, fsAtMark.
        bus.write_long(pb + 46, 0);

        call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(bus.read_long(pb + 40), 6);
        assert_eq!(bus.read_long(pb + 46), 6);
        assert_eq!(bus.read_bytes(read_buf, 6), b"first\r".to_vec());
        assert_eq!(*disp.file_positions.get(&100).unwrap(), 6);

        bus.write_long(pb + 32, read_buf + 16);
        bus.write_long(pb + 36, 32);

        call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(bus.read_long(pb + 40), 7);
        assert_eq!(bus.read_long(pb + 46), 13);
        assert_eq!(bus.read_bytes(read_buf + 16, 7), b"second\r".to_vec());
        assert_eq!(*disp.file_positions.get(&100).unwrap(), 13);
    }

    #[test]
    fn pb_read_newline_mode_returns_eof_without_delimiter() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("Tail".to_string(), b"tail".to_vec());
        disp.open_files.insert(100, "Tail".to_string());
        disp.file_positions.insert(100, 0);

        let pb = 0x300000u32;
        let read_buf = 0x310000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100);
        bus.write_long(pb + 32, read_buf);
        bus.write_long(pb + 36, 16);
        bus.write_word(pb + 44, 0x0D80); // Return-delimited newline mode, fsAtMark.
        bus.write_long(pb + 46, 0);

        call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -39);
        assert_eq!(bus.read_word(pb + 16) as i16, -39);
        assert_eq!(bus.read_long(pb + 40), 4);
        assert_eq!(bus.read_long(pb + 46), 4);
        assert_eq!(bus.read_bytes(read_buf, 4), b"tail".to_vec());
        assert_eq!(*disp.file_positions.get(&100).unwrap(), 4);
    }

    #[test]
    fn pb_read_uses_low_position_bits_when_flags_are_set() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs
            .insert("ReadMe".to_string(), vec![10, 20, 30, 40, 50]);
        disp.open_files.insert(100, "ReadMe".to_string());
        disp.file_positions.insert(100, 0);

        let pb = 0x300000u32;
        let read_buf = 0x310000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100);
        bus.write_long(pb + 32, read_buf);
        bus.write_long(pb + 36, 2);
        bus.write_word(pb + 44, 0x0051); // cache + rdVerify flags, fsFromStart.
        bus.write_long(pb + 46, 2);

        call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(bus.read_long(pb + 40), 2);
        assert_eq!(bus.read_long(pb + 46), 4);
        assert_eq!(bus.read_bytes(read_buf, 2), vec![30, 40]);
        assert_eq!(*disp.file_positions.get(&100).unwrap(), 4);
    }

    // ================================================================
    // 17. FSWrite (0x03)
    // ================================================================
    #[test]
    fn fs_write() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("WriteMe".to_string(), Vec::new());
        disp.open_files.insert(100, "WriteMe".to_string());
        disp.file_positions.insert(100, 0);

        let pb = 0x300000u32;
        let write_buf = 0x310000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100);
        bus.write_long(pb + 32, write_buf);
        bus.write_long(pb + 36, 4); // ioReqCount = 4

        // Write data to the buffer
        bus.write_byte(write_buf, 0xAA);
        bus.write_byte(write_buf + 1, 0xBB);
        bus.write_byte(write_buf + 2, 0xCC);
        bus.write_byte(write_buf + 3, 0xDD);

        call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(pb + 40), 4);
        let file_data = disp.vfs.get("WriteMe").unwrap();
        assert_eq!(file_data, &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn fs_write_from_start_overwrites() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs
            .insert("WriteMe".to_string(), vec![0x11, 0x22, 0x33, 0x44]);
        disp.open_files.insert(100, "WriteMe".to_string());
        disp.file_positions.insert(100, 4);

        let pb = 0x300000u32;
        let write_buf = 0x310000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100);
        bus.write_long(pb + 32, write_buf);
        bus.write_long(pb + 36, 2); // ioReqCount = 2
        bus.write_word(pb + 44, 1); // ioPosMode = fsFromStart
        bus.write_long(pb + 46, 1); // ioPosOffset = 1

        bus.write_byte(write_buf, 0xAA);
        bus.write_byte(write_buf + 1, 0xBB);

        call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(pb + 40), 2);
        assert_eq!(
            bus.read_long(pb + 46),
            3,
            "ioPosOffset should advance to new mark"
        );
        let file_data = disp.vfs.get("WriteMe").unwrap();
        assert_eq!(file_data, &[0x11, 0xAA, 0xBB, 0x44]);
        assert_eq!(*disp.file_positions.get(&100).unwrap(), 3);
    }

    #[test]
    fn fs_write_before_start_returns_poserr_and_keeps_file() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs
            .insert("WriteMe".to_string(), vec![0x11, 0x22, 0x33, 0x44]);
        disp.open_files.insert(100, "WriteMe".to_string());
        disp.file_positions.insert(100, 1);

        let pb = 0x300000u32;
        let write_buf = 0x310000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100);
        bus.write_long(pb + 32, write_buf);
        bus.write_long(pb + 36, 1);
        bus.write_word(pb + 44, 3); // fsFromMark
        bus.write_long(pb + 46, (-2i32) as u32);
        bus.write_byte(write_buf, 0xAA);

        call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -40);
        assert_eq!(bus.read_word(pb + 16) as i16, -40);
        assert_eq!(bus.read_long(pb + 40), 0);
        assert_eq!(bus.read_long(pb + 46), 1);
        assert_eq!(*disp.file_positions.get(&100).unwrap(), 1);
        assert_eq!(disp.vfs.get("WriteMe").unwrap(), &[0x11, 0x22, 0x33, 0x44]);
    }

    // PBOpenRF stores the rsrc fork bytes in self.vfs under the
    // "__rsrc__<name>" key so FSRead/FSWrite share the data-fork code
    // path. Writes through that key must mirror back into self.vfs_rsrc —
    // otherwise a later OpenRFPerm/PBOpenRF reads the stale snapshot.
    #[test]
    fn fs_write_to_rsrc_fork_mirrors_to_vfs_rsrc() {
        let (mut disp, mut cpu, mut bus) = setup();

        let rsrc_key = "__rsrc__InstallerTemp".to_string();
        disp.vfs.insert(rsrc_key.clone(), Vec::new());
        disp.vfs_rsrc
            .insert("InstallerTemp".to_string(), Vec::new());
        disp.open_files.insert(101, rsrc_key);
        disp.file_positions.insert(101, 0);

        let pb = 0x300000u32;
        let write_buf = 0x310000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 101);
        bus.write_long(pb + 32, write_buf);
        bus.write_long(pb + 36, 4);

        bus.write_bytes(write_buf, &[0xDE, 0xAD, 0xBE, 0xEF]);

        call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(
            disp.vfs_rsrc.get("InstallerTemp").unwrap(),
            &vec![0xDE, 0xAD, 0xBE, 0xEF],
            "FSWrite to a __rsrc__ open key must mirror back to \
             vfs_rsrc so a later OpenRFPerm sees the new bytes"
        );
    }

    #[test]
    fn fs_write_invalid_refnum() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        let write_buf = 0x310000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 999); // invalid refnum
        bus.write_long(pb + 32, write_buf);
        bus.write_long(pb + 36, 2);

        call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), (-51i32) as u32);
        assert_eq!(bus.read_word(pb + 16), (-51i16) as u16);
        assert_eq!(bus.read_long(pb + 40), 0);
    }

    // ================================================================
    // 18. FSClose (0x01)
    // ================================================================
    #[test]
    fn fs_close() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.open_files.insert(100, "SomeFile".to_string());
        disp.file_positions.insert(100, 42);

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100); // ioRefNum

        call(&mut disp, false, 0x01, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert!(!disp.open_files.contains_key(&100));
        assert!(!disp.file_positions.contains_key(&100));
    }

    #[test]
    fn fs_close_resource_refnum_releases_resource_map_and_handles() {
        let (mut disp, mut cpu, mut bus) = setup();
        let refnum = 100;
        let data_ptr = bus.alloc(8);
        bus.write_bytes(data_ptr, &[0xCD; 8]);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        disp.resources = Some(super::super::dispatch::LoadedResources {
            files: HashMap::from([
                (0, super::super::dispatch::ResourceFileMap::default()),
                (
                    refnum,
                    super::super::dispatch::ResourceFileMap {
                        loaded: HashMap::from([((*b"PICT", 23002), data_ptr)]),
                        named: HashMap::new(),
                        names_by_id: HashMap::new(),
                        attrs: HashMap::new(),
                        map_attrs: 0,
                    },
                ),
            ]),
            names: HashMap::from([(refnum, "BladeData".to_string())]),
            search_order: vec![0, refnum],
            current_file: refnum,
        });
        disp.loaded_handles
            .insert(handle, (data_ptr, *b"PICT", 23002));
        disp.resource_handle_files.insert(handle, refnum);
        disp.ptr_to_handle.insert(data_ptr, handle);

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, refnum);

        call(&mut disp, false, 0x01, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(disp.current_resource_refnum(), 0);
        assert!(!disp.resources.as_ref().unwrap().files.contains_key(&refnum));
        assert_eq!(bus.get_alloc_size(data_ptr), None);
        assert_eq!(bus.get_alloc_size(handle), None);
        assert!(!disp.loaded_handles.contains_key(&handle));
        assert!(!disp.resource_handle_files.contains_key(&handle));
        assert_eq!(disp.ptr_to_handle.get(&data_ptr), None);
    }

    // ================================================================
    // 19. PBGetVol (0x14)
    // ================================================================
    #[test]
    fn pb_get_vol() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        let name_buf = 0x300100u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_long(pb + 18, name_buf); // ioNamePtr

        call(&mut disp, false, 0x14, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(
            bus.read_word(pb + 22),
            super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
            "ioVRefNum"
        );
        // Check Pascal string "MacintoshHD"
        let name_bytes = bus.read_pstring(name_buf);
        assert_eq!(name_bytes, b"MacintoshHD");
    }

    // ================================================================
    // 20. PBGetVInfo (0x07)
    // ================================================================
    #[test]
    fn pb_get_vinfo() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        let name_buf = 0x300100u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_long(pb + 18, name_buf);

        call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(
            bus.read_word(pb + 22),
            super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
            "ioVRefNum"
        );
        assert_eq!(bus.read_word(pb + 40), 100, "ioVNmFls");
        assert_eq!(
            bus.read_word(pb + 46),
            super::BOOT_VOLUME_ALLOCATION_BLOCKS,
            "ioVNmAlBlks"
        );
        assert_eq!(
            bus.read_long(pb + 48),
            super::BOOT_VOLUME_ALLOCATION_BLOCK_SIZE,
            "ioVAlBlkSiz"
        );
        assert_eq!(
            bus.read_word(pb + 62),
            super::BOOT_VOLUME_FREE_BLOCKS,
            "ioVFrBlk"
        );
        let free_bytes = u32::from(bus.read_word(pb + 62)) * bus.read_long(pb + 48);
        assert!(
            free_bytes >= 8 * 1024 * 1024,
            "PBGetVInfo should report enough free space for launch-time scratch checks"
        );
        // Volume name
        let len = bus.read_byte(name_buf) as usize;
        assert_eq!(len, 11);
    }

    #[test]
    fn pb_hget_vinfo_positive_index_one_returns_boot_volume() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        let name_buf = 0x300100u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_long(pb + 18, name_buf);
        bus.write_word(pb + 22, 0x1234);
        bus.write_word(pb + 28, 1); // ioVolIndex

        call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0, "ioResult");
        assert_eq!(
            bus.read_word(pb + 22),
            super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
            "ioVRefNum"
        );
        assert_eq!(bus.read_pstring(name_buf), b"MacintoshHD");
    }

    #[test]
    fn pb_hget_vinfo_index_after_last_volume_returns_nsv_err_without_outputs() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        let name_buf = 0x300100u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_long(pb + 18, name_buf);
        bus.write_word(pb + 22, 0x1234);
        bus.write_word(pb + 28, 2); // ioVolIndex
        bus.write_long(pb + 30, 0xCAFE_BABE);
        bus.write_pstring(name_buf, b"unchanged");

        call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), (-35i32) as u32);
        assert_eq!(bus.read_word(pb + 16), (-35i16) as u16, "ioResult");
        assert_eq!(bus.read_word(pb + 22), 0x1234, "ioVRefNum");
        assert_eq!(bus.read_long(pb + 30), 0xCAFE_BABE, "ioVCrDate");
        assert_eq!(bus.read_pstring(name_buf), b"unchanged");
    }

    #[test]
    fn pb_hget_vinfo_zero_index_selects_named_read_only_volume() {
        let (mut disp, mut cpu, mut bus) = setup();
        let volume_ref = disp.mount_vfs_volume_with_driver_info(
            "Legend CD",
            ".AppleCD",
            0x0080,
            4,
            1024,
            512,
            512,
            900,
            0,
            0,
            0,
            0,
            0,
            0,
        );

        let pb = 0x300000u32;
        let name_buf = 0x300100u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_long(pb + 18, name_buf);
        bus.write_pstring(name_buf, b"Legend CD");
        bus.write_word(pb + 22, 0x1234);
        bus.write_word(pb + 28, 0); // ioVolIndex: lookup by name

        call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 22), volume_ref as u16);
        assert_eq!(bus.read_pstring(name_buf), b"Legend CD");
        assert_eq!(bus.read_word(pb + 38), 0x0080, "ioVAtrb");
    }

    // ================================================================
    // 21. PBGetEOF (0x11)
    // ================================================================
    #[test]
    fn pb_get_eof() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("EOFFile".to_string(), vec![0; 256]);
        disp.open_files.insert(100, "EOFFile".to_string());

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100);

        call(&mut disp, false, 0x11, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(pb + 28), 256, "ioMisc should be file size");
    }

    // ================================================================
    // 22. PBSetFPos (0x44)
    // ================================================================
    #[test]
    fn pb_set_fpos() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Open a file first
        disp.vfs.insert("TestFile".to_string(), vec![0u8; 256]);
        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"TestFile");
        call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();
        let refnum = bus.read_word(pb + 24);

        // Set position to offset 42 from start (posMode=1)
        bus.write_word(pb + 24, refnum);
        bus.write_word(pb + 44, 1); // fsFromStart
        bus.write_long(pb + 46, 42);
        cpu.write_reg(Register::A0, pb);
        call(&mut disp, false, 0x44, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(*disp.file_positions.get(&refnum).unwrap(), 42);
    }

    #[test]
    fn pb_set_fpos_before_start_returns_poserr_and_keeps_mark() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("TestFile".to_string(), vec![0u8; 256]);
        disp.open_files.insert(100, "TestFile".to_string());
        disp.file_positions.insert(100, 4);

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100);
        bus.write_word(pb + 44, 3); // fsFromMark
        bus.write_long(pb + 46, (-5i32) as u32);

        call(&mut disp, false, 0x44, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -40);
        assert_eq!(bus.read_word(pb + 16) as i16, -40);
        assert_eq!(bus.read_long(pb + 46), 4);
        assert_eq!(*disp.file_positions.get(&100).unwrap(), 4);
    }

    #[test]
    fn pb_set_fpos_past_eof_clamps_to_eof_and_returns_eoferr() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("TestFile".to_string(), vec![0u8; 256]);
        disp.open_files.insert(100, "TestFile".to_string());
        disp.file_positions.insert(100, 4);

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100);
        bus.write_word(pb + 44, 1); // fsFromStart
        bus.write_long(pb + 46, 300);

        call(&mut disp, false, 0x44, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -39);
        assert_eq!(bus.read_word(pb + 16) as i16, -39);
        assert_eq!(bus.read_long(pb + 46), 256);
        assert_eq!(*disp.file_positions.get(&100).unwrap(), 256);
    }

    // ================================================================
    // 23. PBFlushFile (0x45) — validates refnum
    // Inside Macintosh Volume II, II-114. rfNumErr (-51) for unknown refnum.
    // ================================================================
    #[test]
    fn pb_flush_file_unknown_refnum_returns_rfnumerr() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x260900u32;
        for i in 0u32..32 {
            bus.write_byte(pb + i, 0);
        }
        bus.write_word(pb + 24, 9999); // ioRefNum — not open
        cpu.write_reg(Register::A0, pb);
        call(&mut disp, false, 0x45, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, -51);
        assert_eq!(bus.read_word(pb + 16) as i16, -51);
    }

    // ================================================================
    // 24. PBGetFPos (0x18)
    // ================================================================
    #[test]
    fn pb_get_fpos() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.file_positions.insert(100, 42);

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100);

        call(&mut disp, false, 0x18, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(pb + 46), 42, "ioPosOffset should be 42");
        assert_eq!(bus.read_word(pb + 44), 0, "ioPosMode should be 0");
    }

    // PBGetFPos must return rfNumErr for unknown refnums per IM:II-117.
    #[test]
    fn pb_get_fpos_unknown_refnum_returns_rfnumerr() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x260A00u32;
        for i in 0u32..64 {
            bus.write_byte(pb + i, 0);
        }
        bus.write_word(pb + 24, 9999); // ioRefNum — not open
        cpu.write_reg(Register::A0, pb);
        call(&mut disp, false, 0x18, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, -51);
        assert_eq!(bus.read_word(pb + 16) as i16, -51);
    }

    // ================================================================
    // 25. PBFlushVol (0x13)
    // ================================================================
    #[test]
    fn pb_flush_vol() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);

        call(&mut disp, false, 0x13, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
    }

    #[test]
    fn pb_flush_vol_unknown_refnum_returns_nsverr() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 22, (-999i16) as u16);

        call(&mut disp, false, 0x13, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -35);
        assert_eq!(bus.read_word(pb + 16) as i16, -35);
    }

    // ================================================================
    // 26. PBCreate (0x08)
    // ================================================================
    #[test]
    fn pb_create() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Pilot 1");

        call(&mut disp, false, 0x08, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert!(disp.vfs.contains_key("Pilot 1"));
        assert_eq!(disp.vfs.get("Pilot 1").unwrap().len(), 0);
    }

    #[test]
    fn pb_create_uses_working_directory_refnum_and_ignores_hfs_dirid_bytes() {
        // Files 1992, 2-89: legacy PBCreate takes a ParamBlockRec, whose
        // ioVRefNum may be a WDRefNum. The ioDirID field exists only in the
        // HParamBlockRec used by PBHCreate ($A208), so bytes at offset 48
        // must not redirect a legacy $A008 create.
        let (mut disp, mut cpu, mut bus) = setup();
        let pilots_dir_id = disp.ensure_vfs_directory("Pilots");
        let unrelated_dir_id = disp.ensure_vfs_directory("Unrelated");
        let wd_ref = disp
            .open_working_directory(
                super::super::dispatch::BOOT_VOLUME_REF_NUM,
                pilots_dir_id,
                0,
            )
            .expect("working directory");

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Untitled");
        bus.write_word(pb + 22, wd_ref as u16);
        bus.write_long(pb + 48, unrelated_dir_id);

        call_trap_word(&mut disp, 0xA008, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert!(disp.vfs.contains_key("Pilots/Untitled"));
        assert!(!disp.vfs.contains_key("Unrelated/Untitled"));

        bus.write_word(pb + 24, 0);
        bus.write_long(pb + 48, unrelated_dir_id);
        cpu.write_reg(Register::D0, 26);
        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(
            disp.open_files.get(&bus.read_word(pb + 24)),
            Some(&"Pilots/Untitled".to_string())
        );
    }

    // Real Mac files always have both forks; PBCreate must seed an empty
    // rsrc fork so the next PBOpenRF returns noErr instead of fnfErr.
    // Files 1992, 1-58.
    #[test]
    fn pb_create_seeds_empty_rsrc_fork() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Installer Temp");

        call_trap_word(&mut disp, 0xA008, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert!(disp.vfs.contains_key("Installer Temp"));
        assert!(
            disp.vfs_rsrc.contains_key("Installer Temp"),
            "PBCreate must seed an empty resource fork so PBOpenRF \
             on the new file returns noErr"
        );
        assert_eq!(disp.vfs_rsrc.get("Installer Temp").unwrap().len(), 0);
    }

    #[test]
    fn pbh_create_setfinfo_then_open_honors_parent_dir_id() {
        let (mut disp, mut cpu, mut bus) = setup();
        let temp_dir_id = disp.ensure_vfs_directory("Temporary Items");

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"lcache00.tmp");
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
        bus.write_long(pb + 48, temp_dir_id);

        call_trap_word(&mut disp, 0xA208, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert!(disp.vfs.contains_key("Temporary Items/lcache00.tmp"));
        assert!(!disp.vfs.contains_key("lcache00.tmp"));

        bus.write_long(pb + 32, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(pb + 36, u32::from_be_bytes(*b"ttxt"));
        bus.write_word(pb + 40, 0x0400);
        call(&mut disp, false, 0x0D, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        let metadata = disp
            .vfs_file_metadata("Temporary Items/lcache00.tmp")
            .expect("temp file metadata");
        assert_eq!(metadata.file_type, u32::from_be_bytes(*b"TEXT"));
        assert_eq!(metadata.creator, u32::from_be_bytes(*b"ttxt"));
        assert_eq!(metadata.finder_flags, 0x0400);

        cpu.write_reg(Register::D0, 26);
        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        let refnum = bus.read_word(pb + 24);
        assert!(refnum >= 100);
        assert_eq!(
            disp.open_files.get(&refnum),
            Some(&"Temporary Items/lcache00.tmp".to_string())
        );
    }

    #[test]
    fn pbh_create_preserves_slash_as_a_literal_filename_character() {
        // HFS reserves colon as the pathname separator and permits every
        // other character, including slash, in file and directory names.
        // Files 1992, 2-27 to 2-29.
        let (mut disp, mut cpu, mut bus) = setup();
        let parent_dir_id = disp.ensure_vfs_directory("Game Folder");

        let pb = 0x300000u32;
        let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"Object IDs/Level Info");
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
        bus.write_long(pb + 48, parent_dir_id);

        call_trap_word(&mut disp, 0xA208, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
        assert!(
            disp.directory_id_for_vfs_path("Game Folder/Object IDs")
                .is_none(),
            "a literal slash must not synthesize a nested directory"
        );

        call_trap_word(&mut disp, 0xA20C, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
        assert_eq!(bus.read_pstring(name_ptr), b"Object IDs/Level Info");

        bus.write_long(pb + 48, parent_dir_id);
        bus.write_word(pb + 28, 0);
        cpu.write_reg(Register::D0, 9);
        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
        assert_eq!(bus.read_pstring(name_ptr), b"Object IDs/Level Info");
    }

    #[test]
    fn pb_create_truncate_on_exists_preserves_rsrc_fork() {
        // Per IM Files 1992, 2-89, PBCreate returns dupFNErr when a
        // file with the matching name already exists. Some shareware
        // titles (e.g. Meteor Storm's "MS UserKey" marker) ship that
        // file inside their install folder yet still call HCreate at
        // launch without an intervening HDelete and treat any error
        // as fatal. Systemless models the .sit-extracted folder
        // directly, so the marker is always pre-existing on first
        // launch — we truncate the data fork and report noErr to keep
        // these titles bootable, but PRESERVE the resource fork
        // because some titles bake registration templates into it
        // and read them back via FSpOpenResFile + Get1Resource.
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("Pilot 1".to_string(), vec![1, 2, 3]);
        disp.vfs_rsrc
            .insert("Pilot 1".to_string(), vec![9, 9, 9, 9]);
        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Pilot 1");

        call(&mut disp, false, 0x08, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0, "noErr from truncate path");
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(
            disp.vfs.get("Pilot 1").unwrap(),
            &Vec::<u8>::new(),
            "data fork must be empty after truncate"
        );
        assert_eq!(
            disp.vfs_rsrc.get("Pilot 1").unwrap(),
            &vec![9, 9, 9, 9],
            "resource fork must be preserved across truncate"
        );
    }

    // ================================================================
    // 26b. FSDispatch ($A260) selector 6 — PBDirCreate
    // ================================================================
    #[test]
    fn fsdispatch_pbopenwd_colon_selects_requested_directory() {
        // Files 1992, 2-201: a lone colon means the directory identified by
        // ioWDDirID, so PBOpenWD must not search for a child named ":".
        let (mut disp, mut cpu, mut bus) = setup();
        let resources_dir_id = disp.ensure_vfs_directory("Game/H&E Resources");

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b":");
        bus.write_word(pb + 16, 0x3FFF); // ioResult poison
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
        bus.write_long(pb + 28, 0x454E_4C54); // arbitrary working-directory user ID
        bus.write_long(pb + 48, resources_dir_id);
        cpu.write_reg(Register::D0, 1); // HFSDispatch selector: PBOpenWD

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
        let wd_ref = bus.read_word(pb + 22) as i16;
        assert_ne!(wd_ref, super::super::dispatch::BOOT_VOLUME_REF_NUM);
        assert_eq!(bus.read_long(pb + 48), resources_dir_id);
        assert_eq!(
            disp.working_directory_info(wd_ref).map(|wd| wd.dir_id),
            Some(resources_dir_id)
        );
    }

    #[test]
    fn fsdispatch_pbdircreate_creates_child_directory_and_returns_dirid() {
        // PBDirCreate is PBHCreate for directories: it creates a new
        // directory under ioDirID and returns the new directory ID in ioDirID.
        // Inside Macintosh Volume IV, IV-146.
        let (mut disp, mut cpu, mut bus) = setup();

        let parent_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");
        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Sierra");
        bus.write_word(pb + 16, 0x3FFF); // ioResult poison
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
        bus.write_long(pb + 48, parent_dir_id);
        cpu.write_reg(Register::D0, 6); // HFSDispatch selector: PBDirCreate

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
        let created_dir_id = bus.read_long(pb + 48);
        assert_ne!(created_dir_id, parent_dir_id);
        assert_eq!(
            disp.directory_path_for_id(created_dir_id),
            Some("System Folder/Preferences/Sierra")
        );
    }

    #[test]
    fn fsdispatch_pbcatsearch_returns_matching_fsspec() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.vfs.insert("SearchMe".to_string(), vec![1, 2, 3]);
        disp.set_vfs_entry_metadata("SearchMe", *b"TEXT", *b"TEST", 0);
        disp.vfs.insert("ZZZ".to_string(), vec![4]);
        disp.set_vfs_entry_metadata("ZZZ", *b"TEXT", *b"TEST", 0);

        let pb = 0x300000u32;
        let match_ptr = 0x310000u32;
        let info1 = 0x320000u32;
        let info2 = 0x320100u32;
        let name_ptr = 0x320200u32;
        write_pstring(&mut bus, name_ptr, b"SearchMe");
        bus.write_long(info1 + 18, name_ptr);
        bus.write_byte(info1 + 30, 0); // file, not directory
        bus.write_long(info1 + 32, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(info1 + 36, u32::from_be_bytes(*b"TEST"));
        bus.write_long(info2 + 32, u32::MAX);
        bus.write_long(info2 + 36, u32::MAX);
        bus.write_byte(info2 + 30, 0x10); // mask the directory bit

        cpu.write_reg(Register::A0, pb);
        cpu.write_reg(Register::D0, 0x18); // PBCatSearch selector
        bus.write_long(pb + 24, match_ptr);
        bus.write_long(pb + 28, 1); // one FSSpec requested
        bus.write_long(pb + 36, 2 | 4 | 8); // full name, attributes, Finder info
        bus.write_long(pb + 40, info1);
        bus.write_long(pb + 44, info2);
        bus.write_long(pb + 52, 0); // start at the beginning of the catalog

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
        assert_eq!(bus.read_long(pb + 32), 1);
        assert_eq!(bus.read_word(match_ptr), (-1i16) as u16);
        assert_eq!(bus.read_long(match_ptr + 2), 2);
        assert_eq!(bus.read_pstring(match_ptr + 6), b"SearchMe");
        assert_ne!(
            bus.read_long(pb + 52),
            0,
            "search should advance the cursor"
        );

        let other_vref = disp.mount_vfs_volume_with_driver_info(
            "OtherVolume",
            "OtherVolume",
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        );
        bus.write_word(pb + 22, other_vref as u16);
        bus.write_long(pb + 52, 0);
        cpu.write_reg(Register::D0, 0x18);
        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, -39);
        assert_eq!(bus.read_long(pb + 32), 0);
    }

    #[test]
    fn fsdispatch_pbgetcatinfo_prefers_exact_directory_over_file_fallback() {
        let (mut disp, mut cpu, mut bus) = setup();

        let app_dir_id = disp.ensure_vfs_directory("EV Override 1.0.1");
        let pilots_dir_id = disp.ensure_vfs_directory("EV Override 1.0.1/Pilots");
        disp.vfs
            .insert("EV Override 1.0.1/Pilots/Ben".to_string(), vec![0x42]);

        let pb = 0x300000u32;
        let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"Pilots");
        bus.write_word(pb + 16, 0x3FFF); // ioResult poison
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
        bus.write_word(pb + 28, 0); // ioFDirIndex
        bus.write_long(pb + 48, app_dir_id);
        cpu.write_reg(Register::D0, 9); // HFSDispatch selector: PBGetCatInfo

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
        assert_eq!(bus.read_pstring(name_ptr), b"Pilots".to_vec());
        assert_eq!(
            bus.read_byte(pb + 30),
            0x10,
            "PBGetCatInfo returned a directory"
        );
        assert_eq!(bus.read_long(pb + 32), u32::from_be_bytes(*b"fold"));
        assert_eq!(bus.read_long(pb + 36), u32::from_be_bytes(*b"MACS"));
        assert_eq!(bus.read_long(pb + 48), pilots_dir_id);
        assert_eq!(
            bus.read_word(pb + 52),
            1,
            "ioDrNmFls counts the Ben file in the Pilots directory"
        );
        assert_eq!(bus.read_long(pb + 100), app_dir_id);
    }

    #[test]
    fn fsdispatch_pbgetcatinfo_empty_name_returns_directory_itself() {
        // Some legacy apps probe the current directory with ioFDirIndex = 0
        // and an empty ioNamePtr. Treat the empty name as the directory
        // selected by ioDirID rather than a missing child.
        let (mut disp, mut cpu, mut bus) = setup();

        let app_dir_id = disp.ensure_vfs_directory("Game Folder");

        let pb = 0x300000u32;
        let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"");
        bus.write_word(pb + 16, 0x3FFF); // ioResult poison
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
        bus.write_word(pb + 28, 0); // ioFDirIndex
        bus.write_long(pb + 48, app_dir_id);
        cpu.write_reg(Register::D0, 9); // HFSDispatch selector: PBGetCatInfo

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
        assert_eq!(bus.read_pstring(name_ptr), b"Game Folder".to_vec());
        assert_eq!(
            bus.read_byte(pb + 30),
            0x10,
            "PBGetCatInfo returned a directory"
        );
        assert_eq!(bus.read_long(pb + 32), u32::from_be_bytes(*b"fold"));
        assert_eq!(bus.read_long(pb + 36), u32::from_be_bytes(*b"MACS"));
        assert_eq!(bus.read_long(pb + 48), app_dir_id);
        assert_eq!(bus.read_long(pb + 100), 2);
    }

    #[test]
    fn fsdispatch_pbgetcatinfo_root_directory_returns_volume_name() {
        // Files 1992, 2-27 and 2-85: the root directory has dirID 2,
        // parent dirID 1, and the same name as its volume.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"poison");
        bus.write_word(pb + 16, 0x3FFF);
        bus.write_word(
            pb + 22,
            super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
        );
        bus.write_word(pb + 28, (-1i16) as u16);
        bus.write_long(pb + 48, 2);
        cpu.write_reg(Register::D0, 9);

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
        assert_eq!(
            bus.read_pstring(name_ptr),
            super::super::dispatch::BOOT_VOLUME_NAME.as_bytes()
        );
        assert_eq!(bus.read_byte(pb + 30), 0x10);
        assert_eq!(bus.read_long(pb + 48), 2);
        assert_eq!(bus.read_long(pb + 100), 1);
    }

    #[test]
    fn fsdispatch_pbgetcatinfo_resolves_root_by_parent_id_and_volume_name() {
        // Files 1992, 1-27 and 2-85: the File Manager assigns parent dirID 1
        // to a volume's root so callers can identify it consistently by
        // volume reference number, parent directory ID, and volume name.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        let name_ptr = setup_param_block(
            &mut bus,
            &mut cpu,
            pb,
            super::super::dispatch::BOOT_VOLUME_NAME.as_bytes(),
        );
        bus.write_word(pb + 16, 0x3FFF);
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
        bus.write_word(pb + 28, 0);
        bus.write_long(pb + 48, 1);
        cpu.write_reg(Register::D0, 9);

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
        assert_eq!(
            bus.read_pstring(name_ptr),
            super::super::dispatch::BOOT_VOLUME_NAME.as_bytes()
        );
        assert_eq!(bus.read_byte(pb + 30), 0x10);
        assert_eq!(bus.read_long(pb + 48), 2);
        assert_eq!(bus.read_long(pb + 100), 1);
    }

    #[test]
    fn fsdispatch_pbgetcatinfo_finds_resource_only_file() {
        let (mut disp, mut cpu, mut bus) = setup();

        let app_dir_id = disp.ensure_vfs_directory("Game Folder");
        disp.vfs_rsrc.insert(
            "Game Folder/Resource Data".to_string(),
            vec![0xCA, 0xFE, 0xBA, 0xBE],
        );
        disp.set_vfs_entry_metadata("Game Folder/Resource Data", *b"rsrc", *b"GAME", 0x0040);

        let pb = 0x300000u32;
        let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"Resource Data");
        bus.write_word(pb + 16, 0x3FFF); // ioResult poison
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
        bus.write_word(pb + 28, 0); // ioFDirIndex
        bus.write_long(pb + 48, app_dir_id);
        cpu.write_reg(Register::D0, 9); // HFSDispatch selector: PBGetCatInfo

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
        assert_eq!(bus.read_pstring(name_ptr), b"Resource Data".to_vec());
        assert_eq!(bus.read_byte(pb + 30), 0, "PBGetCatInfo returned a file");
        assert_eq!(bus.read_long(pb + 32), u32::from_be_bytes(*b"rsrc"));
        assert_eq!(bus.read_long(pb + 36), u32::from_be_bytes(*b"GAME"));
        assert_eq!(bus.read_word(pb + 40), 0x0040);
        assert!(
            bus.read_long(pb + 48) >= 32,
            "ioDirID should become a file ID"
        );
        assert_eq!(bus.read_long(pb + 54), 0, "data fork length");
        assert_eq!(bus.read_long(pb + 64), 4, "resource fork length");
        assert_eq!(bus.read_long(pb + 100), app_dir_id);
    }

    // ================================================================
    // 27. PBDelete (0x09)
    // ================================================================
    #[test]
    fn pb_delete() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("Pilot 1".to_string(), vec![1, 2, 3]);
        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Pilot 1");

        call(&mut disp, false, 0x09, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert!(!disp.vfs.contains_key("Pilot 1"));
    }

    #[test]
    fn pb_delete_not_found() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"MissingPilot");

        call(&mut disp, false, 0x09, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), (-43i32) as u32);
        assert_eq!(bus.read_word(pb + 16), (-43i16) as u16);
    }

    // ================================================================
    // 28. PBGetFInfo (0x0C) — file exists
    // ================================================================
    #[test]
    fn pb_get_finfo_found() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("InfoFile".to_string(), vec![1, 2, 3]);

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"InfoFile");
        bus.write_long(pb + 100, 0xDEAD_BEEF);

        call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(bus.read_long(pb + 100), 0xDEAD_BEEF);
    }

    #[test]
    fn pb_get_finfo_positive_iofdirindex_enumerates_files_in_working_directory() {
        // Inside Macintosh Volume IV, IV-148: positive ioFDirIndex indexes
        // files on ioVRefNum; when ioVRefNum is a working-directory refnum,
        // it indexes files in that directory.  GetFileInfo indexes files only,
        // unlike GetCatInfo.
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("Mission/A Launcher".to_string(), vec![]);
        disp.vfs
            .insert("Mission/B Suspended".to_string(), vec![0xAA]);
        disp.set_vfs_entry_metadata("Mission/A Launcher", *b"APPL", *b"WSTL", 0);
        disp.set_vfs_entry_metadata("Mission/B Suspended", *b"SAVE", *b"WSTL", 0);
        disp.ensure_vfs_directory("Mission/AAA Folder");
        disp.vfs
            .insert("Mission/AAA Folder/Inner Save".to_string(), vec![0xBB]);
        disp.set_vfs_entry_metadata("Mission/AAA Folder/Inner Save", *b"SAVE", *b"WSTL", 0);
        disp.set_launched_app_path("Mission/A Launcher");
        let suspended_metadata = disp.vfs_file_metadata("Mission/B Suspended").unwrap();

        let pb = 0x300000u32;
        let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"stale output name");
        bus.write_word(pb + 22, disp.app_wd_refnum as u16);
        bus.write_word(pb + 28, 2); // second file in Mission, not second catalog entry

        call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(bus.read_pstring(name_ptr), b"B Suspended".to_vec());
        assert_eq!(bus.read_long(pb + 32), u32::from_be_bytes(*b"SAVE"));
        assert_eq!(bus.read_long(pb + 36), u32::from_be_bytes(*b"WSTL"));
        assert_eq!(bus.read_long(pb + 48), suspended_metadata.file_id);

        let pb2 = 0x300400u32;
        setup_param_block(&mut bus, &mut cpu, pb2, b"A Launcher");
        bus.write_word(pb2 + 22, disp.app_wd_refnum as u16);
        bus.write_word(pb2 + 28, 99);

        call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), (-43i32) as u32);
        assert_eq!(bus.read_word(pb2 + 16), (-43i16) as u16);
    }

    // ================================================================
    // 28b. PBGetFInfo (0x0C) — file not found
    // ================================================================
    #[test]
    fn pb_get_finfo_not_found() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"NoSuchFile");

        call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), (-43i32) as u32);
    }

    // ================================================================
    // 28c. PBAllocate ($A010)
    // ================================================================
    #[test]
    fn pballocate_nominal_call_returns_noerr_and_sets_ioactcount() {
        // Inside Macintosh: Files (1992), pp. 2-130 to 2-131:
        // PBAllocate reports result via D0/ioResult and writes ioActCount.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 16, 0x3FFF); // ioResult poison
        bus.write_long(pb + 36, 0x1234); // ioReqCount
        bus.write_long(pb + 40, 0xFFFF_FFFF); // ioActCount poison

        call_trap_word(&mut disp, 0xA010, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr in D0");
        assert_eq!(bus.read_word(pb + 16) as i16, 0, "ioResult overwritten");
        assert_eq!(bus.read_long(pb + 40), 0x1234, "ioActCount mirrors request");
    }

    #[test]
    fn pballocate_writes_ioresult_noerr_when_paramblock_present() {
        // Inside Macintosh: Files (1992), pp. 2-130 to 2-131:
        // PBAllocate uses ioResult as the parameter-block mirror of D0.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 16, 0x7B7B); // ioResult poison
        bus.write_long(pb + 36, 1);

        call_trap_word(&mut disp, 0xA010, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
    }

    /// Pre-poisons pb.ioResult at pb+16 with a non-noErr, non-OSErr sentinel
    /// (0x3FFF), sets ioRefNum to a clearly-bogus 9999, dispatches $A010,
    /// asserts the sentinel was overwritten AND D0 == ioResult per the
    /// File Manager dispatcher convention (Inside Macintosh Volume II 1985,
    /// p. II-114) AND that A7 is preserved across the call.
    #[test]
    fn pballocate_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 16, 0x3FFF); // ioResult pre-poison
        bus.write_word(pb + 24, 9999); // ioRefNum bogus
        bus.write_long(pb + 36, 256); // ioReqCount

        let sp_pre = cpu.read_reg(Register::A7);
        call_trap_word(&mut disp, 0xA010, &mut cpu, &mut bus).unwrap();
        let sp_post = cpu.read_reg(Register::A7);

        let d0 = cpu.read_reg(Register::D0) as i16;
        let io_result = bus.read_word(pb + 16) as i16;
        assert_ne!(io_result, 0x3FFF, "ioResult must be overwritten");
        assert_eq!(d0, io_result, "D0 must mirror pb.ioResult");
        assert_eq!(sp_pre, sp_post, "A7 must be preserved");
    }

    // ================================================================
    // 28d. PBAllocContig ($A210)
    // ================================================================
    #[test]
    fn pballoccontig_nominal_call_returns_noerr_and_sets_ioactcount() {
        // Inside Macintosh: Files (1992), pp. 2-130 to 2-131:
        // PBAllocContig reports result via D0/ioResult and writes ioActCount.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 16, 0x3FFF); // ioResult poison
        bus.write_long(pb + 36, 0x1234); // ioReqCount
        bus.write_long(pb + 40, 0xFFFF_FFFF); // ioActCount poison

        call_trap_word(&mut disp, 0xA210, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr in D0");
        assert_eq!(bus.read_word(pb + 16) as i16, 0, "ioResult overwritten");
        assert_eq!(bus.read_long(pb + 40), 0x1234, "ioActCount mirrors request");
    }

    #[test]
    fn pballoccontig_overwrites_ioresult_field_with_function_result() {
        // Inside Macintosh: Files (1992), p. 2-131 result-code contract:
        // ioResult is the function-result field for PBAllocContig.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 16, 0x7B7B); // ioResult poison
        bus.write_long(pb + 36, 1);

        call_trap_word(&mut disp, 0xA210, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
    }

    /// Pre-poisons pb.ioResult at pb+16 with a non-noErr, non-OSErr
    /// sentinel (0x3FFF), sets ioRefNum to a clearly-bogus 9999, dispatches
    /// $A210, asserts the sentinel was overwritten AND D0 == ioResult per
    /// the File Manager dispatcher convention (Inside Macintosh Volume II
    /// 1985, p. II-114) AND that A7 is preserved across the call.
    #[test]
    fn pballoccontig_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 16, 0x3FFF); // ioResult pre-poison
        bus.write_word(pb + 24, 9999); // ioRefNum bogus
        bus.write_long(pb + 36, 256); // ioReqCount

        let sp_pre = cpu.read_reg(Register::A7);
        call_trap_word(&mut disp, 0xA210, &mut cpu, &mut bus).unwrap();
        let sp_post = cpu.read_reg(Register::A7);

        let d0 = cpu.read_reg(Register::D0) as i16;
        let io_result = bus.read_word(pb + 16) as i16;
        assert_ne!(io_result, 0x3FFF, "ioResult must be overwritten");
        assert_eq!(d0, io_result, "D0 must mirror pb.ioResult");
        assert_eq!(sp_pre, sp_post, "A7 must be preserved");
    }

    // ================================================================
    // 28e. Volume / I/O queue stubs ($A006, $A016, $A017)
    // ================================================================
    #[test]
    fn pbkillio_returns_noerr_when_hle_has_no_async_io_queue() {
        // Inside Macintosh Volume II (1985), p. II-187:
        // PBKillIO reports an OSErr result; HLE succeeds with noErr.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300200u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100); // ioRefNum

        call(&mut disp, false, 0x06, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr in D0");
    }

    #[test]
    fn pbkillio_writes_ioresult_noerr_when_paramblock_present() {
        // Inside Macintosh Volume II (1985), p. II-187:
        // ioResult is the returned function result field for PBKillIO.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300240u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 16, 0x6A6A); // ioResult poison
        bus.write_word(pb + 24, 200); // ioRefNum

        call(&mut disp, false, 0x06, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0, "ioResult overwritten");
    }

    #[test]
    fn pbkillio_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
        // Pre-poison ioResult with
        // 0x3FFF (not noErr and not a documented OSErr), set ioRefNum to
        // a clearly-bogus value, dispatch _PBKillIO, witness that the
        // sentinel was overwritten AND D0 == ioResult per the Device
        // Manager OS-bit FUNCTION dispatcher convention (IM:II 1985,
        // p. II-114), AND A7 unchanged across the trap.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300280u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 16, 0x3FFFu16); // pre-poison ioResult
        bus.write_word(pb + 24, 9999u16); // bogus ioRefNum

        let sp_pre = cpu.read_reg(Register::A7);
        call(&mut disp, false, 0x06, &mut cpu, &mut bus).unwrap();
        let sp_post = cpu.read_reg(Register::A7);

        let d0 = cpu.read_reg(Register::D0) as i16;
        let io_result = bus.read_word(pb + 16) as i16;
        assert_ne!(
            io_result, 0x3FFFi16,
            "ioResult sentinel must be overwritten"
        );
        assert_eq!(d0, io_result, "D0 == ioResult per dispatcher convention");
        assert_eq!(sp_pre, sp_post, "A7 preserved (register-only ABI)");
    }

    #[test]
    fn finitqueue_has_no_parameters_and_preserves_stack_pointer() {
        // Inside Macintosh Volume II (1985), p. II-103:
        // FInitQueue is declared as PROCEDURE FInitQueue with no parameters.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);

        call(&mut disp, false, 0x16, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            0,
            "HLE returns noErr in D0"
        );
    }

    #[test]
    fn finitqueue_five_call_composition_preserves_stack_and_returns_noerr_each_call() {
        // 5 successive _FInitQueue dispatches inside one StackSpace
        // sandwich. Per-call pop discipline errors accumulate; this
        // pins cumulative drift even when each individual call's
        // drift would be small.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_pre = cpu.read_reg(Register::A7);

        for _ in 0..5 {
            cpu.write_reg(Register::D0, 0xCAFE_F00D);
            call(&mut disp, false, 0x16, &mut cpu, &mut bus).unwrap();
            assert_eq!(
                cpu.read_reg(Register::D0) as i32,
                0,
                "HLE returns noErr in D0 on each call"
            );
        }

        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_pre,
            "A7 preserved after 5-call composition"
        );
    }

    #[test]
    fn adddrive_returns_noerr_and_preserves_stack_pointer() {
        // Inside Macintosh: Files (1992), p. 2-236 and Technical Note #108
        // define AddDrive as a register-only call with a noErr return.
        let (mut disp, mut cpu, mut bus) = setup();
        let qel = 0x320400u32;
        cpu.write_reg(Register::A0, qel);
        cpu.write_reg(Register::D0, (7u32 << 16) | 42u32);
        let sp_before = cpu.read_reg(Register::A7);

        call_trap_word(&mut disp, 0xA04E, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr in D0");
        assert_eq!(cpu.read_reg(Register::A0), qel, "A0 preserved");
    }

    #[test]
    fn adddrive_five_call_composition_preserves_stack_and_returns_noerr_each_call() {
        // 5 successive register-only AddDrive dispatches inside one
        // StackSpace sandwich.
        let (mut disp, mut cpu, mut bus) = setup();
        let qel = 0x320500u32;
        cpu.write_reg(Register::A0, qel);
        let sp_before = cpu.read_reg(Register::A7);

        for _ in 0..5 {
            cpu.write_reg(Register::D0, (11u32 << 16) | 99u32);
            call_trap_word(&mut disp, 0xA04E, &mut cpu, &mut bus).unwrap();
            assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr in D0");
            assert_eq!(cpu.read_reg(Register::A0), qel, "A0 preserved");
        }

        assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
    }

    #[test]
    fn pboffline_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
        // Pre-poisons pb.ioResult at pb+16 with 0x3FFF (neither noErr nor
        // any documented OSErr), dispatches _PBOffLine with a clearly-
        // bogus ioVRefNum 9999, witnesses that the trap overwrote the
        // sentinel AND that D0 == ioResult (per File Manager dispatcher
        // convention IM:II 1985 p. II-114) AND that A7 is preserved
        // across the call (register-only OS-bit FUNCTION calling
        // convention per IM:Files 1992 pp. 2-141..2-142).
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        let pb = 0x300300u32;
        for i in 0..50u32 {
            bus.write_byte(pb + i, 0);
        }
        bus.write_word(pb + 16, 0x3FFF); // ioResult poison
        bus.write_word(pb + 22, 9999); // ioVRefNum bogus
        cpu.write_reg(Register::A0, pb);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);

        call(&mut disp, false, 0x35, &mut cpu, &mut bus).unwrap();

        let d0 = cpu.read_reg(Register::D0) as i16;
        let io_result = bus.read_word(pb + 16) as i16;
        assert_ne!(io_result, 0x3FFFi16, "trap overwrote ioResult sentinel");
        assert_eq!(
            d0, io_result,
            "D0 mirrors ioResult per dispatcher convention"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
    }

    #[test]
    fn pbeject_returns_noerr_when_hle_has_no_removable_media() {
        // Inside Macintosh: Files (1992), p. 2-141:
        // PBEject reports noErr for successful nominal calls.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300280u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);

        call(&mut disp, false, 0x17, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr in D0");
    }

    #[test]
    fn pbeject_writes_ioresult_noerr_when_paramblock_present() {
        // Inside Macintosh: Files (1992), p. 2-141:
        // ioResult carries the function result in the parameter block.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x3002C0u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 16, 0x5151); // ioResult poison
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);

        call(&mut disp, false, 0x17, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0, "ioResult overwritten");
    }

    #[test]
    fn pbeject_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
        // Pre-poisons pb.ioResult at pb+16 with 0x3FFF (neither noErr nor
        // any documented OSErr), dispatches _PBEject with a clearly-
        // bogus ioVRefNum 9999, witnesses that the trap overwrote the
        // sentinel AND that D0 == ioResult (per File Manager dispatcher
        // convention IM:II 1985 p. II-114) AND that A7 is preserved
        // across the call (register-only OS-bit FUNCTION calling
        // convention per IM:Files 1992 p. 2-141).
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        let pb = 0x300340u32;
        for i in 0..50u32 {
            bus.write_byte(pb + i, 0);
        }
        bus.write_word(pb + 16, 0x3FFF); // ioResult poison
        bus.write_word(pb + 22, 9999); // ioVRefNum bogus
        cpu.write_reg(Register::A0, pb);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);

        call(&mut disp, false, 0x17, &mut cpu, &mut bus).unwrap();

        let d0 = cpu.read_reg(Register::D0) as i16;
        let io_result = bus.read_word(pb + 16) as i16;
        assert_ne!(io_result, 0x3FFFi16, "trap overwrote ioResult sentinel");
        assert_eq!(
            d0, io_result,
            "D0 mirrors ioResult per dispatcher convention"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
    }

    /// Pre-poisons pb.ioResult at pb+16 with 0x3FFF (neither noErr nor any
    /// documented OSErr), dispatches _PBMountVol with a clearly-bogus
    /// ioVRefNum=9999 drive number, witnesses that the trap overwrote the
    /// sentinel AND that D0 == ioResult (per File Manager dispatcher
    /// convention IM:II 1985 p. II-114) AND that A7 is preserved across
    /// the call (register-only OS-bit FUNCTION calling convention per
    /// IM:Files 1992 p. 2-139).
    #[test]
    fn pbmountvol_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        let pb = 0x300380u32;
        for i in 0..50u32 {
            bus.write_byte(pb + i, 0);
        }
        bus.write_word(pb + 16, 0x3FFF); // ioResult poison
        bus.write_word(pb + 22, 9999); // ioVRefNum bogus drive number
        cpu.write_reg(Register::A0, pb);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);

        call_trap_word(&mut disp, 0xA00F, &mut cpu, &mut bus).unwrap();

        let d0 = cpu.read_reg(Register::D0) as i16;
        let io_result = bus.read_word(pb + 16) as i16;
        assert_ne!(io_result, 0x3FFFi16, "trap overwrote ioResult sentinel");
        assert_eq!(
            d0, io_result,
            "D0 mirrors ioResult per dispatcher convention"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
    }

    /// Pre-poisons pb.ioResult at pb+16 with 0x3FFF on two separate parameter
    /// blocks, dispatches _PBUnmountVol ($A00E) then _PBUnmountVolImmed
    /// ($A20E) — which share the same low-byte arm — with a clearly-bogus
    /// ioVRefNum 9999, witnesses that each trap overwrote its sentinel AND
    /// that D0 == ioResult per File Manager dispatcher convention (IM:II
    /// 1985 p. II-114) AND that A7 is preserved across both calls
    /// (register-only OS-bit FUNCTION calling convention per IM:Files
    /// 1992 p. 2-148).
    #[test]
    fn pbunmountvol_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        let pb_a00e = 0x3003C0u32;
        let pb_a20e = 0x300400u32;
        for i in 0..50u32 {
            bus.write_byte(pb_a00e + i, 0);
            bus.write_byte(pb_a20e + i, 0);
        }
        bus.write_word(pb_a00e + 16, 0x3FFF);
        bus.write_word(pb_a00e + 22, 9999);
        bus.write_word(pb_a20e + 16, 0x3FFF);
        bus.write_word(pb_a20e + 22, 9999);

        cpu.write_reg(Register::A0, pb_a00e);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);
        call_trap_word(&mut disp, 0xA00E, &mut cpu, &mut bus).unwrap();
        let d0_a00e = cpu.read_reg(Register::D0) as i16;
        let io_a00e = bus.read_word(pb_a00e + 16) as i16;
        assert_ne!(io_a00e, 0x3FFFi16, "A00E overwrote ioResult sentinel");
        assert_eq!(d0_a00e, io_a00e, "A00E D0 mirrors ioResult");

        cpu.write_reg(Register::A0, pb_a20e);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);
        call_trap_word(&mut disp, 0xA20E, &mut cpu, &mut bus).unwrap();
        let d0_a20e = cpu.read_reg(Register::D0) as i16;
        let io_a20e = bus.read_word(pb_a20e + 16) as i16;
        assert_ne!(io_a20e, 0x3FFFi16, "A20E overwrote ioResult sentinel");
        assert_eq!(d0_a20e, io_a20e, "A20E D0 mirrors ioResult");

        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "A7 preserved across both A00E and A20E dispatches",
        );
    }

    /// Pre-poisons pb.ioResult at pb+16 with 0x3FFF, dispatches _PBSetFVers
    /// with a bogus ioVRefNum=9999 (no such volume), witnesses that the
    /// trap overwrote the sentinel AND that D0 == ioResult per File
    /// Manager dispatcher convention (IM:II 1985 p. II-114) AND that A7
    /// is preserved across the call (register-only OS-bit FUNCTION
    /// calling convention per IM:II 1985 p. II-117). Per IM:IV 1986
    /// p. IV-153, PBSetFVers is a documented no-op on hierarchical
    /// volumes; Systemless's HFS-only VFS makes the trap always-noErr.
    #[test]
    fn pbsetfvers_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        let pb = 0x300440u32;
        for i in 0..50u32 {
            bus.write_byte(pb + i, 0);
        }
        bus.write_word(pb + 16, 0x3FFF); // ioResult poison
        bus.write_word(pb + 22, 9999); // ioVRefNum bogus
        cpu.write_reg(Register::A0, pb);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);

        call_trap_word(&mut disp, 0xA043, &mut cpu, &mut bus).unwrap();

        let d0 = cpu.read_reg(Register::D0) as i16;
        let io_result = bus.read_word(pb + 16) as i16;
        assert_ne!(io_result, 0x3FFFi16, "trap overwrote ioResult sentinel");
        assert_eq!(
            d0, io_result,
            "D0 mirrors ioResult per dispatcher convention"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
    }

    /// Pre-poisons pb.ioResult at pb+16 with 0x3FFF, dispatches _PBHRename
    /// with bogus old/new filenames guaranteed not to exist in the VFS,
    /// witnesses that the trap overwrote the sentinel AND that D0 ==
    /// ioResult per File Manager dispatcher convention (IM:II 1985
    /// p. II-114) AND that A7 is preserved across the call (register-only
    /// OS-bit FUNCTION calling convention per IM:Files 1992 p. 2-118).
    #[test]
    fn pbhrename_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        let pb = 0x300480u32;
        for i in 0..60u32 {
            bus.write_byte(pb + i, 0);
        }
        let old_name_ptr = 0x301000u32;
        let new_name_ptr = 0x301040u32;
        let old_name = b"\x1FNoSuchFile_A20B_PBHRename_OLD";
        let new_name = b"\x1FNoSuchFile_A20B_PBHRename_NEW";
        for (i, &b) in old_name.iter().enumerate() {
            bus.write_byte(old_name_ptr + i as u32, b);
        }
        for (i, &b) in new_name.iter().enumerate() {
            bus.write_byte(new_name_ptr + i as u32, b);
        }
        bus.write_word(pb + 16, 0x3FFF); // ioResult poison
        bus.write_long(pb + 18, old_name_ptr); // ioNamePtr
        bus.write_word(pb + 22, 0); // ioVRefNum default
        bus.write_long(pb + 28, new_name_ptr); // ioMisc -> new name
        cpu.write_reg(Register::A0, pb);
        cpu.write_reg(Register::D0, 0xDEAD_BEEF);

        call_trap_word(&mut disp, 0xA20B, &mut cpu, &mut bus).unwrap();

        let d0 = cpu.read_reg(Register::D0) as i16;
        let io_result = bus.read_word(pb + 16) as i16;
        assert_ne!(io_result, 0x3FFFi16, "trap overwrote ioResult sentinel");
        assert_eq!(
            d0, io_result,
            "D0 mirrors ioResult per dispatcher convention"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
    }

    // ================================================================
    // 29. PBSetVol (0x15)
    // ================================================================
    #[test]
    fn pb_set_vol() {
        let (mut disp, mut cpu, mut bus) = setup();

        let dir_id = disp.ensure_vfs_directory("Marathon");
        disp.vfs.insert("Marathon/Marathon".to_string(), vec![]);
        disp.set_launched_app_path("Marathon/Marathon");

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 22, disp.app_wd_refnum as u16);

        call(&mut disp, false, 0x15, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(disp.default_dir_id, dir_id);
        assert_eq!(bus.read_long(addr::CUR_DIR_STORE), dir_id);
        assert_eq!(
            bus.read_word(addr::SF_SAVE_DISK),
            (-super::super::dispatch::BOOT_VOLUME_REF_NUM) as u16
        );
    }

    #[test]
    fn pbsetvol_volume_name_selects_named_volume_root() {
        let (mut disp, mut cpu, mut bus) = setup();
        let volume_ref = disp.mount_vfs_volume_with_driver_info(
            "Legend CD",
            ".AppleCD",
            0,
            1,
            1024,
            512,
            512,
            900,
            0,
            0,
            0,
            0,
            0,
            0,
        );
        let root_dir_id = disp
            .vfs_volume_for_ref_num(volume_ref)
            .expect("mounted volume")
            .root_dir_id;

        let pb = 0x300000u32;
        let name_buf = 0x300100u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_long(pb + 18, name_buf);
        bus.write_pstring(name_buf, b"Legend CD");
        bus.write_word(pb + 22, 0); // name selects the volume

        call(&mut disp, false, 0x15, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(disp.default_dir_id, root_dir_id);
        assert_eq!(disp.resolve_volume_ref_num(disp.app_wd_refnum), volume_ref);
        assert_eq!(bus.read_word(addr::SF_SAVE_DISK), (-volume_ref) as u16);
    }

    #[test]
    fn extracted_volume_rejects_write_open_and_fswrite() {
        let (mut disp, mut cpu, mut bus) = setup();
        let volume_ref = disp.mount_vfs_volume_with_driver_info(
            "Legend CD",
            ".AppleCD",
            0,
            1,
            1024,
            512,
            512,
            900,
            0,
            0,
            0,
            0,
            0,
            0,
        );
        let file_name = "Legend CD/Legend";
        disp.vfs.insert(file_name.to_string(), vec![1, 2, 3]);

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Legend CD/Legend");
        bus.write_word(pb + 22, volume_ref as u16);
        bus.write_byte(pb + 27, 3); // fsRdWrPerm
        call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, -44, "wPrErr");

        bus.write_byte(pb + 27, 1); // fsRdPerm
        call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0), 0);
        let ref_num = bus.read_word(pb + 24);
        bus.write_word(pb + 24, ref_num);
        bus.write_long(pb + 32, 0x310000);
        bus.write_long(pb + 36, 1);
        bus.write_byte(0x310000, 9);
        call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, -44, "wPrErr");
        assert_eq!(disp.vfs.get(file_name), Some(&vec![1, 2, 3]));
    }

    #[test]
    fn pbsetvol_volume_refnum_ignores_stale_iowddirid() {
        // Files 1992, 2-151: PBSetVol uses the basic parameter block and,
        // when passed a volume reference number, sets the default directory
        // to that volume's root. The HFS-only ioWDDirID field belongs to
        // PBHSetVol, not PBSetVol.
        let (mut disp, mut cpu, mut bus) = setup();

        let stale_dir_id = disp.ensure_vfs_directory("Marathon");

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
        bus.write_long(pb + 48, stale_dir_id);
        bus.write_long(pb + 18, 0); // ioNamePtr = NIL

        call_trap_word(&mut disp, 0xA015, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(disp.default_dir_id, 2);
        assert_eq!(
            disp.app_wd_refnum,
            super::super::dispatch::BOOT_VOLUME_REF_NUM
        );
        assert_eq!(bus.read_long(addr::CUR_DIR_STORE), 2);
    }

    #[test]
    fn pbhsetvol_sets_default_directory_from_iowddirid_for_volume_refnum_calls() {
        // Inside Macintosh: Files (1992), pp. 2-153 to 2-154:
        // with ioNamePtr = NIL and a volume refnum in ioVRefNum,
        // PBHSetVol uses ioWDDirID as the default directory.
        let (mut disp, mut cpu, mut bus) = setup();

        let dir_id = disp.ensure_vfs_directory("Marathon");

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
        bus.write_long(pb + 48, dir_id);
        bus.write_long(pb + 18, 0); // ioNamePtr = NIL

        call_trap_word(&mut disp, 0xA215, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(disp.default_dir_id, dir_id);
        assert_ne!(
            disp.app_wd_refnum,
            super::super::dispatch::BOOT_VOLUME_REF_NUM
        );
        assert_eq!(bus.read_long(addr::CUR_DIR_STORE), dir_id);
    }

    #[test]
    fn pbhsetvol_invalid_vrefnum_returns_nsverr() {
        // Inside Macintosh: Files (1992), pp. 2-153 to 2-154:
        // unresolved volume references return nsvErr.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 22, (-999i16) as u16); // ioVRefNum phantom
        bus.write_long(pb + 48, 0); // ioWDDirID = 0 keeps the call on the nsvErr path

        call_trap_word(&mut disp, 0xA215, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -35, "nsvErr in D0");
        assert_eq!(bus.read_word(pb + 16) as i16, -35, "nsvErr in ioResult");
    }

    #[test]
    fn pbhsetvol_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
        // Pre-poisons pb.ioResult @ pb+16 with 0x3FFF and sets a phantom
        // ioVRefNum (-999); asserts the sentinel is overwritten AND
        // D0 == ioResult per the File Manager basic-PB dispatcher
        // convention AND A7 is preserved across the call.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 16, 0x3FFFu16); // pre-poison ioResult
        bus.write_word(pb + 22, (-999i16) as u16); // ioVRefNum phantom
        bus.write_long(pb + 48, 0); // ioWDDirID = 0

        let sp_pre = cpu.read_reg(Register::A7);
        call_trap_word(&mut disp, 0xA215, &mut cpu, &mut bus).unwrap();
        let sp_post = cpu.read_reg(Register::A7);

        let io_result = bus.read_word(pb + 16) as i16;
        let d0_result = cpu.read_reg(Register::D0) as i32 as i16;
        assert_ne!(io_result as u16, 0x3FFFu16, "ioResult sentinel overwritten");
        assert_eq!(
            d0_result, io_result,
            "D0 == ioResult per File Manager basic-PB dispatcher convention"
        );
        assert_eq!(sp_pre, sp_post, "A7 preserved across PBHSetVol");
    }

    // ================================================================
    // 30. PBSetFInfo / HSetFInfo (0x0D)
    // ================================================================
    #[test]
    fn pb_set_finfo() {
        // Updated for Files 1992 2-205 contract: existing file →
        // noErr, missing file → fnfErr. Prior to the fix this test
        // relied on PBSetFInfo silently returning noErr for any
        // filename (including ones not in the VFS).
        let (mut disp, mut cpu, mut bus) = setup();
        disp.vfs.insert("AnyFile".to_string(), Vec::new());

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"AnyFile");

        call(&mut disp, false, 0x0D, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
    }

    #[test]
    fn pb_set_finfo_missing_file_is_fnferr() {
        // Regression: previously PBSetFInfo returned noErr for missing
        // files, silently masking missing-save bugs in games that
        // rely on fnfErr (Files 1992, 2-205).
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"NotPresent");

        call(&mut disp, false, 0x0D, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -43);
        assert_eq!(bus.read_word(pb + 16) as i16, -43);
    }

    // ================================================================
    // 31. PBSetEOF / HSetEOF ($A012)
    // ================================================================
    #[test]
    fn pb_set_eof() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs
            .insert("EOFSet".to_string(), vec![0xAA, 0xBB, 0xCC, 0xDD]);
        disp.open_files.insert(100, "EOFSet".to_string());
        disp.file_positions.insert(100, 4);

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 100); // ioRefNum
        bus.write_long(pb + 28, 2); // ioMisc = new EOF

        call(&mut disp, false, 0x12, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(disp.vfs.get("EOFSet").unwrap(), &vec![0xAA, 0xBB]);
        assert_eq!(*disp.file_positions.get(&100).unwrap(), 2);
    }

    #[test]
    fn pb_set_eof_invalid_refnum() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_word(pb + 24, 999);
        bus.write_long(pb + 28, 2);

        call(&mut disp, false, 0x12, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), (-51i32) as u32);
        assert_eq!(bus.read_word(pb + 16), (-51i16) as u16);
    }

    // ================================================================
    // 32. FSDispatch (0x60) — selector 8 (PBGetFCBInfo), refnum not open
    // ================================================================
    #[test]
    fn fs_dispatch_pbgetfcbinfo_refnum_not_open() {
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"stale output buffer");
        bus.write_word(pb + 24, 999); // ioRefNum

        cpu.write_reg(Register::D0, 8); // selector = PBGetFCBInfo

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            cpu.read_reg(Register::D0),
            (-38i32) as u32,
            "D0 should be fnOpnErr"
        );
        assert_eq!(bus.read_word(pb + 16), (-38i16) as u16);
    }

    // ================================================================
    // 32b. FSDispatch (0x60) — selector 8 (PBGetFCBInfo), app resource file
    // ================================================================
    #[test]
    fn fs_dispatch_pbgetfcbinfo_refnum_zero_ignores_stale_name_output() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs
            .insert("EV/Escape Velocity".to_string(), vec![0xAB]);
        disp.set_vfs_entry_metadata("EV/Escape Velocity", *b"APPL", *b"EV  ", 0);
        disp.set_launched_app_path("EV/Escape Velocity");
        let expected_parent_dir_id = disp.default_dir_id;

        let pb = 0x300000u32;
        let name_ptr = setup_param_block(
            &mut bus,
            &mut cpu,
            pb,
            b"stale output buffer that must not be treated as an input filename",
        );
        bus.write_word(pb + 24, 0); // ioRefNum 0 = current app resource file

        cpu.write_reg(Register::D0, 8);

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(bus.read_pstring(name_ptr), b"Escape Velocity".to_vec());
        assert_eq!(bus.read_long(pb + 58), expected_parent_dir_id);
        assert_eq!(bus.read_word(pb + 52) as i16, -1);
    }

    #[test]
    fn fs_dispatch_pbgetfcbinfo_data_fork_reports_size_position_and_flags() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs
            .insert("Game/Data 1".to_string(), (0u8..12).collect());
        disp.set_vfs_entry_metadata("Game/Data 1", *b"DATA", *b"TST!", 0);
        let metadata = disp.vfs_file_metadata("Game/Data 1").unwrap();
        disp.open_files.insert(123, "Game/Data 1".to_string());
        disp.file_positions.insert(123, 4);

        let pb = 0x300000u32;
        let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"stale output buffer");
        bus.write_word(pb + 24, 123);
        bus.write_word(pb + 28, 0);
        cpu.write_reg(Register::D0, 8);

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(bus.read_pstring(name_ptr), b"Data 1".to_vec());
        assert_eq!(bus.read_word(pb + 22) as i16, -1);
        assert_eq!(bus.read_long(pb + 32), metadata.file_id);
        assert_eq!(bus.read_word(pb + 36) & 0x0200, 0);
        assert_eq!(bus.read_long(pb + 40), 12);
        assert_eq!(bus.read_long(pb + 44), 12);
        assert_eq!(bus.read_long(pb + 48), 4);
        assert_eq!(bus.read_word(pb + 52) as i16, -1);
        assert_eq!(bus.read_long(pb + 58), metadata.parent_dir_id);
    }

    #[test]
    fn fs_dispatch_pbgetfcbinfo_resource_fork_reports_resource_size_and_flag() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("Game/Data 1".to_string(), vec![0xAA, 0xBB]);
        disp.vfs_rsrc
            .insert("Game/Data 1".to_string(), vec![1, 2, 3, 4, 5]);
        disp.set_vfs_entry_metadata("Game/Data 1", *b"DATA", *b"TST!", 0);
        let metadata = disp.vfs_file_metadata("Game/Data 1").unwrap();
        disp.open_files
            .insert(124, "__rsrc__Game/Data 1".to_string());
        disp.file_positions.insert(124, 3);

        let pb = 0x300000u32;
        let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"stale output buffer");
        bus.write_word(pb + 24, 124);
        bus.write_word(pb + 28, 0);
        cpu.write_reg(Register::D0, 8);

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(bus.read_pstring(name_ptr), b"Data 1".to_vec());
        assert_eq!(bus.read_long(pb + 32), metadata.file_id);
        assert_ne!(bus.read_word(pb + 36) & 0x0200, 0);
        assert_eq!(bus.read_long(pb + 40), 5);
        assert_eq!(bus.read_long(pb + 44), 5);
        assert_eq!(bus.read_long(pb + 48), 3);
        assert_eq!(bus.read_long(pb + 58), metadata.parent_dir_id);
    }

    #[test]
    fn pbhgetfinfo_retries_default_directory_for_stale_dirid() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.ensure_vfs_directory("Marathon");
        disp.vfs.insert("Marathon/Marathon".to_string(), vec![]);
        disp.vfs
            .insert("Marathon/Physics Model".to_string(), vec![0xAB]);
        disp.set_launched_app_path("Marathon/Marathon");

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Physics Model");
        bus.write_long(pb + 48, 0x01FF01FF);

        call_trap_word(&mut disp, 0xA20C, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
    }

    #[test]
    fn initfs_returns_noerr_and_preserves_stack_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_pre = cpu.read_reg(Register::A7);

        call(&mut disp, false, 0x6C, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp_pre);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // PBFlushFile (0x45) — valid open refnum returns noErr.
    // Files 1992, 2-114: noErr when the refnum identifies an open access path.
    #[test]
    fn pb_flush_file_valid_refnum_returns_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("Flush.dat".to_string(), vec![1, 2, 3]);
        let pb = 0x270000u32;
        for i in 0u32..32 {
            bus.write_byte(pb + i, 0);
        }
        setup_param_block(&mut bus, &mut cpu, pb, b"Flush.dat");
        call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap(); // PBOpen → assigns refnum
        let refnum = bus.read_word(pb + 24);
        assert!(refnum >= 100, "expected open refnum");

        for i in 0u32..32 {
            bus.write_byte(pb + i, 0);
        }
        bus.write_word(pb + 24, refnum);
        cpu.write_reg(Register::A0, pb);
        call(&mut disp, false, 0x45, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "valid refnum → noErr");
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
    }

    // PBRename (0x0B) — renaming an existing VFS file updates all state.
    // Files 1992, 2-118: the file entry and any open access paths follow the rename.
    #[test]
    fn pb_rename_existing_file_succeeds() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("OldName.dat".to_string(), vec![0xAA, 0xBB]);

        let pb = 0x271000u32;
        let old_name_addr = pb + 0x100;
        let new_name_addr = pb + 0x200;
        write_pstring(&mut bus, old_name_addr, b"OldName.dat");
        write_pstring(&mut bus, new_name_addr, b"NewName.dat");

        for i in 0u32..64 {
            bus.write_byte(pb + i, 0);
        }
        bus.write_long(pb + 18, old_name_addr); // ioNamePtr
        bus.write_long(pb + 28, new_name_addr); // ioMisc — new name
        cpu.write_reg(Register::A0, pb);
        call(&mut disp, false, 0x0B, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "rename → noErr");
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
        assert!(
            !disp.vfs.contains_key("OldName.dat"),
            "old key must be gone"
        );
        assert_eq!(disp.vfs.get("NewName.dat"), Some(&vec![0xAA, 0xBBu8]));
    }

    #[test]
    fn pbhrename_path_new_name_stays_in_source_directory() {
        let (mut disp, mut cpu, mut bus) = setup();

        let dir_id = disp.ensure_vfs_directory("App Folder");
        disp.vfs
            .insert("App Folder/Prefs File__TEMP".to_string(), vec![0xAA, 0xBB]);
        disp.vfs_rsrc
            .insert("App Folder/Prefs File__TEMP".to_string(), vec![0xCC]);

        let pb = 0x271400u32;
        let old_name_addr = pb + 0x100;
        let new_name_addr = pb + 0x200;
        write_pstring(&mut bus, old_name_addr, b":App Folder:Prefs File__TEMP");
        write_pstring(&mut bus, new_name_addr, b":App Folder:Prefs File");

        for i in 0u32..64 {
            bus.write_byte(pb + i, 0);
        }
        bus.write_word(pb + 16, 0x7777);
        bus.write_long(pb + 18, old_name_addr);
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
        bus.write_long(pb + 28, new_name_addr);
        bus.write_long(pb + 48, dir_id);

        cpu.write_reg(Register::A0, pb);
        call_trap_word(&mut disp, 0xA20B, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 16) as i16, 0);
        assert!(!disp.vfs.contains_key("App Folder/Prefs File__TEMP"));
        assert_eq!(
            disp.vfs.get("App Folder/Prefs File"),
            Some(&vec![0xAA, 0xBB])
        );
        assert_eq!(
            disp.vfs_rsrc.get("App Folder/Prefs File"),
            Some(&vec![0xCC])
        );
        assert!(
            !disp.vfs.contains_key("App Folder/App Folder/Prefs File"),
            "PBHRename must rename within the original directory, not append a pathname under it"
        );

        let open_pb = 0x271800u32;
        setup_param_block(&mut bus, &mut cpu, open_pb, b"Prefs File");
        bus.write_word(
            open_pb + 22,
            super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
        );
        bus.write_long(open_pb + 48, dir_id);
        cpu.write_reg(Register::D0, 26);

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(open_pb + 16) as i16, 0);
        let refnum = bus.read_word(open_pb + 24);
        assert_eq!(
            disp.open_files.get(&refnum),
            Some(&"App Folder/Prefs File".to_string())
        );
    }

    #[test]
    fn pbsetflock_existing_file_returns_noerr_and_sets_ioflattrib_locked_bit() {
        // Inside Macintosh: Files (1992), pp. 2-89 and 2-110:
        // PBSetFLock locks a file; PBGetFInfo reports lock state via ioFlAttrib bit 0.
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("Lockable.dat".to_string(), vec![]);

        let pb = 0x272000u32;
        for i in 0u32..64 {
            bus.write_byte(pb + i, 0);
        }
        setup_param_block(&mut bus, &mut cpu, pb, b"Lockable.dat");
        bus.write_word(pb + 16, 0x7B7B); // ioResult poison
        call(&mut disp, false, 0x41, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "SetFLock noErr in D0");
        assert_eq!(
            bus.read_word(pb + 16) as i16,
            0,
            "SetFLock noErr in ioResult"
        );

        for i in 0u32..64 {
            bus.write_byte(pb + i, 0);
        }
        setup_param_block(&mut bus, &mut cpu, pb, b"Lockable.dat");
        bus.write_byte(pb + 30, 0xFF); // ioFlAttrib poison
        call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "PBGetFInfo noErr");
        assert_eq!(
            bus.read_byte(pb + 30) & 0x01,
            0x01,
            "ioFlAttrib bit 0 set after PBSetFLock"
        );
    }

    #[test]
    fn pbrstflock_existing_file_returns_noerr_and_clears_ioflattrib_locked_bit() {
        // Inside Macintosh: Files (1992), pp. 2-89 and 2-111:
        // PBRstFLock unlocks a file; PBGetFInfo reports lock state via ioFlAttrib bit 0.
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("Lockable.dat".to_string(), vec![]);

        let pb = 0x272100u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Lockable.dat");
        call(&mut disp, false, 0x41, &mut cpu, &mut bus).unwrap();

        for i in 0u32..64 {
            bus.write_byte(pb + i, 0);
        }
        setup_param_block(&mut bus, &mut cpu, pb, b"Lockable.dat");
        bus.write_word(pb + 16, 0x7777); // ioResult poison
        call(&mut disp, false, 0x42, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "RstFLock noErr in D0");
        assert_eq!(
            bus.read_word(pb + 16) as i16,
            0,
            "RstFLock noErr in ioResult"
        );

        for i in 0u32..64 {
            bus.write_byte(pb + i, 0);
        }
        setup_param_block(&mut bus, &mut cpu, pb, b"Lockable.dat");
        bus.write_byte(pb + 30, 0xFF); // ioFlAttrib poison
        call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "PBGetFInfo noErr");
        assert_eq!(
            bus.read_byte(pb + 30) & 0x01,
            0x00,
            "ioFlAttrib bit 0 clear after PBRstFLock"
        );
    }

    #[test]
    fn pbsetflock_missing_file_returns_fnferr_in_d0_and_ioresult() {
        // Inside Macintosh: Files (1992), p. 2-110:
        // PBSetFLock returns fnfErr (-43) when the target file is missing.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x272200u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"MissingLockable.dat");
        bus.write_word(pb + 16, 0x2222); // ioResult poison
        call(&mut disp, false, 0x41, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -43, "fnfErr in D0");
        assert_eq!(bus.read_word(pb + 16) as i16, -43, "fnfErr in ioResult");
    }

    #[test]
    fn pbrstflock_missing_file_returns_fnferr_in_d0_and_ioresult() {
        // Inside Macintosh: Files (1992), p. 2-111:
        // PBRstFLock returns fnfErr (-43) when the target file is missing.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x272300u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"MissingLockable.dat");
        bus.write_word(pb + 16, 0x3333); // ioResult poison
        call(&mut disp, false, 0x42, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -43, "fnfErr in D0");
        assert_eq!(bus.read_word(pb + 16) as i16, -43, "fnfErr in ioResult");
    }

    #[test]
    fn pbsetflock_pbrstflock_write_same_oserr_to_d0_and_ioresult_preserving_stack() {
        // Inside Macintosh: Files (1992), pp. 2-110 to 2-111 + Inside
        // Macintosh Volume II (1985), p. II-114.
        //
        // Pre-poisons
        // pb.ioResult @ pb+16 with 0x3FFF, calls _PBSetFLock then
        // _PBRstFLock against bogus filenames not present in the VFS,
        // and asserts the dispatcher convention (D0 == ioResult,
        // ioResult overwritten away from sentinel) plus the register-
        // only ABI (A7 preserved across both calls).
        let (mut disp, mut cpu, mut bus) = setup();

        // PBSetFLock: bogus filename → fnfErr (-43) per IM:Files; both
        // ioResult and D0 receive the same OSErr value.
        let pb = 0x272500u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"NoSuchFileForSetFLock");
        bus.write_word(pb + 16, 0x3FFF); // ioResult pre-poison sentinel
        let sp_pre_set = cpu.read_reg(Register::A7);
        call(&mut disp, false, 0x41, &mut cpu, &mut bus).unwrap();
        let sp_post_set = cpu.read_reg(Register::A7);
        let d0_set = cpu.read_reg(Register::D0) as i32;
        let io_set = bus.read_word(pb + 16) as i16 as i32;
        assert_ne!(
            io_set, 0x3FFF,
            "PBSetFLock must overwrite ioResult sentinel"
        );
        assert_eq!(
            d0_set, io_set,
            "PBSetFLock D0 must mirror ioResult per IM:II II-114"
        );
        assert_eq!(sp_pre_set, sp_post_set, "PBSetFLock must not consume stack");

        // PBRstFLock: same shape, fresh param block.
        let pb2 = 0x272600u32;
        setup_param_block(&mut bus, &mut cpu, pb2, b"NoSuchFileForRstFLock");
        bus.write_word(pb2 + 16, 0x3FFF); // ioResult pre-poison sentinel
        let sp_pre_rst = cpu.read_reg(Register::A7);
        call(&mut disp, false, 0x42, &mut cpu, &mut bus).unwrap();
        let sp_post_rst = cpu.read_reg(Register::A7);
        let d0_rst = cpu.read_reg(Register::D0) as i32;
        let io_rst = bus.read_word(pb2 + 16) as i16 as i32;
        assert_ne!(
            io_rst, 0x3FFF,
            "PBRstFLock must overwrite ioResult sentinel"
        );
        assert_eq!(
            d0_rst, io_rst,
            "PBRstFLock D0 must mirror ioResult per IM:II II-114"
        );
        assert_eq!(sp_pre_rst, sp_post_rst, "PBRstFLock must not consume stack");
    }

    #[test]
    fn pbhsetflock_pbhrstflock_write_same_oserr_to_d0_and_ioresult_preserving_stack() {
        // Inside Macintosh: Files (1992), pp. 2-196 to 2-198 + Inside
        // Macintosh Volume II (1985), p. II-114.
        //
        // Pre-poisons
        // ph.fileParam.ioResult @ pb+16 with 0x3FFF, calls _PBHSetFLock
        // then _PBHRstFLock against bogus filenames not present in the
        // VFS, and asserts the dispatcher convention (D0 == ioResult,
        // ioResult overwritten away from sentinel) plus the register-
        // only ABI (A7 preserved across both calls). $A241/$A242 share
        // the same low-byte arms (false, 0x41) and (false, 0x42) with
        // $A041/$A042 via the OS-trap dispatcher's trap & 0x00FF mask.
        let (mut disp, mut cpu, mut bus) = setup();

        // PBHSetFLock: bogus filename → fnfErr (-43) per IM:Files; both
        // ioResult and D0 receive the same OSErr value.
        let pb = 0x272700u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"NoSuchFileForHSetFLock");
        bus.write_word(pb + 16, 0x3FFF); // ioResult pre-poison sentinel
        let sp_pre_set = cpu.read_reg(Register::A7);
        call_trap_word(&mut disp, 0xA241, &mut cpu, &mut bus).unwrap();
        let sp_post_set = cpu.read_reg(Register::A7);
        let d0_set = cpu.read_reg(Register::D0) as i32;
        let io_set = bus.read_word(pb + 16) as i16 as i32;
        assert_ne!(
            io_set, 0x3FFF,
            "PBHSetFLock must overwrite ioResult sentinel"
        );
        assert_eq!(
            d0_set, io_set,
            "PBHSetFLock D0 must mirror ioResult per IM:II II-114"
        );
        assert_eq!(
            sp_pre_set, sp_post_set,
            "PBHSetFLock must not consume stack"
        );

        // PBHRstFLock: same shape, fresh param block.
        let pb2 = 0x272800u32;
        setup_param_block(&mut bus, &mut cpu, pb2, b"NoSuchFileForHRstFLock");
        bus.write_word(pb2 + 16, 0x3FFF); // ioResult pre-poison sentinel
        let sp_pre_rst = cpu.read_reg(Register::A7);
        call_trap_word(&mut disp, 0xA242, &mut cpu, &mut bus).unwrap();
        let sp_post_rst = cpu.read_reg(Register::A7);
        let d0_rst = cpu.read_reg(Register::D0) as i32;
        let io_rst = bus.read_word(pb2 + 16) as i16 as i32;
        assert_ne!(
            io_rst, 0x3FFF,
            "PBHRstFLock must overwrite ioResult sentinel"
        );
        assert_eq!(
            d0_rst, io_rst,
            "PBHRstFLock D0 must mirror ioResult per IM:II II-114"
        );
        assert_eq!(
            sp_pre_rst, sp_post_rst,
            "PBHRstFLock must not consume stack"
        );
    }

    #[test]
    fn pbhsetflock_existing_file_returns_noerr_and_sets_ioflattrib_locked_bit() {
        // Inside Macintosh: Files (1992), pp. 2-196 to 2-197:
        // PBHSetFLock is the HFS variant and trap macro _HSetFLock ($A241).
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("HLockable.dat".to_string(), vec![]);

        let pb = 0x272400u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"HLockable.dat");
        bus.write_word(pb + 16, 0x4444); // ioResult poison
        call_trap_word(&mut disp, 0xA241, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            0,
            "PBHSetFLock noErr in D0"
        );
        assert_eq!(
            bus.read_word(pb + 16) as i16,
            0,
            "PBHSetFLock noErr in ioResult"
        );

        for i in 0u32..64 {
            bus.write_byte(pb + i, 0);
        }
        setup_param_block(&mut bus, &mut cpu, pb, b"HLockable.dat");
        bus.write_byte(pb + 30, 0xFF); // ioFlAttrib poison
        call_trap_word(&mut disp, 0xA20C, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "PBHGetFInfo noErr");
        assert_eq!(
            bus.read_byte(pb + 30) & 0x01,
            0x01,
            "ioFlAttrib bit 0 set after PBHSetFLock"
        );
    }

    #[test]
    fn pbhrstflock_existing_file_returns_noerr_and_clears_ioflattrib_locked_bit() {
        // Inside Macintosh: Files (1992), pp. 2-197 to 2-198:
        // PBHRstFLock is the HFS variant and trap macro _HRstFLock ($A242).
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("HLockable.dat".to_string(), vec![]);

        let pb = 0x272500u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"HLockable.dat");
        call_trap_word(&mut disp, 0xA241, &mut cpu, &mut bus).unwrap();

        for i in 0u32..64 {
            bus.write_byte(pb + i, 0);
        }
        setup_param_block(&mut bus, &mut cpu, pb, b"HLockable.dat");
        bus.write_word(pb + 16, 0x5555); // ioResult poison
        call_trap_word(&mut disp, 0xA242, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            0,
            "PBHRstFLock noErr in D0"
        );
        assert_eq!(
            bus.read_word(pb + 16) as i16,
            0,
            "PBHRstFLock noErr in ioResult"
        );

        for i in 0u32..64 {
            bus.write_byte(pb + i, 0);
        }
        setup_param_block(&mut bus, &mut cpu, pb, b"HLockable.dat");
        bus.write_byte(pb + 30, 0xFF); // ioFlAttrib poison
        call_trap_word(&mut disp, 0xA20C, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "PBHGetFInfo noErr");
        assert_eq!(
            bus.read_byte(pb + 30) & 0x01,
            0x00,
            "ioFlAttrib bit 0 clear after PBHRstFLock"
        );
    }

    #[test]
    fn pbhsetflock_missing_file_returns_fnferr_in_d0_and_ioresult() {
        // Inside Macintosh: Files (1992), pp. 2-196 to 2-197:
        // PBHSetFLock lists fnfErr (-43) for a missing file.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x272600u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"MissingHLock.dat");
        bus.write_word(pb + 16, 0x6666); // ioResult poison
        call_trap_word(&mut disp, 0xA241, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -43, "fnfErr in D0");
        assert_eq!(bus.read_word(pb + 16) as i16, -43, "fnfErr in ioResult");
    }

    #[test]
    fn pbhrstflock_missing_file_returns_fnferr_in_d0_and_ioresult() {
        // Inside Macintosh: Files (1992), pp. 2-197 to 2-198:
        // PBHRstFLock lists fnfErr (-43) for a missing file.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x272700u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"MissingHLock.dat");
        bus.write_word(pb + 16, 0x7777); // ioResult poison
        call_trap_word(&mut disp, 0xA242, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -43, "fnfErr in D0");
        assert_eq!(bus.read_word(pb + 16) as i16, -43, "fnfErr in ioResult");
    }

    #[test]
    fn fs_dispatch_pbhopendf_retries_default_directory_for_stale_dirid() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.ensure_vfs_directory("Marathon");
        disp.vfs.insert("Marathon/Marathon".to_string(), vec![]);
        disp.vfs
            .insert("Marathon/Physics Model".to_string(), vec![0xAB]);
        disp.set_launched_app_path("Marathon/Marathon");

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Physics Model");
        bus.write_long(pb + 48, 0x01FF01FF);

        cpu.write_reg(Register::D0, 26);

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert!(bus.read_word(pb + 24) >= 100);
    }

    #[test]
    fn fs_dispatch_pbhopendf_empty_name_returns_bdnamerr_and_clears_refnum() {
        // Files 1992, 2-184 lists bdNamErr for a bad filename; the
        // result-code index clarifies that a zero-length filename is in
        // that class. PBHOpenDF must not silently succeed with a stale
        // ioRefNum when ioNamePtr points at an empty Pascal string.
        let (mut disp, mut cpu, mut bus) = setup();

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"");
        bus.write_word(pb + 16, 0x7777);
        bus.write_word(pb + 24, 1023);
        cpu.write_reg(Register::D0, 26);

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -37);
        assert_eq!(bus.read_word(pb + 16) as i16, -37);
        assert_eq!(bus.read_word(pb + 24), 0);
    }

    #[test]
    fn fsdispatch_pbhopendf_explicit_parent_does_not_open_same_basename_elsewhere() {
        // Files 1992, 2-29: poor man's search path applies only when the
        // directory ID field is 0. With an explicit parent dirID, PBHOpenDF
        // must fail instead of opening an unrelated basename elsewhere.
        let (mut disp, mut cpu, mut bus) = setup();

        disp.vfs.insert("App/App".to_string(), vec![]);
        disp.vfs
            .insert("App/Shared Preferences".to_string(), vec![0x42]);
        let pref_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Shared Preferences");
        bus.write_word(pb + 16, 0x7777);
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
        bus.write_word(pb + 24, 0x1234);
        bus.write_long(pb + 48, pref_dir_id);
        cpu.write_reg(Register::D0, 26);

        call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0) as i32, -43);
        assert_eq!(bus.read_word(pb + 16) as i16, -43);
        assert_eq!(bus.read_word(pb + 24), 0);
    }

    // OSDispatch ($A88F) selector contracts.
    // Temporary Memory: Inside Macintosh Volume VI, 28-38 and 28-45.
    // Process Manager: Processes (1994), pp. 2-21 to 2-28 and p. 2-31.
    #[test]
    fn osdispatch_tempfreemem_selector_0018_uses_stack_selector_and_returns_free_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();

        bus.write_word(TEST_SP, 0x0018);
        bus.write_long(TEST_SP + 2, 0xDEAD_BEEF);
        cpu.write_reg(Register::D0, 0x0001);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 2,
            "OSDispatch should pop the selector word"
        );
        assert!(
            cpu.read_reg(Register::D0) >= 8 * 1024 * 1024,
            "TempFreeMem should report usable temporary memory in D0"
        );
        assert_eq!(
            bus.read_long(TEST_SP + 2),
            cpu.read_reg(Register::D0),
            "TempFreeMem must write the Pascal function result slot"
        );
    }

    #[test]
    fn osdispatch_tempmaxmem_selector_0015_clears_grow_and_returns_max_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();

        let grow_ptr = 0x2A0000u32;
        bus.write_long(grow_ptr, 0xDEAD_BEEF);
        bus.write_word(TEST_SP, 0x0015);
        bus.write_long(TEST_SP + 2, grow_ptr);
        bus.write_long(TEST_SP + 6, 0xDEAD_BEEF);
        cpu.write_reg(Register::D0, 0x0001);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 6,
            "OSDispatch should pop the selector word and grow pointer"
        );
        assert_eq!(
            bus.read_long(grow_ptr),
            0,
            "TempMaxMem must write 0 to the grow parameter"
        );
        assert!(
            cpu.read_reg(Register::D0) >= 8 * 1024 * 1024,
            "TempMaxMem should report usable temporary memory in D0"
        );
        assert_eq!(
            bus.read_long(TEST_SP + 6),
            cpu.read_reg(Register::D0),
            "TempMaxMem must write the Pascal function result slot"
        );
    }

    #[test]
    fn osdispatch_temptopmem_selector_0016_returns_memtop_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();

        bus.write_long(crate::memory::globals::addr::MEM_TOP, 0x03F0_0000);
        bus.write_word(TEST_SP, 0x0016);
        bus.write_long(TEST_SP + 2, 0xDEAD_BEEF);
        cpu.write_reg(Register::D0, 0x0001);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 2,
            "OSDispatch should pop the selector word"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0x03F0_0000,
            "TempTopMem should return the top of addressable RAM in D0"
        );
        assert_eq!(
            bus.read_long(TEST_SP + 2),
            0x03F0_0000,
            "TempTopMem must write the Pascal function result slot"
        );
    }

    #[test]
    fn osdispatch_tempnewhandle_selector_001d_allocates_and_writes_result_code() {
        let (mut disp, mut cpu, mut bus) = setup();

        let err_ptr = 0x2A0100u32;
        bus.write_word(err_ptr, 0x7777);
        bus.write_word(TEST_SP, 0x001D);
        bus.write_long(TEST_SP + 2, err_ptr);
        bus.write_long(TEST_SP + 6, 64);
        bus.write_long(TEST_SP + 10, 0xDEAD_BEEF);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        let handle = bus.read_long(TEST_SP + 10);
        let ptr = bus.read_long(handle);
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 10,
            "TempNewHandle should pop selector and two arguments"
        );
        assert_ne!(handle, 0, "TempNewHandle should return a handle");
        assert_eq!(
            cpu.read_reg(Register::D0),
            handle,
            "TempNewHandle should mirror the handle in D0 for inline callers"
        );
        assert_eq!(
            bus.read_word(err_ptr),
            0,
            "TempNewHandle should write noErr to resultCode"
        );
        assert_eq!(bus.get_alloc_size(ptr), Some(64));
        assert_eq!(disp.ptr_to_handle.get(&ptr).copied(), Some(handle));
    }

    #[test]
    fn osdispatch_temphlock_unlock_and_dispose_selectors_manage_temp_handle_state() {
        let (mut disp, mut cpu, mut bus) = setup();

        let data_ptr = bus.alloc(32);
        let handle = bus.alloc(4);
        let err_ptr = 0x2A0200u32;
        bus.write_long(handle, data_ptr);

        bus.write_word(err_ptr, 0x7777);
        bus.write_word(TEST_SP, 0x001E);
        bus.write_long(TEST_SP + 2, err_ptr);
        bus.write_long(TEST_SP + 6, handle);
        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(bus.read_word(err_ptr), 0, "TempHLock resultCode");
        assert_eq!(
            disp.handle_state_bits.get(&handle).copied().unwrap_or(0) & 0x80,
            0x80,
            "TempHLock should set the lock bit"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(err_ptr, 0x7777);
        bus.write_word(TEST_SP, 0x001F);
        bus.write_long(TEST_SP + 2, err_ptr);
        bus.write_long(TEST_SP + 6, handle);
        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(bus.read_word(err_ptr), 0, "TempHUnlock resultCode");
        assert_eq!(
            disp.handle_state_bits.get(&handle).copied().unwrap_or(0) & 0x80,
            0,
            "TempHUnlock should clear the lock bit"
        );

        disp.handle_state_bits.insert(handle, 0xC0);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(err_ptr, 0x7777);
        bus.write_word(TEST_SP, 0x0020);
        bus.write_long(TEST_SP + 2, err_ptr);
        bus.write_long(TEST_SP + 6, handle);
        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(bus.read_word(err_ptr), 0, "TempDisposeHandle resultCode");
        assert_eq!(
            bus.get_alloc_size(data_ptr),
            None,
            "TempDisposeHandle should release the data block"
        );
        assert_eq!(
            bus.get_alloc_size(handle),
            None,
            "TempDisposeHandle should release the master pointer"
        );
        assert!(
            !disp.handle_state_bits.contains_key(&handle),
            "TempDisposeHandle should clear tracked state bits"
        );
    }

    #[test]
    fn osdispatch_temphlock_selector_001e_reports_nil_handle_error() {
        let (mut disp, mut cpu, mut bus) = setup();

        let err_ptr = 0x2A0300u32;
        bus.write_word(err_ptr, 0x7777);
        bus.write_word(TEST_SP, 0x001E);
        bus.write_long(TEST_SP + 2, err_ptr);
        bus.write_long(TEST_SP + 6, 0);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(
            bus.read_word(err_ptr) as i16,
            -109,
            "TempHLock should report nilHandleErr for a NIL handle"
        );
    }

    #[test]
    fn osdispatch_accepthighlevelevent_selector_0033_empty_queue_returns_nooutstandinghle() {
        let (mut disp, mut cpu, mut bus) = setup();

        let sender_ptr = 0x2A0400u32;
        let msg_refcon_ptr = 0x2A0500u32;
        let msg_buff_ptr = 0x2A0600u32;
        let msg_len_ptr = 0x2A0700u32;
        bus.write_long(sender_ptr, 0xAAAA_AAAA);
        bus.write_long(msg_refcon_ptr, 0xBBBB_BBBB);
        bus.write_long(msg_len_ptr, 128);
        bus.write_word(TEST_SP, 0x0033);
        bus.write_long(TEST_SP + 2, msg_len_ptr);
        bus.write_long(TEST_SP + 6, msg_buff_ptr);
        bus.write_long(TEST_SP + 10, msg_refcon_ptr);
        bus.write_long(TEST_SP + 14, sender_ptr);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_word(TEST_SP + 18) as i16, -608, "noOutstandingHLE");
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 18);
        assert_eq!(
            bus.read_long(sender_ptr),
            0xAAAA_AAAA,
            "sender should be untouched when no high-level event is outstanding"
        );
        assert_eq!(bus.read_long(msg_refcon_ptr), 0xBBBB_BBBB);
        assert_eq!(bus.read_long(msg_len_ptr), 128);
        assert_eq!(bus.read_byte(msg_buff_ptr), 0);
    }

    #[test]
    fn osdispatch_getspecifichighlevelevent_selector_0045_without_port_returns_false() {
        // MPW 3.5 EPPC.h emits filter, context, err pointer, then a one-byte
        // Boolean result slot around selector $0045. A direct application with
        // no registered high-level-event port follows BasiliskII's noPortErr
        // branch without invoking the filter.
        let (mut disp, mut cpu, mut bus) = setup();

        let filter_ptr = 0x0003_4000u32;
        let context_ptr = 0x2A_7000u32;
        let err_ptr = 0x2A_7100u32;
        bus.write_word(err_ptr, 0x7777);
        bus.write_word(TEST_SP, 0x0045);
        bus.write_long(TEST_SP + 2, filter_ptr);
        bus.write_long(TEST_SP + 6, context_ptr);
        bus.write_long(TEST_SP + 10, err_ptr);
        bus.write_byte(TEST_SP + 14, 0xFF);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_word(err_ptr) as i16, -903, "noPortErr");
        assert_eq!(bus.read_byte(TEST_SP + 14), 0, "FALSE result");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 14,
            "OSDispatch should pop selector and three pointer arguments"
        );
    }

    #[test]
    fn osdispatch_posthighlevelevent_selector_0034_without_delivery_returns_connectioninvalid() {
        let (mut disp, mut cpu, mut bus) = setup();

        let event_ptr = 0x2A0800u32;
        let receiver_ptr = 0x2A0900u32;
        let msg_buff_ptr = 0x2A0A00u32;
        bus.write_word(event_ptr, 23);
        bus.write_long(event_ptr + 2, 0x7465_7374);
        bus.write_word(TEST_SP, 0x0034);
        bus.write_long(TEST_SP + 2, 0x0000_8000); // receiverIDisPSN
        bus.write_long(TEST_SP + 6, 4);
        bus.write_long(TEST_SP + 10, msg_buff_ptr);
        bus.write_long(TEST_SP + 14, 0x1234_5678);
        bus.write_long(TEST_SP + 18, receiver_ptr);
        bus.write_long(TEST_SP + 22, event_ptr);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(
            bus.read_word(TEST_SP + 26) as i16,
            -609,
            "connectionInvalid"
        );
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 26);
        assert!(
            disp.event_queue.is_empty(),
            "PostHighLevelEvent should not enqueue an event it cannot later deliver"
        );
    }

    #[test]
    fn osdispatch_getprocessserialnumberfromportname_selector_0035_returns_noporterr() {
        let (mut disp, mut cpu, mut bus) = setup();

        let port_name_ptr = 0x2A0B00u32;
        let psn_ptr = 0x2A0C00u32;
        bus.write_long(psn_ptr, 0xAAAA_AAAA);
        bus.write_long(psn_ptr + 4, 0xBBBB_BBBB);
        bus.write_word(TEST_SP, 0x0035);
        bus.write_long(TEST_SP + 2, psn_ptr);
        bus.write_long(TEST_SP + 6, port_name_ptr);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_word(TEST_SP + 10) as i16, -903, "noPortErr");
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(
            bus.read_long(psn_ptr),
            0xAAAA_AAAA,
            "PSN should be untouched when the port name is unknown"
        );
        assert_eq!(bus.read_long(psn_ptr + 4), 0xBBBB_BBBB);
    }

    #[test]
    fn osdispatch_launchdeskaccessory_selector_0036_returns_resnotfound() {
        let (mut disp, mut cpu, mut bus) = setup();

        let fsspec_ptr = 0x2A0D00u32;
        let da_name_ptr = 0x2A0E00u32;
        bus.write_word(TEST_SP, 0x0036);
        bus.write_long(TEST_SP + 2, da_name_ptr);
        bus.write_long(TEST_SP + 6, fsspec_ptr);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_word(TEST_SP + 10) as i16, -192, "resNotFound");
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    }

    #[test]
    fn osdispatch_getcurrentprocess_selector_0037_returns_current_psn() {
        let (mut disp, mut cpu, mut bus) = setup();

        let psn_ptr = 0x2A0000u32;
        bus.write_word(TEST_SP, 0x0037);
        bus.write_long(TEST_SP + 2, psn_ptr);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_long(psn_ptr), 0, "highLongOfPSN");
        assert_eq!(
            bus.read_long(psn_ptr + 4),
            2,
            "lowLongOfPSN = kCurrentProcess"
        );
        assert_eq!(bus.read_word(TEST_SP + 6), 0, "noErr result");
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    }

    #[test]
    fn osdispatch_getnextprocess_selector_0038_from_knoprocess_returns_first_process() {
        let (mut disp, mut cpu, mut bus) = setup();

        let psn_ptr = 0x2A0100u32;
        bus.write_long(psn_ptr, 0);
        bus.write_long(psn_ptr + 4, 0); // kNoProcess
        bus.write_word(TEST_SP, 0x0038);
        bus.write_long(TEST_SP + 2, psn_ptr);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_long(psn_ptr), 0);
        assert_eq!(bus.read_long(psn_ptr + 4), 2, "first process PSN");
        assert_eq!(bus.read_word(TEST_SP + 6), 0, "noErr result");
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    }

    #[test]
    fn osdispatch_getnextprocess_selector_0038_end_of_list_returns_procnotfound_and_knoprocess() {
        let (mut disp, mut cpu, mut bus) = setup();

        let psn_ptr = 0x2A0200u32;
        bus.write_long(psn_ptr, 0);
        bus.write_long(psn_ptr + 4, 2); // current process PSN
        bus.write_word(TEST_SP, 0x0038);
        bus.write_long(TEST_SP + 2, psn_ptr);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_word(TEST_SP + 6) as i16, -600, "procNotFound");
        assert_eq!(bus.read_long(psn_ptr), 0);
        assert_eq!(bus.read_long(psn_ptr + 4), 0, "kNoProcess at end of list");
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    }

    #[test]
    fn osdispatch_getfrontprocess_selector_0039_returns_foreground_psn() {
        // MPW GetFrontProcess glue pushes a 4-byte init slot ($FFFFFFFF) before
        // psn_ptr. After selector pop: sp+0=init(4B), sp+4=psn_ptr(4B),
        // sp+6=result(2B). A7 advances to sp+6 so MOVE.W (SP)+,D0 restores stack.
        let (mut disp, mut cpu, mut bus) = setup();

        let psn_ptr = 0x2A0300u32;
        bus.write_word(TEST_SP, 0x0039);
        bus.write_long(TEST_SP + 2, 0xFFFFFFFF); // MPW init slot
        bus.write_long(TEST_SP + 6, psn_ptr);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_long(psn_ptr), 0);
        assert_eq!(bus.read_long(psn_ptr + 4), 2, "foreground process PSN");
        assert_eq!(bus.read_word(TEST_SP + 8), 0, "noErr result");
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
    }

    #[test]
    fn osdispatch_getprocessinformation_invalid_psn_returns_procnotfound() {
        let (mut disp, mut cpu, mut bus) = setup();

        let info_ptr = 0x2A0400u32;
        let psn_ptr = 0x2A0500u32;
        bus.write_long(psn_ptr, 0);
        bus.write_long(psn_ptr + 4, 3); // invalid in single-process HLE
        bus.write_word(TEST_SP, 0x003A);
        bus.write_long(TEST_SP + 2, info_ptr);
        bus.write_long(TEST_SP + 6, psn_ptr);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_word(TEST_SP + 10) as i16, -600, "procNotFound");
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    }

    #[test]
    fn osdispatch_getprocessinformation_current_process_returns_populated_info() {
        let (mut disp, mut cpu, mut bus) = setup();

        let info_ptr = 0x2A0900u32;
        let psn_ptr = 0x2A0A00u32;
        let name_ptr = 0x2A0B00u32;
        let app_spec_ptr = 0x2A0C00u32;
        let expected_psn_high = 0;
        let expected_psn_low = 2;

        bus.write_long(psn_ptr, expected_psn_high);
        bus.write_long(psn_ptr + 4, expected_psn_low);
        bus.write_word(info_ptr, 60);
        bus.write_long(info_ptr + 4, name_ptr);
        bus.write_long(info_ptr + 56, app_spec_ptr);
        bus.write_word(TEST_SP, 0x003A);
        bus.write_long(TEST_SP + 2, info_ptr);
        bus.write_long(TEST_SP + 6, psn_ptr);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_word(TEST_SP + 10), 0, "noErr");
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(bus.read_long(info_ptr + 8), expected_psn_high);
        assert_eq!(bus.read_long(info_ptr + 12), expected_psn_low);
        assert_eq!(bus.read_long(info_ptr + 24), 0, "processMode");
        assert_eq!(bus.read_long(info_ptr + 28), 0x0010_0000, "processLocation");
        assert_eq!(bus.read_long(info_ptr + 32), bus.ram_size(), "processSize");
        assert_eq!(
            bus.read_long(info_ptr + 36),
            bus.ram_size() / 2,
            "processFreeMem"
        );
        assert_eq!(
            bus.read_long(info_ptr + 40),
            0,
            "processLauncher.highLongOfPSN"
        );
        assert_eq!(
            bus.read_long(info_ptr + 44),
            0,
            "processLauncher.lowLongOfPSN"
        );
        assert_eq!(bus.read_long(info_ptr + 48), 0, "processLaunchDate");
        assert_ne!(
            bus.read_byte(name_ptr),
            0,
            "processName should be populated"
        );
        assert_ne!(bus.read_word(app_spec_ptr), 0, "processAppSpec.vRefNum");
    }

    #[test]
    fn osdispatch_getprocessinformation_selector_only_thunk_preserves_return_address() {
        let (mut disp, mut cpu, mut bus) = setup();

        let trap_return_pc = 0x0003_4112u32;
        let thunk_return_pc = 0x0003_2F36u32;
        let saved_caller_a6 = 0x03FF_FFECu32;
        disp.register_segments(HashMap::from([
            (1i16, 0x0003_1A00u32),
            (2i16, 0x0003_8298u32),
        ]));
        cpu.write_reg(Register::PC, trap_return_pc);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0x003A);
        bus.write_long(TEST_SP + 2, thunk_return_pc);
        bus.write_long(TEST_SP + 6, saved_caller_a6);
        bus.write_long(TEST_SP + 10, 0x1234_5678);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_word(TEST_SP), 0, "selector slot becomes noErr");
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP,
            "selector-only thunks expect MOVE.W (SP)+ to reveal the return PC"
        );
        assert_eq!(
            bus.read_long(TEST_SP + 2),
            thunk_return_pc,
            "OSDispatch must not consume or overwrite the JSR return address"
        );
        assert_eq!(bus.read_long(TEST_SP + 6), saved_caller_a6);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn osdispatch_getprocessinformation_reports_initialized_partition_and_free_heap() {
        let (mut disp, mut cpu, mut bus) = setup();

        let info_ptr = 0x2A0D00u32;
        let psn_ptr = 0x2A0E00u32;
        let app_zone = 0x0020_0000u32;
        let heap_end = app_zone + 64;
        let appl_limit = 0x0050_0000u32;
        bus.write_long(crate::memory::globals::addr::MEM_TOP, 32 * 1024 * 1024);
        bus.write_long(crate::memory::globals::addr::APP_L_ZONE, app_zone);
        bus.write_long(crate::memory::globals::addr::HEAP_END, heap_end);
        bus.write_long(crate::memory::globals::addr::APPL_LIMIT, appl_limit);
        bus.write_long(psn_ptr, 0);
        bus.write_long(psn_ptr + 4, 2);
        bus.write_word(info_ptr, 60);
        bus.write_word(TEST_SP, 0x003A);
        bus.write_long(TEST_SP + 2, info_ptr);
        bus.write_long(TEST_SP + 6, psn_ptr);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_word(TEST_SP + 10), 0, "noErr");
        assert_eq!(bus.read_long(info_ptr + 28), app_zone, "processLocation");
        assert_eq!(
            bus.read_long(info_ptr + 32),
            appl_limit - app_zone,
            "processSize"
        );
        assert_eq!(
            bus.read_long(info_ptr + 36),
            appl_limit - heap_end,
            "processFreeMem"
        );
    }

    #[test]
    fn osdispatch_sameprocess_selector_003d_writes_boolean_result() {
        let (mut disp, mut cpu, mut bus) = setup();

        let psn1_ptr = 0x2A0600u32;
        let psn2_ptr = 0x2A0700u32;
        let result_ptr = 0x2A0800u32;

        // Equal PSNs => TRUE
        bus.write_long(psn1_ptr, 0);
        bus.write_long(psn1_ptr + 4, 2);
        bus.write_long(psn2_ptr, 0);
        bus.write_long(psn2_ptr + 4, 2);
        bus.write_byte(result_ptr, 0xFF);
        bus.write_word(TEST_SP, 0x003D);
        bus.write_long(TEST_SP + 2, result_ptr);
        bus.write_long(TEST_SP + 6, psn2_ptr);
        bus.write_long(TEST_SP + 10, psn1_ptr);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_byte(result_ptr), 1, "equal PSNs => TRUE");
        assert_eq!(bus.read_word(TEST_SP + 14), 0, "noErr result");
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 14);

        // Different PSNs => FALSE
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(psn2_ptr + 4, 0);
        bus.write_byte(result_ptr, 0xFF);
        bus.write_word(TEST_SP, 0x003D);
        bus.write_long(TEST_SP + 2, result_ptr);
        bus.write_long(TEST_SP + 6, psn2_ptr);
        bus.write_long(TEST_SP + 10, psn1_ptr);

        call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_byte(result_ptr), 0, "different PSNs => FALSE");
        assert_eq!(bus.read_word(TEST_SP + 14), 0, "noErr result");
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 14);
    }
}
