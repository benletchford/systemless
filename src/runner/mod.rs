//! Fixture Runner - Loading and execution infrastructure

use crate::callback_manager::CallbackTaskArchitecture;
use crate::cpu::{M68kCpu, Register, StepResult};
use crate::debug_overlay::{DebugOverlayFrameStats, DebugOverlaySnapshot};
use crate::event_queue::{EventManagerSnapshot, EventRecordSnapshot};
use crate::execution_kernel::ExecutionRoute;
use crate::execution_m68k::M68kExecution;
use crate::execution_native::{NativeEngineRole, NativeExecution};
use crate::guest_call::ExecutionTaskId;
use crate::guest_procedure::GuestIsa;
use crate::loader::ppc::{
    PpcDrawSprocketTraceEntry, PpcFrontBuffer, PpcGWorldRecord, PpcHleImportTraceEntry,
    PpcImportBinding, PpcImportDispatcherTarget, PpcInputSnapshot,
    PpcInputSprocketSimpleStateTraceEntry, PpcLoadedApp, PpcQ3GpuFrame, PpcQ3SceneReplay,
    PpcQ3SoftwareRenderStats, PpcRgbColor, PpcSndCommandRecord, PpcSoundCompletionRecord,
    PpcSoundDoubleBackRecord, PpcSoundDoubleBufferPlaybackRecord,
};
use crate::loader::{
    decode_retro68_relocations, ApplicationSizeResource, Code0Header, CodeSegmentHeader,
    JumpTableEntry, LoadedApp, MpwFarSegmentHeader, Retro68RelocationError,
};
use crate::managers::resource::ResourceFork;
use crate::memory::GuestAddressSpace as PpcSectionMem;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::menu_model::GuestMenuSnapshot;
use crate::process_context::{ProcessContext, ProcessMemoryManager, SharedProcessFileSystem};
pub use crate::text_edit::{TextEditManagerSnapshot, TextEditSnapshot};
use crate::trap::dispatch::TrapTableProfile;
use crate::trap::TrapDispatcher;
use crate::ui_theme::{ThemeMetricsMode, UiTheme, UiThemeId};
pub use crate::window_manager::WindowSnapshot;
use crate::{Error, Result};
use m68k::BatchExit;
use ppc::{PpcException, PpcFetchHistogram, PpcMemory, PpcRunResult};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Deterministic List Manager state for fixture and diagnostic assertions.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListManagerSnapshot {
    pub view_rect: (i16, i16, i16, i16),
    pub data_bounds: (i16, i16, i16, i16),
    pub cell_size: (i16, i16),
    pub visible: (i16, i16, i16, i16),
    pub draw_enabled: bool,
    pub active: bool,
    pub cells: BTreeMap<(i16, i16), Vec<u8>>,
    pub selected: BTreeSet<(i16, i16)>,
    pub vertical_scrollbar: Option<(bool, u8)>,
    pub horizontal_scrollbar: Option<(bool, u8)>,
}

/// One resource-map entry exposed by the fixture introspection snapshot.
///
/// This deliberately reports Resource Manager semantics rather than guest
/// addresses: the Resource Browser integration test can assert stable IDs,
/// names, attributes, sizes, and whether the resource's data is resident on
/// both CPU adapters.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceManagerEntrySnapshot {
    pub res_type: [u8; 4],
    pub id: i16,
    pub name: Option<String>,
    pub attrs: u16,
    pub size: usize,
    pub loaded: bool,
}

/// Deterministic, architecture-neutral view of the current Resource Manager
/// file used by fixture tests.
///
/// The snapshot includes counts for every type in the current file and the
/// entries of the `DATA` type used by the Resource Browser fixture. It is a
/// hidden test/diagnostic seam, rather than a public Resource Manager API.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceManagerSnapshot {
    pub current_file: i16,
    pub counts: Vec<([u8; 4], usize)>,
    pub data_entries: Vec<ResourceManagerEntrySnapshot>,
}

fn resource_manager_snapshot_classic(
    dispatcher: &TrapDispatcher,
    bus: &MacMemoryBus,
) -> ResourceManagerSnapshot {
    let current_file = dispatcher.current_resource_refnum() as i16;
    let Some(resources) = dispatcher.resources.as_ref() else {
        return ResourceManagerSnapshot {
            current_file,
            counts: Vec::new(),
            data_entries: Vec::new(),
        };
    };
    let Some(file) = resources.files.get(&(current_file.max(0) as u16)) else {
        return ResourceManagerSnapshot {
            current_file,
            counts: Vec::new(),
            data_entries: Vec::new(),
        };
    };

    let mut counts = BTreeMap::new();
    for (res_type, _) in file.loaded.keys() {
        *counts.entry(*res_type).or_insert(0usize) += 1;
    }

    let mut data_entries = file
        .loaded
        .iter()
        .filter(|((res_type, _), _)| *res_type == *b"DATA")
        .map(|((res_type, id), ptr)| ResourceManagerEntrySnapshot {
            res_type: *res_type,
            id: *id,
            name: file.names_by_id.get(&(*res_type, *id)).cloned(),
            attrs: u16::from(file.attrs.get(&(*res_type, *id)).copied().unwrap_or(0)),
            size: dispatcher
                .resource_backing_data
                .get(&(current_file.max(0) as u16, *res_type, *id))
                .map_or_else(|| bus.get_alloc_size(*ptr).unwrap_or(0) as usize, Vec::len),
            loaded: *ptr != 0,
        })
        .collect::<Vec<_>>();
    data_entries.sort_by_key(|entry| entry.id);

    ResourceManagerSnapshot {
        current_file,
        counts: counts.into_iter().collect(),
        data_entries,
    }
}

fn resource_manager_snapshot_powerpc(
    dispatcher: &TrapDispatcher,
    ppc_app: &mut PpcLoadedApp,
) -> ResourceManagerSnapshot {
    let current_file = ppc_app.current_resource_refnum();
    let app_path = ppc_app.launched_app_path().map(str::to_owned);
    let include_record = |resource: &&crate::process_context::ProcessVfsResourceRecord| {
        resource.ref_num == current_file
            && app_path
                .as_deref()
                .is_none_or(|path| resource.path.eq_ignore_ascii_case(path))
    };

    let mut counts = BTreeMap::new();
    for resource in dispatcher.vfs_resources.iter().filter(include_record) {
        *counts
            .entry(resource.res_type.to_be_bytes())
            .or_insert(0usize) += 1;
    }

    let mut data_entries = dispatcher
        .vfs_resources
        .iter()
        .filter(include_record)
        .filter(|resource| resource.res_type == u32::from_be_bytes(*b"DATA"))
        .map(|resource| ResourceManagerEntrySnapshot {
            res_type: resource.res_type.to_be_bytes(),
            id: resource.res_id,
            name: (!resource.name.is_empty())
                .then(|| String::from_utf8_lossy(&resource.name).into_owned()),
            attrs: resource.attrs,
            size: resource.data.len(),
            loaded: resource.handle != 0
                && ppc_app
                    .memory
                    .read_u32_be(resource.handle)
                    .is_some_and(|ptr| ptr != 0),
        })
        .collect::<Vec<_>>();
    data_entries.sort_by_key(|entry| entry.id);

    ResourceManagerSnapshot {
        current_file,
        counts: counts.into_iter().collect(),
        data_entries,
    }
}

pub mod audio;
pub mod idle;
pub mod interrupt;
pub mod ppc_exec;
pub mod vfs;

#[cfg(feature = "debug")]
mod debug_support;
#[cfg(not(feature = "debug"))]
mod debug_support_disabled;

pub(crate) use idle::*;
pub(crate) use interrupt::*;
pub(crate) use ppc_exec::*;
pub use vfs::*;

// Cache env-var lookups (per-call syscall otherwise).
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::OnceLock;
static TRACE_DIALOG_FILTER: OnceLock<bool> = OnceLock::new();
static TRACE_TIMER: OnceLock<bool> = OnceLock::new();
static TRACE_VBL: OnceLock<bool> = OnceLock::new();
static TRACE_SOUND_RUNNER: OnceLock<bool> = OnceLock::new();
static TRACE_DIALOG_PROCS: OnceLock<bool> = OnceLock::new();

fn ppc_run_result_cycles(result: PpcRunResult) -> u64 {
    match result {
        PpcRunResult::CycleLimit { cycles }
        | PpcRunResult::Halted { cycles, .. }
        | PpcRunResult::Unimplemented { cycles, .. }
        | PpcRunResult::MemoryFault { cycles, .. }
        | PpcRunResult::Exception { cycles, .. }
        | PpcRunResult::FetchFault { cycles, .. } => cycles,
    }
}

fn ppc_halted_by_exit_to_shell(
    imports: &[PpcImportBinding],
    result: PpcRunResult,
    last_import_index: Option<u32>,
    unsupported_import_index: Option<u32>,
) -> bool {
    unsupported_import_index.is_none()
        && matches!(result, PpcRunResult::Halted { .. })
        && last_import_index.is_some_and(|symbol_index| {
            imports
                .iter()
                .find(|binding| binding.symbol_index == symbol_index)
                .is_some_and(|binding| {
                    binding.dispatcher_target == PpcImportDispatcherTarget::ExitToShell
                })
        })
}

fn ppc_resource_sidecar_parent(
    root: &std::path::Path,
    normalized_path: &str,
) -> Option<(std::path::PathBuf, std::ffi::OsString)> {
    let host_path = root.join(normalized_path);
    Some((
        host_path.parent()?.to_path_buf(),
        host_path.file_name()?.to_os_string(),
    ))
}

const APP_HEAP_FLOOR: u32 = 0x0020_0000;
const APP_ZONE_HEADER_SIZE: u32 = 64;
const APP_STACK_SAFETY_MARGIN: u32 = 0x2000;
const DEFAULT_LOAD_ADDRESS: u32 = 0x0001_0000;
/// Process-owned low memory and classic compatibility RAM visible to both
/// execution engines in a native launch. The native loader reserves exactly
/// this range for low-memory globals plus 68K callback code/data; the runner
/// replaces that detached staging copy with its canonical Mac RAM allocation
/// before either CPU executes.
const PROCESS_LOW_MEMORY_SIZE: u32 = 0x0010_0000;
const APP_QD_GLOBALS_RESERVE: u32 = 48 * 1024;
const APP_LOADER_CLEAR_RESERVE: u32 = 0x40000;
const APP_HIGH_MEMORY_RESERVE: u32 = 2 * 1024 * 1024;
const APPLICATION_RESOURCE_REFNUM: u16 = 2;
const HFS_FCB_SIZE: u16 = 94;
const HFS_FCB_BUFFER_SIZE: u16 = 2 + HFS_FCB_SIZE;
const HFS_VCB_SIZE: u32 = 178;
const PPC_SOUND_COMPLETION_CALLBACK_MAX_CYCLES: u64 = 250_000;
// Doubleback routines may transform and refill a whole buffer, unlike a
// completion notification. Keep a bounded watchdog with room for software
// mixers to finish their sample loops. Inside Macintosh: Sound (1994),
// pp. 2-68–2-73, 2-146–2-148.
const PPC_SOUND_DOUBLEBACK_CALLBACK_MAX_CYCLES: u64 = 5_000_000;

/// Result of charging one native execution slice against the runner's host
/// cycle clock.
///
/// The guest can write low-memory `Ticks` while the slice is running.  That
/// write is the baseline for the host's next VBL, so the callback phase must
/// use this result rather than subtracting the post-slice guest scalar from a
/// pre-slice snapshot.  The latter would turn an intentional guest rewind (or
/// jump) into an enormous callback interval.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PpcCycleAdvanceResult {
    /// Guest-owned `Ticks` observed immediately before host VBL advancement.
    baseline_tick: u32,
    /// Number of VBL boundaries crossed by the host cycle budget.
    elapsed_ticks: u32,
}

fn trace_dialog_filter_enabled() -> bool {
    *TRACE_DIALOG_FILTER
        .get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DIALOG_FILTER").is_some())
}

fn trace_timer_enabled() -> bool {
    *TRACE_TIMER.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_TIMER").is_some())
}

fn trace_vbl_enabled() -> bool {
    *TRACE_VBL.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_VBL").is_some())
}

fn trace_sound_runner_enabled() -> bool {
    *TRACE_SOUND_RUNNER.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_SOUND").is_some())
}

fn trace_dialog_procs_enabled() -> bool {
    *TRACE_DIALOG_PROCS.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DIALOG_PROCS").is_some())
}

// Gate the per-instruction trace_buffer behind an env var. The buffer
// is populated on EVERY instruction fetch and only used from
// `dump_trace()` on halt/crash. Default-disabled saves per-instruction
// `VecDeque` pop_front + push_back + an extra `bus.read_word` + 6
// register reads. Enable with `SYSTEMLESS_TRACE_BUFFER=1` when diagnosing
// a crash.
#[cfg(not(target_arch = "wasm32"))]
static TRACE_BUFFER_ENABLED: OnceLock<bool> = OnceLock::new();
fn trace_buffer_enabled() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        return false;
    }
    #[cfg(not(target_arch = "wasm32"))]
    *TRACE_BUFFER_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_BUFFER").is_some())
}

#[cfg(not(target_arch = "wasm32"))]
static TRACE_PC_RANGE: OnceLock<Option<(u32, u32)>> = OnceLock::new();
#[cfg(not(target_arch = "wasm32"))]
static TRACE_PC_RANGE_TICKS: OnceLock<Option<(Option<u32>, Option<u32>)>> = OnceLock::new();
#[cfg(not(target_arch = "wasm32"))]
fn trace_pc_range() -> Option<(u32, u32)> {
    *TRACE_PC_RANGE.get_or_init(|| {
        let value = std::env::var("SYSTEMLESS_TRACE_PC_RANGE").ok()?;
        let mut parts = value.split(':');
        let start = parts.next()?.trim();
        let end = parts.next()?.trim();
        let parse = |s: &str| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok();
        Some((parse(start)?, parse(end)?))
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn trace_pc_range_ticks() -> Option<(Option<u32>, Option<u32>)> {
    *TRACE_PC_RANGE_TICKS.get_or_init(|| {
        let min = std::env::var("SYSTEMLESS_TRACE_PC_RANGE_TICK_MIN")
            .ok()
            .and_then(|v| v.parse::<u32>().ok());
        let max = std::env::var("SYSTEMLESS_TRACE_PC_RANGE_TICK_MAX")
            .ok()
            .and_then(|v| v.parse::<u32>().ok());
        (min.is_some() || max.is_some()).then_some((min, max))
    })
}

fn trace_pc_range_contains(pc: u32, tick: u32) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (pc, tick);
        return false;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let in_range = trace_pc_range()
            .map(|(start, end)| pc >= start && pc <= end)
            .unwrap_or(false);
        if !in_range {
            return false;
        }
        trace_pc_range_ticks()
            .map(|(min, max)| {
                min.map(|min| tick >= min).unwrap_or(true)
                    && max.map(|max| tick <= max).unwrap_or(true)
            })
            .unwrap_or(true)
    }
}

// Gate the most-prominent startup/load chatter behind an env var.
// Library consumers shouldn't see arbitrary debug stderr output by
// default — these prints are useful when bring-up debugging a new game
// but pure noise once the loader works. Enable with
// `SYSTEMLESS_TRACE_LOAD=1` when diagnosing a load/halt.
static TRACE_LOAD_ENABLED: OnceLock<bool> = OnceLock::new();
pub(crate) fn trace_load_enabled() -> bool {
    *TRACE_LOAD_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_LOAD").is_some())
}

// Per-opcode M68K histogram, opt-in via
// `SYSTEMLESS_TRACE_OPCODE_COUNTS=1`. Complements the trap histogram
// (which only sees A-line traps). Populated in `run_steps_internal`
// after each step succeeds. Use to prioritize decode-table / super-
// instruction-fusion work — the instruction mix is the input to that
// kind of optimization.
#[cfg(not(target_arch = "wasm32"))]
static TRACE_OPCODE_COUNTS: OnceLock<bool> = OnceLock::new();
fn trace_opcode_counts_enabled() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        return false;
    }
    #[cfg(not(target_arch = "wasm32"))]
    *TRACE_OPCODE_COUNTS
        .get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_OPCODE_COUNTS").is_some())
}

static TRACE_PPC_FETCH_COUNTS: OnceLock<bool> = OnceLock::new();
fn trace_ppc_fetch_counts_enabled() -> bool {
    *TRACE_PPC_FETCH_COUNTS
        .get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_PPC_FETCH_COUNTS").is_some())
}

static INPUT_TICK_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
fn input_tick_trace_enabled() -> bool {
    *INPUT_TICK_TRACE_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_INPUT_TICK_TRACE").is_some())
}

static PPC_IMPORT_HIST_ENABLED: OnceLock<bool> = OnceLock::new();
fn ppc_import_hist_enabled() -> bool {
    *PPC_IMPORT_HIST_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_IMPORT_HIST").is_some())
}

static PPC_UNIMPL_HIST_ENABLED: OnceLock<bool> = OnceLock::new();
fn ppc_unimpl_hist_enabled() -> bool {
    *PPC_UNIMPL_HIST_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_UNIMPL_HIST").is_some())
}

static PPC_PROFILE_ENABLED: OnceLock<bool> = OnceLock::new();
fn ppc_profile_enabled() -> bool {
    *PPC_PROFILE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_PROFILE").is_some())
}

static PPC_GWORLD_DUMP_ENABLED: OnceLock<bool> = OnceLock::new();
fn ppc_gworld_dump_enabled() -> bool {
    *PPC_GWORLD_DUMP_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_GWORLD_DUMP").is_some())
}

static QD3D_DUMP_FRAME: OnceLock<Option<usize>> = OnceLock::new();
fn qd3d_dump_frame_index() -> Option<usize> {
    *QD3D_DUMP_FRAME.get_or_init(|| {
        let value = std::env::var_os("SYSTEMLESS_QD3D_DUMP_FRAME")?;
        parse_qd3d_dump_frame_value(&value)
    })
}

static QD3D_DUMP_REPLAY_DIR: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
fn qd3d_dump_replay_dir() -> Option<&'static std::path::Path> {
    QD3D_DUMP_REPLAY_DIR
        .get_or_init(|| {
            let value = std::env::var_os("SYSTEMLESS_QD3D_DUMP_REPLAY_DIR")?;
            if value.is_empty() {
                return None;
            }
            Some(std::path::PathBuf::from(value))
        })
        .as_deref()
}

fn parse_qd3d_dump_frame_value(value: &std::ffi::OsStr) -> Option<usize> {
    let value = value.to_str()?.trim();
    if value.is_empty() {
        return None;
    }
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map_or_else(
            || value.parse().ok(),
            |hex| usize::from_str_radix(hex, 16).ok(),
        )
}

fn format_qd3d_frame_dump(frame_index: usize, replay: &PpcQ3SceneReplay) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let memory_bytes: usize = replay
        .memory_regions
        .iter()
        .map(|region| region.data.len())
        .sum();
    let _ = writeln!(
        out,
        "[QD3D-FRAME] frame={} commands={} memory_regions={} memory_bytes={}",
        frame_index,
        replay.commands.len(),
        replay.memory_regions.len(),
        memory_bytes
    );
    for (region_index, region) in replay.memory_regions.iter().enumerate() {
        let _ = writeln!(
            out,
            "[QD3D-FRAME] frame={} memory={} base=${:08X} bytes={} checksum=${:08X}",
            frame_index,
            region_index,
            region.base_addr,
            region.data.len(),
            qd3d_frame_dump_checksum(&region.data)
        );
    }
    for (command_index, command) in replay.commands.iter().enumerate() {
        let _ = writeln!(
            out,
            "[QD3D-FRAME] frame={} command={} {:#?}",
            frame_index, command_index, command
        );
    }
    out
}

fn qd3d_frame_replay_dump_path(
    output_dir: &std::path::Path,
    frame_index: usize,
) -> std::path::PathBuf {
    output_dir.join(format!("q3_frame_{frame_index:06}_replay.json"))
}

fn write_qd3d_frame_replay_dump(
    output_dir: &std::path::Path,
    frame_index: usize,
    replay: &PpcQ3SceneReplay,
) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(output_dir)?;
    let path = qd3d_frame_replay_dump_path(output_dir, frame_index);
    let json = replay
        .to_json_pretty()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    std::fs::write(&path, json)?;
    Ok(path)
}

fn qd3d_frame_dump_checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5u32, |hash, byte| {
        hash.wrapping_mul(0x0100_0193) ^ u32::from(*byte)
    })
}

fn format_input_key_map(key_map: &[u8; 16]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(32);
    for byte in key_map {
        let _ = write!(out, "{:02X}", byte);
    }
    out
}

fn format_oracle_hex32(value: u32) -> String {
    format!("{value:08X}")
}

fn format_oracle_optional_hex32(value: Option<u32>) -> String {
    value.map_or_else(|| "none".to_string(), format_oracle_hex32)
}

fn format_oracle_optional_u32(value: Option<u32>) -> String {
    value.map_or_else(|| "none".to_string(), |value| value.to_string())
}

fn format_oracle_optional_u16(value: Option<u16>) -> String {
    value.map_or_else(|| "none".to_string(), |value| value.to_string())
}

fn format_oracle_optional_bool(value: Option<bool>) -> String {
    value.map_or_else(|| "none".to_string(), |value| value.to_string())
}

fn format_input_tick_trace(
    tick: u32,
    total_instructions: u64,
    input: PpcInputSnapshot,
    event_queue_len: usize,
    mb_state: u8,
) -> String {
    format!(
        "[INPUT-TICK] tick={} total_instructions={} mouse_button={} mouse_v={} mouse_h={} key_map={} event_queue={} mb_state=${:02X}",
        tick,
        total_instructions,
        input.mouse_button,
        input.mouse_v,
        input.mouse_h,
        format_input_key_map(&input.key_map),
        event_queue_len,
        mb_state
    )
}

fn format_optional_profile_u32(value: Option<u32>) -> String {
    value.map_or_else(|| "none".to_string(), |value| value.to_string())
}

fn format_profile_hex32(value: u32) -> String {
    format!("${value:08X}")
}

fn format_ppc_profile_sample(sample: PpcProfileSample) -> String {
    format!(
        "[PPC-PROFILE] max_steps={} cycles={} total_instructions={} tick={} screen_events={} \
         pc={} lr={} current_gworld={} imports={} last_import={} unsupported_import={} q3_frames={} q3_frame_start={} \
         q3_frame_end={} q3_commands={} q3_vertices={} q3_triangles={} q3_pixels={} \
         dsp_front_gworld={} dsp_back_gworld={} run_us={} render_us={} sync_us={} total_us={}",
        sample.max_steps,
        sample.cycles,
        sample.total_instructions,
        sample.tick,
        sample.screen_events,
        format_profile_hex32(sample.pc),
        format_profile_hex32(sample.lr),
        format_profile_hex32(sample.current_gworld),
        sample.handled_import_count,
        format_optional_profile_u32(sample.last_import_index),
        format_optional_profile_u32(sample.unsupported_import_index),
        sample.q3_frames,
        sample.q3_frame_start,
        sample.q3_frame_end,
        sample.q3_commands,
        sample.q3_vertices,
        sample.q3_triangles,
        sample.q3_pixels,
        format_profile_hex32(sample.dsp_front_gworld),
        format_profile_hex32(sample.dsp_back_gworld),
        sample.run_us,
        sample.render_us,
        sample.sync_us,
        sample.total_us
    )
}

fn elapsed_profile_micros(start: Option<Instant>) -> u128 {
    start.map_or(0, |start| start.elapsed().as_micros())
}

fn merge_ppc_import_histogram(
    histogram: &mut HashMap<(String, String), u64>,
    trace: &[PpcHleImportTraceEntry],
) {
    for entry in trace {
        let key = (entry.library_name.clone(), entry.symbol_name.clone());
        *histogram.entry(key).or_insert(0) += entry.repeat_count;
    }
}

fn ppc_import_trace_runs(trace: &[PpcHleImportTraceEntry]) -> Vec<(&PpcHleImportTraceEntry, u64)> {
    let mut runs = Vec::new();
    let mut iter = trace.iter();
    let Some(mut current) = iter.next() else {
        return runs;
    };
    let mut count = current.repeat_count;
    for entry in iter {
        if entry.library_name == current.library_name && entry.symbol_name == current.symbol_name {
            count = count.saturating_add(entry.repeat_count);
        } else {
            runs.push((current, count));
            current = entry;
            count = entry.repeat_count;
        }
    }
    runs.push((current, count));
    runs
}

fn ppc_import_trace_run_eq(
    left: &(&PpcHleImportTraceEntry, u64),
    right: &(&PpcHleImportTraceEntry, u64),
) -> bool {
    left.1 == right.1
        && left.0.library_name == right.0.library_name
        && left.0.symbol_name == right.0.symbol_name
}

fn ppc_import_trace_blocks_equal(
    runs: &[(&PpcHleImportTraceEntry, u64)],
    left: usize,
    right: usize,
    len: usize,
) -> bool {
    (0..len).all(|offset| ppc_import_trace_run_eq(&runs[left + offset], &runs[right + offset]))
}

fn ppc_import_trace_repeated_block_len(
    runs: &[(&PpcHleImportTraceEntry, u64)],
    start: usize,
) -> Option<(usize, u64)> {
    const MAX_BLOCK_LEN: usize = 128;
    let remaining = runs.len().saturating_sub(start);
    let max_block_len = MAX_BLOCK_LEN.min(remaining / 2);
    let mut best: Option<(usize, u64, usize)> = None;
    for block_len in 2..=max_block_len {
        if !ppc_import_trace_blocks_equal(runs, start, start + block_len, block_len) {
            continue;
        }
        let mut repeats = 2u64;
        let mut next = start + block_len * 2;
        while next + block_len <= runs.len()
            && ppc_import_trace_blocks_equal(runs, start, next, block_len)
        {
            repeats = repeats.saturating_add(1);
            next += block_len;
        }
        let saved_rows = block_len
            .saturating_mul(usize::try_from(repeats).unwrap_or(usize::MAX))
            .saturating_sub(1);
        if saved_rows >= block_len.saturating_mul(2)
            && best
                .as_ref()
                .is_none_or(|(_, _, best_saved)| saved_rows > *best_saved)
        {
            best = Some((block_len, repeats, saved_rows));
        }
    }
    best.map(|(block_len, repeats, _)| (block_len, repeats))
}

fn format_ppc_import_histogram(histogram: &HashMap<(String, String), u64>, top_n: usize) -> String {
    use std::fmt::Write as _;

    let mut entries: Vec<((String, String), u64)> = histogram
        .iter()
        .map(|((library, symbol), &count)| ((library.clone(), symbol.clone()), count))
        .collect();
    entries.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0 .0.cmp(&right.0 .0))
            .then_with(|| left.0 .1.cmp(&right.0 .1))
    });
    let total: u64 = entries.iter().map(|(_, count)| *count).sum();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "[PPC-IMPORT-HIST] top {} of {} imports ({} total import calls)",
        top_n.min(entries.len()),
        entries.len(),
        total
    );
    for ((library, symbol), count) in entries.iter().take(top_n) {
        let _ = writeln!(
            out,
            "[PPC-IMPORT-HIST]   {:>10}  {}:{}",
            count, library, symbol
        );
    }
    out
}

fn format_ppc_gworld_sample(
    memory: &mut PpcSectionMem,
    record: PpcGWorldRecord,
    point: (u32, u32),
) -> String {
    let (x, y) = point;
    if x >= record.width || y >= record.height {
        return "out".to_string();
    }
    let Some(row_base) = record
        .base_addr
        .checked_add(y.saturating_mul(record.row_bytes))
    else {
        return "bad".to_string();
    };
    match record.depth {
        8 => row_base
            .checked_add(x)
            .and_then(|addr| memory.read_u8(addr))
            .map_or_else(|| "unmapped".to_string(), |value| format!("${value:02X}")),
        16 => row_base
            .checked_add(x.saturating_mul(2))
            .and_then(|addr| memory.read_u16_be(addr))
            .map_or_else(|| "unmapped".to_string(), |value| format!("${value:04X}")),
        _ => "unsupported-depth".to_string(),
    }
}

fn format_ppc_gworld_dump(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    state: PpcGWorldDumpState,
    top_n: usize,
) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "[PPC-GWORLD-DUMP] {} gworlds current=${:08X} front=${:08X} back=${:08X} swap_count={}",
        gworlds.len(),
        state.current_gworld,
        state.front_gworld,
        state.back_gworld,
        state.swap_count
    );

    for record in gworlds.iter().copied() {
        let mut bytes = Vec::new();
        let mut checksum = 0;
        let mut nonzero_units = 0u64;
        let mut value_counts: BTreeMap<u32, u64> = BTreeMap::new();
        let row_len = match record.depth {
            8 => record.width.min(record.row_bytes),
            16 => record.width.saturating_mul(2).min(record.row_bytes),
            _ => record.row_bytes,
        };
        if row_len > 0 && record.height > 0 && row_len <= record.row_bytes && row_len <= 64 * 1024 {
            let mut row = vec![0u8; row_len as usize];
            for y in 0..record.height {
                let Some(row_addr) = record
                    .base_addr
                    .checked_add(y.saturating_mul(record.row_bytes))
                else {
                    continue;
                };
                if memory.read_bytes_into(row_addr, &mut row).is_none() {
                    continue;
                }
                bytes.extend_from_slice(&row);
                match record.depth {
                    8 => {
                        for value in &row {
                            if *value != 0 {
                                nonzero_units = nonzero_units.saturating_add(1);
                            }
                            *value_counts.entry(u32::from(*value)).or_insert(0) += 1;
                        }
                    }
                    16 => {
                        for chunk in row.chunks_exact(2) {
                            let value = u16::from_be_bytes([chunk[0], chunk[1]]);
                            if value != 0 {
                                nonzero_units = nonzero_units.saturating_add(1);
                            }
                            *value_counts.entry(u32::from(value)).or_insert(0) += 1;
                        }
                    }
                    _ => {
                        for value in &row {
                            if *value != 0 {
                                nonzero_units = nonzero_units.saturating_add(1);
                            }
                            *value_counts.entry(u32::from(*value)).or_insert(0) += 1;
                        }
                    }
                }
            }
            checksum = qd3d_frame_dump_checksum(&bytes);
        }

        let mut roles = Vec::new();
        if record.port == state.current_gworld {
            roles.push("current");
        }
        if record.port == state.front_gworld {
            roles.push("front");
        }
        if record.port == state.back_gworld {
            roles.push("back");
        }
        let role = if roles.is_empty() {
            "none".to_string()
        } else {
            roles.join(",")
        };
        let unique_units = value_counts.len();
        let mut top_values: Vec<(u32, u64)> = value_counts.into_iter().collect();
        top_values.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        let value_width = if record.depth == 16 { 4 } else { 2 };
        let top_values = top_values
            .iter()
            .take(top_n)
            .map(|(value, count)| format!("${value:0width$X}:{count}", width = value_width))
            .collect::<Vec<_>>()
            .join(",");
        let center = (record.width / 2, record.height / 2);
        let bottom_right = (
            record.width.saturating_sub(1),
            record.height.saturating_sub(1),
        );
        let _ = writeln!(
            out,
            "[PPC-GWORLD-DUMP] port=${:08X} role={} base=${:08X} row_bytes={} size={}x{}x{} bytes={} checksum=${:08X} nonzero_units={} unique_units={} top={} samples=(0,0):{},center:{},last:{}",
            record.port,
            role,
            record.base_addr,
            record.row_bytes,
            record.width,
            record.height,
            record.depth,
            bytes.len(),
            checksum,
            nonzero_units,
            unique_units,
            top_values,
            format_ppc_gworld_sample(memory, record, (0, 0)),
            format_ppc_gworld_sample(memory, record, center),
            format_ppc_gworld_sample(memory, record, bottom_right)
        );
    }
    out
}

fn merge_ppc_unimpl_histogram(histogram: &mut HashMap<String, u64>, key: String) {
    *histogram.entry(key).or_insert(0) += 1;
}

fn ppc_unimpl_histogram_key(
    imports: &[PpcImportBinding],
    result: PpcRunResult,
    unsupported_import_index: Option<u32>,
) -> Option<String> {
    if let Some(index) = unsupported_import_index {
        return Some(
            imports
                .iter()
                .find(|binding| binding.symbol_index == index)
                .map_or_else(
                    || format!("import #{} <unknown>", index),
                    |binding| {
                        format!(
                            "import #{} {}:{}",
                            index, binding.library_name, binding.symbol_name
                        )
                    },
                ),
        );
    }

    match result {
        PpcRunResult::Unimplemented { pc, error, .. } => {
            Some(format!("instruction pc=${:08X} {:?}", pc, error))
        }
        PpcRunResult::Exception {
            pc,
            exception: PpcException::IllegalInstruction { word, reason },
            ..
        } => Some(format!(
            "illegal-instruction pc=${:08X} word=${:08X} {:?}",
            pc, word, reason
        )),
        _ => None,
    }
}

fn format_ppc_unimpl_histogram(histogram: &HashMap<String, u64>, top_n: usize) -> String {
    use std::fmt::Write as _;

    let mut entries: Vec<(String, u64)> = histogram
        .iter()
        .map(|(key, &count)| (key.clone(), count))
        .collect();
    entries.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let total: u64 = entries.iter().map(|(_, count)| *count).sum();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "[PPC-UNIMPL-HIST] top {} of {} unsupported stops ({} total)",
        top_n.min(entries.len()),
        entries.len(),
        total
    );
    for (key, count) in entries.iter().take(top_n) {
        let _ = writeln!(out, "[PPC-UNIMPL-HIST]   {:>10}  {}", count, key);
    }
    out
}

// Sampled PC histogram, opt-in via `SYSTEMLESS_TRACE_HOT_PC=1`. Every
// 1000th step's PC increments a `HashMap` entry. Use to locate the
// code address of a hot game loop. 1/1000 sampling keeps `HashMap`
// overhead negligible while still giving high-confidence attribution
// for any loop that takes more than ~0.1% of runtime.
const PC_SAMPLE_INTERVAL: u64 = 1000;

/// Upper bound on instructions handed to `m68k::run_batch` per call.
///
/// The run loop's per-iteration work (sound-callback polling, wait/delay
/// service, PC validity checks) now runs once per batch instead of once
/// per instruction, so this bounds the latency of those checks. 8192
/// instructions is tens of microseconds at batch execution speeds —
/// far below a guest tick (12k instructions) — while amortising the
/// loop overhead to well under 0.1%.
const BATCH_CHUNK: usize = 8192;

#[cfg(not(target_arch = "wasm32"))]
fn trace_pc_range_active() -> bool {
    trace_pc_range().is_some()
}

#[cfg(target_arch = "wasm32")]
fn trace_pc_range_active() -> bool {
    false
}

/// True when any opt-in tracer needs to observe every instruction
/// boundary. The run loop then falls back to single-instruction batches
/// so per-step diagnostics (trace buffer, watchpoints, histograms,
/// PC-range dumps) behave exactly as they did before batching. All the
/// gates are `OnceLock`-cached env-var reads, so this costs a handful of
/// branches per batch.
fn per_instruction_diagnostics_active() -> bool {
    #[cfg(debug_assertions)]
    {
        if crate::memory::bus::watchpoint_armed() {
            return true;
        }
    }
    trace_buffer_enabled()
        || trace_timer_enabled()
        || trace_opcode_counts_enabled()
        || trace_hot_pc_enabled()
        || trace_pc_range_active()
        || crate::memory::bus::fb_write_trace_active()
        || crate::memory::bus::mem_read_trace_active()
        || crate::memory::bus::mem_write_trace_active()
}
#[cfg(not(target_arch = "wasm32"))]
static TRACE_HOT_PC: OnceLock<bool> = OnceLock::new();
fn trace_hot_pc_enabled() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        return false;
    }
    #[cfg(not(target_arch = "wasm32"))]
    *TRACE_HOT_PC.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_HOT_PC").is_some())
}

// Generic TickCount spin-wait fast-forward. Both headless and GUI callers use
// it by default. GUI execution supplies a per-frame tick cap, so a detected
// delay loop can advance only to the current VBL boundary; it cannot batch
// visible animation ticks ahead of the host. Two env vars override:
//   SYSTEMLESS_SPIN_WAIT_FASTFWD=1     force on (any mode)
//   SYSTEMLESS_DISABLE_SPIN_FASTFWD=1  force off (any mode)
static SPIN_WAIT_FASTFWD_FORCE_ON: OnceLock<bool> = OnceLock::new();
static SPIN_WAIT_FASTFWD_FORCE_OFF: OnceLock<bool> = OnceLock::new();

// Wait-elision diagnosis counters (SYSTEMLESS_WAIT_STATS=1): why exact
// idle-cycle proofs fail. Zero cost when the env is unset beyond one
// cached boolean test per event.
static WAIT_STATS_ON: OnceLock<bool> = OnceLock::new();
fn wait_stats_enabled() -> bool {
    *WAIT_STATS_ON.get_or_init(|| std::env::var_os("SYSTEMLESS_WAIT_STATS").is_some())
}
static WS_ANCHOR_CALLS: AtomicU64 = AtomicU64::new(0);
static WS_PROBE_STARTS: AtomicU64 = AtomicU64::new(0);
static WS_EXACT_REPEATS: AtomicU64 = AtomicU64::new(0);
static WS_FAIL_TICK: AtomicU64 = AtomicU64::new(0);
static WS_FAIL_CPU: AtomicU64 = AtomicU64::new(0);
static WS_FAIL_MEM: AtomicU64 = AtomicU64::new(0);
static WS_CANCEL_TRAP: AtomicU64 = AtomicU64::new(0);
static WS_PARKED: AtomicU64 = AtomicU64::new(0);
static WS_PERIOD_STEPS: AtomicU64 = AtomicU64::new(0);
static WS_PROBE_OVERFLOWS: AtomicU64 = AtomicU64::new(0);
static WS_RESUMED: AtomicU64 = AtomicU64::new(0);
static WS_RESUME_FAIL_MEM: AtomicU64 = AtomicU64::new(0);
static WS_RESUME_FAIL_HOST: AtomicU64 = AtomicU64::new(0);
static WS_RESUME_FAIL_OTHER: AtomicU64 = AtomicU64::new(0);
static WS_CANCEL_BACKOFFS: AtomicU64 = AtomicU64::new(0);
static WS_BACKOFF_SKIPS: AtomicU64 = AtomicU64::new(0);
static WS_CANCEL_TRAP_WORDS: std::sync::Mutex<Option<std::collections::BTreeMap<u16, u64>>> =
    std::sync::Mutex::new(None);
static WS_CANCEL_AB1D_SELECTORS: std::sync::Mutex<Option<std::collections::BTreeMap<u32, u64>>> =
    std::sync::Mutex::new(None);

fn ws_note_cancel_trap(opcode: u16) {
    if !wait_stats_enabled() {
        return;
    }
    WS_CANCEL_TRAP.fetch_add(1, AtomicOrdering::Relaxed);
    if let Ok(mut guard) = WS_CANCEL_TRAP_WORDS.lock() {
        *guard
            .get_or_insert_with(Default::default)
            .entry(opcode)
            .or_insert(0) += 1;
    }
}

/// QDExtensions multiplexes many routines through one trap word; a
/// cancel count against $AB1D alone cannot name the selector that needs
/// vetting. Record live D0 at the pre-dispatch cancel site.
fn ws_note_cancel_ab1d_selector(selector: u32) {
    if !wait_stats_enabled() {
        return;
    }
    if let Ok(mut guard) = WS_CANCEL_AB1D_SELECTORS.lock() {
        *guard
            .get_or_insert_with(Default::default)
            .entry(selector)
            .or_insert(0) += 1;
    }
}

/// Print the wait-elision diagnosis counters (no-op when env unset).
pub fn dump_wait_stats() {
    if !wait_stats_enabled() {
        return;
    }
    eprintln!(
        "[WAIT-STATS] anchor_calls={} probe_starts={} probe_overflows={} exact_repeats={} parked={} period_steps={} fail_tick={} fail_cpu={} fail_mem={} cancel_trap={}",
        WS_ANCHOR_CALLS.load(AtomicOrdering::Relaxed),
        WS_PROBE_STARTS.load(AtomicOrdering::Relaxed),
        WS_PROBE_OVERFLOWS.load(AtomicOrdering::Relaxed),
        WS_EXACT_REPEATS.load(AtomicOrdering::Relaxed),
        WS_PARKED.load(AtomicOrdering::Relaxed),
        WS_PERIOD_STEPS.load(AtomicOrdering::Relaxed),
        WS_FAIL_TICK.load(AtomicOrdering::Relaxed),
        WS_FAIL_CPU.load(AtomicOrdering::Relaxed),
        WS_FAIL_MEM.load(AtomicOrdering::Relaxed),
        WS_CANCEL_TRAP.load(AtomicOrdering::Relaxed),
    );
    eprintln!(
        "[WAIT-STATS] resumed={} resume_fail_mem={} resume_fail_host={} resume_fail_other={}",
        WS_RESUMED.load(AtomicOrdering::Relaxed),
        WS_RESUME_FAIL_MEM.load(AtomicOrdering::Relaxed),
        WS_RESUME_FAIL_HOST.load(AtomicOrdering::Relaxed),
        WS_RESUME_FAIL_OTHER.load(AtomicOrdering::Relaxed),
    );
    eprintln!(
        "[WAIT-STATS] cancel_backoffs={} backoff_skips={}",
        WS_CANCEL_BACKOFFS.load(AtomicOrdering::Relaxed),
        WS_BACKOFF_SKIPS.load(AtomicOrdering::Relaxed),
    );
    if let Ok(guard) = WS_CANCEL_TRAP_WORDS.lock() {
        if let Some(map) = guard.as_ref() {
            let mut rows: Vec<_> = map.iter().collect();
            rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
            for (word, n) in rows.iter().take(12) {
                eprintln!("[WAIT-STATS]   cancel trap {word:04X}: {n}");
            }
        }
    }
    if let Ok(guard) = WS_CANCEL_AB1D_SELECTORS.lock() {
        if let Some(map) = guard.as_ref() {
            let mut rows: Vec<_> = map.iter().collect();
            rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
            for (sel, n) in rows.iter().take(12) {
                eprintln!("[WAIT-STATS]   cancel AB1D selector {sel:08X}: {n}");
            }
        }
    }
}

fn spin_wait_fastfwd_force_on() -> bool {
    *SPIN_WAIT_FASTFWD_FORCE_ON
        .get_or_init(|| std::env::var_os("SYSTEMLESS_SPIN_WAIT_FASTFWD").is_some())
}
fn spin_wait_fastfwd_force_off() -> bool {
    *SPIN_WAIT_FASTFWD_FORCE_OFF
        .get_or_init(|| std::env::var_os("SYSTEMLESS_DISABLE_SPIN_FASTFWD").is_some())
}

/// Resolve the fast-forward gate for the current run-loop call.
/// Override precedence:
///   1. force-off env wins.
///   2. force-on env wins next.
///   3. default = on for headless or tick-capped GUI execution; an uncapped
///      GUI caller stays off because it could otherwise batch visible ticks.
fn spin_wait_fastfwd_enabled_for(yield_for_ui: bool, tick_cap: Option<u32>) -> bool {
    spin_wait_fastfwd_gate(
        spin_wait_fastfwd_force_on(),
        spin_wait_fastfwd_force_off(),
        yield_for_ui,
        tick_cap.is_some(),
    )
}

/// Pure decision function for the override gate. Split out from
/// `spin_wait_fastfwd_enabled_for` so the env-var reads can be mocked
/// in unit tests (the `OnceLock`-based env caches initialise once per
/// process and would prevent testing all three modes in one test
/// run).
fn spin_wait_fastfwd_gate(
    force_on: bool,
    force_off: bool,
    yield_for_ui: bool,
    has_tick_cap: bool,
) -> bool {
    if force_off {
        return false;
    }
    if force_on {
        return true;
    }
    !yield_for_ui || has_tick_cap
}

/// Some tracking traps should block the application's foreground event
/// loop without advancing the app-visible tick clock in GUI mode. ModalDialog
/// is different: the dialog manager is itself the active event loop, and
/// Sound/VBL/Time Manager work must keep advancing while it tracks input.
fn tracking_refire_should_freeze_ticks(opcode: u16) -> bool {
    let trap_no_autopop = opcode & !0x0400;
    trap_no_autopop == 0xA93D // MenuSelect
        || trap_no_autopop == 0xA80B // PopUpMenuSelect / MenuKey tracking
        || trap_no_autopop == 0xA968 // TrackControl
        || trap_no_autopop == 0xA91E // TrackGoAway
        || trap_no_autopop == 0xA83B // TrackBox
        || trap_no_autopop == 0xA925 // DragWindow
        || trap_no_autopop == 0xA905 // DragGrayRgn
        || trap_no_autopop == 0xA926 // DragTheRgn
}

/// Only Dialog Manager-owned retained loops may schedule dialog draw and
/// filter callbacks before their next presentation boundary. Other retained
/// managers can coexist with modeless dialogs, but do not own those callbacks.
fn tracking_refire_uses_dialog_callbacks(opcode: u16) -> bool {
    matches!(opcode & !0x0400, 0xA991 | 0xA985 | 0xA986 | 0xA987 | 0xA988)
}

/// Retained managers with their own event loops must keep guest ticks moving
/// while waiting for input. MenuSelect and the control/window/region drag
/// loops instead freeze time while their transient tracking state is presented.
fn tracking_refire_advances_gui_idle_tick(opcode: u16) -> bool {
    matches!(opcode & !0x0400, 0xA991 | 0xA9EA)
}

fn canonical_trap_number(opcode: u16) -> (bool, u16) {
    let is_tool = (opcode & 0x0800) != 0;
    let trap_num = if is_tool {
        opcode & 0x03FF
    } else {
        opcode & 0x00FF
    };
    (is_tool, trap_num)
}

/// Traps an exact idle-cycle proof may observe without cancelling.
///
/// The prover parks a wait loop behind two independent gates, and a trap
/// is admitted here only if it cannot beat either:
///
/// 1. **The proof.** Two arrivals at the same trap site in the same tick
///    must show an identical `CpuArchitecturalSnapshot` *and* a write
///    journal in which every guest-RAM byte written during the cycle
///    still holds its original value (`try_exact_idle_cycle_fastfwd`).
/// 2. **The park.** A proven cycle is reused only while guest memory, the
///    CPU snapshot, `IdleCycleHostSnapshot` (mouse, keys, caps lock, the
///    window list, any staged native menu selection) and the guest tick
///    are all unchanged and the Event Manager has nothing to deliver
///    (`try_resume_proven_idle_cycle`). A park is at most one tick.
///
/// A trap is *journal-complete* -- admissible -- when every input it acts
/// on is CPU state, guest RAM or the current tick, and every consequence
/// of calling it is either nothing at all or a write through the bus into
/// guest RAM, so that any real difference between two iterations shows
/// up in one of the two comparisons above. Concretely, before admitting a
/// trap check all of:
///
/// - it reads no host state outside `IdleCycleHostSnapshot` -- or, if it
///   reads a host mirror (the window list, a staged menu selection), every
///   mutation of that mirror is also written into journaled guest RAM or
///   posts an event the park gate sees;
/// - on any path that writes guest RAM it writes *before* it touches
///   host-cached state the journal cannot see (TEIdle's caret stamps
///   precede its draw; MoveTo, which only mirrors pnLoc into dispatcher
///   state, is the counter-example and stays out);
/// - its only other effects are diagnostics (trace `eprintln!`s).
///
/// Event polls only qualify when they returned a null event; SystemTask
/// only without periodic host work -- callers pass those runtime facts
/// in. `selector` carries live D0, consulted only for the
/// selector-multiplexed QDExtensions trap.
/// ONE definition, consulted from both the inline pre-dispatch check
/// and the post-dispatch quiescence classification (they were once two
/// hand-synced copies and drifted). The membership test
/// `journal_complete_traps_do_not_cancel_an_idle_probe` covers both the
/// plain and the auto-pop encodings.
fn idle_cycle_trap_is_journal_complete(
    opcode: u16,
    null_event: bool,
    system_task_idle: bool,
    selector: u32,
) -> bool {
    match canonical_trap_number(opcode) {
        (true, 0x0170) | (true, 0x0171) => null_event,
        // Pure transforms of a Point on the stack (quickdraw.rs).
        (true, 0x0070) | (true, 0x0071) => true, // LocalToGlobal, GlobalToLocal
        // PtInRect: reads pt+rect from the stack/RAM, writes a Boolean at
        // sp+8 (journaled) and pops A7 (CPU state the proof compares).
        (true, 0x00AD) => true, // PtInRect
        // QDExtensions ($AB1D) multiplexes on the D0 selector; admit only
        // the GetGWorld/SetGWorld save/restore pair (quickdraw.rs) games
        // bracket their poll-loop hit-testing with. GetGWorld writes its
        // two VAR results through the bus (journaled) and pops A7;
        // SetGWorld's writes (the THE_PORT low global and the A5 world's
        // thePort mirror) are journaled, and the dispatcher
        // port/draw-state mirrors it rewrites are pure functions of its
        // stack arguments and guest RAM, so they sit at a fixed point
        // across two proven-identical cycles. A cold-start
        // `ensure_main_gdevice` allocation writes fresh heap bytes the
        // byte-identity proof rejects by itself. Every other selector
        // (NewGWorld, LockPixels, UpdateGWorld, ...) allocates, locks or
        // frees host-mirrored state and must cancel.
        (true, 0x031D) => matches!(selector, 0x0008_0005 | 0x0008_0006),
        // TEIdle (dialog.rs `textedit_idle`) reads the TERec and the tick and
        // either returns having written nothing, or stamps caretState and
        // caretTime into guest RAM *before* it paints -- that ordering is
        // what lets the journal fail a proof a due blink lands in.
        (true, 0x01DA) => true, // TEIdle
        // Window Manager queries (window.rs) whose results reach the guest
        // through RAM and the stack. The host window list they read is
        // rewritten into the guest chain (journaled) by every mutation
        // (`sync_window_list_links`), a staged native menu selection is
        // always paired with a pending event the parked resume sees, and
        // `IdleCycleHostSnapshot` carries both besides.
        (true, 0x0117) | (true, 0x0124) | (true, 0x012C) => true, // GetWRefCon, FrontWindow, FindWindow
        // Input-poll family: GetMouse/StillDown/Button/TickCount/GetKeys.
        (true, 0x0172..=0x0176) => true,
        // SANE Pack4/Pack5: pure transforms of stack operands.
        (true, 0x01EB) | (true, 0x01EC) => true,
        (true, 0x01B4) => system_task_idle,
        _ => false,
    }
}

/// Input/time polls that may anchor an exact idle-cycle proof: pure
/// reads of host state that only changes at tick boundaries. A cycle
/// that repeats around one of these with identical CPU and memory state
/// is waiting, whether or not it ever asks the Event Manager.
fn is_poll_anchor_trap(opcode: u16) -> bool {
    matches!(canonical_trap_number(opcode), (true, 0x0172..=0x0176))
}

fn hle_trap_extra_tick_cost(opcode: u16) -> i32 {
    let (is_tool, trap_num) = canonical_trap_number(opcode);
    match (is_tool, trap_num) {
        // Event/time polling rates vary with host speed; charging these
        // creates false game-time drift instead of modelling ROM work.
        (true, 0x0170) // GetNextEvent
        | (true, 0x0060) // WaitNextEvent
        | (true, 0x0171) // EventAvail
        | (true, 0x0175) // TickCount
        | (true, 0x0062) // Button
        | (false, 0x0031) // GetOSEvent
        | (_, 0x003B) => 0, // Delay already advances requested ticks explicitly

        // SANE Pack4/Pack5 calls can be hot inner-loop arithmetic in games.
        // Treat them as guest computation, not manager work that should yield.
        (true, 0x006C) | (true, 0x006E) | (true, 0x01EB) | (true, 0x01EC) => 0,

        // QuickDraw blits and PICT draws do substantial HLE-side pixel work.
        (true, 0x00EC) | (true, 0x00F6) => 96,

        // Resource loads move and parse data that real ROM/file-system code
        // would not complete in one 68k instruction.
        (true, 0x01A0) | (true, 0x01A1) | (true, 0x01A2) | (true, 0x01BC) => 96,

        // Resource metadata/release calls are cheaper than loads but still
        // non-trivial manager work.
        (true, 0x019D..=0x01CF) => 24,

        // Other Toolbox/OS HLE traps should cost more than a single guest
        // opcode without making simple math/geometry helpers dominate timing.
        (true, _) => 4,
        (false, _) => 2,
    }
}

fn event_manager_yield_trap(opcode: u16) -> bool {
    matches!(
        canonical_trap_number(opcode),
        (true, 0x0170) // GetNextEvent
            | (true, 0x0060) // WaitNextEvent
            | (true, 0x0171) // EventAvail
    )
}

// Cap how many ticks the fast-forward will advance in one shot,
// to protect against pathological target values (e.g. overflowed
// unsigned register values being misinterpreted as huge-future
/// A site whose probes repeatedly die to a non-admitted trap is backed
/// off exponentially (2^streak ticks, capped here: ~2 s at 60 Hz)
/// instead of re-arming a doomed journal on every poll pass.
const IDLE_CYCLE_CANCEL_BACKOFF_CAP_TICKS: u32 = 120;

/// Per-site probe accounting slots. The busiest poll loop measured so
/// far interleaves about six distinct anchor sites per pass.
const IDLE_CYCLE_SITE_SLOTS: usize = 8;

/// Probe accounting for one exact-idle-cycle anchor site.
#[derive(Clone, Copy, Default)]
struct IdleCycleSiteRecord {
    site: u32,
    /// Tick the per-tick probe counter belongs to.
    tick: u32,
    /// Probes begun at (site, tick); one past
    /// [`IDLE_CYCLE_MAX_PROBES_PER_TICK`] means the site was refused a
    /// probe (or overflowed a journal) and is not re-probed this tick.
    probes: u8,
    /// Consecutive probes here killed by a non-admitted trap.
    cancel_streak: u8,
    /// No probing at this site before this tick (see
    /// [`IDLE_CYCLE_CANCEL_BACKOFF_CAP_TICKS`]).
    resume_tick: u32,
}

// Layout for dialog callback scratch region.
const DIALOG_DRAW_TRAMPOLINE_OFFSET: u32 = 0x00;
const DIALOG_FILTER_TRAMPOLINE_OFFSET: u32 = 0x40;
const DIALOG_FILTER_EVENT_OFFSET: u32 = 0x80;
// 2-byte scratch where the filter trampoline writes its Boolean return value.
const DIALOG_FILTER_RESULT_OFFSET: u32 = 0x96;
const DIALOG_CALLBACK_SCRATCH_SIZE: u32 = 0xC0;
/// Compact Mac video hardware refreshes at approximately 60.15 Hz.
pub const DEFAULT_VBL_HZ: f64 = crate::machine_profile::REFERENCE_MACHINE_PROFILE.vbl_hz;

/// Default host execution rate for 68K realtime frontends.
pub const DEFAULT_REALTIME_CPU_MHZ: f64 =
    crate::machine_profile::DEFAULT_HOST_EXECUTION_POLICY.realtime_m68k_cpu_mhz;
/// Default host execution rate for native PowerPC realtime frontends. The
/// Power Macintosh 9500/120 paired a 120 MHz clock with the 604 processor
/// reported by the PPC Gestalt implementation.
/// <https://support.apple.com/en-hk/112050>
pub const DEFAULT_REALTIME_PPC_CPU_MHZ: f64 =
    crate::machine_profile::DEFAULT_HOST_EXECUTION_POLICY.realtime_powerpc_cpu_mhz;
/// Default direct-color display depth for native PowerPC applications.
/// Imaging With QuickDraw (1994), p. 6-16, defines 16 bits per pixel as a
/// supported Color QuickDraw screen and offscreen graphics-world depth.
pub const DEFAULT_POWERPC_SCREEN_DEPTH: u32 = 16;
/// Default 68K realtime CPU budget used by scripted realtime mode and by GUI
/// sessions that do not load a PowerPC executable.
pub const DEFAULT_REALTIME_INSTRUCTIONS_PER_SECOND: f64 = DEFAULT_REALTIME_CPU_MHZ * 1_000_000.0;

/// Returns the per-VBL instruction budget for the loaded guest architecture.
pub fn default_realtime_instructions_per_tick(powerpc: bool) -> u32 {
    let mhz = if powerpc {
        DEFAULT_REALTIME_PPC_CPU_MHZ
    } else {
        DEFAULT_REALTIME_CPU_MHZ
    };
    (mhz * 1_000_000.0 / DEFAULT_VBL_HZ).round() as u32
}
const DEFAULT_LAUNCH_TICKS: u32 = 600;
/// Default double-click interval: 20 VBL ticks, approximately one third of a
/// second. This is the conventional classic Mac OS setting exposed through
/// the low-memory `DoubleTime` global.
const DEFAULT_DOUBLE_TIME_TICKS: u32 = 20;
const MAC_EPOCH_OFFSET_FROM_UNIX: u64 = 2_082_844_800;

fn current_mac_epoch_seconds() -> u32 {
    let unix_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    unix_now
        .saturating_add(MAC_EPOCH_OFFSET_FROM_UNIX)
        .min(u32::MAX as u64) as u32
}

fn align4(value: u32) -> u32 {
    value.saturating_add(3) & !3
}

fn app_heap_start_for_loaded_app(app: &LoadedApp) -> u32 {
    APP_HEAP_FLOOR.max(align4(app.loaded_image_end))
}

fn app_image_start_for_loaded_app(app: &LoadedApp) -> u32 {
    app.a5_base.saturating_sub(app.code0_header.below_a5)
}

fn app_visible_zone_start_for_loaded_app(app: &LoadedApp) -> u32 {
    let image_start = app_image_start_for_loaded_app(app);
    if image_start >= APP_HEAP_FLOOR.saturating_add(APP_ZONE_HEADER_SIZE) {
        APP_HEAP_FLOOR
    } else {
        app_heap_start_for_loaded_app(app)
    }
}

fn load_address_for_size_partition(
    configured_load_address: u32,
    header: &Code0Header,
    size_resource: Option<ApplicationSizeResource>,
    application_memory_limit: u32,
) -> u32 {
    if configured_load_address != DEFAULT_LOAD_ADDRESS {
        return configured_load_address;
    }

    let Some(size) = size_resource else {
        return configured_load_address;
    };
    // A normal launch attempts the preferred partition and may fall back to
    // any available size at or above the minimum; neither path has a size
    // threshold below which the partition stops governing the 68K A5 world.
    // Inside Macintosh: Processes (1994), pp. 1-3 and 2-15;
    // Inside Macintosh: Memory (1992), pp. 1-7 to 1-8.
    let Some(preferred_partition_size) = size.preferred_partition_size() else {
        return configured_load_address;
    };

    let desired_a5 = APP_HEAP_FLOOR
        .saturating_add(preferred_partition_size.saturating_sub(APP_STACK_SAFETY_MARGIN));
    let default_a5 = configured_load_address.saturating_add(header.below_a5);
    if desired_a5 <= default_a5 {
        return configured_load_address;
    }

    let relocated_load = align4(desired_a5.saturating_sub(header.below_a5));
    let loader_headroom = header
        .below_a5
        .saturating_add(header.above_a5)
        .saturating_add(APP_QD_GLOBALS_RESERVE)
        .saturating_add(APP_LOADER_CLEAR_RESERVE)
        .saturating_add(APP_HIGH_MEMORY_RESERVE);
    let max_safe_load = application_memory_limit.saturating_sub(loader_headroom) & !3;

    relocated_load
        .min(max_safe_load)
        .max(configured_load_address)
}

/// Host presentation policy for the classic Mac menu bar.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MenuBarPolicy {
    /// Follow the guest's `MBarHeight` and fullscreen state.
    #[default]
    GuestControlled,
    /// Begin with kiosk presentation, then return control to the guest after
    /// an explicit `DrawMenuBar` request or a real hide-and-reveal transition.
    InitialKiosk,
    /// Keep the guest menu bar suppressed regardless of guest state.
    ForceHidden,
}

/// Configuration knobs for [`FixtureRunner`]. Use
/// [`FixtureRunnerConfig::default`] for the canonical defaults
/// (10M-instruction budget, 0x10000 load base, arrow-keys NOT
/// remapped to numpad, guest-controlled menu bar) — only override fields you
/// actually need.
pub struct FixtureRunnerConfig {
    /// Hard cap on instructions executed by the simpler unbounded
    /// [`FixtureRunner::run`] entry point. Not consulted by
    /// [`FixtureRunner::run_steps`], which uses its own per-call
    /// `max_steps` argument. Default: 10,000,000.
    pub max_instructions: usize,
    /// Base address where 68k CODE segments are loaded into guest
    /// RAM. Default: 0x10000 (64 KiB above the low-mem globals).
    /// Most games tolerate the default; a few with hardcoded
    /// expectations about A5 placement may need a higher value.
    pub load_address: u32,
    /// When true, arrow key virtual key codes are remapped to their numpad equivalents.
    /// Useful on keyboards without a numeric keypad, since many classic Mac games use
    /// the numpad for movement. Inside Macintosh Volume V, V-191.
    pub arrows_as_numpad: bool,
    /// Host presentation policy for the classic Mac menu bar. The default
    /// honors guest `MBarHeight` and fullscreen transitions.
    pub menu_bar_policy: MenuBarPolicy,
    /// Selected UI rendering provider. The default `classic-system7`
    /// provider uses the original renderer and classic guest metrics.
    pub ui_theme: UiThemeId,
    /// Declares whether theme rendering preserves classic guest metrics or opts
    /// into future themed hit/measurement behavior.
    pub theme_metrics_mode: ThemeMetricsMode,
    /// Use the full 32-bit guest address, or mask memory accesses to the low
    /// 24 bits as on classic Macs running in 24-bit addressing mode.
    pub addressing_32_bit: bool,
    /// Initial 68K main-framebuffer depth. Supported values are 1, 2, 4, and
    /// 8; the default remains 8-bit/256-color mode. Native PowerPC launches
    /// retain their architecture default unless
    /// [`FixtureRunner::set_powerpc_screen_depth`] is called explicitly.
    pub screen_depth: u16,
}

impl Default for FixtureRunnerConfig {
    fn default() -> Self {
        Self {
            max_instructions: 10_000_000,
            load_address: DEFAULT_LOAD_ADDRESS,
            arrows_as_numpad: false,
            menu_bar_policy: MenuBarPolicy::GuestControlled,
            ui_theme: UiThemeId::ClassicSystem7,
            theme_metrics_mode: ThemeMetricsMode::ClassicGuestMetrics,
            addressing_32_bit: true,
            screen_depth: 8,
        }
    }
}

impl FixtureRunnerConfig {
    /// Validate and select the initial 68K indexed main-framebuffer depth.
    /// Use [`FixtureRunner::set_powerpc_screen_depth`] for an explicit native
    /// PowerPC launch depth.
    pub fn with_screen_depth(mut self, screen_depth: u16) -> std::result::Result<Self, String> {
        if !matches!(screen_depth, 1 | 2 | 4 | 8) {
            return Err(format!(
                "unsupported screen depth {screen_depth}; expected 1, 2, 4, or 8"
            ));
        }
        self.screen_depth = screen_depth;
        Ok(self)
    }
}

/// Whether a CPU slice owns presentation or only Sound Manager servicing.
/// Audio-only slices retain the existing completion/synchronization boundary;
/// they defer native chrome until the frontend's outer composition pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameFinalization {
    Deferred,
    AudioOnly,
    Complete,
}

/// Canonical entry point of the systemless library.
///
/// `FixtureRunner` serializes one Macintosh process: `ProcessContext` owns
/// state shared across architectures, [`MacMemoryBus`] exposes its flat RAM to
/// 68K, and CPU-specific adapters execute 68K or PowerPC code against that
/// state. [`TrapDispatcher`] provides the shared Toolbox and OS boundary. The
/// runner exposes the load, execute, and halt-inspection surface that drives
/// them.
///
/// **Lifecycle:**
/// 1. [`FixtureRunner::new`] — allocate guest RAM + dispatcher.
/// 2. [`crate::game::load_game`] — auto-detect StuffIt / MacBinary,
///    populate guest memory, seed CPU state.
/// 3. [`run_steps`](Self::run_steps) (preferred) or [`run`](Self::run)
///    — drive the CPU. `run_steps` returns `(steps_executed,
///    still_running)`; `run` runs until halt or
///    [`FixtureRunnerConfig::max_instructions`].
/// 4. After halt: [`halted_pc`](Self::halted_pc) /
///    [`halted_trap`](Self::halted_trap) /
///    [`halted_sp`](Self::halted_sp) / [`halted_d0`](Self::halted_d0)
///    expose per-halt detail, and
///    [`halted_by_exit_to_shell`](Self::halted_by_exit_to_shell) classifies
///    the common clean application-exit path.
///
/// **Defaults:** guest-controlled Mac menu-bar visibility; arrow keys NOT
/// remapped to numpad. Frontends can select a kiosk policy through
/// [`set_menu_bar_policy`](Self::set_menu_bar_policy).
///
/// See `examples/run_headless.rs` for a runnable end-to-end example.
pub struct FixtureRunner {
    m68k: M68kExecution,
    native: NativeExecution<PpcLoadedApp>,
    bus: MacMemoryBus,
    dispatcher: TrapDispatcher,
    /// Canonical owner for state shared by this process's CPU ABI adapters.
    process_context: ProcessContext,
    config: FixtureRunnerConfig,
    /// Explicit display depth carried into native PowerPC launches. `None`
    /// preserves the architecture defaults: 8bpp for 68K and 16bpp for PPC.
    powerpc_screen_depth_override: Option<u16>,
    /// Main host PixMap ColorTable retained while a direct-color PowerPC
    /// framebuffer temporarily requires a NIL `pmTable`.
    ppc_host_indexed_ctab_handle: u32,
    /// Owned host-side framebuffer mirror reused across PowerPC depth changes.
    ppc_host_mirror_base: u32,
    ppc_host_mirror_capacity: u32,
    prefer_powerpc_executables: bool,
    installer_handoff_baseline: Option<BTreeSet<String>>,
    trace_buffer: std::collections::VecDeque<(u32, u16, u32, u32, u32, u32)>, // (PC, Op, A0, SP, A6, A5)
    /// Set to true when the application calls ExitToShell
    halted: bool,
    /// Trap opcode that caused the halt, if known.
    halted_trap: Option<u16>,
    /// Program counter at the point of halt.
    halted_pc: Option<u32>,
    /// Stack pointer at the point of halt.
    halted_sp: Option<u32>,
    /// D0 register at the point of halt.
    halted_d0: Option<u32>,
    /// Total guest instructions accounted by the runner.
    ///
    /// This includes instructions retired by m68k, intercepted trap opcodes,
    /// and instructions represented by proven idle-loop acceleration.
    total_instructions: u64,
    /// Number of guest instructions charged per `Ticks` increment.
    instructions_per_tick: u32,
    /// Optional cap on per-WaitNextEvent-call sleep tick advance in headless
    /// mode (when `run_steps` is called without a `tick_override`). `None`
    /// keeps the legacy drain-all behavior. `Some(n)` advances at most `n`
    /// ticks per WNE call, mirroring GUI mode's 1-tick cap. Used for
    /// scripted tick alignment with Basilisk.
    wait_sleep_cap_in_headless: Option<u32>,
    /// Remaining instruction budget for the current tick. Both 68k instructions
    /// and HLE trap costs are deducted. When this reaches zero or below, the
    /// tick advances and the budget is refilled from `instructions_per_tick`.
    tick_budget: i32,
    /// Most recent null-event boundary seen in the current host execution
    /// slice. A second same-tick visit starts an exact-state cycle probe; no
    /// optimization is enabled by this observation alone.
    idle_cycle_last_seen: Option<(u32, u32)>,
    /// One-cycle proof in progress. The paired memory-bus journal disables
    /// direct fast-memory stores until this call site repeats or the proof is
    /// canceled by a non-quiescent trap.
    idle_cycle_probe: Option<IdleCycleProbe>,
    /// Per-site probe budgets and trap-cancel backoff. A fixed table
    /// rather than a single slot: a play-mode poll loop can interleave
    /// probes from several anchor sites, and a single slot forgets each
    /// site's spent budget the moment another site probes, unbounding
    /// the per-tick armed-journal count. (Boot and speed-calibration
    /// code polls TickCount between bursts of computation; the per-tick
    /// budget keeps those sites unprobed until the tick moves on.)
    idle_cycle_sites: [IdleCycleSiteRecord; IDLE_CYCLE_SITE_SLOTS],
    /// Proven null-event cycle parked at its post-trap boundary. Unlike an
    /// in-progress proof, this may cross frontend slices: a second write
    /// journal plus CPU/input/event checks revoke it before any reuse.
    idle_cycle_sleep: Option<ProvenIdleCycleSleep>,
    /// Tick value saved when menu tracking starts.  While set, run_steps caps
    /// its tick_override to this value so the game clock is frozen — matching
    /// the real Mac where MenuSelect blocks the application event loop.
    frozen_ticks: Option<u32>,
    menu_presentation_remainder: u64,
    /// Guest-memory address of the Time Manager interrupt trampoline code.
    /// Allocated once on first use and reused for all subsequent timer fires.
    timer_trampoline: u32,
    /// Guest-memory address of the Vertical Retrace Manager trampoline code.
    /// Allocated once on first use and reused for all VBL callbacks.
    vbl_trampoline: u32,
    /// Guest-memory address of the low-memory `JCrsrTask` callback trampoline.
    /// Allocated once on first use and reused for cursor task callbacks.
    cursor_task_trampoline: u32,
    /// Callable default cursor updater, also retained when guests wrap JCrsrTask.
    default_cursor_task: u32,
    /// Guest-memory trampoline and packet buffer for ADB service routines.
    adb_callback_trampoline: u32,
    adb_packet_buffer: u32,
    /// Currently executing Time Manager callback, if any.
    ///
    /// Real timer delivery happens from interrupt context, so the same timer source
    /// must not be re-entered by our synthetic tick advancement while the callback
    /// is still unwinding back to interrupted guest code.
    active_interrupt_callback: Option<ActiveInterruptCallback>,
    /// Interrupts may preempt a foreground dialog callback; interrupt handlers
    /// themselves remain non-reentrant. Preserve the foreground return frame.
    suspended_dialog_callback: Option<ActiveInterruptCallback>,
    nested_dialog_calls: Vec<SuspendedDialogCall>,
    /// GrafPort state saved while an application-owned dialog userItem draws.
    dialog_draw_port_snapshot: Option<crate::trap::dispatch::PortStateSnapshot>,
    /// Tracking trap PC to re-fire after an asynchronous callback returns.
    ///
    /// A timer/VBL callback can be injected while MenuSelect or ModalDialog
    /// is being re-fired. Keep the callback's normal return frame intact and
    /// resume the tracking trap only after that frame has been restored.
    deferred_tracking_refire_pc: Option<u32>,
    /// Audio output backend (None = no audio output).
    audio: Option<Box<dyn crate::audio::AudioBackend>>,
    /// Accumulated audio samples for external consumers (e.g. WASM).
    /// Unsigned 8-bit mono PCM at OUTPUT_RATE Hz (silence = 0x80).
    audio_buffer: Vec<u8>,
    /// Guest-memory address of the SndPlayDoubleBuffer doubleback trampoline.
    /// Allocated once on first use and reused for all double-buffer callbacks.
    sound_doubleback_trampoline: u32,
    /// Guest-memory address of the SndNewChannel callback trampoline.
    /// Allocated once on first use and reused for all callback procedures.
    sound_callback_trampoline: u32,
    /// Guest-memory address of the SndStartFilePlay completion trampoline.
    /// Allocated once on first use and reused for all file completion routines.
    sound_file_completion_trampoline: u32,
    /// Guest-memory trampoline used to invoke File Manager asynchronous
    /// completion procedures.
    file_completion_trampoline: u32,
    /// Systemless-owned storage for dialog callback trampolines.
    /// This must not overlap the architectural low-memory trap tables.
    dialog_callback_scratch_base: u32,
    /// Guest-memory address of the dialog userItem draw proc trampoline (26 bytes).
    /// Allocated once on first use and reused for all subsequent draw proc calls.
    dialog_draw_trampoline: u32,
    /// Guest-memory address of the ModalDialog filter proc trampoline.
    /// Allocated once on first use and reused for all callback invocations.
    dialog_filter_trampoline: u32,
    /// Guest-memory address of a scratch EventRecord passed to ModalDialog filters.
    dialog_filter_event: u32,
    /// Last dialog/tick pair that received a synthetic ModalDialog null event.
    /// Real queued events bypass this; it only paces the no-input idle callback.
    dialog_filter_last_null_event_tick: Option<(u32, u32)>,
    /// Last dialog/update-window/tick triple that received a synthetic
    /// ModalDialog update event from the Window Manager invalid-region state.
    /// Queued mouse/key/update events bypass this; it only prevents the same
    /// still-invalid dialog from starving later input in one guest tick.
    dialog_filter_last_update_event_tick: Option<(u32, u32, u32)>,
    /// Override for the application's startup time in Mac-epoch seconds.
    /// Used by scripted frontends to keep guest-visible time deterministic.
    app_start_time: Option<u32>,
    /// Optional deterministic launch state. These values are applied while
    /// initializing an application, before its first guest instruction.
    launch_ticks_override: Option<u32>,
    launch_rnd_seed_override: Option<u32>,
    launch_ppc_time_base_override: Option<u64>,
    /// Optional Finder-style application partition size override in bytes.
    /// Scripted frontends use this to model a user raising the preferred
    /// memory size before launch without mutating the application's resources.
    application_partition_size: Option<u32>,
    /// Per-opcode histogram for M68K instructions. Indexed by the
    /// full 16-bit opcode word (`cpu.core.ir` after step). Always
    /// allocated (512 KB); populated only when
    /// `SYSTEMLESS_TRACE_OPCODE_COUNTS=1` is set at startup. Zero cost
    /// on the hot path when disabled (cached bool compare, branch
    /// short-circuited). Complements the trap histogram, which only
    /// sees A-line opcodes; this captures MOVE/ADD/Bcc/etc. too,
    /// which is what decode-table or super-instruction-fusion work
    /// needs to prioritize.
    opcode_histogram: Box<[u64; 65536]>,
    /// Sampled PC histogram. When `SYSTEMLESS_TRACE_HOT_PC=1` is set,
    /// every 1000th step's PC increments a `HashMap` bucket. Answers
    /// "which CODE ADDRESS is hot", useful for locating game-side
    /// hot loops by routine. Sampling (1/1000) keeps `HashMap`
    /// overhead low; a million hot samples still fits in tens of
    /// unique addresses.
    pc_histogram: HashMap<u32, u64>,
    /// PPC fetched-instruction histogram, opt-in via
    /// `SYSTEMLESS_TRACE_PPC_FETCH_COUNTS=1`. Aggregated across PPC
    /// run budgets so diagnostics can prioritize reachable opcode
    /// gaps without retaining every fetched PC.
    ppc_fetch_histogram: PpcFetchHistogram,
    /// PPC HLE import histogram, opt-in via
    /// `SYSTEMLESS_PPC_IMPORT_HIST=1`. Aggregated across PPC run
    /// budgets and native PPC callback runs by library/symbol.
    ppc_import_histogram: HashMap<(String, String), u64>,
    /// Unsupported PPC runtime stop histogram, opt-in via
    /// `SYSTEMLESS_PPC_UNIMPL_HIST=1`. Records unsupported imports
    /// and unimplemented/illegal instruction stops by stable text key.
    ppc_unimpl_histogram: HashMap<String, u64>,
    /// Completed QD3D frame index used by `SYSTEMLESS_QD3D_DUMP_FRAME`.
    /// Incremented only for non-empty completed-frame snapshots, matching
    /// the renderer's frame accounting.
    q3_completed_frame_index: usize,
    /// When enabled, supported completed QD3D frames are prepared for a host
    /// GPU instead of being rasterized into guest memory.
    external_q3_renderer_enabled: bool,
    pending_q3_gpu_frame: Option<PpcQ3GpuFrame>,
    #[cfg(feature = "debug")]
    pub(crate) debug: crate::debug::DebuggerCoordinator,
}

impl FixtureRunner {
    /// Construct a fresh runner with `ram_size` bytes of guest RAM and
    /// the given [`FixtureRunnerConfig`]. The CPU begins at PC = 0 with
    /// reset vectors not yet loaded. Guest RAM contains the process trap tables,
    /// exception gateways and other system defaults. Call
    /// [`load_app`](Self::load_app) (or the higher-level
    /// `systemless::game::load_game`) to populate guest memory and seed the
    /// run state, then drive the guest with [`run_steps`](Self::run_steps).
    ///
    /// The runner allocates one contiguous host region of `ram_size` bytes.
    /// [`crate::game::RAM_SIZE`] provides the canonical profile-driven size;
    /// tests and specialized embedders may choose a smaller allocation.
    ///
    /// The dispatcher follows the guest's menu-bar state by default. Frontends
    /// that own the surrounding chrome can select an explicit kiosk policy in
    /// [`FixtureRunnerConfig`] or through
    /// [`set_menu_bar_policy`](Self::set_menu_bar_policy).
    pub fn new(ram_size: usize, config: FixtureRunnerConfig) -> Self {
        Self::new_with_file_system(ram_size, config, SharedProcessFileSystem::default())
    }

    fn new_with_file_system(
        ram_size: usize,
        config: FixtureRunnerConfig,
        file_system: SharedProcessFileSystem,
    ) -> Self {
        // A 24-bit address space can expose at most 16 MiB. Keep all allocated
        // guest structures, the stack, and framebuffer below that boundary so
        // masking the flag byte cannot alias a high-memory layout onto low RAM.
        let ram_size = if config.addressing_32_bit {
            ram_size
        } else {
            ram_size.min(0x0100_0000)
        };
        assert!(
            matches!(config.screen_depth, 1 | 2 | 4 | 8),
            "screen_depth must be 1, 2, 4, or 8"
        );
        let mut process_context = ProcessContext::with_file_system(file_system);
        let mut dispatcher =
            TrapDispatcher::new_with_migrated_handles(process_context.migrated_handles());
        dispatcher.attach_unconverted_process_services(&mut process_context);
        dispatcher.set_menu_bar_policy(config.menu_bar_policy);
        dispatcher.mmu_mode = u8::from(config.addressing_32_bit);
        dispatcher.set_ui_theme_id(config.ui_theme);
        let mut bus = MacMemoryBus::new(ram_size);
        bus.set_addressing_32_bit(config.addressing_32_bit);
        bus.configure_screen_depth(config.screen_depth);
        process_context.attach_classic_memory_bus(&mut bus);
        bus.write_word(
            crate::memory::globals::addr::SYS_EVT_MASK,
            crate::memory::globals::DEFAULT_SYS_EVT_MASK,
        );
        bus.write_word(
            crate::memory::globals::addr::MENU_FLASH,
            crate::memory::globals::DEFAULT_MENU_FLASH_COUNT,
        );
        let profile = crate::machine_profile::reference_machine_profile();
        let visible_row_bytes =
            (u32::from(profile.screen_width) * u32::from(config.screen_depth)).div_ceil(8);
        let row_bytes = (visible_row_bytes / 16 + 1) * 16;
        dispatcher.screen_mode = (
            bus.read_long(crate::memory::globals::addr::SCRN_BASE),
            row_bytes,
            profile.screen_width,
            profile.screen_height,
            config.screen_depth,
        );
        let (clut, _) = TrapDispatcher::standard_mac_indexed_clut(config.screen_depth)
            .expect("validated indexed screen depth");
        dispatcher.device_clut.replace(clut);
        dispatcher.color_manager_clut.replace(clut);
        dispatcher.seeded_picture_palette = clut;
        let standard_adb_service = bus.alloc_synthetic(2);
        bus.write_word(standard_adb_service, 0x4E75); // RTS
        dispatcher
            .adb
            .install_standard_service_routine(standard_adb_service);
        let dialog_callback_scratch_base = bus.alloc_synthetic(DIALOG_CALLBACK_SCRATCH_SIZE);
        // Embedders can execute immediately after construction, including
        // before loading an application or installing a native companion.
        // Establish the writable tables and vector identities before any
        // guest patch or instruction can observe the process environment.
        // Inside Macintosh: Operating System Utilities (1994), pp. 8-4--8-6.
        dispatcher
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");
        Self {
            m68k: M68kExecution::new(&dispatcher.guest_calls),
            native: NativeExecution::default(),
            bus,
            dispatcher,
            process_context,
            config,
            powerpc_screen_depth_override: None,
            ppc_host_indexed_ctab_handle: 0,
            ppc_host_mirror_base: 0,
            ppc_host_mirror_capacity: 0,
            prefer_powerpc_executables: false,
            installer_handoff_baseline: None,
            trace_buffer: std::collections::VecDeque::with_capacity(2000),
            halted: false,
            halted_trap: None,
            halted_pc: None,
            halted_sp: None,
            halted_d0: None,
            total_instructions: 0,
            instructions_per_tick: crate::machine_profile::DEFAULT_HOST_EXECUTION_POLICY
                .scripted_instructions_per_tick,
            wait_sleep_cap_in_headless: None,
            tick_budget: crate::machine_profile::DEFAULT_HOST_EXECUTION_POLICY
                .scripted_instructions_per_tick as i32,
            idle_cycle_last_seen: None,
            idle_cycle_probe: None,
            idle_cycle_sites: [IdleCycleSiteRecord::default(); IDLE_CYCLE_SITE_SLOTS],
            idle_cycle_sleep: None,
            frozen_ticks: None,
            menu_presentation_remainder: 0,
            timer_trampoline: 0,
            vbl_trampoline: 0,
            cursor_task_trampoline: 0,
            default_cursor_task: 0,
            adb_callback_trampoline: 0,
            adb_packet_buffer: 0,
            active_interrupt_callback: None,
            suspended_dialog_callback: None,
            nested_dialog_calls: Vec::new(),
            dialog_draw_port_snapshot: None,
            deferred_tracking_refire_pc: None,
            audio: None,
            audio_buffer: Vec::new(),
            sound_doubleback_trampoline: 0,
            sound_callback_trampoline: 0,
            sound_file_completion_trampoline: 0,
            file_completion_trampoline: 0,
            dialog_callback_scratch_base,
            dialog_draw_trampoline: 0,
            dialog_filter_trampoline: 0,
            dialog_filter_event: 0,
            dialog_filter_last_null_event_tick: None,
            dialog_filter_last_update_event_tick: None,
            app_start_time: None,
            launch_ticks_override: None,
            launch_rnd_seed_override: None,
            launch_ppc_time_base_override: None,
            application_partition_size: None,
            opcode_histogram: Box::new([0u64; 65536]),
            pc_histogram: HashMap::new(),
            ppc_fetch_histogram: PpcFetchHistogram::new(),
            ppc_import_histogram: HashMap::new(),
            ppc_unimpl_histogram: HashMap::new(),
            q3_completed_frame_index: 0,
            external_q3_renderer_enabled: false,
            pending_q3_gpu_frame: None,
            #[cfg(feature = "debug")]
            debug: crate::debug::DebuggerCoordinator::fresh(),
        }
    }

    /// Prefer a PowerPC application fragment over a classic `CODE` fallback
    /// when loading a fat application. The default remains classic 68k for
    /// callers that do not select an architecture explicitly.
    pub fn set_prefer_powerpc_executables(&mut self, enabled: bool) {
        self.prefer_powerpc_executables = enabled;
    }

    /// Returns whether fat applications should prefer a PowerPC fragment.
    pub fn prefers_powerpc_executables(&self) -> bool {
        self.prefer_powerpc_executables
    }

    pub(crate) fn arm_installer_handoff(&mut self) {
        self.installer_handoff_baseline = Some(
            self.vfs_file_summaries()
                .into_iter()
                .map(|file| file.path)
                .collect(),
        );
    }

    pub fn set_external_q3_renderer_enabled(&mut self, enabled: bool) {
        self.external_q3_renderer_enabled = enabled;
        if !enabled {
            self.pending_q3_gpu_frame = None;
        }
    }

    pub fn take_q3_gpu_frame(&mut self) -> Option<PpcQ3GpuFrame> {
        self.pending_q3_gpu_frame.take()
    }

    /// Returns the number of non-empty QuickDraw 3D frames completed by the
    /// guest and consumed by either the software or external renderer.
    pub fn completed_qd3d_frame_count(&self) -> usize {
        self.q3_completed_frame_index
    }

    /// Returns true once guest execution has stopped.
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// Returns true when the halt was the documented clean application
    /// termination path, `_ExitToShell` (`$A9F4`).
    ///
    /// This lets runners and tests distinguish apps that intentionally quit
    /// from halts caused by faults, invalid PCs, or other fatal errors.
    pub fn halted_by_exit_to_shell(&self) -> bool {
        self.halted && self.halted_trap == Some(0xA9F4)
    }

    pub fn guest_tick(&self) -> u32 {
        self.bus.read_long(crate::memory::globals::addr::TICKS)
    }

    /// Test-only: seed the guest-owned low-memory clock. Production clock
    /// advancement goes through the VBL boundary; tests use this explicit
    /// helper so no adapter field can become a second TickCount authority.
    #[cfg(test)]
    pub(crate) fn set_guest_tick_for_test(&mut self, tick: u32) {
        self.bus
            .write_long(crate::memory::globals::addr::TICKS, tick);
        self.dispatcher.read_tick_count(&self.bus);
    }

    /// Return an architecture-neutral semantic snapshot of Event Manager
    /// state. Showcase and embedding tests can use this instead of relying on
    /// rendered pixels or 68K/PPC-specific EventRecord offsets.
    pub fn event_manager_snapshot(&self) -> EventManagerSnapshot {
        self.event_manager_snapshot_with_limit(usize::MAX)
    }

    pub(crate) fn event_manager_snapshot_with_limit(&self, limit: usize) -> EventManagerSnapshot {
        let queue = self.process_context.event_queue();
        let ppc_state = self.native.application().map(|app| &app.toolbox_startup);
        let last_record: Option<EventRecordSnapshot> = self
            .dispatcher
            .debug_last_event_record
            .clone()
            .or_else(|| ppc_state.and_then(|state| state.last_event_record.clone()));
        let queue_probe = ppc_state.map_or_else(
            || self.dispatcher.debug_event_queue_probe.clone(),
            |state| state.event_queue_probe.clone(),
        );
        let button_result = ppc_state.map_or(self.dispatcher.debug_last_button_result, |state| {
            state.last_button_result
        });
        let still_down_result = ppc_state
            .map_or(self.dispatcher.debug_last_still_down_result, |state| {
                state.last_still_down_result
            });
        let wait_mouse_up_result = ppc_state
            .map_or(self.dispatcher.debug_last_wait_mouse_up_result, |state| {
                state.last_wait_mouse_up_result
            });
        EventManagerSnapshot {
            last_record,
            queue_probe,
            queue_len: queue.len(),
            queued_event_types: queue.iter().take(limit).map(|event| event.what).collect(),
            mouse_position: self.dispatcher.mouse_position(),
            mouse_button: self.dispatcher.input_state.mouse_button_pressed(),
            button_result,
            still_down_result,
            wait_mouse_up_result,
            key_map: self.dispatcher.input_state.key_map_snapshot(),
            lifecycle_activation_seen: self.dispatcher.debug_activation_event_seen
                || ppc_state.is_some_and(|state| state.activation_event_seen),
            lifecycle_update_seen: self.dispatcher.debug_update_event_seen
                || ppc_state.is_some_and(|state| state.update_event_seen),
            cursor_visible: self.dispatcher.cursor_visible(),
            cursor_level: self.dispatcher.cursor_level(),
        }
    }

    pub fn host_now(&self) -> Instant {
        Instant::now()
    }

    pub fn halted_trap(&self) -> Option<u16> {
        self.halted_trap
    }

    pub fn halted_pc(&self) -> Option<u32> {
        self.halted_pc
    }

    pub fn halted_sp(&self) -> Option<u32> {
        self.halted_sp
    }

    pub fn halted_stack_word0(&self) -> Option<u16> {
        self.halted_sp.map(|sp| self.bus.read_word(sp))
    }

    pub fn halted_stack_word(&self, word_index: u32) -> Option<u16> {
        self.halted_sp
            .map(|sp| self.bus.read_word(sp + word_index.saturating_mul(2)))
    }

    pub fn halted_d0(&self) -> Option<u32> {
        self.halted_d0
    }

    pub fn total_instructions(&self) -> u64 {
        self.total_instructions
    }

    pub fn debug_overlay_snapshot(
        &mut self,
        frame_stats: DebugOverlayFrameStats,
    ) -> DebugOverlaySnapshot {
        use crate::memory::globals::addr;

        let window_bounds = self.window_bounds();
        let window_count = self.window_count();
        let dispatcher = &self.dispatcher;
        let (_, _, screen_width, screen_height, pixel_size) = dispatcher.screen_mode;
        let cursor_image = dispatcher.cursor_data();
        let cursor_mask_nonzero_bytes = cursor_image
            .as_ref()
            .map(|(_, mask, _, _)| mask.iter().filter(|&&byte| byte != 0).count());
        let cursor_hotspot = cursor_image
            .as_ref()
            .map(|(_, _, hot_v, hot_h)| (*hot_v, *hot_h));

        DebugOverlaySnapshot {
            frame_stats,
            guest_tick: self.guest_tick(),
            total_instructions: self.total_instructions,
            trap_count: dispatcher.trap_count,
            game_trap_count: dispatcher.game_trap_count,
            cursor_visible: dispatcher.cursor_visible(),
            cursor_level: dispatcher.cursor_level(),
            cursor_data_present: dispatcher.cursor_data_present(),
            cursor_mask_nonzero_bytes,
            cursor_hotspot,
            cursor_position: dispatcher.mouse_position(),
            mouse_button: dispatcher.input_state.mouse_button_pressed(),
            fullscreen_locked: dispatcher.fullscreen_locked,
            mbar_height: self.bus.read_word(addr::MBAR_HEIGHT),
            screen_width,
            screen_height,
            pixel_size,
            front_window: dispatcher.front_window(),
            window_bounds,
            window_count,
            menu_count: dispatcher.menu_count(),
            halted: self.halted,
            halted_trap: self.halted_trap,
            halted_pc: self.halted_pc,
        }
    }

    pub fn cpu(&self) -> &M68kCpu {
        &self.m68k.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut M68kCpu {
        &mut self.m68k.cpu
    }

    pub fn bus(&self) -> &MacMemoryBus {
        &self.bus
    }

    pub fn bus_mut(&mut self) -> &mut MacMemoryBus {
        &mut self.bus
    }

    pub fn debug_is_paused(&self) -> bool {
        self.guest_work_is_suspended()
    }

    fn guest_work_is_suspended(&self) -> bool {
        #[cfg(feature = "debug")]
        {
            return self.debug.is_paused();
        }
        #[cfg(not(feature = "debug"))]
        {
            return false;
        }
    }

    pub fn dispatcher(&self) -> &crate::trap::dispatch::TrapDispatcher {
        &self.dispatcher
    }

    pub fn dispatcher_mut(&mut self) -> &mut crate::trap::dispatch::TrapDispatcher {
        &mut self.dispatcher
    }

    /// Global content bounds of the frontmost visible guest window.
    pub fn window_bounds(&mut self) -> (i16, i16, i16, i16) {
        self.native.application_mut().map_or_else(
            || self.dispatcher.window_bounds(),
            PpcLoadedApp::window_bounds,
        )
    }

    /// Number of live guest windows, independent of the active CPU adapter.
    pub fn window_count(&mut self) -> usize {
        self.native.application_mut().map_or_else(
            || self.dispatcher.window_count(),
            PpcLoadedApp::window_count,
        )
    }

    #[doc(hidden)]
    pub fn window_stack_snapshot(&mut self) -> Vec<WindowSnapshot> {
        crate::window_manager::snapshot_window_stack(&self.dispatcher.window_list, |address| {
            self.bus.read_byte(address)
        })
    }

    /// Returns the selected UI theme provider. `classic-system7` is the
    /// standard presentation; `systemless-default` remains optional.
    pub fn ui_theme(&self) -> &'static dyn UiTheme {
        self.config.ui_theme.provider()
    }

    pub fn ui_theme_id(&self) -> UiThemeId {
        self.config.ui_theme
    }

    /// Select a presentation provider before initializing the guest UI.
    /// Theme providers preserve classic guest metrics unless configured
    /// separately, so this changes pixels without changing Toolbox layout.
    pub fn set_ui_theme(&mut self, ui_theme: UiThemeId) {
        self.config.ui_theme = ui_theme;
        self.dispatcher.set_ui_theme_id(ui_theme);
    }

    pub fn theme_metrics_mode(&self) -> ThemeMetricsMode {
        self.config.theme_metrics_mode
    }

    pub fn uses_classic_guest_metrics(&self) -> bool {
        self.config
            .theme_metrics_mode
            .preserves_classic_guest_metrics()
    }

    /// Select the host presentation policy for the classic Mac menu bar.
    pub fn set_menu_bar_policy(&mut self, policy: MenuBarPolicy) {
        self.config.menu_bar_policy = policy;
        self.dispatcher.set_menu_bar_policy(policy);
    }

    /// Return the current host presentation policy for the menu bar.
    pub fn menu_bar_policy(&self) -> MenuBarPolicy {
        self.dispatcher.menu_bar_policy
    }

    /// Compatibility toggle for embedders using the older boolean API.
    ///
    /// `true` restores guest-controlled visibility. `false` permanently hides
    /// the bar; game frontends that only need an initial kiosk presentation
    /// should use [`MenuBarPolicy::InitialKiosk`] instead.
    ///
    /// Inside Macintosh Volume I, I-354 (DrawMenuBar);
    /// Inside Macintosh Volume V, V-245 (MBarHeight global).
    pub fn set_menu_bar_visible(&mut self, visible: bool) {
        self.set_menu_bar_policy(if visible {
            MenuBarPolicy::GuestControlled
        } else {
            MenuBarPolicy::ForceHidden
        });
    }

    /// Returns whether host policy currently permits the Mac menu bar to be
    /// rendered. Guest `MBarHeight` and fullscreen state may still hide it.
    pub fn menu_bar_visible(&self) -> bool {
        !self.dispatcher.menu_bar_hidden
    }

    /// Disassemble `count` M68K instructions starting at `pc`.
    ///
    /// Returns `(pc, mnemonic, size_in_bytes)` for each instruction. The size
    /// includes extension words; advance `pc` by `size` to reach the next
    /// instruction.
    ///
    /// Unknown opcodes (including A-line traps and other reserved
    /// patterns) come back as `DC.W $XXXX` with size 2 — the same
    /// convention the underlying [`m68k::dasm::disassemble`] uses.
    /// Reads past the end of guest RAM yield `(addr, "<unmapped>", 2)`
    /// rather than panicking.
    ///
    /// Diagnostic helper for pixel-divergence and trap-misroute
    /// investigations: pair with the framebuffer-write tracer
    /// (`SYSTEMLESS_TRACE_FB_WRITE_RANGE`) to see what the guest is
    /// actually executing at a suspect PC.
    pub fn disassemble_at(&self, pc: u32, count: usize) -> Vec<(u32, String, u32)> {
        use crate::memory::MemoryBus;
        let mut out = Vec::with_capacity(count);
        let mut cur = pc;
        for _ in 0..count {
            // bus.read_word returns 0 for OOB rather than panicking,
            // so wrap-around safety is a property of the underlying
            // bus impl. Tag explicitly when the read landed at an
            // address beyond the configured guest RAM.
            let opcode = self.bus.read_word(cur);
            let unmapped = self.bus.translate_guest_address(cur) >= self.bus.ram_size();
            let (mnemonic, size) = if unmapped {
                ("<unmapped>".to_string(), 2)
            } else {
                m68k::dasm::disassemble(cur, opcode, m68k::CpuType::M68000)
            };
            // m68k's disassemble returns the instruction's TOTAL size
            // including operand words; cap at a reasonable max so a
            // malformed opcode doesn't run away.
            let size = size.clamp(2, 10);
            out.push((cur, mnemonic, size));
            cur = cur.wrapping_add(size);
        }
        out
    }

    /// Dump the top-N M68K opcodes by execution count. No-op when
    /// `SYSTEMLESS_TRACE_OPCODE_COUNTS` wasn't set at startup. Format:
    ///   [OPCODE-HIST]   43210123  $3F3C  MOVE.W #imm,-(SP)
    /// Unknown opcodes fall back to showing just the hex word.
    pub fn print_opcode_histogram(&self, top_n: usize) {
        if !trace_opcode_counts_enabled() {
            return;
        }
        let mut entries: Vec<(u16, u64)> = self
            .opcode_histogram
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| if c > 0 { Some((i as u16, c)) } else { None })
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.1));
        let total: u64 = entries.iter().map(|(_, c)| c).sum();
        eprintln!(
            "[OPCODE-HIST] top {} of {} distinct opcodes ({} total non-Aline instructions)",
            top_n.min(entries.len()),
            entries.len(),
            total
        );
        for (opcode, count) in entries.iter().take(top_n) {
            let group = (opcode >> 12) & 0xF;
            let group_name = match group {
                0x0 => "bit-op/MOVEP/immediate",
                0x1 => "MOVE.B",
                0x2 => "MOVE.L",
                0x3 => "MOVE.W",
                0x4 => "misc (LEA/JSR/etc.)",
                0x5 => "ADDQ/SUBQ/Scc/DBcc",
                0x6 => "Bcc/BSR",
                0x7 => "MOVEQ",
                0x8 => "OR/DIV/SBCD",
                0x9 => "SUB/SUBX",
                0xA => "A-line (should be in trap-hist)",
                0xB => "CMP/EOR",
                0xC => "AND/MUL/ABCD/EXG",
                0xD => "ADD/ADDX",
                0xE => "shift/rotate",
                0xF => "F-line (FPU/coproc)",
                _ => "?",
            };
            eprintln!(
                "[OPCODE-HIST]   {:>10}  ${:04X}  group {:X}: {}",
                count, opcode, group, group_name
            );
        }
    }

    /// Dump the top-N hottest PCs by sampled hit count. No-op when
    /// `SYSTEMLESS_TRACE_HOT_PC` is unset. Each count represents one
    /// `PC_SAMPLE_INTERVAL` (=1000) M68K instructions; multiply by
    /// 1000 for an approximate instruction count attributed to that
    /// PC.
    pub fn print_pc_histogram(&self, top_n: usize) {
        if !trace_hot_pc_enabled() {
            return;
        }
        let mut entries: Vec<(u32, u64)> =
            self.pc_histogram.iter().map(|(&a, &c)| (a, c)).collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.1));
        let total: u64 = entries.iter().map(|(_, c)| c).sum();
        eprintln!(
            "[PC-HIST] top {} of {} distinct PCs ({} samples × {} = ~{} instructions)",
            top_n.min(entries.len()),
            entries.len(),
            total,
            PC_SAMPLE_INTERVAL,
            total * PC_SAMPLE_INTERVAL
        );
        for (pc, count) in entries.iter().take(top_n) {
            // Classify by address region: the common Mac-app
            // convention for loaded segments is roughly $00010000-
            // $00600000 (game code) and $01000000+ (ROM).
            let region = match *pc {
                0x0000_0000..=0x0000_FFFF => "low-mem",
                0x0001_0000..=0x005F_FFFF => "app code",
                0x0060_0000..=0x00FF_FFFF => "heap/data",
                0x0100_0000..=0x01FF_FFFF => "ROM",
                _ => "other",
            };
            eprintln!("[PC-HIST]   {:>8}  PC=${:08X}  ({})", count, pc, region);
        }
    }

    /// Dump the PPC fetched-instruction histogram. No-op unless
    /// `SYSTEMLESS_TRACE_PPC_FETCH_COUNTS=1` is set. This is
    /// reachable-code evidence: unlike PEF static scans, literal
    /// pools and embedded data never enter this count.
    pub fn print_ppc_fetch_histogram(&self, top_n: usize) {
        if !trace_ppc_fetch_counts_enabled() {
            return;
        }
        let decode_summary = self.ppc_fetch_histogram.decode_summary();
        eprintln!(
            "[PPC-FETCH-HIST] decoder coverage: {} decoded / {} total fetched instructions ({} unsupported)",
            decode_summary.decoded(),
            decode_summary.total(),
            decode_summary.unsupported()
        );
        if !decode_summary.is_fully_decoded() {
            let mut unsupported_primary: Vec<(u8, u64)> = decode_summary
                .unsupported_primary()
                .iter()
                .map(|(&primary, &count)| (primary, count))
                .collect();
            unsupported_primary.sort_by_key(|entry| std::cmp::Reverse(entry.1));
            for (primary, count) in unsupported_primary.iter().take(top_n) {
                eprintln!(
                    "[PPC-FETCH-HIST]   {:>10}  unsupported primary {:02}",
                    count, primary
                );
            }

            let mut unsupported_secondary: Vec<((u8, u16), u64)> = decode_summary
                .unsupported_secondary()
                .iter()
                .map(|(&key, &count)| (key, count))
                .collect();
            unsupported_secondary.sort_by_key(|entry| std::cmp::Reverse(entry.1));
            for ((primary, secondary), count) in unsupported_secondary.iter().take(top_n) {
                eprintln!(
                    "[PPC-FETCH-HIST]   {:>10}  unsupported primary {:02} secondary {:03}",
                    count, primary, secondary
                );
            }
        }

        let mut primaries: Vec<(u8, u64)> = (0u8..64)
            .filter_map(|primary| {
                let count = self.ppc_fetch_histogram.primary_count(primary);
                (count > 0).then_some((primary, count))
            })
            .collect();
        primaries.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        eprintln!(
            "[PPC-FETCH-HIST] top {} of {} primary opcodes ({} total fetched instructions)",
            top_n.min(primaries.len()),
            primaries.len(),
            self.ppc_fetch_histogram.total()
        );
        for (primary, count) in primaries.iter().take(top_n) {
            eprintln!("[PPC-FETCH-HIST]   {:>10}  primary {:02}", count, primary);
        }

        let mut secondaries: Vec<((u8, u16), u64)> = self
            .ppc_fetch_histogram
            .secondary_counts()
            .iter()
            .map(|(&key, &count)| (key, count))
            .collect();
        secondaries.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        eprintln!(
            "[PPC-FETCH-HIST] top {} of {} secondary opcode buckets",
            top_n.min(secondaries.len()),
            secondaries.len()
        );
        for ((primary, secondary), count) in secondaries.iter().take(top_n) {
            eprintln!(
                "[PPC-FETCH-HIST]   {:>10}  primary {:02} secondary {:03}",
                count, primary, secondary
            );
        }

        let mut pcs: Vec<(u32, u64)> = self
            .ppc_fetch_histogram
            .pc_counts()
            .iter()
            .map(|(&pc, &count)| (pc, count))
            .collect();
        pcs.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        eprintln!(
            "[PPC-FETCH-HIST] top {} of {} fetched PCs",
            top_n.min(pcs.len()),
            pcs.len()
        );
        for (pc, count) in pcs.iter().take(top_n) {
            eprintln!("[PPC-FETCH-HIST]   {:>10}  pc ${:08X}", count, pc);
        }

        let mut words: Vec<(u32, u64)> = self
            .ppc_fetch_histogram
            .word_counts()
            .iter()
            .map(|(&word, &count)| (word, count))
            .collect();
        words.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        eprintln!(
            "[PPC-FETCH-HIST] top {} of {} fetched instruction words",
            top_n.min(words.len()),
            words.len()
        );
        for (word, count) in words.iter().take(top_n) {
            eprintln!("[PPC-FETCH-HIST]   {:>10}  word ${:08X}", count, word);
        }
    }

    /// Dump the PPC HLE import histogram. No-op unless
    /// `SYSTEMLESS_PPC_IMPORT_HIST=1` is set.
    pub fn print_ppc_import_histogram(&self, top_n: usize) {
        if !ppc_import_hist_enabled() {
            return;
        }
        eprint!(
            "{}",
            format_ppc_import_histogram(&self.ppc_import_histogram, top_n)
        );
    }

    fn record_ppc_import_histogram(&mut self, trace: &[PpcHleImportTraceEntry]) {
        merge_ppc_import_histogram(&mut self.ppc_import_histogram, trace);
    }

    /// Dump tracked PPC GWorld fingerprints. No-op unless
    /// `SYSTEMLESS_PPC_GWORLD_DUMP=1` is set.
    pub fn print_ppc_gworld_dump(&mut self, top_n: usize) {
        if !ppc_gworld_dump_enabled() {
            return;
        }
        let Some(ppc_app) = self.native.application_mut() else {
            return;
        };
        eprint!(
            "{}",
            format_ppc_gworld_dump(
                &mut ppc_app.memory,
                &ppc_app.gworlds,
                PpcGWorldDumpState {
                    current_gworld: *ppc_app.current_gworld,
                    front_gworld: ppc_app.draw_sprocket.front_buffer_gworld,
                    back_gworld: ppc_app.draw_sprocket.back_buffer_gworld,
                    swap_count: ppc_app.draw_sprocket.swap_count,
                },
                top_n
            )
        );
    }

    /// Dump unsupported PPC runtime stops. No-op unless
    /// `SYSTEMLESS_PPC_UNIMPL_HIST=1` is set.
    pub fn print_ppc_unimpl_histogram(&self, top_n: usize) {
        if !ppc_unimpl_hist_enabled() {
            return;
        }
        eprint!(
            "{}",
            format_ppc_unimpl_histogram(&self.ppc_unimpl_histogram, top_n)
        );
    }

    fn record_ppc_unimpl_histogram(
        &mut self,
        imports: &[PpcImportBinding],
        result: PpcRunResult,
        unsupported_import_index: Option<u32>,
    ) {
        if let Some(key) = ppc_unimpl_histogram_key(imports, result, unsupported_import_index) {
            merge_ppc_unimpl_histogram(&mut self.ppc_unimpl_histogram, key);
        }
    }

    pub fn install_application_clut(&mut self, clut: [[u16; 3]; 256]) {
        self.dispatcher
            .install_application_clut(&mut self.bus, clut);
    }

    pub fn set_app_start_time(&mut self, secs: u32) {
        self.app_start_time = Some(secs);
    }

    #[cfg(any(test, feature = "test-support"))]
    /// Override the guest-visible state at application launch.
    /// Scripted frontends use this to align otherwise independent
    /// runtime boot histories without changing normal frontend defaults. The
    /// tick value is a monotonic floor, not permission to rewind TickCount.
    pub fn set_launch_state(&mut self, ticks: u32, rnd_seed: u32, ppc_time_base: u64) {
        self.set_optional_launch_state(Some(ticks), Some(rnd_seed), Some(ppc_time_base));
    }

    #[cfg(any(test, feature = "test-support"))]
    /// Apply independently optional launch-state controls. A requested tick
    /// value is a monotonic floor; initialization never rewinds the runtime's
    /// established launch default. Missing controls leave existing defaults or
    /// overrides unchanged.
    pub fn set_optional_launch_state(
        &mut self,
        ticks: Option<u32>,
        rnd_seed: Option<u32>,
        ppc_time_base: Option<u64>,
    ) {
        if let Some(ticks) = ticks {
            self.launch_ticks_override = Some(ticks);
        }
        if let Some(rnd_seed) = rnd_seed {
            self.launch_rnd_seed_override = Some(rnd_seed);
        }
        if let Some(ppc_time_base) = ppc_time_base {
            self.launch_ppc_time_base_override = Some(ppc_time_base);
        }
    }

    pub fn set_application_partition_size(&mut self, bytes: Option<u32>) {
        self.application_partition_size = bytes.filter(|&bytes| bytes >= 128 * 1024);
    }

    /// Getter for the pinned Mac epoch seconds. Returns `None` when no
    /// pin has been applied — without a pin, `init_app` falls back to
    /// `current_mac_epoch_seconds()` (host wall-clock), which leaks
    /// into the guest's `Time` global (`$020C`) and breaks
    /// reproducibility.
    pub fn app_start_time(&self) -> Option<u32> {
        self.app_start_time
    }

    pub fn application_partition_size(&self) -> Option<u32> {
        self.application_partition_size
    }

    /// Install a trace sink to receive runtime events and screen
    /// snapshots (see [`crate::trace::TraceSink`]). The host owns the sink
    /// and decides how/where its output is persisted.
    pub fn set_trace_sink(&mut self, sink: Box<dyn crate::trace::TraceSink>) {
        self.dispatcher.set_trace_sink(sink)
    }

    pub fn record_oracle_checkpoint_snapshot(&mut self, name: &str) -> Result<u64> {
        self.composite_frame();
        self.dispatcher.record_trace_event(
            &self.bus,
            self.current_oracle_pc(),
            "checkpoint_snapshot",
            BTreeMap::from([("name".to_string(), name.to_string())]),
            true,
        )?;
        Ok(self.dispatcher.screen_event_count)
    }

    pub fn record_oracle_script_input(&mut self, fields: BTreeMap<String, String>) -> Result<()> {
        self.dispatcher.record_trace_event(
            &self.bus,
            self.current_oracle_pc(),
            "script_input",
            fields,
            false,
        )
    }

    fn current_oracle_pc(&self) -> u32 {
        self.native
            .application()
            .map(|app| app.cpu.pc)
            .unwrap_or_else(|| self.m68k.cpu.read_reg(Register::PC))
    }

    /// Advance menu feedback by uncapped elapsed host time, once per frame.
    /// This remains independent of CPU catch-up limits and frozen app ticks.
    /// Toolbox Essentials (1992), SetMenuFlash, p. 3-142.
    pub fn advance_menu_presentation_clock(&mut self, elapsed: std::time::Duration) {
        if self.guest_work_is_suspended() {
            return;
        }
        let Some(tracking) = self.process_context.menu_tracking() else {
            self.menu_presentation_remainder = 0;
            return;
        };
        let tick = tracking.flash_tick.unwrap_or(self.guest_tick());
        let scaled = elapsed.as_nanos() * 60 + u128::from(self.menu_presentation_remainder);
        self.menu_presentation_remainder = (scaled % 1_000_000_000) as u64;
        self.process_context
            .set_menu_presentation_tick(tick.wrapping_add((scaled / 1_000_000_000) as u32));
    }

    /// Prepare the same sharp text surface for either guest CPU and any frontend.
    /// Call after initialization and before presenting a frame to track mode changes.
    pub fn prepare_text_presentation(&mut self) {
        let display_gamma = self.dispatcher.display_gamma.table();
        let palette = crate::display::rgba_palette_from_clut_with_gamma(
            &self.dispatcher.device_clut,
            &display_gamma,
        )
        .map(|word| {
            let [r, g, b, _] = word.to_le_bytes();
            [r, g, b]
        });
        self.bus
            .prepare_outline_presentation(self.dispatcher.screen_mode, palette);
    }

    /// Synchronize deferred PPC visual/VFS state and composite chrome/dialog overlays.
    /// Call before reading raw pixels for screenshots.
    pub fn composite_frame(&mut self) {
        if self.guest_work_is_suspended() && !self.halted {
            return;
        }
        self.sync_ppc_deferred_host_state();
        self.redraw_chrome_outside_idle_journal();
    }

    fn redraw_chrome(&mut self) {
        self.dispatcher.with_process_state(|dispatcher| {
            dispatcher.redraw_chrome(&mut self.bus);
        });
    }

    /// Enable or disable arrow-key-to-numpad remapping.
    pub fn set_arrows_as_numpad(&mut self, enabled: bool) {
        self.config.arrows_as_numpad = enabled;
    }

    /// Returns true when arrow keys are remapped to numpad key codes.
    pub fn arrows_as_numpad(&self) -> bool {
        self.config.arrows_as_numpad
    }

    /// Return the configured initial 68K indexed main-framebuffer depth.
    /// This does not report an explicit or default native PowerPC depth.
    pub fn configured_screen_depth(&self) -> u16 {
        self.config.screen_depth
    }

    /// Explicitly select the display depth used by subsequently loaded native
    /// PowerPC applications. Leaving this unset preserves the historical
    /// 16-bit PowerPC architecture default, independently of the configured
    /// 68K framebuffer depth.
    pub fn set_powerpc_screen_depth(
        &mut self,
        screen_depth: u16,
    ) -> std::result::Result<(), String> {
        if !matches!(screen_depth, 1 | 2 | 4 | 8 | 16) {
            return Err(format!(
                "unsupported PowerPC screen depth {screen_depth}; expected 1, 2, 4, 8, or 16"
            ));
        }
        self.powerpc_screen_depth_override = Some(screen_depth);
        Ok(())
    }

    /// Return the screen depth to use when the selected executable is native
    /// PowerPC. The architecture default is 16bpp; only an explicit call to
    /// [`FixtureRunner::set_powerpc_screen_depth`] overrides it.
    pub(crate) fn configured_powerpc_screen_depth(&self) -> u32 {
        u32::from(
            self.powerpc_screen_depth_override
                .unwrap_or(DEFAULT_POWERPC_SCREEN_DEPTH as u16),
        )
    }

    /// Return the system-owned synthetic code range that a native loader must
    /// exclude from startup allocations. The live shared mapping is attached
    /// only after the loaded state has crossed its detached snapshot boundary.
    pub(crate) fn powerpc_system_reservation_range(&self) -> Option<(u32, u32)> {
        self.bus.synthetic_reservation_range()
    }

    /// Move the mouse without changing the button state. Coordinates are in
    /// the runner's presented framebuffer; a centered native framebuffer is
    /// translated to Macintosh global coordinates at this host-input boundary.
    /// Updates the dispatcher's tracked position and the six mouse-position
    /// low-memory globals (MTemp / RawMouse / Mouse) so guest code that
    /// reads them directly sees the new coordinates immediately. Leaves
    /// MBState ($0172) untouched. Inside Macintosh Volume II, II-371.
    pub fn set_mouse_position(&mut self, v: i16, h: i16) {
        if self.guest_work_is_suspended() {
            return;
        }
        let (v, h) = self.canonical_mouse_position(v, h);
        self.dispatcher.set_mouse_position(v, h);
        self.sync_mouse_position_lowmem();
        self.wake_pending_wait_next_event_if_input_available();
        self.wake_foreground_after_input();
    }

    /// Return an immutable snapshot of the guest's current Menu Manager list.
    /// Native and web frontends can use this without exposing mutable Toolbox
    /// internals.
    pub fn guest_menu_snapshot(&mut self) -> GuestMenuSnapshot {
        if let Some(ppc_app) = self.native.application_mut() {
            ppc_app.guest_menu_snapshot()
        } else {
            self.dispatcher.guest_menu_snapshot(&self.bus)
        }
    }

    /// Inspect caller-created TextEdit records and the process's private scrap.
    #[doc(hidden)]
    pub fn text_edit_snapshot(&mut self) -> TextEditManagerSnapshot {
        if let Some(app) = self.native.application_mut() {
            let handles = app.scrap.text_edit.handles();
            crate::text_edit::snapshot_guest_records(&handles, &mut |addr| app.memory.read_u8(addr))
        } else {
            let handles = self.dispatcher.textedit_states.handles();
            crate::text_edit::snapshot_guest_records(&handles, &mut |addr| {
                Some(self.bus.read_byte(addr))
            })
        }
    }

    /// Inspect logical list contents and the visibility/highlight bytes of
    /// their guest scrollbar controls on either CPU architecture.
    #[doc(hidden)]
    pub fn list_manager_snapshot(&mut self) -> Vec<ListManagerSnapshot> {
        use crate::list_manager::ProcessListRecord;
        let snapshot =
            |record: &ProcessListRecord, bars: [Option<(bool, u8)>; 2]| ListManagerSnapshot {
                view_rect: record.view_rect,
                data_bounds: record.data_bounds,
                cell_size: record.cell_size,
                visible: record.visible,
                draw_enabled: record.draw_enabled,
                active: record.active,
                cells: record
                    .cells
                    .iter()
                    .map(|(cell, bytes)| (*cell, bytes.clone()))
                    .collect(),
                selected: record.selected.clone(),
                vertical_scrollbar: bars[0],
                horizontal_scrollbar: bars[1],
            };
        // ListRec.vScroll/hScroll: More Macintosh Toolbox, pp. 4-3--4-7.
        // ControlRecord.contrlVis/contrlHilite: Toolbox Essentials, pp. 5-61--5-63.
        let mut lists = if let Some(app) = self.native.application_mut() {
            let records = app.list_manager.records();
            records
                .iter()
                .map(|record| {
                    let bars = [28, 32].map(|offset| {
                        let ptr = app.memory.read_u32_be(record.handle).filter(|p| *p != 0)?;
                        let handle = app.memory.read_u32_be(ptr + offset).filter(|p| *p != 0)?;
                        let control = app.memory.read_u32_be(handle).filter(|p| *p != 0)?;
                        Some((
                            app.memory.read_u8(control + 16)? != 0,
                            app.memory.read_u8(control + 17)?,
                        ))
                    });
                    (record.handle, snapshot(record, bars))
                })
                .collect::<Vec<_>>()
        } else {
            let records = self.dispatcher.list_states.records();
            records
                .iter()
                .map(|record| {
                    let bars = [28, 32].map(|offset| {
                        let ptr = self.bus.read_long(record.handle);
                        if ptr == 0 {
                            return None;
                        }
                        let handle = self.bus.read_long(ptr + offset);
                        if handle == 0 {
                            return None;
                        }
                        let control = self.bus.read_long(handle);
                        if control == 0 {
                            return None;
                        }
                        Some((
                            self.bus.read_byte(control + 16) != 0,
                            self.bus.read_byte(control + 17),
                        ))
                    });
                    (record.handle, snapshot(record, bars))
                })
                .collect::<Vec<_>>()
        };
        lists.sort_by_key(|(handle, _)| *handle);
        lists.into_iter().map(|(_, snapshot)| snapshot).collect()
    }

    /// Return a deterministic semantic snapshot of the current Resource
    /// Manager file for fixture and diagnostic assertions. The outward shape
    /// is shared by the classic and native PowerPC adapters even though their
    /// internal resource records differ.
    #[doc(hidden)]
    pub fn resource_manager_snapshot(&mut self) -> ResourceManagerSnapshot {
        if let Some(ppc_app) = self.native.application_mut() {
            resource_manager_snapshot_powerpc(&self.dispatcher, ppc_app)
        } else {
            resource_manager_snapshot_classic(&self.dispatcher, &self.bus)
        }
    }

    /// Route a host-presented menu selection back through the guest's normal
    /// mouseDown -> FindWindow -> MenuSelect path.  Returns false if the menu
    /// or item is no longer present, enabled, and selectable.
    pub fn select_guest_menu_item(&mut self, menu_id: i16, item_number: i16) -> bool {
        if self.guest_work_is_suspended() {
            return false;
        }
        // Host input is an ABI boundary too: a guest store to `$016A` made
        // since the last trap must be visible before the queued EventRecord
        // receives its `when` timestamp.
        self.dispatcher.read_tick_count(&self.bus);
        if let Some(ppc_app) = self.native.application_mut() {
            if !ppc_app.queue_native_menu_selection(menu_id, item_number) {
                return false;
            }
            // A native command still enters the guest through its ordinary
            // mouseDown event loop; MenuSelect consumes the staged item after
            // the application recognizes the menu-bar click.
            self.push_canonical_mouse_down(10, 15);
            self.push_canonical_mouse_up(10, 15);
            return true;
        }
        let Some((_v, _h)) =
            self.dispatcher
                .queue_native_menu_selection(&self.bus, menu_id, item_number)
        else {
            return false;
        };
        self.wake_pending_wait_next_event_if_input_available();
        self.wake_foreground_after_input();
        true
    }

    /// Inject a mouse-down event and sync low-memory globals. Coordinates are
    /// in the runner's presented framebuffer and are normalized to Macintosh
    /// global coordinates before the event enters the shared queue.
    ///
    /// On real hardware the VBL interrupt handler updates MBState ($0172)
    /// and the mouse-position globals whenever the button state changes.
    /// Since our HLE has no interrupt-driven mouse driver, we sync these
    /// globals here so that code polling the low-memory locations directly
    /// (instead of calling Button or GetNextEvent) sees the correct state.
    pub fn push_mouse_down(&mut self, v: i16, h: i16) {
        if self.guest_work_is_suspended() {
            return;
        }
        let (v, h) = self.canonical_mouse_position(v, h);
        self.push_canonical_mouse_down(v, h);
    }

    fn push_canonical_mouse_down(&mut self, v: i16, h: i16) {
        self.dispatcher.read_tick_count(&self.bus);
        self.dispatcher
            .with_process_state(|d| d.push_mouse_down(v, h));
        self.sync_mouse_lowmem();
        self.wake_pending_wait_next_event_if_input_available();
        self.wake_foreground_after_input();
    }

    #[cfg(any(test, feature = "test-support"))]
    /// Return the number of queued mouse-down events awaiting guest delivery.
    /// Input diagnostics can use this to observe whether an application has
    /// consumed a posted mouse-down event; physical button duration is tracked
    /// independently.
    pub fn pending_mouse_down_count(&self) -> usize {
        self.process_context
            .event_queue()
            .iter()
            .filter(|event| event.what == 1)
            .count()
    }

    /// Inject a mouse-up event. Coordinates are in the runner's presented
    /// framebuffer and are normalized to Macintosh global coordinates before
    /// the event enters the shared queue.
    ///
    /// Sync MBState ($0172) immediately so code that polls the low-memory
    /// byte directly (rather than calling Button or GetNextEvent) sees the
    /// release without waiting for the next tick advance.
    ///
    /// On real hardware the ADB manager polls the mouse at ~200 Hz, updating
    /// MBState within a few milliseconds of the physical release. Deferring
    /// the update to the next advance_guest_tick left MBState stale for an
    /// entire tick (~16 ms), which is longer than real hardware and caused
    /// frame-rate-dependent games to read the wrong button state for too
    /// many loop iterations after a click-up.
    /// Inside Macintosh Volume II, II-371
    pub fn push_mouse_up(&mut self, v: i16, h: i16) {
        if self.guest_work_is_suspended() {
            return;
        }
        let (v, h) = self.canonical_mouse_position(v, h);
        self.push_canonical_mouse_up(v, h);
    }

    fn push_canonical_mouse_up(&mut self, v: i16, h: i16) {
        self.dispatcher.read_tick_count(&self.bus);
        self.dispatcher
            .with_process_state(|d| d.push_mouse_up(v, h));
        self.sync_mouse_lowmem();
        self.wake_pending_wait_next_event_if_input_available();
        self.wake_foreground_after_input();
    }

    /// Write the three mouse-position low-memory globals (MTemp $0828,
    /// RawMouse $082C, Mouse $0830) from the process input state.
    /// Inside Macintosh Volume I, I-258.
    fn sync_mouse_position_lowmem(&mut self) {
        let (v, h) = self.dispatcher.input_state.mouse_position();
        self.bus.write_word(0x0828, v as u16);
        self.bus.write_word(0x082A, h as u16);
        self.bus.write_word(0x082C, v as u16);
        self.bus.write_word(0x082E, h as u16);
        self.bus.write_word(0x0830, v as u16);
        self.bus.write_word(0x0832, h as u16);
    }

    /// Sync mouse button + position low-memory globals from internal state.
    ///
    /// MBState ($0172): 0x00 = button down, 0x80 = button up
    /// MTemp ($0828), RawMouse ($082C), Mouse ($0830): current position
    /// Inside Macintosh Volume I, I-258; Inside Macintosh Volume II, II-371
    fn sync_mouse_lowmem(&mut self) {
        let mb_state: u8 = if self.dispatcher.input_state.mouse_button_pressed() {
            0x00
        } else {
            0x80
        };
        self.bus.write_byte(0x0172, mb_state);
        self.sync_mouse_position_lowmem();
    }

    /// Sync the 16-byte KeyMapLM low-memory bitmap from the dispatcher's
    /// current key state. Inside Macintosh Volume I, I-260 documents the
    /// KeyMap returned by GetKeys; MPW SysEqu.h exposes the ROM-maintained
    /// low-memory mirror at $0174 for code that polls it directly.
    fn sync_key_map_lowmem(&mut self) {
        use crate::memory::globals::addr;

        self.bus
            .write_bytes(addr::KEY_MAP_LM, &self.dispatcher.key_map_bytes());
    }

    /// Inject a key-down event, applying arrow→numpad remapping if configured.
    pub fn push_key_down(&mut self, mac_key: u8, char_code: u8) {
        if self.guest_work_is_suspended() {
            return;
        }
        let (key, char_code) = self.remap_key(mac_key, char_code);
        self.dispatcher.read_tick_count(&self.bus);
        self.dispatcher.with_process_state(|d| {
            d.push_key_down(key, char_code);
        });
        self.sync_key_map_lowmem();
        self.wake_pending_wait_next_event_if_input_available();
        self.wake_foreground_after_input();
    }

    /// Inject a key-up event, applying arrow→numpad remapping if configured.
    pub fn push_key_up(&mut self, mac_key: u8, char_code: u8) {
        if self.guest_work_is_suspended() {
            return;
        }
        let (key, char_code) = self.remap_key(mac_key, char_code);
        self.dispatcher.read_tick_count(&self.bus);
        let sys_evt_mask = self
            .bus
            .read_word(crate::memory::globals::addr::SYS_EVT_MASK);
        self.dispatcher.with_process_state(|d| {
            d.push_key_up_with_system_event_mask(sys_evt_mask, key, char_code);
        });
        self.sync_key_map_lowmem();
        self.wake_pending_wait_next_event_if_input_available();
        self.wake_foreground_after_input();
    }

    /// Remap arrow key virtual key codes to numpad equivalents when enabled.
    /// Arrow keys: Left=0x7B, Right=0x7C, Down=0x7D, Up=0x7E
    /// Numpad dirs: 4(left)=0x56, 6(right)=0x58, 5(down)=0x57, 8(up)=0x5B
    /// Inside Macintosh Volume V, V-191
    fn remap_key(&self, mac_key: u8, char_code: u8) -> (u8, u8) {
        if self.config.arrows_as_numpad {
            match mac_key {
                0x7B => (0x56, b'4'), // Left  -> Numpad4
                0x7C => (0x58, b'6'), // Right -> Numpad6
                0x7D => (0x57, b'5'), // Down  -> Numpad5
                0x7E => (0x5B, b'8'), // Up    -> Numpad8
                _ => (mac_key, char_code),
            }
        } else {
            (mac_key, char_code)
        }
    }

    /// Set the audio output backend. If not set, no audio is produced.
    pub fn set_audio(&mut self, audio: Box<dyn crate::audio::AudioBackend>) {
        self.audio = Some(audio);
    }

    pub fn set_instructions_per_tick(&mut self, instructions_per_tick: u32) {
        let old = self.instructions_per_tick.max(1);
        let new = instructions_per_tick.max(1);
        // Scale the remaining budget proportionally so a mid-run change
        // doesn't cause an immediate tick advance or an artificially long tick.
        self.tick_budget = ((self.tick_budget as i64 * new as i64) / old as i64) as i32;
        self.instructions_per_tick = new;
    }

    pub fn instructions_per_tick(&self) -> u32 {
        self.instructions_per_tick
    }

    /// Whether the active application executes through the PowerPC runtime.
    pub fn is_powerpc_app(&self) -> bool {
        self.native.application().is_some()
    }

    /// Cap the per-WaitNextEvent-call sleep tick advance in headless mode.
    /// None (default) preserves the legacy drain-all behavior. Some(n) caps
    /// each WNE sleep to at most n tick advances, mirroring GUI mode.
    pub fn set_wait_sleep_cap_in_headless(&mut self, cap: Option<u32>) {
        self.wait_sleep_cap_in_headless = cap;
    }

    pub fn wait_sleep_cap_in_headless(&self) -> Option<u32> {
        self.wait_sleep_cap_in_headless
    }

    /// Drain accumulated audio samples for external consumers (e.g. WASM).
    /// Returns unsigned 8-bit mono PCM at 22050 Hz (silence = 0x80).
    pub fn drain_audio(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.audio_buffer)
    }

    /// Drain accumulated audio samples into a caller-owned buffer.
    ///
    /// This avoids transferring the runner's `Vec` allocation out on every
    /// browser frame, so the next audio mix can reuse its existing capacity.
    pub fn drain_audio_into(&mut self, out: &mut Vec<u8>) {
        out.clear();
        out.extend_from_slice(&self.audio_buffer);
        self.audio_buffer.clear();
    }

    /// Current number of buffered audio samples (for diagnostics).
    pub fn audio_buffer_len(&self) -> usize {
        self.audio_buffer.len()
    }

    pub fn has_pending_sound_work(&self) -> bool {
        let has_routable_process_callback = self
            .dispatcher
            .sound_manager
            .pending_sound_callbacks
            .iter()
            .any(|callback| match callback {
                crate::sound::PendingSoundCallback::Command {
                    architecture: CallbackTaskArchitecture::M68k,
                    ..
                }
                | crate::sound::PendingSoundCallback::FileCompletion {
                    architecture: CallbackTaskArchitecture::M68k,
                    ..
                } => true,
                crate::sound::PendingSoundCallback::Command {
                    architecture: CallbackTaskArchitecture::PowerPc,
                    ..
                }
                | crate::sound::PendingSoundCallback::FileCompletion {
                    architecture: CallbackTaskArchitecture::PowerPc,
                    ..
                } => self.native.application().is_some(),
            });
        self.active_interrupt_callback
            .map(|callback| {
                matches!(
                    callback.source,
                    ActiveInterruptCallbackSource::SoundCallback
                        | ActiveInterruptCallbackSource::SoundFileCompletion
                        | ActiveInterruptCallbackSource::SoundDoubleBack
                )
            })
            .unwrap_or(false)
            || has_routable_process_callback
            || !self.dispatcher.sound_manager.pending_callbacks.is_empty()
    }

    pub fn is_ui_tracking_active(&self) -> bool {
        self.frozen_ticks.is_some()
            || self.process_context.menu_tracking().is_some()
            || self.dispatcher.is_dialog_tracking()
            || self.dispatcher.is_control_tracking()
            || self.dispatcher.is_window_tracking()
            || self.dispatcher.is_grow_window_tracking()
            || self.dispatcher.is_region_tracking()
            || self.dispatcher.textedit_states.has_click_tracking()
    }

    /// Advance the guest tick counter by one, firing VBL and timer tasks.
    /// Used by the GUI runner to force-advance ticks when the CPU can't
    /// keep up with wall-clock time (e.g. during expensive PICT draws).
    pub fn force_advance_guest_tick(&mut self) {
        self.advance_guest_tick();
    }

    pub fn set_output_path(&mut self, path: std::path::PathBuf) {
        // The path points to a specific file (e.g. temp/foo/fixture_dump.bin).
        // Use its parent directory as the VFS output directory.
        if let Some(dir) = path.parent() {
            self.dispatcher.output_dir = Some(dir.to_path_buf());
        }
    }

    /// Get the contents of a file from the virtual filesystem.
    pub fn vfs_read(&self, filename: &str) -> Option<&[u8]> {
        self.dispatcher.vfs.get(filename).map(|v| v.as_slice())
    }

    pub fn vfs_file_summaries(&mut self) -> Vec<VfsFileSummary> {
        self.vfs_file_summaries_where(|_| true)
    }

    pub fn vfs_file_summaries_where<F>(&mut self, mut include: F) -> Vec<VfsFileSummary>
    where
        F: FnMut(&str) -> bool,
    {
        self.vfs_file_paths()
            .into_iter()
            .filter(|path| include(path))
            .filter_map(|path| self.vfs_file_summary_for_path(&path))
            .collect()
    }

    pub fn vfs_file_stats_where<F>(&mut self, mut include: F) -> Vec<VfsFileStat>
    where
        F: FnMut(&str) -> bool,
    {
        self.vfs_file_paths()
            .into_iter()
            .filter(|path| include(path))
            .filter_map(|path| self.vfs_file_stat_for_path(&path))
            .collect()
    }

    pub fn vfs_file_summary(&mut self, path: &str) -> Option<VfsFileSummary> {
        let normalized = TrapDispatcher::normalize_vfs_path(path);
        if normalized.is_empty() || normalized.starts_with("__rsrc__") {
            return None;
        }
        self.vfs_file_summary_for_path(&normalized)
    }

    pub fn vfs_file_snapshot(&mut self, path: &str) -> Option<VfsFileSnapshot> {
        let normalized = TrapDispatcher::normalize_vfs_path(path);
        if normalized.is_empty() || normalized.starts_with("__rsrc__") {
            return None;
        }
        if !self.dispatcher.vfs.contains_key(&normalized)
            && !self.dispatcher.vfs_rsrc.contains_key(&normalized)
        {
            return None;
        }
        let metadata = self.dispatcher.vfs_file_metadata(&normalized)?;
        Some(VfsFileSnapshot {
            path: normalized.clone(),
            data_fork: self
                .dispatcher
                .vfs
                .get(&normalized)
                .map(|bytes| bytes.to_vec())
                .unwrap_or_default(),
            resource_fork: self
                .dispatcher
                .vfs_rsrc
                .get(&normalized)
                .map(|bytes| bytes.to_vec())
                .unwrap_or_default(),
            file_type: metadata.file_type,
            creator: metadata.creator,
            finder_flags: metadata.finder_flags,
            created_date: metadata.created_date,
            modified_date: metadata.modified_date,
        })
    }

    /// Reconstruct resource entries embedded by installers and publish the
    /// complete forks through the virtual filesystem snapshot API.
    pub fn prepare_vfs_resource_forks_for_native_export(&mut self) -> usize {
        let Some(mut native_context) = self.native.take(NativeEngineRole::Application) else {
            return 0;
        };
        let mut ppc_app = native_context.adapter_mut();
        let materialized_count = ppc_app.prepare_vfs_resource_forks_for_native_export();
        self.persist_ppc_vfs_to_host(&mut ppc_app);
        self.native
            .restore(native_context)
            .unwrap_or_else(|_| panic!("native context lost its owner"));
        materialized_count
    }

    pub fn import_vfs_file(&mut self, file: &VfsFileSnapshot) {
        let normalized = TrapDispatcher::normalize_vfs_path(&file.path);
        if normalized.is_empty() || normalized.starts_with("__rsrc__") {
            return;
        }

        self.dispatcher
            .vfs
            .insert(normalized.clone(), file.data_fork.clone());
        self.dispatcher
            .vfs_rsrc
            .insert(normalized.clone(), file.resource_fork.clone());
        self.dispatcher.set_vfs_entry_finfo(
            &normalized,
            file.file_type,
            file.creator,
            file.finder_flags,
        );
        self.dispatcher.vfs_metadata.update(&normalized, |metadata| {
            if file.created_date != 0 {
                metadata.created_date = file.created_date;
            }
            if file.modified_date != 0 {
                metadata.modified_date = file.modified_date;
            }
        });
    }

    pub fn import_vfs_file_relative_to_launched_app(
        &mut self,
        relative_dir: &str,
        file: &VfsFileSnapshot,
    ) -> std::result::Result<(), String> {
        let app_path = self
            .dispatcher
            .launched_app_path()
            .map(str::to_owned)
            .ok_or_else(|| "launched app path is not available".to_string())?;
        let app_parent = TrapDispatcher::vfs_parent_path(&app_path);
        if app_parent.is_empty() {
            return Err("launched app has no parent folder".to_string());
        }

        let relative_dir = TrapDispatcher::normalize_vfs_path(relative_dir);
        if relative_dir.is_empty() || relative_dir.starts_with('/') || relative_dir.contains("..") {
            return Err(format!("invalid relative VFS directory {relative_dir:?}"));
        }
        let file_path = TrapDispatcher::normalize_vfs_path(&file.path);
        let filename = TrapDispatcher::vfs_basename(&file_path);
        if filename.is_empty() {
            return Err("plugin file has no filename".to_string());
        }

        let mut mounted = file.clone();
        mounted.path = format!("{app_parent}/{relative_dir}/{filename}");
        self.import_vfs_file(&mounted);
        Ok(())
    }

    pub fn remove_vfs_file(&mut self, path: &str) -> bool {
        self.dispatcher.remove_vfs_path(path)
    }

    fn vfs_file_paths(&mut self) -> Vec<String> {
        self.dispatcher.ensure_vfs_catalog();
        let mut paths = BTreeSet::new();
        for path in self.dispatcher.vfs.keys() {
            if !path.starts_with("__rsrc__") {
                paths.insert(TrapDispatcher::normalize_vfs_path(path));
            }
        }
        for path in self.dispatcher.vfs_rsrc.keys() {
            if !path.starts_with("__rsrc__") {
                paths.insert(TrapDispatcher::normalize_vfs_path(path));
            }
        }
        paths.into_iter().filter(|path| !path.is_empty()).collect()
    }

    fn vfs_file_summary_for_path(&mut self, path: &str) -> Option<VfsFileSummary> {
        let stat = self.vfs_file_stat_for_path(path)?;
        let data_fork = self
            .dispatcher
            .vfs
            .get(path)
            .map(|bytes| bytes.as_slice())
            .unwrap_or(&[]);
        let resource_fork = self
            .dispatcher
            .vfs_rsrc
            .get(path)
            .map(|bytes| bytes.as_slice())
            .unwrap_or(&[]);
        Some(VfsFileSummary {
            path: stat.path,
            data_len: stat.data_len,
            resource_len: stat.resource_len,
            data_hash: vfs_fork_hash(data_fork),
            resource_hash: vfs_fork_hash(resource_fork),
            file_type: stat.file_type,
            creator: stat.creator,
            finder_flags: stat.finder_flags,
            created_date: stat.created_date,
            modified_date: stat.modified_date,
        })
    }

    fn vfs_file_stat_for_path(&mut self, path: &str) -> Option<VfsFileStat> {
        let metadata = self.dispatcher.vfs_file_metadata(path)?;
        let data_len = self
            .dispatcher
            .vfs
            .get(path)
            .map(|bytes| bytes.len())
            .unwrap_or(0);
        let resource_len = self
            .dispatcher
            .vfs_rsrc
            .get(path)
            .map(|bytes| bytes.len())
            .unwrap_or(0);
        Some(VfsFileStat {
            path: path.to_string(),
            data_len,
            resource_len,
            file_type: metadata.file_type,
            creator: metadata.creator,
            finder_flags: metadata.finder_flags,
            created_date: metadata.created_date,
            modified_date: metadata.modified_date,
        })
    }

    /// Execute exactly one 68k instruction through the precise CPU path.
    ///
    /// The result distinguishes ordinary completion, STOP, and an A-line trap.
    /// Most embedders should call [`run_steps`](Self::run_steps) instead; it
    /// amortizes tick advancement, halt detection, and trace collection across
    /// the instruction budget.
    pub fn step(&mut self) -> StepResult {
        if self.guest_work_is_suspended() {
            return StepResult::Blocked;
        }
        let previous_mouse = self.bus.read_long(crate::memory::globals::addr::MOUSE_LOC2);
        let result = self.m68k.cpu.step(&mut self.bus);
        self.sync_guest_mouse_position(previous_mouse);
        self.dispatcher
            .retire_returned_native_trap_call(&mut self.m68k.cpu);
        result
    }

    /// Load a parsed Mac resource fork into guest memory: registers
    /// every resource with the Resource Manager, links the application
    /// CODE segments through the trap dispatcher's segment table, and
    /// returns a [`LoadedApp`] describing the entry-point base address
    /// and per-segment offsets.
    ///
    /// Lower-level than [`systemless::game::load_game`](crate::game::load_game) —
    /// that helper auto-detects StuffIt / MacBinary / raw-resource-fork
    /// containers and calls this method internally. Use `load_app`
    /// directly only when you've already parsed the resource fork
    /// yourself (e.g. building a custom test fixture).
    pub fn load_app(&mut self, fork: &ResourceFork) -> Option<LoadedApp> {
        let app = load_app_generic(fork, &mut self.bus, self.config.load_address)?;
        let heap_start = app_heap_start_for_loaded_app(&app);
        let image_start = app_image_start_for_loaded_app(&app);

        // A relocated A5 world leaves valid application-heap space below the
        // direct-loaded image. Reserve the image itself, plus the zone header,
        // without throwing that lower partition space away.
        // Inside Macintosh: Memory (1992), pp. 1-7 to 1-9 and 2-19.
        self.process_context
            .reserve_classic_heap(APP_ZONE_HEADER_SIZE);
        self.process_context
            .reserve_classic_heap_range(image_start, heap_start);
        self.dispatcher.load_resources(fork, &mut self.bus);

        let segments: HashMap<i16, u32> = app.segment_bases.iter().map(|(&k, &v)| (k, v)).collect();
        self.dispatcher.register_segments(segments);

        Some(app)
    }

    pub(crate) fn clear_startup_framebuffer(&mut self) {
        if self.menu_bar_visible() {
            let (_, _, screen_width, screen_height, _) = self.dispatcher.screen_mode;
            self.dispatcher.fill_theme_desktop_rect(
                &mut self.bus,
                0,
                0,
                screen_height as i16,
                screen_width as i16,
            );
            if let Some(ppc_app) = self.native.application_mut() {
                ppc_app.repaint_theme_desktop(true);
            }
            return;
        }

        let (scrn_base, row_bytes, _, scrn_height, _) = self.dispatcher.screen_mode;
        self.bus
            .fill_bytes(scrn_base, row_bytes * scrn_height as u32, 0xFF);
    }

    /// Loading an application is a once-per-launch operation, but its caller
    /// runs on the per-batch path. Left to itself the optimiser inlined this
    /// whole body there, giving the hot function a 34 KB stack frame; keep it
    /// out of line so the common case stays a cheap `Option` check.
    #[cold]
    #[inline(never)]
    fn switch_to_launched_application(
        &mut self,
        app_path: &str,
    ) -> std::result::Result<(), String> {
        use crate::memory::globals::addr;

        let normalized = TrapDispatcher::normalize_vfs_path(app_path);
        let rsrc_key = self
            .dispatcher
            .find_vfs_rsrc_file(&normalized)
            .ok_or_else(|| format!("no resource fork for launched application {normalized:?}"))?;
        let rsrc_bytes = self
            .dispatcher
            .vfs_rsrc
            .get(&rsrc_key)
            .cloned()
            .ok_or_else(|| format!("resource fork {rsrc_key:?} disappeared before launch"))?;
        let fork = ResourceFork::parse(&rsrc_bytes)
            .ok_or_else(|| format!("failed to parse launched application {normalized:?}"))?;

        let ram_size = self.bus.ram_size() as usize;
        let config = FixtureRunnerConfig {
            max_instructions: self.config.max_instructions,
            load_address: self.config.load_address,
            arrows_as_numpad: self.config.arrows_as_numpad,
            menu_bar_policy: self.config.menu_bar_policy,
            ui_theme: self.config.ui_theme,
            theme_metrics_mode: self.config.theme_metrics_mode,
            addressing_32_bit: self.bus.addressing_32_bit(),
            screen_depth: self.config.screen_depth,
        };
        let menu_bar_policy = self.dispatcher.menu_bar_policy;
        let menu_bar_hidden = self.dispatcher.menu_bar_hidden;
        let initial_kiosk_guest_hide_observed = self.dispatcher.initial_kiosk_guest_hide_observed;
        let instructions_per_tick = self.instructions_per_tick;
        let wait_sleep_cap_in_headless = self.wait_sleep_cap_in_headless;
        let app_start_time = self.app_start_time;
        let launch_ticks_override = self.launch_ticks_override;
        let launch_rnd_seed_override = self.launch_rnd_seed_override;
        let launch_ppc_time_base_override = self.launch_ppc_time_base_override;
        let application_partition_size = self.application_partition_size;
        let powerpc_screen_depth_override = self.powerpc_screen_depth_override;
        let ppc_host_mirror_capacity = self.ppc_host_mirror_capacity;
        let total_instructions = self.total_instructions;
        let launch_tick = self.guest_tick();
        let launch_time = self.bus.read_long(addr::TIME);
        let launch_rnd_seed = self.bus.read_long(addr::RND_SEED);
        let mouse_pos = self.dispatcher.input_state.mouse_position();
        let mouse_button = self.dispatcher.input_state.mouse_button_pressed();
        let output_dir = self.dispatcher.output_dir.clone();
        let file_system = self.process_context.detached_vfs_snapshot();

        let mut replacement = FixtureRunner::new_with_file_system(ram_size, config, file_system);
        replacement.dispatcher.menu_bar_policy = menu_bar_policy;
        replacement.dispatcher.menu_bar_hidden = menu_bar_hidden;
        replacement.dispatcher.initial_kiosk_guest_hide_observed =
            initial_kiosk_guest_hide_observed;
        replacement.instructions_per_tick = instructions_per_tick;
        replacement.tick_budget = instructions_per_tick as i32;
        replacement.wait_sleep_cap_in_headless = wait_sleep_cap_in_headless;
        replacement.app_start_time = app_start_time;
        replacement.launch_ticks_override = launch_ticks_override;
        replacement.launch_rnd_seed_override = launch_rnd_seed_override;
        replacement.launch_ppc_time_base_override = launch_ppc_time_base_override;
        replacement.application_partition_size = application_partition_size;
        replacement.powerpc_screen_depth_override = powerpc_screen_depth_override;
        replacement.total_instructions = total_instructions;

        replacement.dispatcher.output_dir = output_dir;
        replacement.dispatcher.set_launched_app_path(&normalized);

        let app = replacement
            .load_app(&fork)
            .ok_or_else(|| format!("failed to load launched application {normalized:?}"))?;
        replacement.init_app(&app);
        if ppc_host_mirror_capacity != 0 {
            let base = replacement.bus.alloc(ppc_host_mirror_capacity);
            if base == 0 {
                return Err("failed to preserve the PowerPC host framebuffer mirror".to_string());
            }
            replacement.ppc_host_mirror_base = base;
            replacement.ppc_host_mirror_capacity = ppc_host_mirror_capacity;
        }
        replacement.bus.write_long(addr::TICKS, launch_tick);
        replacement.bus.write_long(addr::TIME, launch_time);
        replacement.bus.write_long(addr::RND_SEED, launch_rnd_seed);
        replacement.dispatcher.read_tick_count(&replacement.bus);
        replacement
            .dispatcher
            .input_state
            .set_mouse_state(mouse_pos, mouse_button);
        replacement
            .bus
            .write_byte(addr::MB_STATE, if mouse_button { 0x00 } else { 0x80 });
        replacement.clear_startup_framebuffer();

        replacement.audio = self.audio.take();
        replacement.audio_buffer = std::mem::take(&mut self.audio_buffer);

        self.dispatcher.teardown_trap_table_process_context();
        eprintln!("[LAUNCH] Switched foreground application to {normalized}");
        *self = replacement;
        Ok(())
    }

    fn service_pending_launch_application(
        &mut self,
        event_yield_reached: bool,
        caller_exited: bool,
    ) -> bool {
        let Some(path) = self
            .dispatcher
            .take_pending_launch_application(event_yield_reached, caller_exited)
        else {
            return false;
        };

        if let Err(err) = self.switch_to_launched_application(&path) {
            eprintln!("[LAUNCH] Failed to switch to queued application {path:?}: {err}");
            self.halted = true;
            self.halted_pc = Some(self.m68k.cpu.read_reg(Register::PC));
            self.halted_sp = Some(self.m68k.cpu.read_reg(Register::A7));
            self.halted_d0 = Some((-43i32) as u32);
        }
        true
    }

    fn service_installer_handoff(&mut self) -> bool {
        let Some(baseline) = self.installer_handoff_baseline.take() else {
            return false;
        };
        let Some(path) = crate::game::launch::select_installed_application(self, &baseline) else {
            return false;
        };

        if let Err(err) = self.switch_to_launched_application(&path) {
            eprintln!("[LAUNCH] Failed to switch to installed application {path:?}: {err}");
            return false;
        }
        true
    }

    pub(crate) fn merge_resources_into_application(&mut self, fork: &ResourceFork) -> usize {
        self.dispatcher
            .merge_resources_into_existing_file(fork, &mut self.bus, 0)
    }

    fn alloc_handle_with_bytes(&mut self, bytes: &[u8]) -> u32 {
        let data_ptr = if bytes.is_empty() {
            0
        } else {
            let data_ptr = self.bus.alloc(bytes.len() as u32);
            if data_ptr == 0 {
                return 0;
            }
            self.bus.write_bytes(data_ptr, bytes);
            data_ptr
        };

        let handle = self.bus.alloc(4);
        if handle == 0 {
            if data_ptr != 0 {
                self.bus.free(data_ptr);
            }
            return 0;
        }

        self.bus.write_long(handle, data_ptr);
        if data_ptr != 0 {
            self.dispatcher.track_handle_ptr(data_ptr, handle);
        }
        handle
    }

    fn write_fixed_pstring(&mut self, ptr: u32, value: &str, max_len: usize) {
        let bytes = value.as_bytes();
        let len = bytes.len().min(max_len);
        self.bus.write_byte(ptr, len as u8);
        for (i, &byte) in bytes.iter().take(len).enumerate() {
            self.bus.write_byte(ptr + 1 + i as u32, byte);
        }
    }

    fn seed_current_application_file_manager_state(&mut self) {
        use crate::memory::globals::addr;

        let Some(app_path) = self.dispatcher.launched_app_path().map(str::to_owned) else {
            return;
        };
        self.dispatcher.ensure_vfs_file_metadata(&app_path);
        let metadata = self
            .dispatcher
            .vfs_metadata
            .get(&app_path)
            .copied()
            .unwrap_or(crate::trap::dispatch::VfsMetadata {
                file_id: 0,
                parent_dir_id: *self.dispatcher.default_dir_id,
                file_type: u32::from_be_bytes(*b"APPL"),
                creator: u32::from_be_bytes(*b"????"),
                finder_flags: 0,
                created_date: 0,
                modified_date: 0,
            });
        let resource_len = self
            .dispatcher
            .vfs_rsrc
            .get(&app_path)
            .map(|bytes| bytes.len() as u32)
            .unwrap_or(0);

        let vcb_ptr = self.bus.alloc(HFS_VCB_SIZE);
        if vcb_ptr == 0 {
            return;
        }
        self.bus.fill_bytes(vcb_ptr, HFS_VCB_SIZE, 0);
        self.bus.write_word(vcb_ptr + 8, 0x4244); // vcbSigWord: HFS volume
        self.write_fixed_pstring(
            vcb_ptr + 44,
            crate::trap::dispatch::TrapDispatcher::boot_volume_name(),
            27,
        );
        self.bus.write_word(
            vcb_ptr + 78,
            crate::trap::dispatch::BOOT_VOLUME_REF_NUM as u16,
        ); // vcbVRefNum
        self.bus
            .write_long(vcb_ptr + 172, *self.dispatcher.default_dir_id);

        self.bus.write_long(addr::DEF_VCB_PTR, vcb_ptr);
        self.bus.write_word(addr::VCB_Q_HDR, 0);
        self.bus.write_long(addr::VCB_Q_HDR + 2, vcb_ptr);
        self.bus.write_long(addr::VCB_Q_HDR + 6, vcb_ptr);

        let fcb_buffer = self.bus.alloc(HFS_FCB_BUFFER_SIZE as u32);
        if fcb_buffer == 0 {
            return;
        }
        self.bus
            .fill_bytes(fcb_buffer, HFS_FCB_BUFFER_SIZE as u32, 0);
        self.bus.write_word(fcb_buffer, HFS_FCB_BUFFER_SIZE);
        let fcb = fcb_buffer + APPLICATION_RESOURCE_REFNUM as u32;
        self.bus.write_long(fcb, metadata.file_id);
        self.bus.write_word(fcb + 4, 0x0200); // fcbFlags bit 9: resource fork
        self.bus.write_long(fcb + 8, resource_len);
        self.bus.write_long(fcb + 12, resource_len);
        self.bus.write_long(fcb + 20, vcb_ptr);
        self.bus.write_long(fcb + 50, metadata.file_type);
        self.bus.write_long(fcb + 58, metadata.parent_dir_id);
        let app_name = crate::trap::dispatch::TrapDispatcher::vfs_basename(&app_path).to_string();
        self.write_fixed_pstring(fcb + 62, &app_name, 31);

        // Files 1992, 2-81 and 2-384: the FCB buffer begins with a length
        // word, file reference numbers are offsets into that buffer, and
        // System 7 FCBs are 94 bytes. The first application resource fork
        // access path therefore has refnum 2.
        self.bus.write_long(addr::FCB_S_PTR, fcb_buffer);
        self.bus.write_word(addr::FS_FCB_LEN, HFS_FCB_SIZE);
        self.bus
            .write_word(addr::CUR_APREF_NUM, APPLICATION_RESOURCE_REFNUM);

        self.dispatcher
            .open_files
            .insert(APPLICATION_RESOURCE_REFNUM, format!("__rsrc__{}", app_path));
        self.dispatcher
            .file_positions
            .insert(APPLICATION_RESOURCE_REFNUM, 0);
    }

    /// Seed the Mac-canonical low-memory globals (`MemTop`,
    /// `CurStackBase`, `ApplLimit`, `Lo3Bytes`, `Ticks`, etc.) and
    /// the A5 World start so `run_steps` lands the guest in a
    /// runnable state. Must be called after [`load_app`](Self::load_app)
    /// and before the first call to [`run_steps`](Self::run_steps);
    /// the higher-level `systemless::game::load_game` helper invokes it
    /// automatically.
    ///
    /// Without `init_app`, A5-relative startup code (CodeWarrior /
    /// Think C runtimes, e.g. Koji / Munchies) sees `CurStackBase` =
    /// 0 and spins forever in the globals-decompression loop.
    pub fn init_app(&mut self, app: &LoadedApp) {
        assert!(
            self.m68k.can_relaunch() && self.native.can_relaunch(),
            "cannot relaunch with parked execution contexts"
        );
        assert!(
            app.ppc.as_ref().is_none_or(|native| native.cfm.is_some()),
            "native relaunch requires an uninstalled CFM seed"
        );
        let native_launch = app.ppc.clone().map(|native| {
            let launch_ticks = self.launch_ticks_override.unwrap_or(0);
            let migrated_services = native
                .preflight_migrated_services(&self.process_context, launch_ticks)
                .expect("native application services conflict with the process registry");
            (native, migrated_services)
        });
        assert!(self.native.reset_for_launch(app.ppc.is_some()));
        self.debug_advance_generation();
        self.process_context.reset_cfm_for_launch();
        if let Some((ppc_app, migrated_services)) = native_launch {
            self.init_ppc_app_with_services(ppc_app, migrated_services);
            return;
        }

        assert!(self.dispatcher.guest_calls.bind_task_entry_isa(
            ExecutionTaskId::APPLICATION,
            crate::guest_procedure::GuestIsa::M68k
        ));
        self.bus.detach_guest_address_space();

        use crate::memory::globals::addr;
        let ram_size = self.bus.ram_size();

        let high_level_event_aware = app
            .size_resource
            .is_some_and(ApplicationSizeResource::is_high_level_event_aware);
        self.dispatcher
            .apple_event_launch_state
            .reset_for_launch(high_level_event_aware);

        // Classic Mac OS application code runs in supervisor mode with the
        // processor priority open to level-1 VBL interrupts. The m68k core
        // starts from CPU reset with all interrupts masked; make the launch
        // state explicit so interrupt-time HLE can honor guest SR masking.
        // Inside Macintosh: Processes (1994), pp. 1-11 and 6-3.
        let launch_sr = (self.m68k.cpu.core.get_sr() & !0x0700) | 0x2000;
        self.m68k.cpu.core.set_sr_noint_nosp(launch_sr);

        // Initialize low-memory globals
        self.bus.write_long(addr::MEM_TOP, ram_size);
        // CurStackBase ($0908): "Address of base of stack; start of
        // application global variables." Per Inside Macintosh: Memory
        // 1992, p. 2-104, this points at the boundary where the
        // application's stack region meets the A5 World — equivalently,
        // the address of the *first* below-A5 global. CodeWarrior /
        // Think C runtime startup code (e.g. Koji the Frog, Munchies)
        // reads $0908 as a destination pointer when decompressing
        // initial values into the A5 globals area; pointing it at the
        // stack top instead leaves that decompression loop spinning
        // forever because its termination condition compares the
        // walked-forward A1 against (A5)+. Use `a5_base - below_a5`
        // here — that's the Mac-canonical "start of application
        // globals" regardless of where the stack lives in our
        // (inverted) host memory map.
        let app_globals_start = app.a5_base.saturating_sub(app.code0_header.below_a5);
        self.bus.write_long(addr::CUR_STACK_BASE, app_globals_start);
        self.bus.write_long(addr::CURRENT_A5, app.a5_base);
        self.bus.write_word(addr::ROM85, 0x0000);
        // MMU32Bit ($0CB2): TRUE when 32-bit addressing mode is in effect.
        // Inside Macintosh: Memory 1992, p. 4-25 says applications can test
        // this low-memory byte directly; Systemless's TrapDispatcher already
        // defaults SwapMMUMode to true32b and Gestalt('addr') bit 0 to set.
        self.bus
            .write_byte(addr::MMU32_BIT, u8::from(self.bus.addressing_32_bit()));
        // Initialize Ticks ($016A) to a realistic post-boot value.
        // On a real Mac, hundreds of ticks elapse during the boot ROM,
        // system extensions, and Finder startup before the application
        // launches. Games that read Ticks early (e.g. to seed a PRNG)
        // expect a non-zero value; starting at 0 produces degenerate
        // random sequences (e.g., ship heading always zero in EV).
        // 600 ticks ≈ 10 seconds of post-boot time, a conservative
        // estimate for a minimal System 7 configuration.
        let launch_ticks = self
            .launch_ticks_override
            .map_or(DEFAULT_LAUNCH_TICKS, |floor| {
                DEFAULT_LAUNCH_TICKS.max(floor)
            });
        self.bus.write_long(addr::TICKS, launch_ticks);
        self.dispatcher.read_tick_count(&self.bus);
        let time = self
            .app_start_time
            .unwrap_or_else(current_mac_epoch_seconds);
        self.bus.write_long(addr::TIME, time);
        // DoubleTime ($02F0): maximum interval between mouseDown events that
        // constitutes a double-click. RAM starts zeroed, but zero makes the
        // canonical unsigned comparison `(thisClick - lastClick) < DoubleTime`
        // impossible. Lemmings uses that exact sequence for its nuke control.
        self.bus
            .write_long(addr::DOUBLE_TIME, DEFAULT_DOUBLE_TIME_TICKS);
        // RndSeed ($0156): system random seed initialized during boot.
        // On a real Mac, the boot code seeds this from the real-time clock
        // so that programs that read it directly (without calling Random)
        // get non-deterministic entropy. Use the startup time so play
        // scripts produce repeatable but non-trivial random sequences.
        // Inside Macintosh Volume II, II-387
        self.bus.write_long(
            addr::RND_SEED,
            self.launch_rnd_seed_override.unwrap_or(time),
        );
        // MBState: $80 = button UP (Mac convention: 0 = down, $80 = up).
        // RAM is zero-initialized which would mean "button down" — must set explicitly.
        self.bus.write_byte(addr::MB_STATE, 0x80);
        // KeyMapLM ($0174): ROM-maintained current-key bitmap mirrored by
        // GetKeys. Keep it explicitly clear at launch for direct pollers.
        // Inside Macintosh Volume I, I-260; MPW SysEqu.h `KeyMapLM`.
        self.sync_key_map_lowmem();
        self.install_cursor_task();
        // MBarHeight: 20 pixels (standard Roman system script value).
        // Games may set this to 0 to hide the menu bar for full-screen mode.
        // Inside Macintosh Volume V, V-245
        self.bus.write_word(addr::MBAR_HEIGHT, 20);
        // SdVolume ($0260): current speaker volume, low three bits only.
        // Inside Macintosh Volume III, III-425. Some classic apps also read
        // this byte directly as a "Sound Driver present" sentinel — most
        // notably Marathon 1, whose sound module (CODE 5 +$0003F2:
        // `MOVE.B (mem $260).W, (A0)`) short-circuits its audio submission
        // path when this byte is zero. Initialize it to the minimum nonzero
        // compatibility value; higher legacy volume values can change old
        // Sound Driver clients' control flow.
        self.bus.write_byte(addr::SD_VOLUME, 1);

        // Memory Manager zone globals
        // Inside Macintosh Volume II, II-19 and II-29..II-30.
        // Keep actual Systemless allocations above the direct-loaded image,
        // while the guest-visible application zone can remain at the normal
        // floor when the loader has placed the application image above it.
        // Real Mac CODE/resources live inside the app heap; Systemless writes
        // them directly and then protects them by bumping the allocator.
        let allocation_heap_start = app_heap_start_for_loaded_app(app);
        let visible_zone_start = app_visible_zone_start_for_loaded_app(app);
        let zone_header_size: u32 = APP_ZONE_HEADER_SIZE;
        let initial_heap_end = visible_zone_start + zone_header_size;
        let minimum_safe_appl_limit = self
            .process_context
            .classic_heap_bump_ptr()
            .max(allocation_heap_start);
        let stack_base = app.initial_sp;
        let default_appl_limit = stack_base - APP_STACK_SAFETY_MARGIN;
        let requested_partition_size = self.application_partition_size.or_else(|| {
            app.size_resource
                .and_then(|size| size.preferred_partition_size())
        });
        let appl_limit = requested_partition_size
            .and_then(|partition_size| {
                // Processes 1994, pp. 1-3 and 2-18: the Process Manager
                // allocates the application partition from the app's 'SIZE'
                // resource preferred size when available. A scripted override
                // represents the same Finder-style preferred-memory setting
                // applied to a temporary launch. Systemless keeps the physical
                // stack at the top of guest RAM, but narrows the observable
                // heap limit so FreeMem/MaxMem/process info see the same
                // partition pressure.
                partition_size
                    .checked_sub(APP_STACK_SAFETY_MARGIN)
                    .and_then(|heap_span| visible_zone_start.checked_add(heap_span))
            })
            .map(|limit| limit.max(minimum_safe_appl_limit))
            .filter(|&limit| limit > initial_heap_end && limit < default_appl_limit)
            .unwrap_or(default_appl_limit.max(initial_heap_end));
        let buf_ptr = appl_limit; // Buffer area at the limit
        self.bus.write_long(addr::SYS_ZONE, visible_zone_start);
        self.bus.write_long(addr::APP_L_ZONE, visible_zone_start);
        self.bus.write_long(addr::HEAP_END, initial_heap_end);
        self.bus.write_long(addr::APPL_LIMIT, appl_limit);
        self.bus.write_long(addr::BUF_PTR, buf_ptr);
        self.bus.write_long(addr::THE_ZONE, visible_zone_start);

        // CurApRefNum: resource file reference number of the application (word).
        // CurApName: application name as Pascal string (Str31).
        // AppParmHandle is allocated below, after the zone header is reserved.
        // Inside Macintosh Volume II, II-57 to II-58
        self.bus.write_word(addr::CUR_APREF_NUM, 0);
        if let Some(app_path) = self.dispatcher.launched_app_path() {
            let app_name = crate::trap::dispatch::TrapDispatcher::vfs_basename(app_path);
            let name_bytes = crate::mac_roman::encode_mac_roman_lossy(app_name);
            let len = name_bytes.len().min(31);
            self.bus.write_byte(addr::CUR_APNAME, len as u8);
            for (i, &b) in name_bytes.iter().take(len).enumerate() {
                self.bus.write_byte(addr::CUR_APNAME + 1 + i as u32, b);
            }
        }

        // Set the current directory to the application's parent folder so that
        // file-relative lookups (e.g. Marathon opening "Music") resolve correctly.
        // CurDirStore: directory ID of directory last opened (long)
        // SFSaveDisk: negative of volume reference number (word)
        // Inside Macintosh Volume IV, IV-72
        let app_dir_id = *self.dispatcher.default_dir_id;
        self.bus.write_long(addr::CUR_DIR_STORE, app_dir_id);
        self.bus.write_word(
            addr::SF_SAVE_DISK,
            (-crate::trap::dispatch::BOOT_VOLUME_REF_NUM) as u16,
        );
        eprintln!(
            "[INIT] CurDirStore={} SFSaveDisk={}",
            app_dir_id,
            (-crate::trap::dispatch::BOOT_VOLUME_REF_NUM) as u16
        );

        // The application stack is carved from the freshly initialized
        // application partition. Keep that initial stack window zeroed to
        // match a newly booted classic Mac environment. Ordinary NewPtr and
        // NewHandle allocations retain their documented undefined contents;
        // this only establishes the process's initial stack state.
        let stack_seed_start = stack_base.saturating_sub(0x8000);
        self.bus
            .fill_zeros(stack_seed_start, stack_base - stack_seed_start);

        // Zone header at visible_zone_start (Inside Macintosh Volume II, II-22)
        // Apps and the Memory Manager read the zone header to determine
        // available memory. zcbFree (offset +12) must reflect free bytes.
        // Reserve heap space so alloc() doesn't overwrite the zone header.
        self.process_context.reserve_classic_heap(zone_header_size);
        let zone_size = appl_limit.saturating_sub(visible_zone_start);
        let free_bytes = zone_size.saturating_sub(zone_header_size);
        self.bus.write_long(visible_zone_start, appl_limit); // bkLim: end of zone
        self.bus.write_long(
            visible_zone_start + 8,
            visible_zone_start + zone_header_size,
        ); // hFstFree
        self.bus.write_long(visible_zone_start + 12, free_bytes); // zcbFree: total free
        self.bus.write_long(
            visible_zone_start + 56,
            visible_zone_start + zone_header_size,
        ); // allocPtr
        eprintln!(
            "[INIT] Zone header: start=${:08X} allocStart=${:08X} bkLim=${:08X} zcbFree={} ({:.1}MB)",
            visible_zone_start,
            allocation_heap_start,
            appl_limit,
            free_bytes,
            free_bytes as f64 / (1024.0 * 1024.0)
        );

        // The Device Manager unit table is a nonrelocatable array of DCE
        // handles addressed through UTableBase; UnitNtryCnt is its entry
        // count. Inside Macintosh: Devices (1994), pp. 1-8--1-9. Use 96 for
        // the Mac OS 8.1 machine profile, within the documented 64-to-128
        // expandable range (Inside Macintosh Volume V, 1986, p. V-215).
        let unit_table_entry_count = crate::memory::globals::DEFAULT_UNIT_TABLE_ENTRY_COUNT;
        let unit_table = self.bus.alloc(u32::from(unit_table_entry_count) * 4);
        if unit_table != 0 {
            self.bus
                .fill_zeros(unit_table, u32::from(unit_table_entry_count) * 4);
        }
        self.bus.write_long(addr::U_TABLE_BASE, unit_table);
        self.bus
            .write_word(addr::UNIT_NTRY_CNT, unit_table_entry_count);
        self.seed_current_application_file_manager_state();

        // AppParmHandle: handle to Finder information about files selected
        // when launching the application. A normal Finder application launch
        // with no documents still provides the message/count header:
        // appOpen (0), count 0. Assembly code may read this global directly.
        // Inside Macintosh Volume II, II-57; Files 1992, 1-58.
        let app_param_handle = self.alloc_handle_with_bytes(&[0, 0, 0, 0]);
        self.bus.write_long(addr::APP_PARM_HANDLE, app_param_handle);

        // Write an ExitToShell trap at a known low-memory address so that when
        // main() returns via RTS, the CPU executes ExitToShell and halts cleanly.
        // We use address 0x100 (safe, unused low memory) to hold the A-line instruction.
        let exit_trampoline = 0x100u32;
        self.bus.write_word(exit_trampoline, 0xA9F4); // ExitToShell

        // Pre-allocate the main GDevice with 800x600 8bpp settings
        // and set the low-memory globals that games read directly.
        // screenBits is already initialized to 800x600 8bpp by the bus.
        let gdh = self.dispatcher.ensure_main_gdevice(&mut self.bus);
        let gd_ptr = self.bus.read_long(gdh);
        self.bus.write_long(0x8A4, gdh); // MainDevice
        self.bus.write_long(0xCC8, gdh); // TheGDevice
        self.bus.write_long(0x8A8, gdh); // DeviceList
        eprintln!(
            "[INIT] Set MainDevice=${:08X}, TheGDevice=${:08X} (ptr=${:08X})",
            gdh, gdh, gd_ptr
        );

        // Initialize screen_mode from the main GDevice PixMap.
        let pmap_h = self.bus.read_long(gd_ptr + 22);
        let pmap = self.bus.read_long(pmap_h);
        let scrn_base = self.bus.read_long(pmap);
        let rb = (self.bus.read_word(pmap + 4) & 0x3FFF) as u32;
        let top = self.bus.read_word(pmap + 6) as i16;
        let left = self.bus.read_word(pmap + 8) as i16;
        let bottom = self.bus.read_word(pmap + 10) as i16;
        let right = self.bus.read_word(pmap + 12) as i16;
        let pixel_size = self.bus.read_word(pmap + 32);
        let width = (right - left).max(1) as u16;
        let height = (bottom - top).max(1) as u16;
        self.dispatcher.screen_mode = (scrn_base, rb, width, height, pixel_size);

        // Debug: dump the GDevice chain to verify correctness
        {
            let main_dev = self.bus.read_long(0x8A4);
            let gd = self.bus.read_long(main_dev);
            let pmap_h = self.bus.read_long(gd + 22);
            let pmap = self.bus.read_long(pmap_h);
            let rb = self.bus.read_word(pmap + 4);
            let top = self.bus.read_word(pmap + 6) as i16;
            let left = self.bus.read_word(pmap + 8) as i16;
            let bottom = self.bus.read_word(pmap + 10) as i16;
            let right = self.bus.read_word(pmap + 12) as i16;
            let ps = self.bus.read_word(pmap + 32);
            let gd_top = self.bus.read_word(gd + 34) as i16;
            let gd_left = self.bus.read_word(gd + 36) as i16;
            let gd_bottom = self.bus.read_word(gd + 38) as i16;
            let gd_right = self.bus.read_word(gd + 40) as i16;
            let gd_flags = self.bus.read_word(gd + 20);
            eprintln!(
                "[INIT] GDevice chain: $8A4→${:08X}→${:08X} gdPMap→${:08X}→${:08X}",
                main_dev, gd, pmap_h, pmap
            );
            eprintln!(
                "[INIT]   PixMap: rowBytes=${:04X} bounds=({},{},{},{}) pixelSize={}",
                rb, top, left, bottom, right, ps
            );
            eprintln!(
                "[INIT]   GDevice: gdRect=({},{},{},{}) gdFlags=${:04X}",
                gd_top, gd_left, gd_bottom, gd_right, gd_flags
            );
        }

        // Mac OS exposes complete writable OS and Toolbox dispatch tables in
        // low memory. Materialize all 1,280 callable entries before installing
        // the adjacent QuickDraw vectors. IM:OSUtils 1994, pp. 8-4--8-6.
        self.dispatcher
            .materialize_trap_tables(&mut self.bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");
        // JHideCursor ($0800): argument-free QuickDraw cursor bottleneck.
        // Adapt a direct JSR to the existing A-line trap by removing the JSR
        // return address before dispatch and jumping back afterward.
        let hide_cursor_trampoline = self.bus.alloc(6);
        self.bus.write_word(hide_cursor_trampoline, 0x205F); // MOVEA.L (SP)+,A0
        self.bus.write_word(hide_cursor_trampoline + 2, 0xA852); // HideCursor
        self.bus.write_word(hide_cursor_trampoline + 4, 0x4ED0); // JMP (A0)
        self.bus
            .write_long(addr::J_HIDE_CURSOR, hide_cursor_trampoline);
        // JShowCursor ($0804): QuickDraw glue vector for ShowCursor.
        // On Macintosh Programming: Advanced Techniques (1990) identifies
        // the vector address; MPW Quickdraw.h declares ShowCursor as the
        // argument-free $A853 trap. A direct JSR therefore needs only the
        // trap instruction followed by RTS.
        let show_cursor_trampoline = self.bus.alloc(4);
        self.bus.write_word(show_cursor_trampoline, 0xA853); // ShowCursor
        self.bus.write_word(show_cursor_trampoline + 2, 0x4E75); // RTS
        self.bus
            .write_long(addr::J_SHOW_CURSOR, show_cursor_trampoline);
        // JShieldCursor ($0808): low-level QuickDraw cursor-shielding vector.
        // MPW Universal Interfaces Quickdraw.h declares QDJShieldCursorProcPtr
        // as a Pascal procedure taking four INTEGER values (left, top, right,
        // bottom). These occupy the same eight stack bytes consumed by the
        // ShieldCursor ($A855) HLE. Pop the JSR return address before entering
        // the trap, then jump back after the trap consumes that payload.
        // Inside Macintosh Volume I, I-474; MPW Quickdraw.h.
        let shield_cursor_trampoline = self.bus.alloc(6);
        self.bus.write_word(shield_cursor_trampoline, 0x205F); // MOVEA.L (SP)+,A0
        self.bus.write_word(shield_cursor_trampoline + 2, 0xA855); // ShieldCursor
        self.bus.write_word(shield_cursor_trampoline + 4, 0x4ED0); // JMP (A0)
        self.bus
            .write_long(addr::J_SHIELD_CURSOR, shield_cursor_trampoline);

        // JInitCrsr ($0814): low-level cursor initialization vector.
        // InitCursor takes no arguments, so a direct JSR returns through a
        // normal RTS after the A-line HLE runs.
        // MPW Interfaces/AIncludes/LowMemEqu.a: `JInitCrsr EQU $814`.
        let init_cursor_trampoline = self.bus.alloc(4);
        self.bus.write_word(init_cursor_trampoline, 0xA850); // InitCursor
        self.bus.write_word(init_cursor_trampoline + 2, 0x4E75); // RTS
        self.bus
            .write_long(addr::J_INIT_CRSR, init_cursor_trampoline);

        // JSwapFont ($08E0): private Font Manager vector used by QuickDraw to
        // call FMSwapFont directly. Executor's clean-room low-memory table
        // identifies the address and initializes it from the $A901 routine.
        //
        // A JSR has placed its return address above the four-byte pointer to
        // the caller's FMInput record. Pop that return address before entering
        // the HLE trap so A7 points at the documented argument, then jump back
        // after the trap
        // leaves A7 on the four-byte FMOutPtr result slot. Allocate this only
        // after reserving the application zone header so it remains live.
        let swap_font_trampoline = self.bus.alloc(6);
        self.bus.write_word(swap_font_trampoline, 0x205F); // MOVEA.L (SP)+,A0
        self.bus.write_word(swap_font_trampoline + 2, 0xA901); // FMSwapFont
        self.bus.write_word(swap_font_trampoline + 4, 0x4ED0); // JMP (A0)
        self.bus.write_long(addr::J_SWAP_FONT, swap_font_trampoline);

        // Set CPU state
        self.m68k.cpu.write_reg(Register::A5, app.a5_base);
        // Push the exit trampoline as the return address on the stack
        let sp = app.initial_sp.wrapping_sub(4);
        self.bus.write_long(sp, exit_trampoline);
        self.m68k.cpu.write_reg(Register::A7, sp);
        // Initialize A6 (frame pointer) to the stack pointer.
        // On a real Mac, the Process Manager sets up A6 before launching
        // the application. The CRT startup code (e.g. Think C's __start)
        // expects A6 to be a valid stack address for its initial LINK frame.
        self.m68k.cpu.write_reg(Register::A6, sp);
        self.m68k
            .cpu
            .write_reg(Register::PC, app.entry_point(app.a5_base));
    }

    pub(crate) fn stage_ppc_companion(&mut self, ppc_companion: PpcLoadedApp) {
        self.native
            .stage_companion(ppc_companion)
            .unwrap_or_else(|_| panic!("native companion is already installed"));
    }

    #[cfg(test)]
    pub(crate) fn has_ppc_companion(&self) -> bool {
        self.native.companion().is_some() || self.native.has_staged_companion()
    }

    #[cfg(test)]
    fn init_ppc_companion(&mut self, ppc_companion: PpcLoadedApp) {
        assert!(
            self.process_context
                .can_install_cfm_seed(&ppc_companion.cfm),
            "CFM seed conflicts with the process registry"
        );
        let current_tick = self.process_context.migrated_handles().ticks.current_tick();
        let migrated_services = ppc_companion
            .preflight_migrated_services(&self.process_context, current_tick)
            .expect("native companion services conflict with the process registry");
        self.init_ppc_companion_with_services(ppc_companion, migrated_services);
    }

    fn init_ppc_companion_with_services(
        &mut self,
        mut ppc_companion: PpcLoadedApp,
        migrated_services: crate::process_context::MigratedServiceAdoption,
    ) {
        ppc_companion.commit_migrated_services(&self.process_context, migrated_services);
        self.share_ppc_process_memory(&mut ppc_companion);
        ppc_companion.attach_unconverted_process_services(&mut self.process_context);
        assert!(self
            .process_context
            .install_cfm_seed(&mut ppc_companion.cfm));
        self.bus
            .attach_guest_address_space(ppc_companion.memory.shared_view());
        self.native
            .install(NativeEngineRole::Companion, ppc_companion)
            .unwrap_or_else(|_| panic!("native engine slot is occupied"));
    }

    #[cfg(test)]
    fn init_ppc_app(&mut self, ppc_app: PpcLoadedApp) {
        let launch_ticks = self.launch_ticks_override.unwrap_or(0);
        let migrated_services = ppc_app
            .preflight_migrated_services(&self.process_context, launch_ticks)
            .expect("native application services conflict with the process registry");
        self.init_ppc_app_with_services(ppc_app, migrated_services);
    }

    fn init_ppc_app_with_services(
        &mut self,
        mut ppc_app: PpcLoadedApp,
        migrated_services: crate::process_context::MigratedServiceAdoption,
    ) {
        assert!(
            self.process_context.can_install_cfm_seed(&ppc_app.cfm),
            "CFM seed conflicts with the process registry"
        );
        use crate::memory::globals::addr;
        let launch_ticks = self.launch_ticks_override.unwrap_or(0);
        // A native application carries its parsed SIZE capability before it
        // attaches to the runner-owned process state. Start this launch with
        // that capability and a fresh process-wide OAPP claim so a prior
        // application cannot suppress or duplicate delivery. Inside
        // Macintosh: Toolbox Essentials (1992), pp. 2-30--2-32 and 5-90.
        let high_level_event_aware = ppc_app
            .apple_events
            .apple_event_launch_state
            .is_high_level_event_aware();
        ppc_app
            .apple_events
            .apple_event_launch_state
            .reset_for_launch(high_level_event_aware);
        self.process_context
            .reset_apple_event_launch_state_for_launch(high_level_event_aware);

        self.bus.detach_guest_address_space();
        self.adopt_ppc_process_memory_image(&mut ppc_app);
        // These are process launch defaults, not PEF-loader state. Reapply
        // them after adopting a sparse construction image so a synthetic
        // adapter that maps zero-filled low memory cannot erase the process
        // environment established by `FixtureRunner::new`.
        self.bus.write_word(
            addr::SYS_EVT_MASK,
            crate::memory::globals::DEFAULT_SYS_EVT_MASK,
        );
        self.bus.write_word(
            addr::MENU_FLASH,
            crate::memory::globals::DEFAULT_MENU_FLASH_COUNT,
        );
        self.bus
            .write_long(addr::DOUBLE_TIME, DEFAULT_DOUBLE_TIME_TICKS);
        self.bus.write_byte(addr::MMU32_BIT, 1);
        self.bus.write_byte(addr::SD_VOLUME, 1);
        self.bus.write_word(0x09dc, 1); // PaintWhite
        let ram_size = self.bus.ram_size();
        self.bus.write_long(addr::MEM_TOP, ram_size);
        self.bus.write_long(addr::TICKS, launch_ticks);
        self.dispatcher.read_tick_count(&self.bus);
        ppc_app.commit_migrated_services(&self.process_context, migrated_services);
        let time = self
            .app_start_time
            .unwrap_or_else(current_mac_epoch_seconds);
        self.bus.write_long(addr::TIME, time);
        let rnd_seed = self.launch_rnd_seed_override.unwrap_or(time);
        self.bus.write_long(addr::RND_SEED, rnd_seed);
        self.dispatcher
            .materialize_trap_tables(&mut self.bus, TrapTableProfile::PowerPc604)
            .expect("trap table construction requires writable cells and system storage");
        self.share_ppc_process_memory(&mut ppc_app);
        let detached_events = ppc_app.event_queue.take();
        self.process_context
            .shared_event_queue()
            .merge(detached_events);
        ppc_app.attach_unconverted_process_services(&mut self.process_context);
        if let Some(partition_size) = self.application_partition_size.or_else(|| {
            ppc_app
                .launch_size_resource()
                .and_then(|size| size.preferred_partition_size())
        }) {
            ppc_app.grow_application_partition(partition_size.min(ram_size));
        }
        assert!(self.process_context.install_cfm_seed(&mut ppc_app.cfm));
        if let Some(time_base) = self.launch_ppc_time_base_override {
            ppc_app.cpu.set_time_base(time_base);
        }
        self.bus.write_byte(addr::MB_STATE, 0x80);
        self.bus.write_word(addr::MBAR_HEIGHT, 20);
        self.bus.write_byte(addr::SOUND_LEVEL, 1);

        let app_dir_id = *self.dispatcher.default_dir_id;
        self.bus.write_long(addr::CUR_DIR_STORE, app_dir_id);
        self.bus.write_word(
            addr::SF_SAVE_DISK,
            (-crate::trap::dispatch::BOOT_VOLUME_REF_NUM) as u16,
        );

        let gdh = self.dispatcher.ensure_main_gdevice(&mut self.bus);
        self.bus.write_long(0x8A4, gdh);
        self.bus.write_long(0xCC8, *self.dispatcher.current_gdevice);
        self.bus.write_long(0x8A8, gdh);
        if let Some(front_buffer) = ppc_app.presented_front_buffer() {
            self.ensure_ppc_host_screen_mode(front_buffer);
        } else {
            let gd_ptr = self.bus.read_long(gdh);
            let pmap_h = self.bus.read_long(gd_ptr + 22);
            let pmap = self.bus.read_long(pmap_h);
            let scrn_base = self.bus.read_long(pmap);
            let rb = (self.bus.read_word(pmap + 4) & 0x3FFF) as u32;
            let top = self.bus.read_word(pmap + 6) as i16;
            let left = self.bus.read_word(pmap + 8) as i16;
            let bottom = self.bus.read_word(pmap + 10) as i16;
            let right = self.bus.read_word(pmap + 12) as i16;
            let pixel_size = self.bus.read_word(pmap + 32);
            let width = (right - left).max(1) as u16;
            let height = (bottom - top).max(1) as u16;
            self.dispatcher.screen_mode = (scrn_base, rb, width, height, pixel_size);
        }

        self.halted = false;
        self.halted_trap = None;
        self.halted_pc = None;
        self.halted_sp = None;
        self.halted_d0 = None;
        self.m68k.cpu.write_reg(Register::PC, 0);
        self.bus
            .attach_guest_address_space(ppc_app.memory.shared_view());
        assert!(self.dispatcher.guest_calls.bind_task_entry_isa(
            ExecutionTaskId::APPLICATION,
            crate::guest_procedure::GuestIsa::PowerPc
        ));
        ppc_app.set_ui_theme(self.config.ui_theme);
        ppc_app
            .toolbox_startup
            .execution
            .calls()
            .start_native_engine();
        self.native
            .install(NativeEngineRole::Application, ppc_app)
            .unwrap_or_else(|_| panic!("native engine slot is occupied"));
    }

    fn share_ppc_process_memory(&mut self, ppc_app: &mut PpcLoadedApp) {
        let ram_end = self.bus.ram_size();
        let low_memory_end = PROCESS_LOW_MEMORY_SIZE.min(ram_end);

        // Native PEF sections retain precedence over the shared flat-RAM
        // holes below. Keep the process-owned classic heap from selecting any
        // of those occupied spans before the overlays are installed. Inside
        // Macintosh: Memory (1992), pp. 2-19--2-21.
        let classic_heap_floor = APP_HEAP_FLOOR;
        let classic_heap_limit = self.bus.classic_heap_limit();
        for (mapping_start, mapping_end) in ppc_app.memory.mapping_ranges() {
            let start = mapping_start.max(classic_heap_floor);
            let end = mapping_end.min(classic_heap_limit);
            if start < end {
                self.process_context.reserve_classic_heap_range(start, end);
            }
        }

        // Check the native layout before our own RAM overlays occupy its holes.
        let system_reservation =
            self.bus
                .shared_synthetic_reservation()
                .filter(|(base, region)| {
                    let len = u32::try_from(region.len())
                        .expect("synthetic reservation fits guest address");
                    ppc_app.prepare_shared_system_reservation(*base, len)
                });

        let process_low_memory = self
            .bus
            .shared_ram_region(0, low_memory_end)
            .expect("FixtureRunner owns the complete process low-memory range");
        self.process_context
            .attach_memory(0, process_low_memory, &mut ppc_app.memory);
        for (base, end) in ppc_app.memory.mapping_holes(low_memory_end, ram_end) {
            let len = end - base;
            let process_memory = self
                .bus
                .shared_ram_region(base, len)
                .expect("FixtureRunner owns every process RAM hole");
            self.process_context
                .attach_memory(base, process_memory, &mut ppc_app.memory);
        }
        let Some((base, region)) = system_reservation else {
            return;
        };
        // SAFETY: the same serialized ownership contract applies, while the
        // read-only mapping preserves the ROM-like protection enforced by the
        // 68k bus for trap gateways and permanent come-from heads.
        unsafe {
            ppc_app
                .memory
                .add_shared_readonly_region(Some(GuestIsa::M68k), base, region);
        }
    }

    /// Move the native loader's detached construction image into the one
    /// process-owned low-memory allocation before either CPU executes.
    ///
    /// Production PEF loads map the complete range, while focused Mixed Mode
    /// tests often map only a callback or descriptor. Preserve every mapped
    /// byte in either case. Process startup deliberately follows this step so
    /// canonical clocks, trap tables, and devices replace loader defaults.
    fn adopt_ppc_process_memory_image(&mut self, ppc_app: &mut PpcLoadedApp) {
        let mut image = vec![0; PROCESS_LOW_MEMORY_SIZE as usize];
        if ppc_app.memory.read_bytes_into(0, &mut image).is_some() {
            self.bus.load(0, &image);
            return;
        }

        for address in 0..PROCESS_LOW_MEMORY_SIZE {
            if let Some(byte) = ppc_app.memory.read_u8(address) {
                self.bus.write_byte(address, byte);
            }
        }
    }

    /// Mix and queue audio samples without full frame finalization.
    /// Used to keep the audio buffer fed during long CPU frames.
    pub fn mix_audio(&mut self, num_samples: usize) {
        if self.guest_work_is_suspended() {
            return;
        }
        self.mix_host_audio(num_samples);
    }

    /// Keep guest-visible Sound Manager completion callbacks moving when a
    /// headless caller advances only CPU/tick time. Ordinary playback remains
    /// opt-in through `mix_audio`, so fixtures that inspect PCM do not consume
    /// unrelated channels implicitly.
    fn advance_headless_callback_audio(&mut self, elapsed_ticks: u32) {
        if elapsed_ticks == 0 || !self.dispatcher.sound_manager.has_playback_gated_callback() {
            return;
        }

        let samples_per_tick = (crate::sound::OUTPUT_RATE as f64 / DEFAULT_VBL_HZ).round() as usize;
        let samples = (elapsed_ticks as usize).saturating_mul(samples_per_tick);
        self.mix_host_audio(samples);
        self.dispatcher
            .sync_guest_sound_channel_state(&mut self.bus);
    }

    fn queue_mixed_audio(&mut self, stereo_samples: &[u8]) {
        if stereo_samples.is_empty() {
            return;
        }
        if let Some(ref mut audio) = self.audio {
            audio.queue_stereo_samples(stereo_samples);
        }
        for frame in stereo_samples.chunks_exact(2) {
            let left = frame[0] as i32 - 0x80;
            let right = frame[1] as i32 - 0x80;
            self.audio_buffer
                .push(((left + right) / 2 + 0x80).clamp(0, 255) as u8);
        }
    }

    fn queue_host_silence_audio(&mut self, num_samples: usize) {
        if num_samples == 0 {
            return;
        }
        if let Some(ref mut audio) = self.audio {
            audio.queue_stereo_samples(&vec![0x80; num_samples * 2]);
        }
    }

    fn mix_host_audio(&mut self, mut remaining_samples: usize) {
        self.service_ppc_double_buffer_playbacks();
        while remaining_samples > 0 {
            self.try_load_pending_double_buffers();
            self.dispatcher.service_guest_sound_queues(&mut self.bus);

            let chunk = self
                .dispatcher
                .sound_manager
                .samples_until_next_exhaustion()
                .map(|samples| samples.max(1).min(remaining_samples))
                .unwrap_or(remaining_samples);

            let mut samples = self.dispatcher.sound_manager.mix_frame_stereo(chunk);
            self.dispatcher.mix_movie_music(&mut samples, chunk);
            if samples.is_empty() {
                self.queue_host_silence_audio(remaining_samples);
                self.dispatcher
                    .release_finished_internal_sound_channels(&mut self.bus);
                break;
            }
            self.queue_mixed_audio(&samples);
            self.dispatcher
                .release_finished_internal_sound_channels(&mut self.bus);
            remaining_samples -= chunk;
            self.service_ppc_double_buffer_playbacks();
        }
        self.fire_pending_ppc_sound_completions();
    }

    /// Repaint host chrome without recording it against an armed
    /// idle-proof journal (the per-frame repaint would overflow the
    /// journal's guest-wait-cycle entry cap at 8 bpp and void every proof).
    ///
    /// Correctness boundary (issue #1052): suspending the journal is safe
    /// only while the repaint cannot change guest RAM relative to a proven
    /// state. During the probing phase that holds trivially -- proofs
    /// re-execute the whole cycle, so a changed repaint just fails to
    /// prove. While PARKED the proof is reused without re-execution, so
    /// the first repaint after a park (guest code may have overwritten a
    /// chrome pixel before entering its idle loop) is checked for
    /// byte-identity with an uncapped journal; a repaint that changed
    /// anything revokes the park and the site re-proves from scratch.
    fn redraw_chrome_outside_idle_journal(&mut self) {
        let suspended = self.bus.suspend_write_probe();
        let check_parked_repaint = self.idle_cycle_sleep.is_some() && suspended.is_some();
        if check_parked_repaint {
            self.bus.begin_uncapped_write_probe();
        }
        self.redraw_chrome();
        let revoke = check_parked_repaint && !self.bus.finish_write_probe_unchanged();
        if let Some(journal) = suspended {
            self.bus.resume_write_probe(journal);
        }
        if revoke {
            if wait_stats_enabled() {
                WS_RESUME_FAIL_MEM.fetch_add(1, AtomicOrdering::Relaxed);
            }
            self.cancel_idle_cycle_detector();
        }
    }

    fn finish_host_frame(
        &mut self,
        finalization: FrameFinalization,
        audio_samples: usize,
        sound_interrupt_dispatched: bool,
    ) {
        // Guest framebuffer writes can overwrite menu/window chrome, so a
        // complete frame restores it. Realtime sound slices leave that work
        // to the caller's outer composition pass, but still service audio.
        if finalization == FrameFinalization::Complete {
            self.redraw_chrome_outside_idle_journal();
        }
        debug_assert_ne!(finalization, FrameFinalization::Deferred);
        self.finish_audio_frame(audio_samples, sound_interrupt_dispatched);
        // The logical-frame capture boundary is defined as the point after
        // host chrome redraw and guest-audio finalization. Audio-only slices
        // defer chrome to the frontend's outer composition pass, so they must
        // not complete it; the frontend does so once per frame via
        // [`Self::finish_gui_frame`].
        if finalization == FrameFinalization::Complete {
            self.finish_logical_frame();
        }
    }

    fn finish_logical_frame(&mut self) {
        if self.guest_work_is_suspended() || self.halted {
            return;
        }
        self.debug_note_logical_frame_boundary();
    }

    /// Complete an outer GUI frame after audio work, compositing pending captures.
    pub fn finish_gui_frame(&mut self) {
        if self.guest_work_is_suspended() || self.halted {
            return;
        }
        if self.debug_has_pending_logical_frame_capture() {
            self.composite_frame();
        }
        self.finish_logical_frame();
    }

    fn finish_audio_frame(&mut self, audio_samples: usize, sound_interrupt_dispatched: bool) {
        // Try to load any double-buffer data that callbacks have refilled.
        self.try_load_pending_double_buffers();

        // Sound channels expose an in-memory queue that some games update
        // directly instead of routing every command through SndDoCommand.
        self.dispatcher.service_guest_sound_queues(&mut self.bus);

        // Mix and output audio for this frame.
        if audio_samples > 0 {
            self.mix_host_audio(audio_samples);
        }

        self.dispatcher
            .sync_guest_sound_channel_state(&mut self.bus);

        if !sound_interrupt_dispatched {
            let fired_sound_callback = self.fire_sound_callbacks();
            if !fired_sound_callback {
                // Fire pending double-buffer callbacks (SndPlayDoubleBuffer).
                self.fire_sound_doubleback_callbacks();
            }
        }
    }

    /// Mix a GUI-only audio slice without running foreground guest code or
    /// redrawing the frame. Used by realtime frontends to let Sound Manager
    /// doubleback callbacks run between small audio chunks when TickCount is
    /// already caught up to the wall clock.
    pub fn mix_gui_audio_slice(&mut self, audio_samples: usize) {
        if self.guest_work_is_suspended() {
            return;
        }
        self.finish_audio_frame(audio_samples, false);
    }

    /// Try to fast-forward past a TickCount spin-wait loop. Returns
    /// true iff the `tick_cap` was hit during advancement (caller
    /// should break the outer run loop); returns false if no match,
    /// if the advance succeeded, or if the max-cap
    /// (`SPIN_FASTFWD_MAX_TICKS`) protected us from a runaway target.
    /// All bytes are read from guest memory — the function never runs
    /// game code.
    fn try_tickcount_spin_fastfwd(
        &mut self,
        pc_after_trap: u32,
        tick_cap: Option<u32>,
        count: &mut usize,
    ) -> bool {
        let w0 = self.bus.read_word(pc_after_trap);

        // Template D consumes TickCount directly from its stack result slot:
        //   CLR.L -(A7); _TickCount; CMP.L (A7)+,Dn; Bcc.S <back-to-CLR>
        // Lemmings uses the BEQ form for its frame delay loop.
        if (w0 & 0xF1FF) == 0xB09F {
            let dn = ((w0 >> 9) & 7) as usize;
            return self.try_spin_template_d(pc_after_trap, dn, tick_cap);
        }

        // Template E computes a signed deadline after TickCount returns:
        //   MOVE.W (d16,An),Dn; EXT.L Dn; ADD.L (d16,Am),Dn
        //   CMP.L (A7)+,Dn; BGT.S/BGE.S <back-to-SUBQ.W #4,A7>
        if (w0 & 0xF1F8) == 0x3028 {
            let dn = ((w0 >> 9) & 7) as usize;
            return self.try_spin_template_e(pc_after_trap, dn, w0, tick_cap);
        }

        // Step 1: MOVE.L (A7)+, Dn (shared by templates A-C).
        if (w0 & 0xF1FF) != 0x201F {
            return false;
        }
        let dn = ((w0 >> 9) & 7) as usize;

        let w1 = self.bus.read_word(pc_after_trap.wrapping_add(2));

        // Template F: compare this TickCount result with a saved register.
        // Repeat while equal, or while an unsigned deadline is still ahead:
        //   MOVE.L (A7)+,Dn; CMP.L Dn,Dm; BEQ.S/BHI.S <back-to-SUBQ #4,A7>
        if (w1 & 0xF1F8) == 0xB080 && (w1 & 7) as usize == dn {
            return self.try_spin_template_f(pc_after_trap, w1, tick_cap);
        }

        // Template G stores the sampled tick in a frame local, subtracts a
        // stable tick origin, and compares the elapsed value with a signed
        // word delay. This is another common classic-compiler delay shape.
        if (w1 & 0xF1F8) == 0x2140 && (w1 & 7) as usize == dn {
            return self.try_spin_template_g(pc_after_trap, dn, w1, tick_cap);
        }

        // Template A: SUBQ.L #imm, Dn; CMP.L Dn, Dm; BHI.S <back-to-SUBQ-#4,A7>
        if (w1 & 0xF1F8) == 0x5180 && (w1 & 0x0007) as usize == dn {
            return self.try_spin_template_a(pc_after_trap, dn, w1, tick_cap, count);
        }

        // Template B: CMP.L (d16, An), Dn; BLS.S/BLT.S/BLE.S <back>
        if (w1 & 0xF1F8) == 0xB0A8 && ((w1 >> 9) & 7) as usize == dn {
            return self.try_spin_template_b(pc_after_trap, dn, w1, tick_cap, count);
        }

        // Template C: CMP.L (xxx).L, Dn; BCS.S <back>
        if (w1 & 0xF1FF) == 0xB0B9 && ((w1 >> 9) & 7) as usize == dn {
            return self.try_spin_template_c(pc_after_trap, dn, tick_cap, count);
        }

        false
    }

    /// Template F: saved-register tick-change or unsigned-deadline wait.
    ///   SUBQ #4,A7; _TickCount; MOVE.L (A7)+,Dn
    ///   CMP.L Dn,Dm; BEQ.S/BHI.S back-to-SUBQ
    ///
    /// Leave the post-trap instructions for exact CPU execution after moving
    /// the captured result to the first tick that makes the branch fall through.
    fn try_spin_template_f(
        &mut self,
        pc_after_trap: u32,
        w_cmp: u16,
        tick_cap: Option<u32>,
    ) -> bool {
        let dm = ((w_cmp >> 9) & 7) as usize;
        let dn = (w_cmp & 7) as usize;
        // MOVE overwrites Dn before CMP. If it also holds the deadline, CMP
        // compares the sample with itself, not with the old register value.
        if dm == dn {
            return false;
        }
        let branch_pc = pc_after_trap.wrapping_add(4);
        let w_branch = self.bus.read_word(branch_pc);
        if !matches!(w_branch & 0xFF00, 0x6200 | 0x6700) || (w_branch & 0x00FF) == 0 {
            return false;
        }
        let displacement = (w_branch & 0xFF) as i8 as i32;
        let target = (branch_pc.wrapping_add(2) as i32).wrapping_add(displacement) as u32;
        let canonical_target = pc_after_trap.wrapping_sub(4);
        if target != canonical_target
            || !matches!(self.bus.read_word(canonical_target), 0x594F | 0x598F)
            || self.bus.read_word(canonical_target.wrapping_add(2)) != 0xA975
        {
            return false;
        }

        let sp = self.m68k.cpu.core.a(7);
        let captured_tick = self.bus.read_long(sp);
        let saved_tick = self.m68k.cpu.core.d(dm);
        let target_tick = match w_branch & 0xFF00 {
            0x6700 if captured_tick == saved_tick => captured_tick.wrapping_add(1),
            // CMP.L Dn,Dm; BHI waits while saved_tick > captured_tick using
            // unsigned arithmetic. Do not reinterpret an expired/wrapped
            // deadline as a future wait. SC2K uses this for its newspaper.
            0x6200 if saved_tick > captured_tick => saved_tick,
            _ => return false,
        };
        match self.advance_until_tick(target_tick, tick_cap) {
            AdvanceResult::CapHit => {
                self.bus.write_long(sp, self.guest_tick());
                true
            }
            AdvanceResult::Advanced => {
                self.bus.write_long(sp, self.guest_tick());
                false
            }
            AdvanceResult::Interrupted | AdvanceResult::TooFar => false,
        }
    }

    /// Template G: elapsed-tick delay through a frame local.
    ///   SUBQ #4,A7; _TickCount; MOVE.L (A7)+,Dn
    ///   MOVE.L Dn,(d16,Am); MOVE.L (d16,Am),Dn
    ///   SUB.L (d16,Ab),Dn; MOVEA.W (d16,Af),An
    ///   CMPA.L Dn,An; BGT.S back-to-SUBQ
    ///
    /// The loop exits when `TickCount - base >= signed_delay`. All post-trap
    /// instructions remain for the CPU so stack, local, registers, and flags
    /// are produced by the guest exactly once at the synthetic exit tick.
    fn try_spin_template_g(
        &mut self,
        pc_after_trap: u32,
        dn: usize,
        w_store: u16,
        tick_cap: Option<u32>,
    ) -> bool {
        let local_an = ((w_store >> 9) & 7) as usize;
        let local_disp = self.bus.read_word(pc_after_trap.wrapping_add(4));

        let w_reload = self.bus.read_word(pc_after_trap.wrapping_add(6));
        if (w_reload & 0xF1F8) != 0x2028
            || ((w_reload >> 9) & 7) as usize != dn
            || (w_reload & 7) as usize != local_an
            || self.bus.read_word(pc_after_trap.wrapping_add(8)) != local_disp
        {
            return false;
        }

        let w_sub = self.bus.read_word(pc_after_trap.wrapping_add(10));
        if (w_sub & 0xF1F8) != 0x90A8 || ((w_sub >> 9) & 7) as usize != dn {
            return false;
        }
        let base_disp = self.bus.read_word(pc_after_trap.wrapping_add(12)) as i16 as i32;
        let base_an = (w_sub & 7) as usize;

        let w_delay = self.bus.read_word(pc_after_trap.wrapping_add(14));
        if (w_delay & 0xF1F8) != 0x3068 || (w_delay & 7) as usize != local_an {
            return false;
        }
        let delay_an = ((w_delay >> 9) & 7) as usize;
        let delay_disp = self.bus.read_word(pc_after_trap.wrapping_add(16)) as i16 as i32;

        let w_cmp = self.bus.read_word(pc_after_trap.wrapping_add(18));
        if (w_cmp & 0xF1F8) != 0xB1C0
            || ((w_cmp >> 9) & 7) as usize != delay_an
            || (w_cmp & 7) as usize != dn
        {
            return false;
        }

        let branch_pc = pc_after_trap.wrapping_add(20);
        let w_branch = self.bus.read_word(branch_pc);
        if (w_branch & 0xFF00) != 0x6E00 || (w_branch & 0x00FF) == 0 {
            return false;
        }
        let displacement = (w_branch & 0xFF) as i8 as i32;
        let target = (branch_pc.wrapping_add(2) as i32).wrapping_add(displacement) as u32;
        let canonical_target = pc_after_trap.wrapping_sub(4);
        if target != canonical_target
            || !matches!(self.bus.read_word(canonical_target), 0x594F | 0x598F)
            || self.bus.read_word(canonical_target.wrapping_add(2)) != 0xA975
        {
            return false;
        }

        let base_addr = (self.m68k.cpu.core.a(base_an) as i32).wrapping_add(base_disp) as u32;
        let base_tick = self.bus.read_long(base_addr);
        let delay_addr = (self.m68k.cpu.core.a(local_an) as i32).wrapping_add(delay_disp) as u32;
        let delay = self.bus.read_word(delay_addr) as i16 as i32;
        let sp = self.m68k.cpu.core.a(7);
        let captured_tick = self.bus.read_long(sp);
        let elapsed = captured_tick.wrapping_sub(base_tick) as i32;
        if delay <= elapsed {
            return false;
        }
        let target_tick = base_tick.wrapping_add(delay as u32);
        match self.advance_until_tick(target_tick, tick_cap) {
            AdvanceResult::CapHit => {
                self.bus.write_long(sp, self.guest_tick());
                true
            }
            AdvanceResult::Advanced => {
                self.bus.write_long(sp, self.guest_tick());
                false
            }
            AdvanceResult::Interrupted | AdvanceResult::TooFar => false,
        }
    }

    /// Cancel only a same-slice observation. A proven sleep has its own write
    /// guard and intentionally survives the frontend boundary.
    fn cancel_idle_cycle_observation(&mut self) {
        if self.idle_cycle_probe.is_some() {
            self.bus.cancel_write_probe();
        }
        self.idle_cycle_probe = None;
        self.idle_cycle_last_seen = None;
    }

    fn cancel_idle_cycle_detector(&mut self) {
        self.bus.cancel_write_probe();
        self.idle_cycle_probe = None;
        self.idle_cycle_last_seen = None;
        self.idle_cycle_sleep = None;
    }

    /// True once (site, tick) has been refused a probe -- or overflowed a
    /// journal -- this tick (the count sits one past the budget), or
    /// while the site is backed off after its probes died to
    /// non-admitted traps.
    fn idle_cycle_site_is_busy(&self, trap_pc: u32, tick: u32) -> bool {
        self.idle_cycle_sites.iter().any(|rec| {
            rec.site == trap_pc
                && ((rec.tick == tick && rec.probes > IDLE_CYCLE_MAX_PROBES_PER_TICK)
                    || (rec.cancel_streak >= 2 && (rec.resume_tick.wrapping_sub(tick) as i32) > 0))
        })
    }

    /// Find the accounting slot for `trap_pc`, evicting the stalest
    /// record when the site is new. A backed-off site looks stale by
    /// `tick` but is doing its job by sitting there; rank staleness by
    /// the larger of the two tick stamps so it is evicted last.
    fn idle_cycle_site_slot(&mut self, trap_pc: u32) -> usize {
        if let Some(i) = self
            .idle_cycle_sites
            .iter()
            .position(|rec| rec.site == trap_pc)
        {
            return i;
        }
        let i = self
            .idle_cycle_sites
            .iter()
            .enumerate()
            .min_by_key(|(_, rec)| rec.tick.max(rec.resume_tick))
            .map_or(0, |(i, _)| i);
        self.idle_cycle_sites[i] = IdleCycleSiteRecord {
            site: trap_pc,
            ..IdleCycleSiteRecord::default()
        };
        i
    }

    /// Mark (site, tick) busy for the rest of the tick: it works between polls.
    fn mark_idle_cycle_site_busy(&mut self, trap_pc: u32, tick: u32) {
        let i = self.idle_cycle_site_slot(trap_pc);
        let rec = &mut self.idle_cycle_sites[i];
        rec.tick = tick;
        rec.probes = IDLE_CYCLE_MAX_PROBES_PER_TICK + 1;
        self.idle_cycle_last_seen = None;
    }

    /// A probe died to a non-admitted trap. One cancel is routine (a
    /// menu command, a real redraw); a streak means this site's cycle
    /// funnels through a trap the admission list does not cover, so no
    /// proof can ever close here and every probe is a pure
    /// de-optimized-execution tax (the armed journal withdraws the bus
    /// fast paths). Back the site off for exponentially longer, capped;
    /// a probe that closes on its origin site -- whatever the verdict --
    /// resets the streak. Probing less often is always sound: the
    /// backoff schedules proofs, it never fabricates one.
    fn note_idle_cycle_trap_cancel_site(&mut self) {
        let Some(probe) = self.idle_cycle_probe.as_ref() else {
            return;
        };
        let (trap_pc, tick) = (probe.trap_pc, probe.tick);
        let i = self.idle_cycle_site_slot(trap_pc);
        let rec = &mut self.idle_cycle_sites[i];
        rec.cancel_streak = rec.cancel_streak.saturating_add(1);
        if rec.cancel_streak >= 2 {
            let backoff = (1u32 << rec.cancel_streak.min(7))
                .min(IDLE_CYCLE_CANCEL_BACKOFF_CAP_TICKS);
            rec.resume_tick = tick.wrapping_add(backoff);
            if wait_stats_enabled() {
                WS_CANCEL_BACKOFFS.fetch_add(1, AtomicOrdering::Relaxed);
            }
        }
    }

    fn reset_idle_cycle_cancel_streak(&mut self, trap_pc: u32) {
        if let Some(rec) = self
            .idle_cycle_sites
            .iter_mut()
            .find(|rec| rec.site == trap_pc)
        {
            rec.cancel_streak = 0;
            rec.resume_tick = 0;
        }
    }

    fn begin_idle_cycle_probe(&mut self, trap_pc: u32, tick: u32, cpu: CpuArchitecturalSnapshot) {
        let slot = self.idle_cycle_site_slot(trap_pc);
        let rec = self.idle_cycle_sites[slot];
        if rec.cancel_streak >= 2 && (rec.resume_tick.wrapping_sub(tick) as i32) > 0 {
            // Backed off after repeated trap cancels; see
            // `note_idle_cycle_trap_cancel_site`.
            if wait_stats_enabled() {
                WS_BACKOFF_SKIPS.fetch_add(1, AtomicOrdering::Relaxed);
            }
            return;
        }
        let probes = if rec.tick == tick { rec.probes } else { 0 };
        if probes >= IDLE_CYCLE_MAX_PROBES_PER_TICK {
            // Budget spent: this site keeps failing to prove within the
            // tick, so it is working, not waiting. No journal.
            self.mark_idle_cycle_site_busy(trap_pc, tick);
            return;
        }
        {
            let rec = &mut self.idle_cycle_sites[slot];
            rec.tick = tick;
            rec.probes = probes + 1;
        }
        if wait_stats_enabled() {
            WS_PROBE_STARTS.fetch_add(1, AtomicOrdering::Relaxed);
        }
        self.bus.begin_write_probe();
        self.idle_cycle_probe = Some(IdleCycleProbe {
            trap_pc,
            tick,
            cpu,
            arrivals: 0,
        });
        self.idle_cycle_last_seen = Some((trap_pc, tick));
    }

    fn park_proven_idle_cycle(&mut self, trap_pc: u32, wake_tick: u32) {
        if wait_stats_enabled() {
            WS_PARKED.fetch_add(1, AtomicOrdering::Relaxed);
        }
        self.bus.cancel_write_probe();
        self.idle_cycle_probe = None;
        self.idle_cycle_last_seen = None;

        let tick = self.guest_tick();
        self.idle_cycle_sleep = Some(ProvenIdleCycleSleep {
            trap_pc,
            wake_tick,
            tick,
            cpu: CpuArchitecturalSnapshot::capture(&self.m68k.cpu.core),
            host: IdleCycleHostSnapshot::capture(&self.dispatcher),
        });
        // No guest code runs while parked. This second journal therefore
        // catches every bus-visible mutation made by the frontend or an HLE
        // subsystem between slices, without scanning all guest RAM.
        self.bus.begin_write_probe();
    }

    /// Reuse a proven identity cycle without executing it again. The proof is
    /// valid across a frontend boundary only if CPU state, every bus-visible
    /// memory write, host input state, the event stream, and SystemTask's
    /// periodic-work condition all remain quiescent. Any mismatch resumes the
    /// guest at the ordinary proven boundary.
    fn try_resume_proven_idle_cycle(&mut self, tick_cap: Option<u32>) -> bool {
        let Some(sleep) = self.idle_cycle_sleep.take() else {
            return false;
        };

        let memory_unchanged = self.bus.finish_write_probe_unchanged();
        let cpu_unchanged = sleep.cpu == CpuArchitecturalSnapshot::capture(&self.m68k.cpu.core);
        let tick_unchanged = self.guest_tick() == sleep.tick
                // The canonical process clock owns the value, while this
                // second check verifies its low-memory guest projection did
                // not diverge during the idle probe.
                && self.bus.read_long(0x016A) == sleep.tick;
        let host_unchanged = sleep.host == IdleCycleHostSnapshot::capture(&self.dispatcher);
        let can_observe_events = self.active_interrupt_callback.is_none()
            && !self.dispatcher.input_state.has_key_repeat()
            && self.dispatcher.pending_launch_app.is_none()
            && !self.dispatcher.system_task_has_periodic_work();
        let event_stream_empty = can_observe_events
            && self
                .dispatcher
                .peek_toolbox_event(&self.bus, u16::MAX)
                .is_none();

        if !memory_unchanged
            || !cpu_unchanged
            || !tick_unchanged
            || !host_unchanged
            || !event_stream_empty
        {
            if wait_stats_enabled() {
                let counter = if !memory_unchanged {
                    &WS_RESUME_FAIL_MEM
                } else if !host_unchanged {
                    &WS_RESUME_FAIL_HOST
                } else {
                    &WS_RESUME_FAIL_OTHER
                };
                counter.fetch_add(1, AtomicOrdering::Relaxed);
            }
            self.cancel_idle_cycle_detector();
            return false;
        }

        let Some(cap) = tick_cap else {
            self.cancel_idle_cycle_detector();
            return false;
        };
        match self.advance_until_tick(sleep.wake_tick, Some(cap)) {
            AdvanceResult::CapHit => {
                if wait_stats_enabled() {
                    WS_RESUMED.fetch_add(1, AtomicOrdering::Relaxed);
                }
                self.park_proven_idle_cycle(sleep.trap_pc, sleep.wake_tick);
                true
            }
            AdvanceResult::Advanced => {
                self.cancel_idle_cycle_detector();
                false
            }
            AdvanceResult::Interrupted | AdvanceResult::TooFar => {
                self.cancel_idle_cycle_detector();
                false
            }
        }
    }

    /// Record whether a returned HLE trap remained quiescent during an exact
    /// cycle probe. Null GetNextEvent/EventAvail calls are deterministic while
    /// the frontend is executing on its event thread: newly arrived host input
    /// cannot be injected until the slice returns. SystemTask is quiescent only
    /// while the HLE has no periodic desk-accessory or driver work.
    fn note_idle_cycle_trap_result(&mut self, opcode: u16) -> bool {
        let null_event = matches!(
            canonical_trap_number(opcode),
            (true, 0x0170) | (true, 0x0171)
        ) && self.bus.read_word(self.m68k.cpu.core.a(7)) == 0;
        if self.idle_cycle_probe.is_none() {
            return null_event;
        }
        // Live D0 still holds the QDExtensions selector here for any
        // admitted selector (GetGWorld/SetGWorld write no registers but
        // A7); a non-admitted selector never reaches this classification
        // -- the pre-dispatch check cancelled the probe before the
        // handler ran.
        let quiescent = idle_cycle_trap_is_journal_complete(
            opcode,
            null_event,
            !self.dispatcher.system_task_has_periodic_work(),
            self.m68k.cpu.read_reg(Register::D0),
        );
        if !quiescent {
            ws_note_cancel_trap(opcode);
            self.note_idle_cycle_trap_cancel_site();
            self.cancel_idle_cycle_detector();
        }
        null_event
    }

    /// Prove that a complete idle event-loop iteration returned to the same
    /// architectural CPU and guest-memory state, then advance only to the
    /// first known dependency: the next guest tick, the GUI wall-clock cap, or
    /// an interrupt.
    ///
    /// The first same-tick repeat starts a one-cycle write journal; a third
    /// visit closes it. This warm-up is evidence collection, not an eligibility
    /// threshold. Any changed final byte, CPU state, non-null event, unknown
    /// trap, periodic SystemTask work, tick change, or interrupt rejects the
    /// proof and leaves normal execution in place.
    fn try_exact_idle_cycle_fastfwd(
        &mut self,
        trap_pc: u32,
        wake_tick: u32,
        tick_cap: Option<u32>,
    ) -> bool {
        let tick = self.guest_tick();
        if self.idle_cycle_site_is_busy(trap_pc, tick) {
            // This site spent its probe budget (or overflowed a journal)
            // this tick: it is working between polls, not waiting.
            return false;
        }
        let cpu = CpuArchitecturalSnapshot::capture(&self.m68k.cpu.core);

        if let Some(probe) = self.idle_cycle_probe.take() {
            if self.bus.take_write_probe_overflow() {
                // The journal blew past its cap since the probe began, so
                // this cycle did real work. The bus already dropped the
                // journal and restored its fast paths; drop the observation
                // too and back off from the site until the tick changes.
                if wait_stats_enabled() {
                    WS_PROBE_OVERFLOWS.fetch_add(1, AtomicOrdering::Relaxed);
                }
                self.mark_idle_cycle_site_busy(probe.trap_pc, probe.tick);
                return false;
            }
            let same_site_tick = probe.trap_pc == trap_pc && probe.tick == tick;
            if same_site_tick {
                // The probe closed on its origin without dying to a
                // foreign trap: cycles here are provable-shaped.
                self.reset_idle_cycle_cancel_streak(trap_pc);
            }
            if same_site_tick && probe.cpu == cpu {
                // A cycle of whatever small period closed on its origin
                // state; the journal -- held open across every arrival
                // since the probe began -- decides whether it was a wait.
                let memory_unchanged = self.bus.finish_write_probe_unchanged();
                if wait_stats_enabled() {
                    if memory_unchanged {
                        WS_EXACT_REPEATS.fetch_add(1, AtomicOrdering::Relaxed);
                    } else {
                        WS_FAIL_MEM.fetch_add(1, AtomicOrdering::Relaxed);
                    }
                }
                if memory_unchanged {
                    self.idle_cycle_last_seen = Some((trap_pc, tick));
                    if tick_cap.is_some_and(|cap| tick >= cap) {
                        self.park_proven_idle_cycle(trap_pc, wake_tick);
                        return true;
                    }
                    match self.advance_until_tick(wake_tick, tick_cap) {
                        AdvanceResult::CapHit => {
                            self.park_proven_idle_cycle(trap_pc, wake_tick);
                            return true;
                        }
                        AdvanceResult::Advanced => {
                            self.cancel_idle_cycle_detector();
                            return false;
                        }
                        AdvanceResult::Interrupted | AdvanceResult::TooFar => {
                            self.cancel_idle_cycle_detector();
                            return false;
                        }
                    }
                }
                // Writes did not restore: real progress, not a wait.
                // Re-prove from the current state.
                self.begin_idle_cycle_probe(trap_pc, tick, cpu);
                return false;
            }

            if same_site_tick && probe.arrivals + 1 < IDLE_CYCLE_MAX_PERIOD {
                // Mid-period arrival (EV Override's crawl alternates two
                // polled keycodes, a strict period-2 cycle): keep both the
                // probe and its write journal open and wait for the origin
                // state to come around.
                if wait_stats_enabled() {
                    WS_PERIOD_STEPS.fetch_add(1, AtomicOrdering::Relaxed);
                }
                self.idle_cycle_probe = Some(IdleCycleProbe {
                    arrivals: probe.arrivals + 1,
                    ..probe
                });
                return false;
            }

            // Period cap exceeded, tick changed, or a different site: this
            // observation is dead. Close the journal before rebaselining.
            if wait_stats_enabled() {
                if !same_site_tick && probe.tick != tick {
                    WS_FAIL_TICK.fetch_add(1, AtomicOrdering::Relaxed);
                } else if same_site_tick {
                    let n = WS_FAIL_CPU.fetch_add(1, AtomicOrdering::Relaxed);
                    if n < 40 {
                        let a = &probe.cpu;
                        let b = &cpu;
                        let mut diffs: Vec<String> = Vec::new();
                        for i in 0..16 {
                            if a.dar[i] != b.dar[i] {
                                diffs.push(format!(
                                    "{}{}:{:08X}->{:08X}",
                                    if i < 8 { "D" } else { "A" },
                                    i & 7,
                                    a.dar[i],
                                    b.dar[i]
                                ));
                            }
                        }
                        if a.ppc != b.ppc {
                            diffs.push(format!("ppc:{:08X}->{:08X}", a.ppc, b.ppc));
                        }
                        if a.pc != b.pc {
                            diffs.push(format!("pc:{:08X}->{:08X}", a.pc, b.pc));
                        }
                        if a.ir != b.ir {
                            diffs.push(format!("ir:{:04X}->{:04X}", a.ir, b.ir));
                        }
                        if a.sr != b.sr {
                            diffs.push(format!("sr:{:04X}->{:04X}", a.sr, b.sr));
                        }
                        if a.prefetch_count != b.prefetch_count {
                            diffs.push(format!("pfn:{}->{}", a.prefetch_count, b.prefetch_count));
                        }
                        if a.prefetch != b.prefetch {
                            diffs.push(format!("pf:{:04X?}->{:04X?}", a.prefetch, b.prefetch));
                        }
                        if a.change_of_flow != b.change_of_flow {
                            diffs.push(format!("cof:{}->{}", a.change_of_flow, b.change_of_flow));
                        }
                        if a.stack_pointers != b.stack_pointers {
                            diffs.push("sp".to_owned());
                        }
                        if diffs.is_empty() {
                            diffs.push("other-field".to_owned());
                        }
                        eprintln!(
                            "[WAIT-STATS] cpu-diff sample {n} pc={trap_pc:08X}: {}",
                            diffs.join(" ")
                        );
                    }
                }
            }
            self.bus.cancel_write_probe();
            if same_site_tick {
                self.begin_idle_cycle_probe(trap_pc, tick, cpu);
            } else {
                self.idle_cycle_last_seen = Some((trap_pc, tick));
            }
            return false;
        }

        if self.idle_cycle_last_seen == Some((trap_pc, tick)) {
            self.begin_idle_cycle_probe(trap_pc, tick, cpu);
        } else {
            self.idle_cycle_last_seen = Some((trap_pc, tick));
        }
        false
    }

    /// Observe an entire null-event state-machine pass rather than a specific
    /// compiler template. The proof starts at the post-GetNextEvent boundary
    /// and closes only when execution returns to that exact trap site with the
    /// same CPU state and identical final values for every RAM byte written in
    /// between. A successful proof can safely park only until the next tick:
    /// unlike a decoded timeout predicate, arbitrary guest code may begin
    /// tick-dependent work then.
    fn try_exact_null_event_cycle_fastfwd(&mut self, trap_pc: u32, tick_cap: Option<u32>) -> bool {
        if wait_stats_enabled() {
            WS_ANCHOR_CALLS.fetch_add(1, AtomicOrdering::Relaxed);
        }
        if let Some(probe) = self.idle_cycle_probe.as_ref() {
            // Avoid switching between multiple event sites while a complete
            // cycle is being measured.
            if probe.trap_pc != trap_pc {
                return false;
            }
        } else if let Some((anchor_pc, anchor_tick)) = self.idle_cycle_last_seen {
            // Preserve the first same-tick null-event site as the proof
            // anchor across other permitted polling sites. A nested event
            // loop may alternate A → B → A; replacing the anchor at B would
            // prevent the complete A-to-A cycle from ever being observed.
            if anchor_pc != trap_pc && anchor_tick == self.guest_tick() {
                return false;
            }
        }

        let wake_tick = self.guest_tick().wrapping_add(1);
        self.try_exact_idle_cycle_fastfwd(trap_pc, wake_tick, tick_cap)
    }

    /// Template E: signed computed-deadline variant.
    ///   SUBQ.W  #4, A7
    ///   _TickCount
    ///   MOVE.W (d16, An), Dn
    ///   EXT.L   Dn
    ///   ADD.L   (d16, Am), Dn
    ///   CMP.L   (A7)+, Dn
    ///   BGT.S/BGE.S <back-to-SUBQ.W #4,A7>
    ///
    /// The loop repeats while `base_tick + signed_delay > TickCount()` or
    /// `>= TickCount()`, according to the branch condition. Read the two stable
    /// operands to obtain that deadline, advance to the first tick that exits
    /// the loop, and leave the five post-trap instructions for the CPU.
    /// Executing the final iteration normally preserves the exact arithmetic
    /// flags, stack pop, register result, and branch behavior.
    fn try_spin_template_e(
        &mut self,
        pc_after_trap: u32,
        dn: usize,
        w_move: u16,
        tick_cap: Option<u32>,
    ) -> bool {
        let w_ext = self.bus.read_word(pc_after_trap.wrapping_add(4));
        if w_ext != (0x48C0 | dn as u16) {
            return false;
        }

        let w_add = self.bus.read_word(pc_after_trap.wrapping_add(6));
        if (w_add & 0xF1F8) != 0xD0A8 || ((w_add >> 9) & 7) as usize != dn {
            return false;
        }

        let w_cmp = self.bus.read_word(pc_after_trap.wrapping_add(10));
        if (w_cmp & 0xF1FF) != 0xB09F || ((w_cmp >> 9) & 7) as usize != dn {
            return false;
        }

        let branch_pc = pc_after_trap.wrapping_add(12);
        let w_branch = self.bus.read_word(branch_pc);
        let condition = w_branch & 0xFF00;
        if condition != 0x6E00 && condition != 0x6C00 {
            return false;
        }
        let displacement = (w_branch & 0xFF) as i8 as i32;
        if displacement == 0 {
            return false;
        }
        let target = (branch_pc.wrapping_add(2) as i32).wrapping_add(displacement) as u32;
        if target != pc_after_trap.wrapping_sub(4) || self.bus.read_word(target) != 0x594F {
            return false;
        }

        let move_an = (w_move & 7) as usize;
        let move_disp = self.bus.read_word(pc_after_trap.wrapping_add(2)) as i16 as i32;
        let delay_addr = (self.m68k.cpu.core.a(move_an) as i32).wrapping_add(move_disp) as u32;
        let delay = self.bus.read_word(delay_addr) as i16 as i32;

        let add_an = (w_add & 7) as usize;
        let add_disp = self.bus.read_word(pc_after_trap.wrapping_add(8)) as i16 as i32;
        let base_addr = (self.m68k.cpu.core.a(add_an) as i32).wrapping_add(add_disp) as u32;
        let deadline = self.bus.read_long(base_addr).wrapping_add(delay as u32);

        let sp = self.m68k.cpu.core.a(7);
        let captured_tick = self.bus.read_long(sp);
        let deadline_signed = deadline as i32;
        let captured_tick_signed = captured_tick as i32;
        let still_waiting = match condition {
            0x6E00 => deadline_signed > captured_tick_signed,
            0x6C00 => deadline_signed >= captured_tick_signed,
            _ => unreachable!(),
        };
        if !still_waiting {
            // The branch would already fall through, so there is no wait to skip.
            return false;
        }

        // BGE needs the first signed tick strictly after the deadline. Crossing
        // i32::MAX would instead wrap to i32::MIN and keep the branch taken, so
        // leave that rare boundary to normal execution.
        let exit_tick = if condition == 0x6C00 {
            if deadline_signed == i32::MAX {
                return false;
            }
            deadline.wrapping_add(1)
        } else {
            deadline
        };

        match self.advance_until_tick(exit_tick, tick_cap) {
            AdvanceResult::CapHit => {
                self.bus.write_long(sp, self.guest_tick());
                true
            }
            AdvanceResult::Advanced => {
                self.bus.write_long(sp, self.guest_tick());
                false
            }
            AdvanceResult::Interrupted | AdvanceResult::TooFar => false,
        }
    }

    /// Template D: direct stack-result compare variant.
    ///   CLR.L  -(A7)
    ///   _TickCount
    ///   CMP.L  (A7)+, Dn
    ///   BCC.S/BEQ.S <back-to-CLR.L>
    ///
    /// BCC repeats while `Dn >= TickCount()`, so its first fall-through tick is
    /// `Dn + 1`. BEQ repeats while `Dn == TickCount()`; only accelerate it when
    /// the just-captured tick equals Dn, and advance by exactly one tick. Leave
    /// CMP/Bcc for the CPU to execute once after advancing; that preserves its
    /// exact flags, stack update, and instruction count.
    fn try_spin_template_d(
        &mut self,
        pc_after_trap: u32,
        dn: usize,
        tick_cap: Option<u32>,
    ) -> bool {
        let w_branch = self.bus.read_word(pc_after_trap.wrapping_add(2));
        let branch_condition = w_branch & 0xFF00;
        if branch_condition != 0x6400 && branch_condition != 0x6700 {
            return false;
        }
        let displacement = (w_branch & 0xFF) as i8 as i32;
        if displacement == 0 {
            return false;
        }
        let branch_pc = pc_after_trap.wrapping_add(2);
        let target = (branch_pc.wrapping_add(2) as i32).wrapping_add(displacement) as u32;
        if target != pc_after_trap.wrapping_sub(4) || self.bus.read_word(target) != 0x42A7 {
            return false;
        }

        let dn_value = self.m68k.cpu.core.d(dn);
        let captured_tick = self.bus.read_long(self.m68k.cpu.core.a(7));
        if branch_condition == 0x6700 && captured_tick != dn_value {
            // BEQ would already fall through, so there is no wait to skip.
            return false;
        }
        let target_tick = dn_value.wrapping_add(1);
        match self.advance_until_tick(target_tick, tick_cap) {
            AdvanceResult::CapHit => {
                // The trap's result was captured before the synthetic VBLs.
                // Refresh it so the resumed comparison observes the same tick
                // that a real busy loop would obtain on its next iteration.
                let sp = self.m68k.cpu.core.a(7);
                self.bus.write_long(sp, self.guest_tick());
                true
            }
            AdvanceResult::Advanced => {
                let sp = self.m68k.cpu.core.a(7);
                self.bus.write_long(sp, self.guest_tick());
                false
            }
            AdvanceResult::Interrupted | AdvanceResult::TooFar => false,
        }
    }

    /// Template A: classic pre-System-7 SUBQ-compare spin.
    ///   MOVE.L (A7)+, Dn
    ///   SUBQ.L #imm, Dn
    ///   CMP.L  Dn, Dm
    ///   BHI.S  <SUBQ.W #4, A7 before the _TickCount>
    fn try_spin_template_a(
        &mut self,
        pc_after_trap: u32,
        dn: usize,
        w1: u16,
        tick_cap: Option<u32>,
        count: &mut usize,
    ) -> bool {
        let imm_bits = ((w1 >> 9) & 7) as u32;
        let imm = if imm_bits == 0 { 8 } else { imm_bits };

        let w2 = self.bus.read_word(pc_after_trap.wrapping_add(4));
        let w3 = self.bus.read_word(pc_after_trap.wrapping_add(6));

        // CMP.L Dn, Dm (0xB_80 family, src-mode 000 = data reg direct).
        if (w2 & 0xF1F8) != 0xB080 || (w2 & 0x0007) as usize != dn {
            return false;
        }
        let dm = ((w2 >> 9) & 7) as usize;

        // BHI.S
        if (w3 & 0xFF00) != 0x6200 {
            return false;
        }
        let disp8 = (w3 & 0xFF) as i8 as i32;
        if disp8 == 0 {
            return false;
        }
        let branch_src = pc_after_trap.wrapping_add(6);
        let target = (branch_src.wrapping_add(2) as i32).wrapping_add(disp8) as u32;
        if target != pc_after_trap.wrapping_sub(4) {
            return false;
        }

        let dm_val = self.m68k.cpu.core.d(dm);
        let target_tick = dm_val.wrapping_add(imm);
        match self.advance_until_tick(target_tick, tick_cap) {
            AdvanceResult::CapHit => return true,
            AdvanceResult::Interrupted | AdvanceResult::TooFar => return false,
            AdvanceResult::Advanced => {}
        }

        // Synthesise exit: Dn = final_tick - imm = Dm (by definition of
        // the fall-through condition), A7 += 4, PC past BHI.S.
        let final_tick = self.guest_tick();
        let sp = self.m68k.cpu.core.a(7);
        self.m68k.cpu.core.set_a(7, sp.wrapping_add(4));
        self.m68k.cpu.core.set_d(dn, final_tick.wrapping_sub(imm));
        self.m68k.cpu.core.pc = pc_after_trap.wrapping_add(8);

        *count += 4;
        self.total_instructions = self.total_instructions.wrapping_add(4);
        false
    }

    /// Template B: memory-target variant.
    ///   MOVE.L (A7)+, Dn
    ///   CMP.L  (d16, An), Dn    ; 4 bytes (opcode word + d16)
    ///   BLS.S/BLT.S/BLE.S <back-to-SUBQ.W #4,A7 before the _TickCount>
    ///
    /// BLS and BLE exit after the memory target; BLT exits at the target.
    /// Classic compilers use both signed and unsigned comparisons. A signed
    /// inclusive comparison is accelerated only while incrementing its target
    /// cannot cross i32::MAX.
    fn try_spin_template_b(
        &mut self,
        pc_after_trap: u32,
        dn: usize,
        w1: u16,
        tick_cap: Option<u32>,
        count: &mut usize,
    ) -> bool {
        let an = (w1 & 7) as usize;
        let d16 = self.bus.read_word(pc_after_trap.wrapping_add(4)) as i16 as i32;
        let w_brk = self.bus.read_word(pc_after_trap.wrapping_add(6));

        // BLS.S/BLT.S/BLE.S disp8
        let branch_condition = w_brk & 0xFF00;
        if branch_condition != 0x6300 && branch_condition != 0x6D00 && branch_condition != 0x6F00 {
            return false;
        }
        let disp8 = (w_brk & 0xFF) as i8 as i32;
        if disp8 == 0 {
            return false;
        }
        // Only skip the canonical result-slot allocation and _TickCount trap.
        // A branch farther back may include stateful work (for example, a
        // calibration counter) that must execute on every iteration.
        let branch_src = pc_after_trap.wrapping_add(6);
        let target = (branch_src.wrapping_add(2) as i32).wrapping_add(disp8) as u32;
        let canonical_target = pc_after_trap.wrapping_sub(4);
        if target != canonical_target
            || self.bus.read_word(canonical_target) != 0x594F
            || self.bus.read_word(canonical_target.wrapping_add(2)) != 0xA975
        {
            return false;
        }

        let an_val = self.m68k.cpu.core.a(an);
        let mem_addr = (an_val as i32).wrapping_add(d16) as u32;
        let mem_target = self.bus.read_long(mem_addr);
        if branch_condition == 0x6D00 || branch_condition == 0x6F00 {
            let captured_tick = self.bus.read_long(self.m68k.cpu.core.a(7));
            let still_waiting = if branch_condition == 0x6D00 {
                (captured_tick as i32) < (mem_target as i32)
            } else {
                (captured_tick as i32) <= (mem_target as i32)
            };
            if !still_waiting || (branch_condition == 0x6F00 && mem_target == i32::MAX as u32) {
                // The signed branch would already fall through, or signed
                // TickCount overflow would keep BLE taken at target + 1.
                return false;
            }
        }
        let target_tick = if branch_condition == 0x6D00 {
            mem_target
        } else {
            mem_target.wrapping_add(1)
        };

        match self.advance_until_tick(target_tick, tick_cap) {
            AdvanceResult::CapHit => return true,
            AdvanceResult::Interrupted | AdvanceResult::TooFar => return false,
            AdvanceResult::Advanced => {}
        }

        // Synthesise exit: Dn = final_tick, A7 += 4, PC past the branch.
        // body_size: MOVE.L (2) + CMP.L w/d16 (4) + Bcc.S (2) = 8 bytes.
        let final_tick = self.guest_tick();
        let sp = self.m68k.cpu.core.a(7);
        self.m68k.cpu.core.set_a(7, sp.wrapping_add(4));
        self.m68k.cpu.core.set_d(dn, final_tick);
        self.m68k.cpu.core.pc = pc_after_trap.wrapping_add(8);

        *count += 3;
        self.total_instructions = self.total_instructions.wrapping_add(3);
        false
    }

    /// Template C: absolute-long target variant.
    ///   MOVE.L (A7)+, Dn
    ///   CMP.L  (xxx).L, Dn
    ///   BCS.S  <back-to-SUBQ.W #4,A7 before the _TickCount>
    ///
    /// Exit when `TickCount() >= *(xxx).L`.
    fn try_spin_template_c(
        &mut self,
        pc_after_trap: u32,
        dn: usize,
        tick_cap: Option<u32>,
        count: &mut usize,
    ) -> bool {
        let target_addr = self.bus.read_long(pc_after_trap.wrapping_add(4));
        let w_brk = self.bus.read_word(pc_after_trap.wrapping_add(8));

        // BCS.S/BLO.S disp8. The loop repeats while Dn < *(xxx).L.
        if (w_brk & 0xFF00) != 0x6500 {
            return false;
        }
        let disp8 = (w_brk & 0xFF) as i8 as i32;
        if disp8 == 0 {
            return false;
        }
        let branch_src = pc_after_trap.wrapping_add(8);
        let target = (branch_src.wrapping_add(2) as i32).wrapping_add(disp8) as u32;
        if target != pc_after_trap.wrapping_sub(4) {
            return false;
        }

        let target_tick = self.bus.read_long(target_addr);
        match self.advance_until_tick(target_tick, tick_cap) {
            AdvanceResult::CapHit => return true,
            AdvanceResult::Interrupted | AdvanceResult::TooFar => return false,
            AdvanceResult::Advanced => {}
        }

        let final_tick = self.guest_tick();
        let sp = self.m68k.cpu.core.a(7);
        self.m68k.cpu.core.set_a(7, sp.wrapping_add(4));
        self.m68k.cpu.core.set_d(dn, final_tick);
        self.m68k.cpu.core.pc = pc_after_trap.wrapping_add(10);

        *count += 3;
        self.total_instructions = self.total_instructions.wrapping_add(3);
        false
    }

    /// Shared helper: advance guest ticks until `target_tick` is
    /// reached.
    fn advance_until_tick(&mut self, target_tick: u32, tick_cap: Option<u32>) -> AdvanceResult {
        let current_tick = self.guest_tick();
        let ticks_to_advance = target_tick.wrapping_sub(current_tick);
        if ticks_to_advance > SPIN_FASTFWD_MAX_TICKS {
            return AdvanceResult::TooFar;
        }
        for _ in 0..ticks_to_advance {
            if let Some(cap) = tick_cap {
                if self.guest_tick() >= cap {
                    return AdvanceResult::CapHit;
                }
            }
            self.advance_guest_tick();
            // This path bypasses `charge_tick_budget`, so each synthetic
            // vertical-retrace boundary must start with a full budget.
            // Inside Macintosh: Processes (1993), p. 3-46.
            self.tick_budget = self.instructions_per_tick as i32;
            if self.active_interrupt_callback.is_some() {
                return AdvanceResult::Interrupted;
            }
        }
        AdvanceResult::Advanced
    }

    fn dispatch_classic_with_process_services(&mut self, opcode: u16) -> Result<()> {
        let role = if self.native.availability().application {
            NativeEngineRole::Application
        } else {
            NativeEngineRole::Companion
        };
        let mut bindings = self
            .native
            .adapter_mut(role)
            .map(PpcLoadedApp::cfm_symbol_bindings);
        self.dispatcher.dispatch_with_process_services(
            opcode,
            &mut self.m68k.cpu,
            &mut self.bus,
            self.process_context.cfm(),
            bindings
                .as_mut()
                .map(|bindings| bindings as &mut dyn crate::cfm::CfmSymbolBindings),
        )
    }

    fn run_steps_internal(
        &mut self,
        max_steps: usize,
        tick_cap: Option<u32>,
        audio_samples: usize,
        yield_for_ui: bool,
        sound_work_only: bool,
        finish_frame: FrameFinalization,
    ) -> (usize, bool) {
        let previous_mouse = self.bus.read_long(crate::memory::globals::addr::MOUSE_LOC2);
        let result = self.run_steps_internal_impl(
            max_steps,
            tick_cap,
            audio_samples,
            yield_for_ui,
            sound_work_only,
            finish_frame,
        );
        self.sync_guest_mouse_position(previous_mouse);
        // Returning to the embedding is a scheduler safe point. Publish any
        // context transition that occurred during this execution chunk.
        self.debug_note_active_context();
        if self.halted {
            self.debug_note_terminal();
        } else {
            // A step completes only after execution returns to this safe point.
            self.debug_finish_step_if_ready();
        }
        result
    }

    fn run_steps_internal_impl(
        &mut self,
        max_steps: usize,
        tick_cap: Option<u32>,
        audio_samples: usize,
        yield_for_ui: bool,
        sound_work_only: bool,
        finish_frame: FrameFinalization,
    ) -> (usize, bool) {
        if self.guest_work_is_suspended() {
            return (0, !self.halted);
        }
        self.dispatcher.guest_calls.resume_ready_task();
        self.m68k.apply_task_handoff();
        let route = self
            .dispatcher
            .guest_calls
            .execution_route(self.native.availability());
        self.debug_note_active_context();
        if route == ExecutionRoute::Blocked {
            return (0, !self.halted);
        }
        let session_task = self.dispatcher.guest_calls.current_task();
        if route == ExecutionRoute::NativeApplication {
            if yield_for_ui {
                return self.run_ppc_steps(
                    max_steps,
                    tick_cap,
                    audio_samples,
                    false,
                    finish_frame,
                    true,
                );
            }

            let mut count = 0usize;
            let mut running = !self.halted;
            while count < max_steps && running {
                let (steps, still_running) = self.run_ppc_steps(
                    max_steps - count,
                    tick_cap,
                    0,
                    true,
                    FrameFinalization::Deferred,
                    true,
                );
                count = count.saturating_add(steps);
                running = still_running;
                if self.guest_work_is_suspended() || self.debug_step_units_remaining() == Some(0) {
                    break;
                }
                if steps == 0 {
                    break;
                }
                // A callback may select another task, or a worker's native
                // call may finish and expose classic work. Re-arbitrate before
                // another native batch can consume the retained application CPU.
                if self.dispatcher.guest_calls.current_task() != session_task
                    || self
                        .dispatcher
                        .guest_calls
                        .execution_route(self.native.availability())
                        != route
                {
                    break;
                }
            }
            if self.dispatcher.guest_calls.current_task() != session_task
                || self
                    .dispatcher
                    .guest_calls
                    .execution_route(self.native.availability())
                    != route
            {
                if finish_frame != FrameFinalization::Deferred {
                    self.sync_ppc_deferred_host_state();
                    self.finish_host_frame(finish_frame, audio_samples, false);
                }
                return (count, running);
            }
            if finish_frame != FrameFinalization::Deferred {
                self.sync_ppc_deferred_host_state();
                if self.halted_by_exit_to_shell() {
                    if finish_frame == FrameFinalization::Complete {
                        self.redraw_chrome_outside_idle_journal();
                    }
                } else {
                    self.finish_host_frame(finish_frame, audio_samples, false);
                }
            }
            return (count, running);
        }

        if route == ExecutionRoute::PrepareCompanion {
            let current_tick = self.process_context.migrated_handles().ticks.current_tick();
            let migrated_services = {
                let companion = self
                    .native
                    .staged_companion()
                    .expect("selected staged native companion");
                assert!(
                    self.process_context.can_install_cfm_seed(&companion.cfm),
                    "CFM seed conflicts with the process registry"
                );
                companion
                    .preflight_migrated_services(&self.process_context, current_tick)
                    .expect("native companion services conflict with the process registry")
            };
            let companion = self
                .native
                .take_staged_companion()
                .expect("selected staged native companion");
            self.init_ppc_companion_with_services(companion, migrated_services);
        }
        if matches!(
            route,
            ExecutionRoute::NativeCompanion | ExecutionRoute::PrepareCompanion
        ) {
            return self.run_ppc_steps(
                max_steps,
                tick_cap,
                audio_samples,
                false,
                finish_frame,
                false,
            );
        }

        // An unfinished proof may never span a frontend scheduling boundary.
        // A *completed* proof is different: it remains parked behind a second
        // memory-write journal and exact CPU/input/event guards, all checked
        // before it can be reused below.
        self.cancel_idle_cycle_observation();

        // Freeze ticks while menu/control tracking is active. ModalDialog
        // refires still return to the GUI for intermediate rendering, but
        // they must not freeze ticks: EV's pilot dialogs keep Sound/VBL/Time
        // Manager work alive through the dialog manager's event loop.
        // On entry, cap tick_cap to the frozen value; when tracking ends
        // mid-frame, snap $016A to wall-clock time so there's no gap to catch
        // up on.
        let real_tick_cap = tick_cap;
        let tick_cap = match self.frozen_ticks {
            Some(frozen) => tick_cap.map(|_| frozen),
            None => tick_cap,
        };

        self.dispatcher.instruction_count = self.total_instructions;
        let mut count = 0;
        let mut tick_cap_reached = false;
        let mut sound_interrupt_dispatched = self
            .active_interrupt_callback
            .map(|callback| is_sound_interrupt_source(callback.source))
            .unwrap_or(false);
        let mut watch_buf = Vec::with_capacity(4);

        while count < max_steps && !self.halted && !tick_cap_reached {
            if self.debug_finish_step_if_ready() {
                return (count, !self.halted);
            }
            if sound_work_only
                && !self.callback_suspends_guest_clock()
                && (sound_interrupt_dispatched || !self.has_pending_sound_work())
            {
                break;
            }

            // File Manager async completions are interrupt work. Deliver a
            // completed request before the foreground application can inspect
            // or reuse its parameter block.
            if !sound_work_only && !self.callback_suspends_guest_clock() {
                self.fire_file_completion_callback();
                self.fire_adb_callback();
            }

            // Sound callbacks are interrupt work. If a previous slice queued
            // one, dispatch it before running more foreground guest code. Do
            // not drain the whole queue in one CPU slice: double-buffer
            // callbacks are paced by audio-buffer completion, and firing
            // several back-to-back at the same guest PC/tick makes games that
            // run their own mixer refill with click-sized fragments.
            if !self.callback_suspends_guest_clock() && !sound_interrupt_dispatched {
                sound_interrupt_dispatched = self.fire_sound_callbacks();
                if !sound_interrupt_dispatched {
                    sound_interrupt_dispatched = self.fire_sound_doubleback_callbacks();
                }
                if sound_work_only && !sound_interrupt_dispatched {
                    continue;
                }
            }

            if sound_work_only && !self.callback_suspends_guest_clock() {
                break;
            }

            if !sound_work_only && !self.callback_suspends_guest_clock() {
                self.fire_timer_tasks_at(self.current_timer_subtick());
                if self.callback_suspends_guest_clock() {
                    continue;
                }
            }

            // Service blocking traps (Delay, WaitNextEvent sleep).
            if !sound_work_only {
                if self.service_wait_sleep_ticks(tick_cap) {
                    break;
                }
                if self.service_delay_ticks(tick_cap) {
                    break;
                }
            }

            if self.m68k.cpu.is_stopped() {
                self.halted = true;
                self.debug_note_terminal();
                self.halted_pc = Some(self.m68k.cpu.read_reg(Register::PC));
                self.halted_sp = Some(self.m68k.cpu.read_reg(Register::A7));
                self.halted_d0 = Some(self.m68k.cpu.read_reg(Register::D0));
                self.dump_trace();
                return (count, false);
            }

            let pc = self.m68k.cpu.read_reg(Register::PC);
            // Defer reading SP until needed. sp is only used by the
            // interrupt-callback match (rare), the env-gated
            // trace_buffer path, and the PC-bounds error branch.
            //
            // Opcode + trace_buffer reads are gated behind
            // `SYSTEMLESS_TRACE_BUFFER`. Without the gate the `read_word`
            // and `VecDeque` pop/push run on every instruction fetch
            // just so `dump_trace()` can show recent instructions on a
            // halt. Default off; enable for crash diagnostics.
            if trace_buffer_enabled() {
                let opcode = self.bus.read_word(pc);
                let a0 = self.m68k.cpu.read_reg(Register::A0);
                let a6 = self.m68k.cpu.read_reg(Register::A6);
                let a5 = self.m68k.cpu.read_reg(Register::A5);
                let sp = self.m68k.cpu.read_reg(Register::A7);
                if self.trace_buffer.len() >= 200 {
                    self.trace_buffer.pop_front();
                }
                self.trace_buffer.push_back((pc, opcode, a0, sp, a6, a5));
            }

            if let Some(active_interrupt_callback) = self.active_interrupt_callback {
                let sp = self.m68k.cpu.read_reg(Register::A7);
                if pc == active_interrupt_callback.resume_pc
                    && sp == active_interrupt_callback.resume_sp
                {
                    if trace_timer_enabled() {
                        eprintln!(
                            "[TIMER] resume {:?} pc=${:08X} sp=${:08X} restore_ccr=${:02X}",
                            active_interrupt_callback.source, pc, sp, active_interrupt_callback.ccr
                        );
                    }
                    if trace_sound_runner_enabled()
                        && is_sound_interrupt_source(active_interrupt_callback.source)
                    {
                        eprintln!(
                            "[SOUND-CB] resume {:?} pc=${:08X} sp=${:08X} restore_ccr=${:02X}",
                            active_interrupt_callback.source, pc, sp, active_interrupt_callback.ccr
                        );
                    }
                    for (index, value) in
                        active_interrupt_callback.d_regs.iter().copied().enumerate()
                    {
                        self.m68k.cpu.write_reg(
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
                    for (index, value) in
                        active_interrupt_callback.a_regs.iter().copied().enumerate()
                    {
                        self.m68k.cpu.write_reg(
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
                    self.m68k
                        .cpu
                        .core
                        .set_sr_noint_nosp(active_interrupt_callback.sr);
                    if matches!(
                        active_interrupt_callback.source,
                        ActiveInterruptCallbackSource::DialogDrawProc
                    ) {
                        if let Some(snapshot) = self.dialog_draw_port_snapshot.take() {
                            self.dispatcher.restore_current_port_state(
                                &mut self.bus,
                                &mut self.m68k.cpu,
                                &snapshot,
                            );
                        }
                    }
                    if let Some((port, gdevice)) = active_interrupt_callback.restore_port {
                        self.dispatcher.set_current_port_state(
                            &mut self.bus,
                            &mut self.m68k.cpu,
                            port,
                            Some(gdevice),
                        );
                    }
                    let completed_dialog_draw_proc = matches!(
                        active_interrupt_callback.source,
                        ActiveInterruptCallbackSource::DialogDrawProc
                    );
                    let completed_modeless_dialog_draw_proc = completed_dialog_draw_proc
                        && self.dispatcher.active_modeless_dialog_draw_proc.is_some();
                    let completed_modal_dialog_draw_proc =
                        completed_dialog_draw_proc && !completed_modeless_dialog_draw_proc;
                    if completed_dialog_draw_proc {
                        self.dispatcher
                            .finalize_dialog_draw_procs_if_idle(&mut self.bus);
                    }
                    self.active_interrupt_callback = self.suspended_dialog_callback.take();
                    if matches!(
                        active_interrupt_callback.source,
                        ActiveInterruptCallbackSource::DialogDrawProc
                            | ActiveInterruptCallbackSource::DialogFilterProc
                    ) {
                        self.resume_parent_dialog_call(
                            active_interrupt_callback.source
                                == ActiveInterruptCallbackSource::DialogFilterProc,
                        );
                    }
                    self.refill_foreground_budget_after_async_return();
                    if completed_modeless_dialog_draw_proc && self.fire_modeless_dialog_draw_proc()
                    {
                        continue;
                    }
                    // A ModalDialog update pass calls every pending userItem
                    // procedure before it starts handling events. Drain the
                    // remaining callbacks directly from the completed callback
                    // boundary so a foreground idle loop cannot run between
                    // items. Inside Macintosh Volume I, I-405.
                    if completed_modal_dialog_draw_proc
                        && self
                            .dispatcher
                            .dialog_tracking
                            .as_ref()
                            .is_some_and(|tracking| !tracking.draw_procs_done)
                        && self.fire_dialog_draw_procs()
                    {
                        continue;
                    }
                    if completed_modal_dialog_draw_proc {
                        // The callback completion check already proved that
                        // PC/SP are ModalDialog's exact resume boundary. Do
                        // not let an older deferred tracking address redirect
                        // the completed update pass away from that trap.
                        self.deferred_tracking_refire_pc = None;
                        continue;
                    }
                    if let Some(refire_pc) = self.deferred_tracking_refire_pc.take() {
                        self.m68k.cpu.write_reg(Register::PC, refire_pc);
                        continue;
                    }
                    if sound_work_only {
                        break;
                    }
                } else if trace_timer_enabled() {
                    eprintln!(
                        "[TIMER] pending {:?} pc=${:08X} sp=${:08X} waiting_for pc=${:08X} sp=${:08X}",
                        active_interrupt_callback.source,
                        pc,
                        sp,
                        active_interrupt_callback.resume_pc,
                        active_interrupt_callback.resume_sp
                    );
                }
            }

            if !sound_work_only && self.try_resume_proven_idle_cycle(tick_cap) {
                break;
            }

            if !sound_work_only
                && self.active_interrupt_callback.is_none()
                && self.dispatcher.has_ready_menu_tracking()
            {
                // Preserve the guest-time charge of a menu wait step after removing trap reentry.
                let wait_cost = 1 + hle_trap_extra_tick_cost(0xa93d);
                if self.frozen_ticks.is_none() && self.charge_tick_budget(wait_cost, tick_cap) {
                    break;
                }
                if self.active_interrupt_callback.is_some() {
                    continue;
                }
                self.dispatcher.yield_for_ui = yield_for_ui;
                if let Some(opcode) = self
                    .dispatcher
                    .resume_menu_tracking(&mut self.m68k.cpu, &mut self.bus)
                {
                    count += 1;
                    self.total_instructions = self.total_instructions.wrapping_add(1);
                    self.debug_note_m68k_executed_units(1);
                    if self.dispatcher.has_ready_menu_tracking() {
                        if yield_for_ui && self.frozen_ticks.is_none() {
                            self.frozen_ticks = Some(self.guest_tick());
                        }
                        let fired_hook = self.fire_menu_hook_proc(opcode);
                        if yield_for_ui && !fired_hook {
                            if finish_frame != FrameFinalization::Deferred {
                                self.finish_host_frame(
                                    finish_frame,
                                    audio_samples,
                                    sound_interrupt_dispatched,
                                );
                            }
                            return (count, true);
                        }
                    } else if self.process_context.menu_tracking().is_none() && self.frozen_ticks.is_some()
                    {
                        self.unfreeze_ticks_to(real_tick_cap);
                    }
                    continue;
                }
            }

            if pc == 0 {
                // App's RTS chain reached PC=0 — treat as clean exit.
                // Some apps (e.g. Centaurian 1.2.1) zero out our
                // exit-trampoline at \$100 during their CRT init then
                // pop past the saved A6 chain and JMP through a
                // popped-from-out-of-RAM zero. Real Mac OS would have
                // a launcher-provided return-to-Finder address; on
                // the HLE we just halt gracefully.
                if trace_load_enabled() {
                    eprintln!(
                        "[RUN_STEPS] App reached PC=0 (clean exit via deep RTS chain) at count={}",
                        count
                    );
                }
                self.dump_trace();
                self.halted = true;
                self.debug_note_terminal();
                self.halted_pc = Some(0);
                self.halted_sp = Some(self.m68k.cpu.read_reg(Register::A7));
                self.halted_d0 = Some(self.m68k.cpu.read_reg(Register::D0));
                return (count, false);
            }

            let translated_pc = self.bus.translate_guest_address(pc);
            if translated_pc < 0x60 || !self.bus.is_guest_address_mapped(pc, 2) {
                // Read opcode + sp on-demand in the error branch.
                let opcode = self.bus.read_word(pc);
                let sp = self.m68k.cpu.read_reg(Register::A7);
                eprintln!(
                    "[RUN_STEPS] Invalid PC ${:08X} at count={} sp=${:08X} op=${:04X}",
                    pc, count, sp, opcode
                );
                self.dump_invalid_pc_state();
                self.halted = true;
                self.debug_note_terminal();
                self.halted_pc = Some(pc);
                self.halted_sp = Some(sp);
                self.halted_d0 = Some(self.m68k.cpu.read_reg(Register::D0));
                self.dump_trace();
                return (count, false);
            }

            // Update debug counters for watchpoint tracking (debug builds only)
            #[cfg(debug_assertions)]
            if crate::memory::bus::watchpoint_armed() {
                crate::memory::bus::increment_step();
                crate::memory::bus::set_current_pc(pc);
                crate::memory::bus::set_watch_registers(
                    self.m68k.cpu.read_reg(Register::A0),
                    self.m68k.cpu.read_reg(Register::A1),
                    self.m68k.cpu.read_reg(Register::A6),
                    self.m68k.cpu.read_reg(Register::A7),
                );
            }

            // Mirror PC for release-mode memory/framebuffer traces;
            // watchpoint context above is debug-only.
            if crate::memory::bus::fb_write_trace_active()
                || crate::memory::bus::mem_read_trace_active()
                || crate::memory::bus::mem_write_trace_active()
            {
                crate::memory::bus::set_current_pc(pc);
            }

            let trace_pc_range_hit = trace_pc_range_contains(pc, self.guest_tick());
            if trace_pc_range_hit {
                let sp = self.m68k.cpu.read_reg(Register::A7);
                let a6 = self.m68k.cpu.read_reg(Register::A6);
                let stack0 = self.bus.read_long(sp);
                let stack4 = self.bus.read_long(sp.wrapping_add(4));
                let stack8 = self.bus.read_word(sp.wrapping_add(8));
                let frame_ret = self.bus.read_long(a6.wrapping_add(4));
                let frame_arg = self.bus.read_word(a6.wrapping_add(8));
                eprintln!(
                    "[TRACE-PC-RANGE] pc=${:08X} op=${:04X} next=${:08X} ccr=${:02X} d0=${:08X} d1=${:08X} d2=${:08X} d3=${:08X} d4=${:08X} d5=${:08X} d6=${:08X} d7=${:08X} a0=${:08X} a1=${:08X} a2=${:08X} a3=${:08X} a4=${:08X} a5=${:08X} a6=${:08X} sp=${:08X} stack0=${:08X} stack4=${:08X} stack8=${:04X} frame_ret=${:08X} frame_arg=${:04X}",
                    pc,
                    self.bus.read_word(pc),
                    self.bus.read_long(pc.wrapping_add(2)),
                    self.m68k.cpu.core.get_ccr(),
                    self.m68k.cpu.read_reg(Register::D0),
                    self.m68k.cpu.read_reg(Register::D1),
                    self.m68k.cpu.read_reg(Register::D2),
                    self.m68k.cpu.read_reg(Register::D3),
                    self.m68k.cpu.read_reg(Register::D4),
                    self.m68k.cpu.read_reg(Register::D5),
                    self.m68k.cpu.read_reg(Register::D6),
                    self.m68k.cpu.read_reg(Register::D7),
                    self.m68k.cpu.read_reg(Register::A0),
                    self.m68k.cpu.read_reg(Register::A1),
                    self.m68k.cpu.read_reg(Register::A2),
                    self.m68k.cpu.read_reg(Register::A3),
                    self.m68k.cpu.read_reg(Register::A4),
                    self.m68k.cpu.read_reg(Register::A5),
                    a6,
                    sp,
                    stack0,
                    stack4,
                    stack8,
                    frame_ret,
                    frame_arg,
                );
            }
            // Execute up to `batch_max` instructions inside the m68k core's
            // JIT-enabled batch loop. Control returns here on the first
            // A-line trap, STOP, watched PC, or when the budget runs out —
            // so the per-iteration bookkeeping above amortises across the
            // whole batch instead of running per instruction.
            //
            // Tick accounting: the old per-step loop charged the budget
            // BEFORE each instruction, so tick-boundary side effects (a
            // tick-cap break, timer/VBL callbacks that redirect PC) took
            // effect before the boundary instruction executed. To keep
            // those semantics, the boundary instruction is pre-charged
            // here, and each batch is clamped to stop short of the next
            // boundary; everything in between is charged in bulk after
            // the batch retires (pre/post order is indistinguishable away
            // from a boundary).
            let charging = !sound_work_only
                && !self.callback_suspends_guest_clock()
                && self.frozen_ticks.is_none();
            let mut precharged = false;
            if charging && self.tick_budget <= 1 {
                if self.charge_tick_budget(1, tick_cap) {
                    break;
                }
                precharged = true;
            }
            let batch_max = if per_instruction_diagnostics_active() {
                1
            } else {
                let mut n = (max_steps - count).min(BATCH_CHUNK);
                if charging && !self.callback_suspends_guest_clock() {
                    n = n.min((self.tick_budget - 1).max(1) as usize);
                }
                if let Some(units) = self.debug_step_units_remaining() {
                    n = n.min(units.max(1) as usize);
                }
                if self.debug_breakpoint_rearming() {
                    n = n.min(1);
                }
                n as u32
            };
            let entry_pc = self.m68k.cpu.read_reg(Register::PC);
            if self.debug_stop_at_m68k_breakpoint(entry_pc) {
                return (count, !self.halted);
            }
            // PC 0 is always watched: a deep RTS chain unwinding to 0 is
            // the clean-exit signal handled at the top of this loop, and it
            // must halt before low memory gets executed as code. While an
            // interrupt callback is active (including one fired by the
            // pre-charge above), its resume PC is watched so the resume
            // PC+SP check above fires at the exact boundary. Native trap
            // return PCs are watched for the same reason: ordinary RTS
            // returns can then be retired exactly without globally reducing
            // the batch size to one instruction.
            watch_buf.clear();
            watch_buf.push(0);
            if let Some(callback) = self.active_interrupt_callback {
                watch_buf.push(callback.resume_pc);
            }
            self.dispatcher
                .append_pending_native_trap_return_pcs(&mut watch_buf);
            watch_buf.extend(self.debug_m68k_breakpoint_addresses());
            let previous_mouse = self.bus.read_long(crate::memory::globals::addr::MOUSE_LOC2);
            let batch = self
                .m68k
                .cpu
                .run_batch(&mut self.bus, batch_max, &watch_buf);
            // Publish guest-written Mouse before dispatching an event/input
            // trap, even when the cursor task was called inside this batch.
            self.sync_guest_mouse_position(previous_mouse);
            self.dispatcher
                .retire_returned_native_trap_call(&mut self.m68k.cpu);
            // Trap exits consumed their opcode word too; count it like the
            // old per-step path did.
            let executed = batch.instructions as usize
                + usize::from(matches!(
                    batch.exit,
                    BatchExit::AlineTrap { .. } | BatchExit::FlineTrap { .. }
                ));
            self.debug_note_m68k_executed_units(executed);
            if executed > 0 {
                count += executed;
                self.total_instructions = self.total_instructions.wrapping_add(executed as u64);
                if !sound_work_only {
                    let charge_units = executed as i32 - i32::from(precharged);
                    if self.charge_tick_budget(charge_units, tick_cap) {
                        tick_cap_reached = true;
                    }
                }
                // Per-instruction histograms: any enabled tracer forces
                // batch_max == 1 above, so these still see every retired
                // instruction. A-line exits retire zero and keep going
                // through the trap histogram instead, as before.
                if batch.instructions > 0 {
                    if trace_opcode_counts_enabled() {
                        let opcode = self.m68k.cpu.core.ir as u16 as usize;
                        self.opcode_histogram[opcode] =
                            self.opcode_histogram[opcode].saturating_add(1);
                    }
                    if trace_hot_pc_enabled()
                        && self.total_instructions.is_multiple_of(PC_SAMPLE_INTERVAL)
                    {
                        *self.pc_histogram.entry(pc).or_insert(0) += 1;
                    }
                }
            }
            match batch.exit {
                BatchExit::BudgetExhausted => {}
                BatchExit::WatchedPc { pc } => {
                    if self.debug_stop_at_m68k_breakpoint(pc) {
                        return (count, !self.halted);
                    }
                }
                BatchExit::FlineTrap { .. } => {
                    // A writable vector 11 is the guest's architectural
                    // authority. Only the generated profile default retains
                    // the HLE's legacy no-op policy for unsupported F-line
                    // words; a replacement receives the real 68040 frame.
                    // Inside Macintosh Volume III (1985), p. III-17 names
                    // `$2C` as the Line 1111 emulator vector.
                    if !self.dispatcher.fline_vector_is_default(&self.bus) {
                        self.m68k.cpu.core.take_fline_exception(&mut self.bus);
                    }
                }
                BatchExit::Stopped => {
                    // STOP retired mid-batch. Halt silently, matching the
                    // old flow where the next loop iteration's is_stopped
                    // check caught it.
                    self.halted = true;
                    self.debug_note_terminal();
                    self.halted_pc = Some(self.m68k.cpu.read_reg(Register::PC));
                    self.halted_sp = Some(self.m68k.cpu.read_reg(Register::A7));
                    self.halted_d0 = Some(self.m68k.cpu.read_reg(Register::D0));
                    self.dump_trace();
                    return (count, false);
                }
                BatchExit::TrapInstruction { trap_num } => {
                    eprintln!("[CPU] TrapInstruction: #{}", trap_num);
                    self.halt_with_stop_diagnostics(count);
                    return (count, false);
                }
                BatchExit::Breakpoint { bp_num } => {
                    eprintln!("[CPU] Breakpoint: #{}", bp_num);
                    self.halt_with_stop_diagnostics(count);
                    return (count, false);
                }
                BatchExit::IllegalInstruction { opcode } => {
                    eprintln!(
                        "[CPU] IllegalInstruction: ${:04X} at PC=${:08X}",
                        opcode, self.m68k.cpu.core.pc
                    );
                    self.halt_with_stop_diagnostics(count);
                    return (count, false);
                }
                BatchExit::AlineTrap { opcode } => {
                    if self.m68k.complete_manager_return(&self.bus)
                        && (self.dispatcher.resume_completed_menu_bar_build(&mut self.m68k.cpu, &mut self.bus)
                            || self.dispatcher.resume_menu_tracking(&mut self.m68k.cpu, &mut self.bus).is_some())
                    {
                        continue;
                    }
                    // Accounting (count/ticks) happened via `executed`
                    // above. The batch may have retired instructions before
                    // the trap, so the loop-top `pc` is stale; the trap
                    // word's own address is in `ppc` (PC already advanced
                    // past it).
                    let pc = self.m68k.cpu.core.ppc;

                    // The processor enters the Trap Dispatcher through
                    // exception vector 10 at `$28`. If guest code replaced
                    // that process-scoped vector, build the architectural
                    // exception frame and execute its handler instead of
                    // making the write inert. Inside Macintosh Volume I
                    // (1985), p. I-89; Interapplication Communication
                    // (1993), p. 1-87.
                    if !self.dispatcher.aline_vector_is_default(&self.bus) {
                        self.m68k.cpu.core.take_aline_exception(&mut self.bus);
                        continue;
                    }

                    // An exact-cycle probe permits only journal-complete
                    // traps (see `idle_cycle_trap_is_journal_complete`);
                    // any other HLE trap may carry host-side state the
                    // guest CPU/RAM snapshot cannot see, so reject the
                    // proof before dispatch.
                    // Pre-dispatch the null-event and SystemTask outcomes
                    // are not yet known, so they pass here optimistically
                    // and are classified for real after dispatch.
                    if self.idle_cycle_probe.is_some()
                        && !idle_cycle_trap_is_journal_complete(
                            opcode,
                            true,
                            true,
                            self.m68k.cpu.read_reg(Register::D0),
                        )
                    {
                        if opcode == 0xAB1D {
                            ws_note_cancel_ab1d_selector(self.m68k.cpu.read_reg(Register::D0));
                        }
                        ws_note_cancel_trap(opcode);
                        self.note_idle_cycle_trap_cancel_site();
                        self.cancel_idle_cycle_detector();
                    }

                    self.dispatcher.yield_for_ui = yield_for_ui;
                    let dispatch_result = self.dispatch_classic_with_process_services(opcode);
                    match dispatch_result {
                        Ok(()) => {
                            let null_event = self.note_idle_cycle_trap_result(opcode);
                            // TickCount spin-loop acceleration is a post-dispatch
                            // control-flow optimization. The generated trap gateway,
                            // native-patch routing, canonical Toolbox operation, and
                            // accounting above always run before it can take effect.
                            if opcode == 0xA975
                                && !self.dispatcher.has_native_trap_patch(&self.bus, opcode)
                                && spin_wait_fastfwd_enabled_for(yield_for_ui, tick_cap)
                            {
                                let hit_cap = self.try_tickcount_spin_fastfwd(
                                    pc.wrapping_add(2),
                                    tick_cap,
                                    &mut count,
                                );
                                if hit_cap {
                                    break;
                                }
                            }
                            let extra_tick_cost = hle_trap_extra_tick_cost(opcode)
                                .saturating_add(self.dispatcher.take_hle_tick_cost());
                            if extra_tick_cost > 0
                                && self.charge_tick_budget(extra_tick_cost, tick_cap)
                            {
                                tick_cap_reached = true;
                            }
                            // The m68k CPU already advanced PC past the A-line
                            // instruction during fetch (read_imm_16 does pc += 2).
                            //
                            // For remaining tracking traps, rewind PC
                            // back to the A-line instruction so it re-fires on
                            // the next frame.
                            //
                            // Shared check with `dispatch.rs`'s auto-pop
                            // push-back logic — both call
                            // `TrapDispatcher::is_tracking_refire` so
                            // they can never diverge. Strips auto-pop
                            // bit so `$AD91`
                            // match too.
                            let is_tracking_refire =
                                self.dispatcher.is_tracking_refire(opcode);
                            if is_tracking_refire {
                                // An asynchronous callback may have been
                                // injected while this tracking trap was
                                // executing. Do not rewind PC over the
                                // callback's synthetic return frame: let the
                                // callback return normally, then re-fire the
                                // tracking trap from its original address.
                                if self.callback_suspends_guest_clock() {
                                    self.deferred_tracking_refire_pc = Some(pc);
                                    continue;
                                }
                                // A retained manager can redirect execution into an
                                // application callback before it refires. ModalDialog,
                                // for example, enters a custom CDEF after establishing
                                // its tracking state. Preserve that guest PC; replacing
                                // it with the A-line address would skip the callback and
                                // leave the dialog unable to consume queued input.
                                let post_dispatch_pc = self.m68k.cpu.read_reg(Register::PC);
                                if post_dispatch_pc != pc
                                    && post_dispatch_pc != pc.wrapping_add(2)
                                {
                                    continue;
                                }
                                // A retained TrackControl may have redirected
                                // execution into its guest action procedure.
                                // That callback returns directly to this trap;
                                // rewinding PC now would skip it entirely.
                                if self.dispatcher.is_control_action_callback_pending() {
                                    continue;
                                }
                                if self
                                    .dispatcher
                                    .is_menu_definition_callback_pending_with_tracking(
                                        self.process_context.menu_tracking(),
                                    )
                                {
                                    continue;
                                }
                                // In GUI mode, freeze ticks so the game clock doesn't
                                // advance while the host renders intermediate frames.
                                // In headless mode (scripted harnesses), let the budget
                                // advance ticks naturally — frozen_ticks would snap
                                // $016A on each re-fire, which consumes ticks at a
                                // different rate than real hardware where ModalDialog's
                                // WNE loop paces against the VBL.
                                if yield_for_ui
                                    && self.frozen_ticks.is_none()
                                    && tracking_refire_should_freeze_ticks(opcode)
                                {
                                    self.frozen_ticks = Some(self.guest_tick());
                                }
                                if yield_for_ui
                                    && self.frozen_ticks.is_some()
                                    && !tracking_refire_should_freeze_ticks(opcode)
                                {
                                    self.unfreeze_ticks_to(real_tick_cap);
                                }
                                self.m68k.cpu.write_reg(Register::PC, pc);

                                // Fire pending dialog userItem draw procs.
                                // The trampoline redirects PC to execute the
                                // 68K draw proc; when it RTS's, PC returns to
                                // the ModalDialog A-line for the next re-fire.
                                let uses_dialog_callbacks =
                                    tracking_refire_uses_dialog_callbacks(opcode);
                                let fired_draw_proc = if !uses_dialog_callbacks {
                                    false
                                } else {
                                    self.fire_dialog_draw_procs()
                                };
                                let mut fired_filter_proc = false;
                                if uses_dialog_callbacks && !fired_draw_proc {
                                    // Fire the filter proc for any dialog that has one,
                                    // once draw procs are complete. On a real Mac,
                                    // ModalDialog calls the filter for every event
                                    // (including null events) regardless of item types.
                                    // Inside Macintosh Volume I, I-415
                                    if self.should_fire_dialog_filter_proc() {
                                        self.fire_dialog_filter_proc();
                                        fired_filter_proc = true;
                                    }
                                }

                                // In realtime frontends, yield only once the
                                // tracking trap is idle. A just-scheduled
                                // dialog draw/filter proc has not run yet, so
                                // presenting here shows half-painted screens.
                                // Headless mode keeps executing as before.
                                if yield_for_ui
                                    && !fired_draw_proc
                                    && !fired_filter_proc
                                {
                                    if tracking_refire_advances_gui_idle_tick(opcode)
                                        && !self.service_gui_retained_idle_tick(tick_cap)
                                    {
                                        continue;
                                    }
                                    if finish_frame != FrameFinalization::Deferred {
                                        self.finish_host_frame(
                                            finish_frame,
                                            audio_samples,
                                            sound_interrupt_dispatched,
                                        );
                                    }
                                    return (count, true);
                                }
                            }
                            // If tracking just ended this trap (MenuSelect or
                            // ModalDialog completed), unfreeze ticks and snap
                            // $016A to wall-clock time so the game doesn't
                            // fast-forward through the pause gap.
                            if self.frozen_ticks.is_some() {
                                self.unfreeze_ticks_to(real_tick_cap);
                            }

                            if !is_tracking_refire && self.fire_modeless_dialog_draw_proc() {
                                continue;
                            }

                            // Service any pending Delay ticks immediately after
                            // the trap dispatch, before the next instruction.
                            self.service_delay_ticks(tick_cap);
                            if self.service_pending_launch_application(
                                event_manager_yield_trap(opcode),
                                false,
                            ) {
                                if self.halted {
                                    return (count, false);
                                }
                                continue;
                            }
                            if (null_event || is_poll_anchor_trap(opcode))
                                && spin_wait_fastfwd_enabled_for(yield_for_ui, tick_cap)
                                && self.try_exact_null_event_cycle_fastfwd(pc, tick_cap)
                            {
                                break;
                            }
                        }
                        Err(Error::Halted) => {
                            if matches!(opcode, 0xA9F2 | 0xA9F4)
                                && self.service_pending_launch_application(false, true)
                            {
                                if self.halted {
                                    return (count, false);
                                }
                                continue;
                            }
                            if matches!(opcode, 0xA9F2 | 0xA9F4) && self.service_installer_handoff()
                            {
                                continue;
                            }
                            // Surface the auto-pop caller PC if the
                            // halted trap was called via JSR through a
                            // trampoline. Without this, the halt log
                            // only shows the trampoline PC; the actual
                            // game-side caller is what investigators
                            // want to disassemble.
                            let caller_str = self
                                .dispatcher
                                .current_trap_caller
                                .map(|c| format!(" caller=${:08X}", c))
                                .unwrap_or_default();
                            eprintln!(
                                "[RUN_STEPS] Application halted at count={} pc=${:08X} trap=${:04X}{}",
                                count,
                                pc,
                                opcode,
                                caller_str,
                            );
                            self.halted = true;
                            self.halted_pc = Some(pc);
                            self.halted_trap = Some(opcode);
                            self.halted_sp = Some(self.m68k.cpu.read_reg(Register::A7));
                            self.halted_d0 = Some(self.m68k.cpu.read_reg(Register::D0));
                            self.dump_trace();
                            return (count, false);
                        }
                        Err(Error::UnimplementedTrap(t)) => {
                            // This is an internal registry/implementation gap,
                            // not the profile's callable `_Unimplemented`
                            // routine (which takes the documented fatal
                            // SysError 12 path). With no classified operation
                            // there is no valid stack, result, CCR, or memory
                            // effect to synthesize, so execution must stop at
                            // the faulting A-line instead of advancing into an
                            // undefined guest state.
                            eprintln!(
                                "[RUN_STEPS] Unimplemented trap ${:04X} at PC=${:08X} — halting",
                                t, pc
                            );
                            self.halted = true;
                            self.halted_pc = Some(pc);
                            self.halted_trap = Some(t);
                            self.halted_sp = Some(self.m68k.cpu.read_reg(Register::A7));
                            self.halted_d0 = Some(self.m68k.cpu.read_reg(Register::D0));
                            self.dump_trace();
                            return (count, false);
                        }
                        Err(e) => {
                            eprintln!(
                                "[RUN_STEPS] Error {:?} at PC=${:08X} trap=${:04X} count={}",
                                e, pc, opcode, count
                            );
                            self.halted = true;
                            self.halted_pc = Some(pc);
                            self.halted_sp = Some(self.m68k.cpu.read_reg(Register::A7));
                            self.halted_d0 = Some(self.m68k.cpu.read_reg(Register::D0));
                            self.dump_trace();
                            return (count, false);
                        }
                    }
                }
            }

            self.dispatcher.instruction_count = self.total_instructions;
            if self
                .dispatcher
                .guest_calls
                .execution_route(self.native.availability())
                != ExecutionRoute::Classic
            {
                break;
            }
        }

        self.cancel_idle_cycle_observation();
        if finish_frame != FrameFinalization::Deferred {
            self.finish_host_frame(
                finish_frame,
                audio_samples,
                sound_interrupt_dispatched || sound_work_only,
            );
        }

        (count, !self.halted)
    }

    fn run_ppc_steps(
        &mut self,
        max_steps: usize,
        tick_cap: Option<u32>,
        audio_samples: usize,
        coalesce_to_tick_cap: bool,
        finish_frame: FrameFinalization,
        foreground: bool,
    ) -> (usize, bool) {
        if max_steps == 0 || self.halted {
            return (0, !self.halted);
        }
        let ppc_max_steps = if coalesce_to_tick_cap {
            self.ppc_cycle_budget_for_tick_cap(max_steps, tick_cap)
        } else {
            max_steps
        };
        // Return control near each guest tick boundary so interrupt-time PPC
        // VBL and Time Manager callbacks run before the application begins
        // the following frame. Classic games such as Marathon depend on a
        // periodic timer callback to feed their simulation heartbeat.
        let cycles_to_next_tick = usize::try_from(self.tick_budget.max(1)).unwrap_or(1);
        let ppc_max_steps = ppc_max_steps.min(cycles_to_next_tick);
        if ppc_max_steps == 0 {
            if finish_frame != FrameFinalization::Deferred {
                self.finish_host_frame(finish_frame, audio_samples, false);
            }
            return (0, true);
        }

        self.dispatcher.instruction_count = self.total_instructions;
        let role = if foreground {
            NativeEngineRole::Application
        } else {
            NativeEngineRole::Companion
        };
        let Some(mut native_context) = self.native.take(role) else {
            return (0, !self.halted);
        };
        let mut ppc_app = native_context.adapter_mut();

        ppc_app.set_ui_theme(self.config.ui_theme);
        if !ppc_app.toolbox_startup.execution.calls().prepare_native_task(&mut ppc_app.cpu) {
            return (0, !self.halted);
        }
        ppc_app.toolbox_startup.host_menu_bar_hidden = self.dispatcher.menu_bar_hidden;
        self.prepare_ppc_execution_clock(&mut ppc_app);
        let cycles_per_tick = self.instructions_per_tick.max(1);
        let remaining_cycles =
            (i64::from(self.tick_budget).clamp(0, i64::from(cycles_per_tick))) as u32;
        ppc_app.set_clock_cycle_timing(
            cycles_per_tick,
            cycles_per_tick.saturating_sub(remaining_cycles),
        );
        if ppc_app.toolbox_startup.execution.calls().pending_powerpc_from_m68k().is_some()
            && ppc_app
                .activate_powerpc_from_m68k(&mut self.m68k.cpu)
                .is_none()
        {
            self.halted = true;
            self.halted_pc = Some(self.m68k.cpu.read_reg(Register::PC));
            self.halted_sp = Some(self.m68k.cpu.read_reg(Register::A7));
            self.halted_d0 = Some(self.m68k.cpu.read_reg(Register::D0));
            self.native
                .restore(native_context)
                .unwrap_or_else(|_| panic!("native context lost its owner"));
            return (0, false);
        }
        if ppc_app.toolbox_startup.execution.calls().has_m68k_execution() {
            let ppc_task = ppc_app.toolbox_startup.execution.calls().current_task();
            let ppc_start_time = self.bus.read_long(crate::memory::globals::addr::TIME);
            let (mixed_steps, running) = self
                .run_pending_m68k_guest_call(&mut ppc_app, ppc_max_steps)
                .unwrap_or((0, false));
            let mixed_cycles = mixed_steps as u64;
            self.total_instructions = self.total_instructions.saturating_add(mixed_cycles);
            self.dispatcher.instruction_count = self.total_instructions;
            let tick_advance = self.advance_ticks_for_ppc_cycles(mixed_cycles, tick_cap);
            // A classic callback can yield to another cooperative task while
            // the native caller is parked. Do not deliver the native
            // application's VBL/Time Manager work against that successor's
            // task; the callback phase is retried once its owner is selected.
            let native_callbacks_allowed = ppc_app.toolbox_startup.execution.calls().current_task_is_running()
                && ppc_app.toolbox_startup.execution.calls().current_task() == ppc_task
                && ExecutionTaskId::from_thread_id(
                    self.dispatcher.guest_calls.current_task().thread_id(),
                ) == ppc_task;
            let (vbl_probes, timer_probes) = if native_callbacks_allowed {
                self.process_context
                    .with_memory_and_cfm(|memory_manager, cfm| {
                        Self::fire_ppc_tick_callbacks(
                            &mut ppc_app,
                            memory_manager,
                            cfm,
                            tick_advance.baseline_tick,
                            ppc_start_time,
                            tick_advance.elapsed_ticks,
                            u64::from(cycles_per_tick),
                            false,
                            false,
                        )
                    })
            } else {
                (Vec::new(), Vec::new())
            };
            let vbl_cycles = vbl_probes
                .iter()
                .map(|probe| probe.invocation.cycles)
                .sum::<u64>();
            let timer_cycles = timer_probes
                .iter()
                .map(|probe| probe.invocation.cycles)
                .sum::<u64>();
            self.total_instructions = self
                .total_instructions
                .saturating_add(vbl_cycles)
                .saturating_add(timer_cycles);
            self.dispatcher.instruction_count = self.total_instructions;
            if !running {
                self.halted = true;
                self.halted_pc = Some(self.m68k.cpu.read_reg(Register::PC));
                self.halted_sp = Some(self.m68k.cpu.read_reg(Register::A7));
                self.halted_d0 = Some(self.m68k.cpu.read_reg(Register::D0));
            }
            self.native
                .restore(native_context)
                .unwrap_or_else(|_| panic!("native context lost its owner"));
            if finish_frame != FrameFinalization::Deferred {
                self.sync_ppc_deferred_host_state();
                self.finish_host_frame(finish_frame, audio_samples, false);
            }
            return (
                usize::try_from(
                    mixed_cycles
                        .saturating_add(vbl_cycles)
                        .saturating_add(timer_cycles),
                )
                .unwrap_or(usize::MAX),
                running,
            );
        }
        let ppc_task = ppc_app.toolbox_startup.execution.calls().current_task();
        let ppc_start_time = self.bus.read_long(crate::memory::globals::addr::TIME);
        let profile_ppc = ppc_profile_enabled();
        let profile_total_start = profile_ppc.then(Instant::now);
        let record_ppc_imports = self.dispatcher.is_trace_recording();
        let trace_ppc_import_hist = ppc_import_hist_enabled();
        let trace_ppc_imports = record_ppc_imports || trace_ppc_import_hist;
        let trace_ppc_fetches = trace_ppc_fetch_counts_enabled();
        let profile_run_start = profile_ppc.then(Instant::now);
        let probe = self
            .process_context
            .with_memory_and_cfm(|memory_manager, cfm| {
                ppc_app.run_with_process_services(
                    ppc_max_steps as u64,
                    trace_ppc_imports,
                    trace_ppc_fetches,
                    memory_manager,
                    cfm,
                )
            });
        let profile_run_us = elapsed_profile_micros(profile_run_start);
        let ppc_cycles = ppc_run_result_cycles(probe.result);
        let resumed_m68k = self.resume_m68k_after_powerpc(&mut ppc_app);
        let mixed_mode_budget =
            ppc_max_steps.saturating_sub(usize::try_from(ppc_cycles).unwrap_or(usize::MAX));
        let mixed_mode = if resumed_m68k {
            self.run_pending_m68k_guest_call(&mut ppc_app, mixed_mode_budget)
                .or(Some((0, true)))
        } else {
            self.run_pending_m68k_guest_call(&mut ppc_app, mixed_mode_budget)
        };
        let mixed_mode_cycles = mixed_mode.map_or(0, |(cycles, _)| cycles as u64);
        let cycles = ppc_cycles.saturating_add(mixed_mode_cycles);
        let pc = ppc_app.cpu.pc;
        let sp = ppc_app.cpu.gpr[1];
        let gpr3 = ppc_app.cpu.gpr[3];
        let unsupported_import_index = probe.unsupported_import_index;
        let exited_via_ppc_exit_to_shell = ppc_halted_by_exit_to_shell(
            &ppc_app.imports,
            probe.result,
            probe.last_import_index,
            unsupported_import_index,
        );
        let still_running = mixed_mode.map_or_else(
            || {
                matches!(probe.result, PpcRunResult::CycleLimit { .. })
                    && unsupported_import_index.is_none()
            },
            |(_, running)| running,
        );

        self.total_instructions = self.total_instructions.saturating_add(cycles);
        self.dispatcher.instruction_count = self.total_instructions;
        let (vbl_probes, timer_probes) = if exited_via_ppc_exit_to_shell {
            (Vec::new(), Vec::new())
        } else {
            let tick_advance = self.advance_ticks_for_ppc_cycles(cycles, tick_cap);
            let native_callbacks_allowed = ppc_app.toolbox_startup.execution.calls().current_task_is_running()
                && ppc_app.toolbox_startup.execution.calls().current_task() == ppc_task
                && ExecutionTaskId::from_thread_id(
                    self.dispatcher.guest_calls.current_task().thread_id(),
                ) == ppc_task;
            if native_callbacks_allowed {
                self.process_context
                    .with_memory_and_cfm(|memory_manager, cfm| {
                        Self::fire_ppc_tick_callbacks(
                            &mut ppc_app,
                            memory_manager,
                            cfm,
                            tick_advance.baseline_tick,
                            ppc_start_time,
                            tick_advance.elapsed_ticks,
                            u64::from(cycles_per_tick),
                            trace_ppc_imports,
                            trace_ppc_fetches,
                        )
                    })
            } else {
                (Vec::new(), Vec::new())
            }
        };
        if trace_vbl_enabled() {
            for vbl_probe in &vbl_probes {
                eprintln!(
                    "[VBL-PPC] task=${:08X} callback=${:08X} entry=${:08X} tick={} cycles={} end_r3=${:08X} result={:?}",
                    vbl_probe.invocation.task_ptr,
                    vbl_probe.invocation.callback,
                    vbl_probe.invocation.callback_entry,
                    vbl_probe.invocation.tick,
                    vbl_probe.invocation.cycles,
                    vbl_probe.invocation.end_r3,
                    vbl_probe.invocation.result,
                );
            }
        }
        if trace_timer_enabled() {
            for timer_probe in &timer_probes {
                eprintln!(
                    "[TIMER-PPC] task=${:08X} callback=${:08X} entry=${:08X} tick={} cycles={} end_r3=${:08X} result={:?}",
                    timer_probe.invocation.task_ptr,
                    timer_probe.invocation.callback,
                    timer_probe.invocation.callback_entry,
                    timer_probe.invocation.tick,
                    timer_probe.invocation.cycles,
                    timer_probe.invocation.end_r3,
                    timer_probe.invocation.result,
                );
            }
        }
        let vbl_cycles = vbl_probes
            .iter()
            .map(|probe| probe.invocation.cycles)
            .sum::<u64>();
        let timer_cycles = timer_probes
            .iter()
            .map(|probe| probe.invocation.cycles)
            .sum::<u64>();
        self.prepare_ppc_execution_clock(&mut ppc_app);
        self.total_instructions = self
            .total_instructions
            .saturating_add(vbl_cycles)
            .saturating_add(timer_cycles);
        self.dispatcher.instruction_count = self.total_instructions;
        if record_ppc_imports {
            self.record_ppc_import_trace(&probe.import_trace);
            for vbl_probe in &vbl_probes {
                self.record_ppc_import_trace(&vbl_probe.import_trace);
            }
            for timer_probe in &timer_probes {
                self.record_ppc_import_trace(&timer_probe.import_trace);
            }
            self.record_draw_sprocket_trace(&probe.draw_sprocket_trace);
            self.record_input_sprocket_trace(&probe.input_sprocket_trace);
        }
        if trace_ppc_import_hist {
            self.record_ppc_import_histogram(&probe.import_trace);
            for vbl_probe in &vbl_probes {
                self.record_ppc_import_histogram(&vbl_probe.import_trace);
            }
            for timer_probe in &timer_probes {
                self.record_ppc_import_histogram(&timer_probe.import_trace);
            }
        }
        if let Some(histogram) = probe.fetch_histogram.as_ref() {
            self.ppc_fetch_histogram.merge_from(histogram);
        }
        for vbl_probe in &vbl_probes {
            if let Some(histogram) = vbl_probe.fetch_histogram.as_ref() {
                self.ppc_fetch_histogram.merge_from(histogram);
            }
        }
        for timer_probe in &timer_probes {
            if let Some(histogram) = timer_probe.fetch_histogram.as_ref() {
                self.ppc_fetch_histogram.merge_from(histogram);
            }
        }
        let q3_frame_start = self.q3_completed_frame_index;
        let profile_render_start = profile_ppc.then(Instant::now);
        let render_stats = if foreground && finish_frame != FrameFinalization::Deferred {
            self.render_ppc_completed_frames(&mut ppc_app, pc)
        } else {
            PpcQ3SoftwareRenderStats::default()
        };
        let profile_render_us = elapsed_profile_micros(profile_render_start);
        let next_q3_frame_index = self.q3_completed_frame_index;
        let profile_sync_start = profile_ppc.then(Instant::now);
        if foreground && finish_frame != FrameFinalization::Deferred {
            self.sync_ppc_front_buffer_to_host(&mut ppc_app);
            self.persist_ppc_vfs_to_host(&mut ppc_app);
        }
        self.service_ppc_sound_adapter(&mut ppc_app);
        let profile_sync_us = elapsed_profile_micros(profile_sync_start);

        if profile_ppc {
            eprintln!(
                "{}",
                format_ppc_profile_sample(PpcProfileSample {
                    max_steps,
                    cycles,
                    total_instructions: self.total_instructions,
                    tick: self.guest_tick(),
                    pc,
                    lr: ppc_app.cpu.lr,
                    current_gworld: *ppc_app.current_gworld,
                    screen_events: self.dispatcher.screen_event_count,
                    handled_import_count: probe.handled_import_count,
                    last_import_index: probe.last_import_index,
                    unsupported_import_index,
                    q3_frames: render_stats.frames,
                    q3_frame_start,
                    q3_frame_end: next_q3_frame_index,
                    q3_commands: render_stats.commands,
                    q3_vertices: render_stats.vertices,
                    q3_triangles: render_stats.triangles,
                    q3_pixels: render_stats.pixels,
                    dsp_front_gworld: ppc_app.draw_sprocket.front_buffer_gworld,
                    dsp_back_gworld: ppc_app.draw_sprocket.back_buffer_gworld,
                    run_us: profile_run_us,
                    render_us: profile_render_us,
                    sync_us: profile_sync_us,
                    total_us: elapsed_profile_micros(profile_total_start),
                })
            );
        }

        if ppc_unimpl_hist_enabled() && !still_running {
            self.record_ppc_unimpl_histogram(
                &ppc_app.imports,
                probe.result,
                unsupported_import_index,
            );
        }

        if !still_running {
            self.halted = true;
            if exited_via_ppc_exit_to_shell {
                self.halted_trap = Some(0xA9F4);
            }
            self.halted_pc = Some(pc);
            self.halted_sp = Some(sp);
            self.halted_d0 = Some(gpr3);
            if trace_load_enabled() {
                let word = ppc_app.memory.read_u32_be(pc);
                let word_text = word
                    .map(|word| format!(" word=${word:08X}"))
                    .unwrap_or_default();
                let lr = ppc_app.cpu.lr;
                let gpr4 = ppc_app.cpu.gpr[4];
                let gpr29 = ppc_app.cpu.gpr[29];
                let gpr30 = ppc_app.cpu.gpr[30];
                let gpr31 = ppc_app.cpu.gpr[31];
                let node_text = if gpr30 != 0 {
                    let prev = ppc_app.memory.read_u32_be(gpr30).unwrap_or(0);
                    let next = ppc_app
                        .memory
                        .read_u32_be(gpr30.wrapping_add(4))
                        .unwrap_or(0);
                    format!(" r30[0..8]=${prev:08X}/${next:08X}")
                } else {
                    String::new()
                };
                let global_text = {
                    let g29 = ppc_app.memory.read_u32_be(gpr29).unwrap_or(0);
                    let g31 = ppc_app.memory.read_u32_be(gpr31).unwrap_or(0);
                    let g31_8 = ppc_app
                        .memory
                        .read_u32_be(gpr31.wrapping_add(8))
                        .unwrap_or(0);
                    format!(" [r29]=${g29:08X} [r31]=${g31:08X} [r31+8]=${g31_8:08X}")
                };
                if let Some(index) = unsupported_import_index {
                    eprintln!(
                        "[PPC] halted at unsupported import #{} pc=${:08X} sp=${:08X} lr=${:08X} r3=${:08X} r4=${:08X} r29=${:08X} r30=${:08X} r31=${:08X}{}{}{}",
                        index, pc, sp, lr, gpr3, gpr4, gpr29, gpr30, gpr31, word_text, node_text, global_text
                    );
                } else {
                    eprintln!(
                        "[PPC] halted after {:?} pc=${:08X} sp=${:08X} lr=${:08X} r3=${:08X} r4=${:08X} r29=${:08X} r30=${:08X} r31=${:08X}{}{}{}",
                        probe.result, pc, sp, lr, gpr3, gpr4, gpr29, gpr30, gpr31, word_text, node_text, global_text
                    );
                }
            }
        }

        self.native
            .restore(native_context)
            .unwrap_or_else(|_| panic!("native context lost its owner"));
        if foreground && exited_via_ppc_exit_to_shell {
            if finish_frame == FrameFinalization::Complete {
                self.redraw_chrome_outside_idle_journal();
            }
        } else {
            if finish_frame != FrameFinalization::Deferred {
                self.finish_host_frame(finish_frame, audio_samples, false);
            }
        }
        (
            usize::try_from(
                cycles
                    .saturating_add(vbl_cycles)
                    .saturating_add(timer_cycles),
            )
            .unwrap_or(usize::MAX),
            still_running,
        )
    }

    /// Run the top parked native-to-68k call without recursively borrowing
    /// either CPU adapter. A-lines return to the same process dispatcher; the
    /// exact switch-frame return PC and restored 68k stack close the call and
    /// resume the PowerPC continuation.
    fn run_pending_m68k_guest_call(
        &mut self,
        ppc_app: &mut PpcLoadedApp,
        max_steps: usize,
    ) -> Option<(usize, bool)> {
        self.m68k.apply_task_handoff();
        if !ppc_app.toolbox_startup.execution.calls().has_m68k_execution() {
            return None;
        }
        if !ppc_app.toolbox_startup.execution.calls().prepare_native_task(&mut ppc_app.cpu) {
            return Some((0, true));
        }
        let task = ppc_app.toolbox_startup.execution.calls().current_task();
        let pending = self.m68k.activate_pending(&mut ppc_app.cpu)?;

        // Thread Manager calls are allowed to yield while guest callback code
        // is running. Once that happens, `self.m68k.cpu` belongs to the successor
        // task and the continuation captured above must remain suspended until
        // its owner is scheduled again. Continuing here would execute the old
        // callback against the new task's registers and stack.
        if ppc_app.toolbox_startup.execution.calls().current_task() != task
            || !ppc_app.toolbox_startup.execution.calls().current_task_is_running()
        {
            return Some((0, true));
        }

        if !self.dispatcher.has_ready_menu_tracking()
            && self.m68k.cpu.read_reg(Register::PC) == pending.return_pc
            && self.m68k.cpu.read_reg(Register::A7) == pending.final_sp
        {
            let completed = self.process_context.with_memory_and_cfm(|manager, _| {
                self.m68k
                    .complete_pending(&mut ppc_app.memory, &mut ppc_app.cpu, pending, manager)
            });
            return Some((0, completed));
        }
        if max_steps == 0 {
            return Some((0, true));
        }

        let mut executed = 0usize;
        let mut running = true;
        let mut watch_buf = Vec::with_capacity(4);
        while executed < max_steps {
            if self.debug_finish_step_if_ready() {
                return Some((executed, true));
            }
            if self.dispatcher.resume_menu_tracking(&mut self.m68k.cpu, &mut self.bus).is_some() {
                executed += 1;
                self.debug_note_m68k_executed_units(1);
                continue;
            }
            if self.m68k.cpu.read_reg(Register::PC) == pending.return_pc {
                running = self.process_context.with_memory_and_cfm(|manager, _| {
                    self.m68k.complete_pending(
                        &mut ppc_app.memory,
                        &mut ppc_app.cpu,
                        pending,
                        manager,
                    )
                });
                break;
            }
            let mut batch_max = u32::try_from(max_steps - executed).unwrap_or(u32::MAX);
            if let Some(units) = self.debug_step_units_remaining() {
                batch_max = batch_max.min(units.max(1));
            }
            if self.debug_breakpoint_rearming() {
                batch_max = batch_max.min(1);
            }
            let entry_pc = self.m68k.cpu.read_reg(Register::PC);
            if self.debug_stop_at_m68k_breakpoint(entry_pc) {
                return Some((executed, true));
            }
            watch_buf.clear();
            watch_buf.push(pending.return_pc);
            self.dispatcher
                .append_pending_native_trap_return_pcs(&mut watch_buf);
            watch_buf.extend(self.debug_m68k_breakpoint_addresses());
            let batch = self.m68k.cpu.run_batch(&mut self.bus, batch_max, &watch_buf);
            self.dispatcher
                .retire_returned_native_trap_call(&mut self.m68k.cpu);
            let retired = batch.instructions as usize
                + usize::from(matches!(
                    batch.exit,
                    BatchExit::AlineTrap { .. } | BatchExit::FlineTrap { .. }
                ));
            executed = executed.saturating_add(retired);
            self.debug_note_m68k_executed_units(retired);
            match batch.exit {
                BatchExit::BudgetExhausted => break,
                BatchExit::WatchedPc { pc } => {
                    // User breakpoints take precedence over an internal return
                    // sentinel at the same address.
                    if self.debug_stop_at_m68k_breakpoint(pc) {
                        return Some((executed, true));
                    }
                    if pc == pending.return_pc {
                        running = self.process_context.with_memory_and_cfm(|manager, _| {
                            self.m68k.complete_pending(
                                &mut ppc_app.memory,
                                &mut ppc_app.cpu,
                                pending,
                                manager,
                            )
                        });
                        break;
                    }
                }
                BatchExit::AlineTrap { opcode } => {
                    if self.m68k.complete_manager_return(&self.bus)
                        && (self.dispatcher.resume_completed_menu_bar_build(&mut self.m68k.cpu, &mut self.bus)
                            || self.dispatcher.resume_menu_tracking(&mut self.m68k.cpu, &mut self.bus).is_some())
                    {
                        continue;
                    }
                    if !self.dispatcher.aline_vector_is_default(&self.bus) {
                        self.m68k.cpu.core.take_aline_exception(&mut self.bus);
                        continue;
                    }
                    let dispatch_err = {
                        let mut bindings = ppc_app.cfm_symbol_bindings();
                        self.dispatcher
                            .dispatch_with_process_services(
                                opcode,
                                &mut self.m68k.cpu,
                                &mut self.bus,
                                self.process_context.cfm(),
                                Some(&mut bindings),
                            )
                            .is_err()
                    };
                    if dispatch_err {
                        running = false;
                        break;
                    }
                    if ppc_app.toolbox_startup.execution.calls().current_task() != task
                        || !ppc_app.toolbox_startup.execution.calls().current_task_is_running()
                    {
                        // The trap switched cooperative tasks. Preserve the active
                        // frame and let the outer scheduler run the newly selected
                        // task; only this task may resume the captured continuation.
                        return Some((executed, true));
                    }
                    if ppc_app.toolbox_startup.execution.calls().has_powerpc_from_m68k() {
                        break;
                    }
                }
                BatchExit::FlineTrap { .. } => {
                    if !self.dispatcher.fline_vector_is_default(&self.bus) {
                        self.m68k.cpu.core.take_fline_exception(&mut self.bus);
                    }
                }
                BatchExit::Stopped
                | BatchExit::TrapInstruction { .. }
                | BatchExit::Breakpoint { .. }
                | BatchExit::IllegalInstruction { .. } => {
                    running = false;
                    break;
                }
            }
        }
        Some((executed, running))
    }

    fn resume_m68k_after_powerpc(&mut self, ppc_app: &mut PpcLoadedApp) -> bool {
        if !self.m68k.resume_after_powerpc(&mut ppc_app.memory) {
            return false;
        }
        // A native RoutineDescriptor can be the head of a raw A-line patch.
        // Mixed Mode resumes at the synthesized JSR continuation directly,
        // so retire that Trap Manager frame before the next 68k instruction.
        self.dispatcher
            .retire_returned_native_trap_call(&mut self.m68k.cpu);
        true
    }

    fn prepare_ppc_execution_clock(&mut self, _ppc_app: &mut PpcLoadedApp) {
        // `$016A` is process memory shared by both adapters. A native slice
        // may have ended after PPC guest code wrote that long directly. Read
        // the shared bytes before the next PPC instruction executes so the
        // host pacing snapshot follows the guest-owned value; never project
        // a scalar back into guest memory here.
        self.dispatcher.read_tick_count(&self.bus);
    }

    #[allow(clippy::too_many_arguments)]
    fn fire_ppc_tick_callbacks(
        ppc_app: &mut PpcLoadedApp,
        memory_manager: &mut ProcessMemoryManager,
        cfm: &mut crate::cfm::CfmState,
        baseline_tick: u32,
        start_time: u32,
        elapsed_ticks: u32,
        max_cycles: u64,
        trace_imports: bool,
        trace_fetches: bool,
    ) -> (
        Vec<crate::loader::ppc::PpcVblCallbackProbe>,
        Vec<crate::loader::ppc::PpcTimerCallbackProbe>,
    ) {
        use crate::memory::globals::addr;

        let mut vbl_probes = Vec::new();
        let mut timer_probes = Vec::new();
        let mut callback_time = start_time;
        for tick_offset in 0..elapsed_ticks {
            let callback_tick = ppc_app
                .publish_host_epoch_tick(baseline_tick.wrapping_add(tick_offset).wrapping_add(1));
            if callback_tick.is_multiple_of(60) {
                callback_time = callback_time.wrapping_add(1);
            }
            let _ = ppc_app.memory.write_u32_be(addr::TICKS, callback_tick);
            let _ = ppc_app.memory.write_u32_be(addr::TIME, callback_time);
            vbl_probes.extend(ppc_app.fire_vbl_tasks_for_ticks_with_process_services(
                callback_tick.wrapping_sub(1),
                1,
                usize::MAX,
                max_cycles,
                trace_imports,
                trace_fetches,
                memory_manager,
                cfm,
            ));
            timer_probes.extend(ppc_app.fire_timer_tasks_for_ticks_with_process_services(
                callback_tick.wrapping_sub(1),
                1,
                usize::MAX,
                max_cycles,
                trace_imports,
                trace_fetches,
                memory_manager,
                cfm,
            ));
        }
        (vbl_probes, timer_probes)
    }

    fn render_ppc_completed_frames(
        &mut self,
        ppc_app: &mut PpcLoadedApp,
        pc: u32,
    ) -> PpcQ3SoftwareRenderStats {
        if self.external_q3_renderer_enabled {
            if let Some(frame) = ppc_app.take_completed_q3_gpu_frame() {
                self.pending_q3_gpu_frame = Some(frame);
                self.q3_completed_frame_index = self.q3_completed_frame_index.saturating_add(1);
                return PpcQ3SoftwareRenderStats::default();
            }
        }
        let qd3d_dump_frame = qd3d_dump_frame_index();
        let q3_frame_start = self.q3_completed_frame_index;
        let mut next_q3_frame_index = self.q3_completed_frame_index;
        let render_stats = if qd3d_dump_frame.is_some() {
            ppc_app.render_completed_q3_frames_to_front_buffer_with_observer(|_, replay| {
                if qd3d_dump_frame == Some(next_q3_frame_index) {
                    eprint!("{}", format_qd3d_frame_dump(next_q3_frame_index, replay));
                    if let Some(output_dir) = qd3d_dump_replay_dir() {
                        match write_qd3d_frame_replay_dump(output_dir, next_q3_frame_index, replay)
                        {
                            Ok(path) => eprintln!(
                                "[QD3D-FRAME] frame={} replay_json={}",
                                next_q3_frame_index,
                                path.display()
                            ),
                            Err(err) => eprintln!(
                                "[QD3D-FRAME] frame={} replay_json_error={}",
                                next_q3_frame_index, err
                            ),
                        }
                    }
                }
                next_q3_frame_index = next_q3_frame_index.saturating_add(1);
            })
        } else {
            let stats = ppc_app.render_completed_q3_frames_to_front_buffer_fast();
            next_q3_frame_index = next_q3_frame_index.saturating_add(stats.frames);
            stats
        };
        self.q3_completed_frame_index = next_q3_frame_index;
        self.record_q3_frame_trace(pc, q3_frame_start, next_q3_frame_index, render_stats);
        render_stats
    }

    fn sync_ppc_deferred_host_state(&mut self) {
        let Some(mut native_context) = self.native.take(NativeEngineRole::Application) else {
            self.dispatcher.external_host_overlay_rects.clear();
            return;
        };
        let mut ppc_app = native_context.adapter_mut();
        let pc = ppc_app.cpu.pc;
        self.render_ppc_completed_frames(&mut ppc_app, pc);
        self.sync_ppc_front_buffer_to_host(&mut ppc_app);
        self.persist_ppc_vfs_to_host(&mut ppc_app);
        let host_overlay_rects = ppc_app.toolbox_startup.retained_host_overlay_rects();
        self.native
            .restore(native_context)
            .unwrap_or_else(|_| panic!("native context lost its owner"));
        self.dispatcher.external_host_overlay_rects = host_overlay_rects;
    }

    fn record_ppc_import_trace(&mut self, trace: &[PpcHleImportTraceEntry]) {
        let runs = ppc_import_trace_runs(trace);
        let mut index = 0usize;
        while index < runs.len() {
            if let Some((block_len, sequence_repeat_count)) =
                ppc_import_trace_repeated_block_len(&runs, index)
            {
                let (entry, _) = runs[index];
                let sequence = runs[index..index + block_len]
                    .iter()
                    .map(|(entry, repeat_count)| {
                        serde_json::json!({
                            "library": entry.library_name,
                            "symbol": entry.symbol_name,
                            "repeat_count": repeat_count,
                        })
                    })
                    .collect::<Vec<_>>();
                let fields = TrapDispatcher::trace_field_map(&[
                    ("import_index", entry.import_index.to_string()),
                    ("library", entry.library_name.clone()),
                    ("symbol", entry.symbol_name.clone()),
                    ("lr", format!("{:08X}", entry.lr)),
                    ("rtoc", format!("{:08X}", entry.rtoc)),
                    ("sp", format!("{:08X}", entry.sp)),
                    (
                        "dispatcher_target",
                        format!("{:?}", entry.dispatcher_target),
                    ),
                    ("repeat_count", sequence_repeat_count.to_string()),
                    ("sequence", serde_json::Value::Array(sequence).to_string()),
                ]);
                if let Err(err) = self.dispatcher.record_trace_event(
                    &self.bus,
                    entry.pc,
                    "ppc_import",
                    fields,
                    false,
                ) {
                    eprintln!("[ORACLE] failed to record PPC import event: {:?}", err);
                }
                index += block_len
                    .saturating_mul(usize::try_from(sequence_repeat_count).unwrap_or(usize::MAX));
                continue;
            }
            let (entry, repeat_count) = runs[index];
            let fields = TrapDispatcher::trace_field_map(&[
                ("import_index", entry.import_index.to_string()),
                ("library", entry.library_name.clone()),
                ("symbol", entry.symbol_name.clone()),
                ("lr", format!("{:08X}", entry.lr)),
                ("rtoc", format!("{:08X}", entry.rtoc)),
                ("sp", format!("{:08X}", entry.sp)),
                (
                    "dispatcher_target",
                    format!("{:?}", entry.dispatcher_target),
                ),
                ("repeat_count", repeat_count.to_string()),
            ]);
            if let Err(err) =
                self.dispatcher
                    .record_trace_event(&self.bus, entry.pc, "ppc_import", fields, false)
            {
                eprintln!("[ORACLE] failed to record PPC import event: {:?}", err);
            }
            index += 1;
        }
    }

    fn record_input_sprocket_trace(&mut self, trace: &[PpcInputSprocketSimpleStateTraceEntry]) {
        for entry in trace {
            let fields = TrapDispatcher::trace_field_map(&[
                ("import_index", entry.import_index.to_string()),
                ("element", format!("{:08X}", entry.element)),
                ("state_ptr", format!("{:08X}", entry.state_ptr)),
                ("state", format!("{:08X}", entry.state)),
                ("kind", format!("{:08X}", entry.kind)),
                ("kind_name", entry.kind_name.clone()),
                ("fallback_state", format!("{:08X}", entry.fallback_state)),
                ("need_name", entry.need_name.clone()),
                ("action_binding", entry.action_binding.clone()),
                ("key_map", format_input_key_map(&entry.input.key_map)),
                ("mouse_button", entry.input.mouse_button.to_string()),
                ("mouse_v", entry.input.mouse_v.to_string()),
                ("mouse_h", entry.input.mouse_h.to_string()),
                ("initialized", entry.input_sprocket.initialized.to_string()),
                ("suspended", entry.input_sprocket.suspended.to_string()),
                (
                    "keyboard_active",
                    entry.input_sprocket.keyboard_active.to_string(),
                ),
                (
                    "mouse_active",
                    entry.input_sprocket.mouse_active.to_string(),
                ),
            ]);
            if let Err(err) = self.dispatcher.record_trace_event(
                &self.bus,
                entry.pc,
                "input_sprocket",
                fields,
                false,
            ) {
                eprintln!(
                    "[ORACLE] failed to record InputSprocket simple-state event: {:?}",
                    err
                );
            }
        }
    }

    fn record_draw_sprocket_trace(&mut self, trace: &[PpcDrawSprocketTraceEntry]) {
        for entry in trace {
            let fields = TrapDispatcher::trace_field_map(&[
                ("import_index", entry.import_index.to_string()),
                ("action", entry.action.clone()),
                ("result", entry.result.to_string()),
                ("result_hex", format!("{:04X}", entry.result as u16)),
                ("context", format_oracle_optional_hex32(entry.context)),
                (
                    "requested_state",
                    entry
                        .requested_state
                        .clone()
                        .unwrap_or_else(|| "none".to_string()),
                ),
                (
                    "requested_frequency",
                    format_oracle_optional_u32(entry.requested_frequency),
                ),
                (
                    "requested_width",
                    format_oracle_optional_u32(entry.requested_width),
                ),
                (
                    "requested_height",
                    format_oracle_optional_u32(entry.requested_height),
                ),
                (
                    "requested_context_options",
                    format_oracle_optional_u32(entry.requested_context_options),
                ),
                (
                    "requested_display_depth_mask",
                    format_oracle_optional_u32(entry.requested_display_depth_mask),
                ),
                (
                    "requested_back_buffer_depth_mask",
                    format_oracle_optional_u32(entry.requested_back_buffer_depth_mask),
                ),
                (
                    "requested_display_depth",
                    format_oracle_optional_u32(entry.requested_display_depth),
                ),
                (
                    "requested_back_buffer_depth",
                    format_oracle_optional_u32(entry.requested_back_buffer_depth),
                ),
                (
                    "requested_page_count",
                    format_oracle_optional_u32(entry.requested_page_count),
                ),
                (
                    "can_user_select",
                    format_oracle_optional_bool(entry.can_user_select),
                ),
                (
                    "fade_kind",
                    entry
                        .fade_kind
                        .clone()
                        .unwrap_or_else(|| "none".to_string()),
                ),
                (
                    "fade_percent",
                    entry
                        .fade_percent
                        .map_or_else(|| "none".to_string(), |percent| percent.to_string()),
                ),
                (
                    "fade_zero_red",
                    format_oracle_optional_u16(entry.fade_zero_red),
                ),
                (
                    "fade_zero_green",
                    format_oracle_optional_u16(entry.fade_zero_green),
                ),
                (
                    "fade_zero_blue",
                    format_oracle_optional_u16(entry.fade_zero_blue),
                ),
                (
                    "reserved_context",
                    format_oracle_optional_hex32(entry.reserved_context),
                ),
                (
                    "active_context",
                    format_oracle_optional_hex32(entry.active_context),
                ),
                (
                    "has_reserved_context",
                    entry.reserved_context.is_some().to_string(),
                ),
                (
                    "has_active_context",
                    entry.active_context.is_some().to_string(),
                ),
                ("context_state", entry.context_state.clone()),
                (
                    "front_gworld",
                    format_oracle_hex32(entry.front_buffer_gworld),
                ),
                ("back_gworld", format_oracle_hex32(entry.back_buffer_gworld)),
                (
                    "last_swap_context",
                    format_oracle_optional_hex32(entry.last_swap_context),
                ),
                ("swap_count", entry.swap_count.to_string()),
                ("fade_count", entry.fade_count.to_string()),
                ("frequency", entry.frequency.to_string()),
                ("width", entry.width.to_string()),
                ("height", entry.height.to_string()),
                ("context_options", entry.context_options.to_string()),
                ("display_depth_mask", entry.display_depth_mask.to_string()),
                (
                    "back_buffer_depth_mask",
                    entry.back_buffer_depth_mask.to_string(),
                ),
                ("display_depth", entry.display_depth.to_string()),
                ("back_buffer_depth", entry.back_buffer_depth.to_string()),
                ("page_count", entry.page_count.to_string()),
            ]);
            if let Err(err) = self.dispatcher.record_trace_event(
                &self.bus,
                entry.pc,
                "draw_sprocket",
                fields,
                false,
            ) {
                eprintln!("[ORACLE] failed to record DrawSprocket event: {:?}", err);
            }
        }
    }

    fn record_q3_frame_trace(
        &mut self,
        pc: u32,
        frame_start: usize,
        frame_end: usize,
        stats: PpcQ3SoftwareRenderStats,
    ) {
        if stats.frames == 0 || !self.dispatcher.is_trace_recording() {
            return;
        }
        let fields = TrapDispatcher::trace_field_map(&[
            ("frame_start", frame_start.to_string()),
            ("frame_end", frame_end.to_string()),
            ("frames", stats.frames.to_string()),
            ("commands", stats.commands.to_string()),
            ("vertices", stats.vertices.to_string()),
            ("triangles", stats.triangles.to_string()),
            ("pixels", stats.pixels.to_string()),
            (
                "target_base",
                format_oracle_optional_hex32(stats.target_base),
            ),
            (
                "target_row_bytes",
                format_oracle_optional_u32(stats.target_row_bytes),
            ),
            (
                "target_width",
                format_oracle_optional_u32(stats.target_width),
            ),
            (
                "target_height",
                format_oracle_optional_u32(stats.target_height),
            ),
            (
                "target_depth",
                format_oracle_optional_u32(stats.target_depth),
            ),
            (
                "target_source",
                stats.target_source.unwrap_or("none").to_string(),
            ),
            (
                "target_draw_context",
                format_oracle_optional_hex32(stats.target_draw_context),
            ),
            (
                "target_gworld",
                format_oracle_optional_hex32(stats.target_gworld),
            ),
            ("target_consistent", stats.target_consistent.to_string()),
        ]);
        if let Err(err) = self
            .dispatcher
            .record_trace_event(&self.bus, pc, "q3_frame", fields, false)
        {
            eprintln!("[ORACLE] failed to record QD3D frame event: {:?}", err);
        }
    }

    fn ppc_input_snapshot(&self) -> PpcInputSnapshot {
        let (mouse_v, mouse_h) = self.dispatcher.input_state.mouse_position();
        PpcInputSnapshot {
            key_map: self.dispatcher.input_state.key_map_snapshot(),
            mouse_button: self.dispatcher.input_state.mouse_button_pressed(),
            mouse_v,
            mouse_h,
        }
    }

    fn canonical_mouse_position(&self, v: i16, h: i16) -> (i16, i16) {
        // EventRecord.where is always in Macintosh global coordinates; the
        // centered matte is host presentation, not part of that coordinate
        // system. Inside Macintosh Volume I, I-259.
        let (offset_v, offset_h) = self.ppc_viewport_offset();
        (v.saturating_sub(offset_v), h.saturating_sub(offset_h))
    }

    fn ppc_viewport_offset(&self) -> (i16, i16) {
        let Some(front_buffer) = self
            .native
            .application()
            .and_then(PpcLoadedApp::presented_front_buffer)
        else {
            return (0, 0);
        };
        let (canvas_width, canvas_height) = Self::ppc_host_canvas_dimensions(front_buffer);
        (
            i16::try_from(canvas_height.saturating_sub(front_buffer.height) / 2).unwrap_or(0),
            i16::try_from(canvas_width.saturating_sub(front_buffer.width) / 2).unwrap_or(0),
        )
    }

    fn service_ppc_sound_adapter(&mut self, ppc_app: &mut PpcLoadedApp) {
        self.service_ppc_double_buffer_memory(ppc_app);
    }

    /// Decode native guest buffer records into the process Sound Manager.
    ///
    /// Guest-byte decoding is an ABI adapter responsibility. Playback state,
    /// buffer progression, and doubleback scheduling remain process-owned.
    fn service_ppc_double_buffer_memory(&mut self, ppc_app: &mut PpcLoadedApp) {
        let stopped_channels = ppc_app
            .sound
            .manager
            .double_buffer_playbacks
            .iter()
            .filter(|playback| !playback.active && playback.host_buffer_loaded)
            .filter(|playback| {
                !ppc_app
                    .sound
                    .manager
                    .double_buffer_playbacks
                    .iter()
                    .any(|other| other.active && other.channel == playback.channel)
            })
            .map(|playback| playback.channel)
            .collect::<Vec<_>>();
        for channel in stopped_channels {
            self.dispatcher
                .sound_manager
                .with_channel_mut(channel, |host_channel| {
                    host_channel.quiet();
                });
        }
        ppc_app.sound.manager.with_mut(|sound| {
            for playback in &mut sound.double_buffer_playbacks {
                if !playback.active {
                    playback.host_buffer_loaded = false;
                }
            }
        });

        for index in 0..ppc_app.sound.manager.double_buffer_playbacks.len() {
            let playback = ppc_app.sound.manager.double_buffer_playbacks[index];
            if !playback.active {
                continue;
            }
            if !playback.host_initialized {
                ppc_app.sound.manager.with_mut(|sound| {
                    sound.double_buffer_playbacks[index].host_initialized = true;
                });
                self.dispatcher
                    .sound_manager
                    .note_double_buffer_submission();
            }
            if playback.host_buffer_loaded {
                continue;
            }

            let Some(decoded) = Self::decode_ppc_double_buffer(&mut ppc_app.memory, playback)
            else {
                if trace_sound_runner_enabled() {
                    eprintln!(
                        "[PPC-SOUND] unsupported double buffer chan=${:08X} header=${:08X} channels={} bits={} compression={} packet={}",
                        playback.channel,
                        playback.header,
                        playback.num_channels,
                        playback.sample_size,
                        playback.compression_id,
                        playback.packet_size
                    );
                }
                continue;
            };
            if decoded.flags & 0x01 == 0 || decoded.samples.is_empty() {
                ppc_app.sound.manager.with_mut(|sound| {
                    Self::queue_ppc_doubleback(
                        sound,
                        index,
                        self.guest_tick(),
                        self.total_instructions,
                    );
                });
                continue;
            }

            self.play_ppc_decoded_double_buffer(playback, &decoded);
            ppc_app.sound.manager.with_mut(|sound| {
                sound.double_buffer_playbacks[index].host_buffer_loaded = true;
            });
        }
    }

    fn decode_ppc_double_buffer(
        memory: &mut PpcSectionMem,
        playback: PpcSoundDoubleBufferPlaybackRecord,
    ) -> Option<PpcDecodedDoubleBuffer> {
        const MAX_RETAINED_SAMPLE_BYTES: usize = 64 * 1024 * 1024;

        if playback.compression_id != 0 {
            return None;
        }
        let buffer_index = usize::from(playback.current_buffer_index & 1);
        let buffer_ptr = playback.buffers[buffer_index];
        if buffer_ptr == 0 {
            return None;
        }
        let num_frames = usize::try_from(memory.read_u32_be(buffer_ptr)?).ok()?;
        let flags = memory.read_u32_be(buffer_ptr.checked_add(4)?)?;
        if flags & 0x01 == 0 || num_frames == 0 {
            return Some(PpcDecodedDoubleBuffer {
                buffer_ptr,
                flags,
                samples: Vec::new(),
            });
        }
        let num_channels = usize::from(playback.num_channels);
        let sample_size = usize::from(playback.sample_size);
        let bytes_per_sample = match sample_size {
            8 => 1usize,
            16 => 2usize,
            _ => return None,
        };
        let byte_count = num_frames
            .checked_mul(num_channels)?
            .checked_mul(bytes_per_sample)?;
        if byte_count > MAX_RETAINED_SAMPLE_BYTES {
            return None;
        }
        let mut raw = vec![0; byte_count];
        memory.read_bytes_into(buffer_ptr.checked_add(16)?, &mut raw)?;
        let samples = crate::trap::decode_interleaved_stereo_samples(
            &raw,
            num_frames,
            num_channels,
            sample_size,
        )?;
        Some(PpcDecodedDoubleBuffer {
            buffer_ptr,
            flags,
            samples,
        })
    }

    fn play_ppc_decoded_double_buffer(
        &mut self,
        playback: PpcSoundDoubleBufferPlaybackRecord,
        decoded: &PpcDecodedDoubleBuffer,
    ) {
        let non_silent_frames = decoded
            .samples
            .iter()
            .filter(|sample| sample.left != 0x80 || sample.right != 0x80)
            .count();
        if trace_sound_runner_enabled() {
            eprintln!(
                "[PPC-SOUND] process double buffer chan=${:08X} buf=${:08X} index={} frames={} non_silent={} flags=${:08X}",
                playback.channel,
                decoded.buffer_ptr,
                playback.current_buffer_index,
                decoded.samples.len(),
                non_silent_frames,
                decoded.flags
            );
        }
        self.dispatcher.sound_manager.play_double_buffer_samples(
            playback.channel,
            decoded.samples.clone(),
            playback.sample_rate_fixed,
        );
    }

    fn queue_ppc_doubleback(
        sound: &mut crate::sound::SoundManager,
        playback_index: usize,
        tick: u32,
        instruction_count: u64,
    ) {
        let Some(playback) = sound.double_buffer_playbacks.get_mut(playback_index) else {
            return;
        };
        let buffer_index = playback.current_buffer_index & 1;
        let pending_bit = 1u8 << buffer_index;
        let buffer_ptr = playback.buffers[usize::from(buffer_index)];
        if playback.callback == 0
            || buffer_ptr == 0
            || playback.callback_pending_mask & pending_bit != 0
        {
            return;
        }
        playback.callback_pending_mask |= pending_bit;
        sound
            .pending_process_doublebacks
            .push(PpcSoundDoubleBackRecord {
                architecture: playback.callback_architecture,
                channel: playback.channel,
                header: playback.header,
                exhausted_buffer: buffer_ptr,
                exhausted_buffer_index: u32::from(buffer_index),
                callback: playback.callback,
                tick,
                instruction_count,
            });
    }

    fn service_ppc_double_buffer_playbacks(&mut self) {
        let Some(mut native_context) = self.native.take(NativeEngineRole::Application) else {
            return;
        };
        let mut ppc_app = native_context.adapter_mut();

        for index in 0..ppc_app.sound.manager.double_buffer_playbacks.len() {
            let playback = ppc_app.sound.manager.double_buffer_playbacks[index];
            if !playback.active || !playback.host_buffer_loaded {
                continue;
            }
            let host_is_playing = self
                .dispatcher
                .sound_manager
                .channels
                .iter()
                .find(|channel| channel.guest_ptr == playback.channel)
                .is_some_and(crate::sound::SndChannel::is_playing);
            if host_is_playing {
                continue;
            }

            let buffer_index = playback.current_buffer_index & 1;
            let buffer_ptr = playback.buffers[usize::from(buffer_index)];
            let flags = ppc_app
                .memory
                .read_u32_be(buffer_ptr.wrapping_add(4))
                .unwrap_or(0);
            if buffer_ptr != 0 {
                let _ = ppc_app
                    .memory
                    .write_u32_be(buffer_ptr.wrapping_add(4), flags & !0x01);
            }
            ppc_app.sound.manager.with_mut(|sound| {
                sound.double_buffer_playbacks[index].host_buffer_loaded = false;
            });
            if flags & 0x04 != 0 {
                ppc_app.sound.manager.with_mut(|sound| {
                    sound.double_buffer_playbacks[index].active = false;
                });
                continue;
            }

            ppc_app.sound.manager.with_mut(|sound| {
                Self::queue_ppc_doubleback(
                    sound,
                    index,
                    self.guest_tick(),
                    self.total_instructions,
                );
                sound.double_buffer_playbacks[index].current_buffer_index = buffer_index ^ 1;
            });
        }

        self.service_ppc_double_buffer_memory(&mut ppc_app);
        self.native
            .restore(native_context)
            .unwrap_or_else(|_| panic!("native context lost its owner"));
        self.fire_pending_ppc_sound_doublebacks();

        let Some(mut native_context) = self.native.take(NativeEngineRole::Application) else {
            return;
        };
        let mut ppc_app = native_context.adapter_mut();
        self.service_ppc_double_buffer_memory(&mut ppc_app);
        self.native
            .restore(native_context)
            .unwrap_or_else(|_| panic!("native context lost its owner"));
    }

    fn fire_pending_ppc_sound_doublebacks(&mut self) {
        let Some(ppc_app) = self.native.application() else {
            return;
        };
        if ppc_app
            .sound
            .manager
            .pending_process_doublebacks
            .iter()
            .all(|doubleback| doubleback.architecture != CallbackTaskArchitecture::PowerPc)
        {
            return;
        }

        let record_ppc_imports = self.dispatcher.is_trace_recording();
        let trace_ppc_import_hist = ppc_import_hist_enabled();
        let trace_ppc_imports = record_ppc_imports || trace_ppc_import_hist;
        let trace_ppc_fetches = trace_ppc_fetch_counts_enabled();
        let Some(mut native_context) = self.native.take(NativeEngineRole::Application) else {
            return;
        };
        let mut ppc_app = native_context.adapter_mut();
        self.prepare_ppc_execution_clock(&mut ppc_app);
        let mut fired_count = 0usize;
        while fired_count < 16 {
            let Some(doubleback) = ppc_app.sound.manager.with_mut(|sound| {
                sound
                    .pending_process_doublebacks
                    .iter()
                    .position(|doubleback| {
                        doubleback.architecture == CallbackTaskArchitecture::PowerPc
                    })
                    .map(|index| sound.pending_process_doublebacks.remove(index))
            }) else {
                break;
            };
            let resume_pc = ppc_app.cpu.pc;
            let probe = self
                .process_context
                .with_memory_and_cfm(|memory_manager, cfm| {
                    ppc_app.run_sound_doubleback_callback_with_process_services(
                        doubleback,
                        PPC_SOUND_DOUBLEBACK_CALLBACK_MAX_CYCLES,
                        trace_ppc_imports,
                        trace_ppc_fetches,
                        memory_manager,
                        cfm,
                    )
                });
            let invocation = probe.invocation;
            if trace_sound_runner_enabled() {
                eprintln!(
                    "[PPC-SOUND] doubleback chan=${:08X} buffer={} callback=${:08X} entry=${:08X} resume_pc=${:08X} end_pc=${:08X} cycles={} result={:?}",
                    doubleback.channel,
                    doubleback.exhausted_buffer_index,
                    doubleback.callback,
                    invocation.callback_entry,
                    resume_pc,
                    invocation.end_pc,
                    invocation.cycles,
                    invocation.result
                );
            }
            if record_ppc_imports {
                self.record_ppc_import_trace(&probe.import_trace);
            }
            if trace_ppc_import_hist {
                self.record_ppc_import_histogram(&probe.import_trace);
            }
            if let Some(histogram) = probe.fetch_histogram.as_ref() {
                self.ppc_fetch_histogram.merge_from(histogram);
            }
            if ppc_unimpl_hist_enabled() {
                self.record_ppc_unimpl_histogram(
                    &ppc_app.imports,
                    invocation.result,
                    invocation.unsupported_import_index,
                );
            }
            self.total_instructions = self.total_instructions.saturating_add(invocation.cycles);
            self.dispatcher.instruction_count = self.total_instructions;
            let _ = self.advance_ticks_for_ppc_cycles(invocation.cycles, None);
            self.prepare_ppc_execution_clock(&mut ppc_app);

            let buffer_bit = 1u8 << (doubleback.exhausted_buffer_index.min(1) as u8);
            ppc_app.sound.manager.with_mut(|sound| {
                if let Some(playback) = sound
                    .double_buffer_playbacks
                    .iter_mut()
                    .rev()
                    .find(|playback| {
                        playback.channel == doubleback.channel
                            && playback.header == doubleback.header
                    })
                {
                    playback.callback_pending_mask &= !buffer_bit;
                }
            });

            let callback_failed = invocation.unsupported_import_index.is_some()
                || !matches!(invocation.result, PpcRunResult::Halted { .. });
            if callback_failed {
                self.halted = true;
                self.halted_pc = Some(invocation.end_pc);
                self.halted_sp = Some(invocation.end_sp);
                self.halted_d0 = Some(invocation.end_r3);
                if trace_load_enabled() {
                    eprintln!(
                        "[PPC] sound doubleback callback failed pc=${:08X} callback=${:08X} result={:?} unsupported_import={:?}",
                        invocation.end_pc,
                        invocation.completion,
                        invocation.result,
                        invocation.unsupported_import_index
                    );
                }
            }

            ppc_app.sound.completion_invocations.push(invocation);
            fired_count += 1;
            if callback_failed {
                break;
            }
        }
        self.persist_ppc_vfs_to_host(&mut ppc_app);
        self.service_ppc_sound_adapter(&mut ppc_app);
        self.native
            .restore(native_context)
            .unwrap_or_else(|_| panic!("native context lost its owner"));
    }

    fn fire_pending_ppc_sound_completions(&mut self) {
        let Some(ppc_app) = self.native.application() else {
            return;
        };
        // Playback and its exhaustion boundary remain process Sound Manager
        // state until the PowerPC adapter constructs the callback ABI frame.
        // Inside Macintosh: Sound (1994), pp. 2-134--2-151.
        if ppc_app
            .sound
            .manager
            .pending_sound_callbacks
            .iter()
            .all(|callback| {
                !matches!(
                    callback,
                    crate::sound::PendingSoundCallback::Command {
                        architecture: CallbackTaskArchitecture::PowerPc,
                        ..
                    } | crate::sound::PendingSoundCallback::FileCompletion {
                        architecture: CallbackTaskArchitecture::PowerPc,
                        ..
                    }
                )
            })
        {
            return;
        }

        let record_ppc_imports = self.dispatcher.is_trace_recording();
        let trace_ppc_import_hist = ppc_import_hist_enabled();
        let trace_ppc_imports = record_ppc_imports || trace_ppc_import_hist;
        let trace_ppc_fetches = trace_ppc_fetch_counts_enabled();
        let Some(mut native_context) = self.native.take(NativeEngineRole::Application) else {
            return;
        };
        let mut ppc_app = native_context.adapter_mut();
        self.prepare_ppc_execution_clock(&mut ppc_app);
        let mut fired_count = 0usize;
        while fired_count < 16 {
            let Some(pending) = ppc_app.sound.manager.with_mut(|sound| {
                sound
                    .pending_sound_callbacks
                    .iter()
                    .position(|callback| {
                        matches!(
                            callback,
                            crate::sound::PendingSoundCallback::Command {
                                architecture: CallbackTaskArchitecture::PowerPc,
                                ..
                            } | crate::sound::PendingSoundCallback::FileCompletion {
                                architecture: CallbackTaskArchitecture::PowerPc,
                                ..
                            }
                        )
                    })
                    .map(|index| sound.pending_sound_callbacks.remove(index))
            }) else {
                break;
            };
            let (callback_addr, chan_ptr, file_playback_index, command) = match pending {
                crate::sound::PendingSoundCallback::Command {
                    callback_addr,
                    chan_ptr,
                    cmd,
                    ..
                } => (
                    callback_addr,
                    chan_ptr,
                    u32::MAX,
                    Some(PpcSndCommandRecord {
                        channel: chan_ptr,
                        command: cmd.cmd,
                        param1: cmd.param1,
                        param2: cmd.param2,
                    }),
                ),
                crate::sound::PendingSoundCallback::FileCompletion {
                    callback_addr,
                    chan_ptr,
                    ..
                } => {
                    let Some((file_playback_index, playback)) = ppc_app
                        .sound
                        .file_playbacks
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(_, playback)| playback.channel == chan_ptr)
                    else {
                        continue;
                    };
                    (
                        callback_addr,
                        chan_ptr,
                        u32::try_from(file_playback_index).unwrap_or(u32::MAX),
                        playback.completion_command,
                    )
                }
            };
            if callback_addr == 0 {
                continue;
            }
            let completion = PpcSoundCompletionRecord {
                file_playback_index,
                channel: chan_ptr,
                completion: callback_addr,
                command,
                tick: self.guest_tick(),
                instruction_count: self.total_instructions,
                scheduled_tick: self.guest_tick(),
                scheduled_instruction_count: self.total_instructions,
            };
            let probe = self
                .process_context
                .with_memory_and_cfm(|memory_manager, cfm| {
                    ppc_app.run_sound_completion_callback_with_process_services(
                        completion,
                        PPC_SOUND_COMPLETION_CALLBACK_MAX_CYCLES,
                        trace_ppc_imports,
                        trace_ppc_fetches,
                        memory_manager,
                        cfm,
                    )
                });
            let invocation = probe.invocation;
            if record_ppc_imports {
                self.record_ppc_import_trace(&probe.import_trace);
            }
            if trace_ppc_import_hist {
                self.record_ppc_import_histogram(&probe.import_trace);
            }
            if let Some(histogram) = probe.fetch_histogram.as_ref() {
                self.ppc_fetch_histogram.merge_from(histogram);
            }
            if ppc_unimpl_hist_enabled() {
                self.record_ppc_unimpl_histogram(
                    &ppc_app.imports,
                    invocation.result,
                    invocation.unsupported_import_index,
                );
            }
            self.total_instructions = self.total_instructions.saturating_add(invocation.cycles);
            self.dispatcher.instruction_count = self.total_instructions;
            let _ = self.advance_ticks_for_ppc_cycles(invocation.cycles, None);
            self.prepare_ppc_execution_clock(&mut ppc_app);

            let callback_failed = invocation.unsupported_import_index.is_some()
                || !matches!(invocation.result, PpcRunResult::Halted { .. });
            if callback_failed {
                self.halted = true;
                self.halted_pc = Some(invocation.end_pc);
                self.halted_sp = Some(invocation.end_sp);
                self.halted_d0 = Some(invocation.end_r3);
                if trace_load_enabled() {
                    eprintln!(
                        "[PPC] sound completion callback failed pc=${:08X} completion=${:08X} result={:?} unsupported_import={:?}",
                        invocation.end_pc,
                        invocation.completion,
                        invocation.result,
                        invocation.unsupported_import_index
                    );
                }
            }

            ppc_app.sound.completion_invocations.push(invocation);
            fired_count += 1;
            if callback_failed {
                break;
            }
        }
        self.persist_ppc_vfs_to_host(&mut ppc_app);
        self.service_ppc_sound_adapter(&mut ppc_app);
        self.native
            .restore(native_context)
            .unwrap_or_else(|_| panic!("native context lost its owner"));
    }

    fn persist_ppc_vfs_to_host(&mut self, ppc_app: &mut PpcLoadedApp) {
        for path in ppc_app.take_deleted_vfs_file_paths() {
            let normalized = crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(&path);
            if normalized.is_empty() {
                continue;
            }
            if let Some(dir) = &self.dispatcher.output_dir {
                let host_path = dir.join(&normalized);
                let _ = std::fs::remove_file(&host_path);
                if let Some((parent, file_name)) = ppc_resource_sidecar_parent(dir, &normalized) {
                    let _ = std::fs::remove_file(parent.join(".rsrc").join(file_name));
                }
            }
        }

        for directory in ppc_app.take_dirty_vfs_directories() {
            let normalized =
                crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(&directory.path);
            if normalized.is_empty() {
                continue;
            }
            if let Some(dir) = &self.dispatcher.output_dir {
                let _ = std::fs::create_dir_all(dir.join(&normalized));
            }
        }

        for file in ppc_app.take_dirty_vfs_files() {
            let normalized = crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(&file.path);
            if normalized.is_empty() {
                continue;
            }
            if let Some(dir) = &self.dispatcher.output_dir {
                let host_path = dir.join(&normalized);
                if let Some(parent) = host_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(host_path, &file.data);
            }
        }

        for fork in ppc_app.take_dirty_vfs_resource_forks() {
            let normalized = crate::trap::dispatch::TrapDispatcher::normalize_vfs_path(&fork.path);
            if normalized.is_empty() {
                continue;
            }
            if let Some(dir) = &self.dispatcher.output_dir {
                if let Some((parent, file_name)) = ppc_resource_sidecar_parent(dir, &normalized) {
                    let rsrc_dir = parent.join(".rsrc");
                    if std::fs::create_dir_all(&rsrc_dir).is_ok() {
                        let _ = std::fs::write(rsrc_dir.join(file_name), &fork.data);
                    }
                }
            }
        }
    }

    fn sync_ppc_front_buffer_to_host(&mut self, ppc_app: &mut PpcLoadedApp) {
        let Some(primary_buffer) = ppc_app.presented_front_buffer() else {
            return;
        };
        let (canvas_width, canvas_height) = Self::ppc_host_canvas_dimensions(primary_buffer);
        let Some(canvas_row_bytes) = Self::ppc_host_row_bytes(canvas_width, primary_buffer.depth)
        else {
            return;
        };
        let canvas = PpcFrontBuffer {
            base_addr: primary_buffer.base_addr,
            row_bytes: canvas_row_bytes,
            width: canvas_width,
            height: canvas_height,
            depth: primary_buffer.depth,
        };
        let Some(host_base) = self.ensure_ppc_host_screen_mode(canvas) else {
            return;
        };
        if primary_buffer.depth <= 8
            && !self.sync_ppc_host_indexed_color_table(
                primary_buffer.depth as u16,
                &ppc_app.screen_clut,
            )
        {
            return;
        }
        let matte_byte =
            Self::ppc_indexed_matte_byte(primary_buffer.depth, &ppc_app.screen_clut).unwrap_or(0);
        if canvas_row_bytes.checked_mul(canvas_height).is_none() {
            return;
        }
        let primary_destination_x = canvas_width.saturating_sub(primary_buffer.width) / 2;
        let primary_destination_y = canvas_height.saturating_sub(primary_buffer.height) / 2;
        // Only the matte lies outside the incoming image. Clearing the image
        // itself would discard retained text on every host synchronization.
        if primary_buffer.depth < 8 {
            self.bus.write_bytes(
                host_base,
                &vec![matte_byte; (canvas_row_bytes * canvas_height) as usize],
            );
        } else if canvas_width != primary_buffer.width || canvas_height != primary_buffer.height {
            let left_bytes = (primary_destination_x * primary_buffer.depth / 8) as usize;
            let right_byte = ((primary_destination_x + primary_buffer.width) * primary_buffer.depth)
                .div_ceil(8) as usize;
            let row = vec![matte_byte; canvas_row_bytes as usize];
            for y in 0..canvas_height {
                let address = host_base + y * canvas_row_bytes;
                if y < primary_destination_y || y >= primary_destination_y + primary_buffer.height {
                    self.bus.write_bytes(address, &row);
                } else {
                    self.bus.write_bytes(address, &row[..left_bytes]);
                    self.bus
                        .write_bytes(address + right_byte as u32, &row[right_byte..]);
                }
            }
        }
        if !Self::copy_ppc_front_buffer_rows_to_host(
            &mut self.bus,
            ppc_app,
            primary_buffer,
            host_base,
            canvas_row_bytes,
            primary_destination_x,
            primary_destination_y,
        ) {
            return;
        }
    }

    fn ppc_host_canvas_dimensions(front_buffer: PpcFrontBuffer) -> (u32, u32) {
        if front_buffer.width >= 512 && front_buffer.height >= 342 {
            (
                front_buffer.width.max(u32::from(
                    crate::machine_profile::REFERENCE_MACHINE_PROFILE.screen_width,
                )),
                front_buffer.height.max(u32::from(
                    crate::machine_profile::REFERENCE_MACHINE_PROFILE.screen_height,
                )),
            )
        } else {
            (front_buffer.width, front_buffer.height)
        }
    }

    fn ppc_host_row_bytes(width: u32, depth: u32) -> Option<u32> {
        if !matches!(depth, 1 | 2 | 4 | 8 | 16) {
            return None;
        }
        width.checked_mul(depth)?.checked_add(7)?.checked_div(8)
    }

    fn sync_ppc_host_indexed_color_table(&mut self, depth: u16, clut: &[[u16; 3]; 256]) -> bool {
        if !matches!(depth, 1 | 2 | 4 | 8) {
            return false;
        }
        let gdevice_handle = self.dispatcher.ensure_main_gdevice(&mut self.bus);
        let gdevice = self.bus.read_long(gdevice_handle);
        if gdevice == 0 {
            return false;
        }
        let pixmap_handle = self.bus.read_long(gdevice + 22);
        if pixmap_handle == 0 {
            return false;
        }
        let pixmap = self.bus.read_long(pixmap_handle);
        if pixmap == 0 {
            return false;
        }
        let color_table_handle = self.bus.read_long(pixmap + 42);
        if color_table_handle == 0 {
            return false;
        }
        let entry_count = 1u32 << depth;
        let color_table = self.dispatcher.ensure_color_table_capacity(
            &mut self.bus,
            color_table_handle,
            entry_count,
        );
        if color_table == 0 {
            return false;
        }
        // An indexed screen PixMap's pmTable is the live mapping from pixel
        // values to RGB colors. Imaging With QuickDraw (1994), pp. 4-10--4-11
        // and 4-56--4-57.
        self.bus.write_long(color_table, u32::from(depth));
        self.bus.write_word(color_table + 4, 0x8000);
        self.bus.write_word(color_table + 6, entry_count as u16 - 1);
        for index in 0..entry_count {
            let entry = color_table + 8 + index * 8;
            let rgb = clut[index as usize];
            self.bus.write_word(entry, 0);
            self.bus.write_word(entry + 2, rgb[0]);
            self.bus.write_word(entry + 4, rgb[1]);
            self.bus.write_word(entry + 6, rgb[2]);
        }
        true
    }

    fn ppc_indexed_matte_byte(depth: u32, clut: &[[u16; 3]; 256]) -> Option<u8> {
        if !matches!(depth, 1 | 2 | 4 | 8) {
            return None;
        }
        let color_count = 1usize.checked_shl(depth)?;
        let index = clut
            .iter()
            .take(color_count)
            .enumerate()
            .min_by_key(|(_, color)| {
                u32::from(color[0]) + u32::from(color[1]) + u32::from(color[2])
            })?
            .0 as u8;
        let field_mask = ((1u16 << depth) - 1) as u8;
        let mut packed = 0u8;
        for field in 0..(8 / depth) {
            let shift = 8 - depth * (field + 1);
            packed |= (index & field_mask) << shift;
        }
        Some(packed)
    }

    fn copy_ppc_packed_indexed_row(
        source: &[u8],
        depth: u32,
        width: u32,
        destination: &mut [u8],
        destination_x: u32,
    ) -> bool {
        if !matches!(depth, 1 | 2 | 4) {
            return false;
        }
        let Some(visible_bits) = width.checked_mul(depth) else {
            return false;
        };
        let Some(source_bytes) = visible_bits.checked_add(7).map(|bits| bits / 8) else {
            return false;
        };
        let Some(destination_start_bit) = destination_x.checked_mul(depth) else {
            return false;
        };
        let Some(destination_end_bit) = destination_start_bit.checked_add(visible_bits) else {
            return false;
        };
        if usize::try_from(source_bytes)
            .ok()
            .is_none_or(|bytes| bytes > source.len())
            || usize::try_from(destination_end_bit.div_ceil(8))
                .ok()
                .is_none_or(|bytes| bytes > destination.len())
        {
            return false;
        }

        let mut first_scalar_pixel = 0u32;
        if destination_start_bit & 7 == 0 {
            let full_bytes = visible_bits / 8;
            let Some(source_end) = usize::try_from(full_bytes).ok() else {
                return false;
            };
            let Some(destination_start) = usize::try_from(destination_start_bit / 8).ok() else {
                return false;
            };
            let Some(destination_end) = destination_start.checked_add(source_end) else {
                return false;
            };
            destination[destination_start..destination_end].copy_from_slice(&source[..source_end]);
            first_scalar_pixel = full_bytes * 8 / depth;
        }

        let field_mask = ((1u16 << depth) - 1) as u8;
        for x in first_scalar_pixel..width {
            let source_bit = x * depth;
            let source_byte = source[(source_bit / 8) as usize];
            let source_shift = 8 - depth - (source_bit & 7);
            let pixel = (source_byte >> source_shift) & field_mask;

            let destination_bit = destination_start_bit + x * depth;
            let destination_byte = &mut destination[(destination_bit / 8) as usize];
            let destination_shift = 8 - depth - (destination_bit & 7);
            let mask = field_mask << destination_shift;
            *destination_byte =
                (*destination_byte & !mask) | ((pixel & field_mask) << destination_shift);
        }
        true
    }

    fn copy_ppc_front_buffer_rows_to_host(
        bus: &mut MacMemoryBus,
        ppc_app: &mut PpcLoadedApp,
        front_buffer: PpcFrontBuffer,
        host_base: u32,
        host_row_bytes: u32,
        destination_x: u32,
        destination_y: u32,
    ) -> bool {
        let Some(visible_row_bytes) = Self::ppc_front_buffer_visible_row_bytes(front_buffer) else {
            return false;
        };
        let Some(destination_end_bits) = destination_x
            .checked_add(front_buffer.width)
            .and_then(|pixels| pixels.checked_mul(front_buffer.depth))
        else {
            return false;
        };
        let Some(host_row_bits) = host_row_bytes.checked_mul(8) else {
            return false;
        };
        if destination_end_bits > host_row_bits {
            return false;
        }
        let Ok(source_row_len) = usize::try_from(front_buffer.row_bytes) else {
            return false;
        };
        let Ok(visible_row_len) = usize::try_from(visible_row_bytes) else {
            return false;
        };
        let Ok(host_row_len) = usize::try_from(host_row_bytes) else {
            return false;
        };
        let mut row = vec![0u8; source_row_len];
        for y in 0..front_buffer.height {
            if ppc_app
                .read_front_buffer_row(front_buffer, y, &mut row)
                .is_none()
            {
                return false;
            }
            Self::apply_ppc_draw_sprocket_gamma_fade(
                &mut row[..visible_row_len],
                front_buffer.depth,
                ppc_app.draw_sprocket.last_fade_percent,
                ppc_app.draw_sprocket.last_fade_zero_color,
            );
            let Some(destination_row) = destination_y.checked_add(y) else {
                return false;
            };
            let Some(destination_row_addr) = destination_row
                .checked_mul(host_row_bytes)
                .and_then(|offset| host_base.checked_add(offset))
            else {
                return false;
            };
            if matches!(front_buffer.depth, 1 | 2 | 4) {
                let mut packed_destination = bus.read_bytes(destination_row_addr, host_row_len);
                if packed_destination.len() != host_row_len
                    || !Self::copy_ppc_packed_indexed_row(
                        &row[..visible_row_len],
                        front_buffer.depth,
                        front_buffer.width,
                        &mut packed_destination,
                        destination_x,
                    )
                {
                    return false;
                }
                bus.write_bytes(destination_row_addr, &packed_destination);
            } else {
                let bytes_per_pixel = front_buffer.depth / 8;
                let Some(destination_x_bytes) = destination_x.checked_mul(bytes_per_pixel) else {
                    return false;
                };
                if ppc_app
                    .draw_sprocket
                    .last_fade_percent
                    .is_none_or(|percent| percent == 100)
                    && ppc_app.draw_sprocket.last_fade_zero_color.is_none()
                {
                    bus.sync_presented_bytes(
                        destination_row_addr + destination_x_bytes,
                        front_buffer.base_addr + y * front_buffer.row_bytes,
                        &row[..visible_row_len],
                    );
                } else {
                    bus.write_bytes(
                        destination_row_addr + destination_x_bytes,
                        &row[..visible_row_len],
                    );
                }
            }
        }
        true
    }

    fn ppc_front_buffer_visible_row_bytes(front_buffer: PpcFrontBuffer) -> Option<u32> {
        let visible = Self::ppc_host_row_bytes(front_buffer.width, front_buffer.depth)?;
        (visible <= front_buffer.row_bytes).then_some(visible)
    }

    fn apply_ppc_draw_sprocket_gamma_fade(
        row: &mut [u8],
        depth: u32,
        percent: Option<i32>,
        zero_color: Option<PpcRgbColor>,
    ) {
        let Some(percent) = percent else {
            return;
        };
        if depth != 16 {
            return;
        }
        let percent = percent.clamp(0, 100) as u32;
        if percent == 100 && zero_color.is_none() {
            return;
        }
        let zero = Self::ppc_rgb_color_to_rgb555_channels(zero_color.unwrap_or(PpcRgbColor {
            red: 0,
            green: 0,
            blue: 0,
        }));
        for pixel in row.chunks_exact_mut(2) {
            let value = u16::from_be_bytes([pixel[0], pixel[1]]);
            let red = (value >> 10) & 0x1f;
            let green = (value >> 5) & 0x1f;
            let blue = value & 0x1f;
            let faded = Self::rgb555_channels_to_pixel(
                Self::ppc_draw_sprocket_fade_channel(red, zero.0, percent),
                Self::ppc_draw_sprocket_fade_channel(green, zero.1, percent),
                Self::ppc_draw_sprocket_fade_channel(blue, zero.2, percent),
            );
            pixel.copy_from_slice(&faded.to_be_bytes());
        }
    }

    fn ppc_rgb_color_to_rgb555_channels(color: PpcRgbColor) -> (u16, u16, u16) {
        (
            Self::ppc_rgb_component_to_rgb555(color.red),
            Self::ppc_rgb_component_to_rgb555(color.green),
            Self::ppc_rgb_component_to_rgb555(color.blue),
        )
    }

    fn ppc_rgb_component_to_rgb555(component: u16) -> u16 {
        ((u32::from(component) * 31 + 32_767) / 65_535) as u16
    }

    fn ppc_draw_sprocket_fade_channel(channel: u16, zero: u16, percent: u32) -> u16 {
        let channel = i32::from(channel);
        let zero = i32::from(zero);
        let blended = zero + ((channel - zero) * percent as i32 + 50) / 100;
        blended.clamp(0, 31) as u16
    }

    fn rgb555_channels_to_pixel(red: u16, green: u16, blue: u16) -> u16 {
        ((red & 0x1f) << 10) | ((green & 0x1f) << 5) | (blue & 0x1f)
    }

    fn ensure_ppc_host_screen_mode(&mut self, front_buffer: PpcFrontBuffer) -> Option<u32> {
        let width = u16::try_from(front_buffer.width).ok()?;
        let height = u16::try_from(front_buffer.height).ok()?;
        let depth = u16::try_from(front_buffer.depth).ok()?;
        let bytes_needed = front_buffer.row_bytes.checked_mul(front_buffer.height)?;
        if bytes_needed == 0 || front_buffer.row_bytes > u32::from(u16::MAX) {
            return None;
        }

        let (base, row_bytes, current_width, current_height, current_depth) =
            self.dispatcher.screen_mode;
        let base_valid = base != 0
            && self
                .bus
                .get_alloc_size(base)
                .is_some_and(|size| size >= bytes_needed)
            && base
                .checked_add(bytes_needed)
                .is_some_and(|end| end <= self.bus.ram_size());
        let owned_base_valid = self.ppc_host_mirror_base != 0
            && self.ppc_host_mirror_capacity >= bytes_needed
            && self
                .bus
                .get_alloc_size(self.ppc_host_mirror_base)
                .is_some_and(|size| size >= self.ppc_host_mirror_capacity)
            && self
                .ppc_host_mirror_base
                .checked_add(bytes_needed)
                .is_some_and(|end| end <= self.bus.ram_size());
        if base_valid
            && row_bytes == front_buffer.row_bytes
            && current_width == width
            && current_height == height
            && current_depth == depth
            && (self.ppc_host_mirror_base == 0
                || (owned_base_valid && base == self.ppc_host_mirror_base))
        {
            return Some(base);
        }

        let base = if owned_base_valid {
            self.ppc_host_mirror_base
        } else {
            let new_base = self.bus.alloc(bytes_needed);
            if new_base == 0 {
                return None;
            }
            let old_base = self.ppc_host_mirror_base;
            self.ppc_host_mirror_base = new_base;
            self.ppc_host_mirror_capacity = bytes_needed;
            if old_base != 0 {
                self.bus.free(old_base);
            }
            new_base
        };
        self.dispatcher.screen_mode = (base, front_buffer.row_bytes, width, height, depth);
        self.write_ppc_host_screen_lowmem(base, front_buffer.row_bytes, width, height, depth);
        Some(base)
    }

    fn write_ppc_host_screen_lowmem(
        &mut self,
        base: u32,
        row_bytes: u32,
        width: u16,
        height: u16,
        depth: u16,
    ) {
        use crate::memory::globals::addr;

        self.bus.write_long(addr::SCRN_BASE, base);
        self.bus.write_word(addr::SCREEN_ROW, row_bytes as u16);
        self.bus.write_long(addr::SCREEN_BITS, base);
        self.bus.write_word(addr::SCREEN_BITS + 4, row_bytes as u16);
        self.bus.write_word(addr::SCREEN_BITS + 6, 0);
        self.bus.write_word(addr::SCREEN_BITS + 8, 0);
        self.bus.write_word(addr::SCREEN_BITS + 10, height);
        self.bus.write_word(addr::SCREEN_BITS + 12, width);

        let gdevice_handle = self.dispatcher.ensure_main_gdevice(&mut self.bus);
        self.bus.write_long(0x08A4, gdevice_handle);
        self.bus.write_long(0x0CC8, gdevice_handle);
        self.bus.write_long(0x08A8, gdevice_handle);
        let gdevice = self.bus.read_long(gdevice_handle);
        if gdevice == 0 {
            return;
        }
        let pixmap_handle = self.bus.read_long(gdevice + 22);
        let pixmap = self.bus.read_long(pixmap_handle);
        if pixmap == 0 {
            return;
        }
        let current_ctab_handle = self.bus.read_long(pixmap + 42);
        if current_ctab_handle != 0 {
            self.ppc_host_indexed_ctab_handle = current_ctab_handle;
        }
        let ctab_handle = if depth <= 8 {
            self.ppc_host_indexed_ctab_handle
        } else {
            0
        };
        self.bus.write_long(pixmap, base);
        self.bus.write_word(pixmap + 4, (row_bytes as u16) | 0x8000);
        self.bus.write_word(pixmap + 6, 0);
        self.bus.write_word(pixmap + 8, 0);
        self.bus.write_word(pixmap + 10, height);
        self.bus.write_word(pixmap + 12, width);
        // Indexed PixMaps use one component whose size equals pixelSize;
        // 16bpp direct PixMaps use three 5-bit RGB components. Imaging With
        // QuickDraw (1994), pp. 4-10--4-11.
        let (pixel_type, component_count, component_size, gdevice_type) = if depth <= 8 {
            (0, 1, depth, 0)
        } else {
            (16, 3, 5, 2)
        };
        self.bus.write_word(pixmap + 30, pixel_type);
        self.bus.write_word(pixmap + 32, depth);
        self.bus.write_word(pixmap + 34, component_count);
        self.bus.write_word(pixmap + 36, component_size);
        self.bus.write_long(pixmap + 42, ctab_handle);
        self.bus.write_word(gdevice + 4, gdevice_type);
        let mut gdevice_flags = self.bus.read_word(gdevice + 20);
        if depth == 1 {
            gdevice_flags &= !1;
        } else {
            gdevice_flags |= 1;
        }
        self.bus.write_word(gdevice + 20, gdevice_flags);
        self.bus.write_word(gdevice + 34, 0);
        self.bus.write_word(gdevice + 36, 0);
        self.bus.write_word(gdevice + 38, height);
        self.bus.write_word(gdevice + 40, width);
        let depth_mode = crate::display::classic_depth_mode(depth)
            .expect("validated PPC host depth has a classic Video.h mode");
        self.bus.write_long(gdevice + 42, u32::from(depth_mode));
    }

    fn advance_ticks_for_ppc_cycles(
        &mut self,
        cycles: u64,
        tick_cap: Option<u32>,
    ) -> PpcCycleAdvanceResult {
        let baseline_tick = self.guest_tick();
        let mut elapsed_ticks = 0u32;
        let mut remaining = cycles;
        while remaining > 0 {
            if self.tick_budget > 0 && remaining < self.tick_budget as u64 {
                self.tick_budget -= remaining as i32;
                break;
            }

            let consumed = self.tick_budget.max(1) as u64;
            remaining = remaining.saturating_sub(consumed);
            self.tick_budget = 0;
            if self.frozen_ticks.is_none() {
                if let Some(cap) = tick_cap {
                    if self.guest_tick() >= cap {
                        break;
                    }
                }
                self.advance_guest_tick();
                elapsed_ticks = elapsed_ticks.saturating_add(1);
            }
            self.tick_budget += self.instructions_per_tick as i32;
        }
        PpcCycleAdvanceResult {
            baseline_tick,
            elapsed_ticks,
        }
    }

    fn ppc_cycle_budget_for_tick_cap(&self, max_steps: usize, tick_cap: Option<u32>) -> usize {
        let Some(cap) = tick_cap else {
            return max_steps;
        };
        if self.frozen_ticks.is_some() || self.guest_tick() >= cap {
            return 0;
        }

        let mut budget = u64::try_from(max_steps).unwrap_or(u64::MAX);
        let mut ticks_remaining = u64::from(cap.wrapping_sub(self.guest_tick()));
        if ticks_remaining == 0 {
            return 0;
        }

        let current_tick_budget = self.tick_budget.max(1) as u64;
        budget = budget.max(current_tick_budget);
        ticks_remaining = ticks_remaining.saturating_sub(1);
        budget = budget.saturating_add(
            ticks_remaining.saturating_mul(u64::from(self.instructions_per_tick.max(1))),
        );
        usize::try_from(budget).unwrap_or(usize::MAX)
    }

    /// Run for a specific number of steps and mix the supplied amount of host audio.
    /// Returns the number of instructions executed and whether the CPU is still running.
    ///
    /// `tick_override`: If `Some(ticks)`, `Ticks` is capped to the supplied external
    /// wall-clock target. If `None`, `Ticks` advances from the runner's configured
    /// instruction cadence.
    pub fn run_steps_with_audio(
        &mut self,
        max_steps: usize,
        tick_override: Option<u32>,
        audio_samples: usize,
    ) -> (usize, bool) {
        self.run_steps_internal(
            max_steps,
            tick_override,
            audio_samples,
            tick_override.is_some(),
            false,
            FrameFinalization::Complete,
        )
    }

    /// Run a realtime GUI/WASM slice using the runner's internal tick cadence.
    /// The caller is responsible for converting wall-clock time into `max_steps`
    /// and `audio_samples`.
    pub fn run_realtime_steps_with_audio(
        &mut self,
        max_steps: usize,
        audio_samples: usize,
    ) -> (usize, bool) {
        self.run_steps_internal(
            max_steps,
            None,
            audio_samples,
            true,
            false,
            FrameFinalization::Complete,
        )
    }

    /// Run a GUI frame slice paced by wall-clock time.
    ///
    /// Wall-clock GUI pacing works differently from the reference runtime:
    /// in the reference runtime, ticks are driven purely by the instruction budget
    /// (deterministic, host-speed-independent). In the GUI, the user expects
    /// the game to run at real time regardless of how fast the emulator can
    /// execute instructions, so the caller computes a `deadline_tick` from
    /// host wall-clock time and we cap `$016A` advancement there. The CPU
    /// runs flat out (up to `max_steps`) until either the tick cap is hit
    /// or the instruction budget is exhausted, at which point the caller
    /// yields to the UI thread for rendering.
    /// Call [`Self::advance_menu_presentation_clock`] once per host frame with
    /// uncapped elapsed time; menu feedback must not inherit the CPU tick cap.
    pub fn run_gui_slice_with_audio(
        &mut self,
        max_steps: usize,
        deadline_tick: u32,
        audio_samples: usize,
    ) -> (usize, bool) {
        self.run_steps_internal(
            max_steps,
            Some(deadline_tick),
            audio_samples,
            true,
            false,
            FrameFinalization::Complete,
        )
    }

    /// Run a GUI CPU slice paced by wall-clock time without finalizing a host
    /// frame. Browser frontends use this to execute several small CPU batches
    /// and then redraw chrome / mix queued audio once for the outer frame.
    pub fn run_gui_cpu_slice(&mut self, max_steps: usize, deadline_tick: u32) -> (usize, bool) {
        self.run_steps_internal(
            max_steps,
            Some(deadline_tick),
            0,
            true,
            false,
            FrameFinalization::Deferred,
        )
    }

    /// Run pending Sound Manager interrupt work without advancing TickCount
    /// or continuing into foreground guest code after the callback returns.
    pub fn run_pending_sound_work(&mut self, max_steps: usize) -> (usize, bool) {
        if self.guest_work_is_suspended() {
            return (0, !self.halted);
        }
        self.fire_pending_ppc_sound_completions();
        self.run_steps_internal(max_steps, None, 0, true, true, FrameFinalization::Complete)
    }

    /// Service pending sound work like [`Self::run_pending_sound_work`], but
    /// leave native chrome to the frontend's next [`Self::composite_frame`].
    /// Sound queues, refilled double buffers and channel state are serviced at
    /// the same slice boundary. This does not defer audio or enlarge the guest
    /// callback budget. Use this only when the caller owns outer presentation.
    pub fn run_gui_pending_sound_work(&mut self, max_steps: usize) -> (usize, bool) {
        if self.guest_work_is_suspended() {
            return (0, !self.halted);
        }
        self.fire_pending_ppc_sound_completions();
        self.run_steps_internal(max_steps, None, 0, true, true, FrameFinalization::AudioOnly)
    }

    /// Run for a specific number of steps (for GUI/headless callers that don't
    /// provide a real wall-clock audio budget).
    ///
    /// Returns `(steps_executed, still_running)` — note that the bool is
    /// **`still_running`**, not `halted`. `false` means the CPU halted
    /// (via `ExitToShell`, an unimplemented opcode, or a memory fault).
    /// The per-halt detail (trap word, PC, SP, D0) is exposed via the
    /// [`halted_trap`](Self::halted_trap), [`halted_pc`](Self::halted_pc),
    /// [`halted_sp`](Self::halted_sp), [`halted_d0`](Self::halted_d0)
    /// accessors after this call returns.
    pub fn run_steps(&mut self, max_steps: usize, tick_override: Option<u32>) -> (usize, bool) {
        let start_tick = self.guest_tick();
        let result = self.run_steps_internal(
            max_steps,
            tick_override,
            0,
            false,
            false,
            FrameFinalization::Complete,
        );
        self.advance_headless_callback_audio(self.guest_tick().wrapping_sub(start_tick));
        result
    }

    /// Halt at the CPU's current position with the standard crash
    /// diagnostics (stop banner and trace dump). Shared by
    /// the batch-exit arms (TRAP #n / BKPT / illegal instruction) that
    /// previously funneled through `StepResult::Stopped`.
    fn halt_with_stop_diagnostics(&mut self, count: usize) {
        let halted_pc = self.m68k.cpu.read_reg(Register::PC);
        eprintln!(
            "[RUN_STEPS] CPU stopped at count={} pc=${:08X} op=${:04X}",
            count,
            halted_pc,
            self.bus.read_word(halted_pc)
        );
        self.halted = true;
        self.debug_note_terminal();
        self.halted_pc = Some(halted_pc);
        self.halted_sp = Some(self.m68k.cpu.read_reg(Register::A7));
        self.halted_d0 = Some(self.m68k.cpu.read_reg(Register::D0));
        self.dump_trace();
    }

    // Dialog Manager callbacks execute in the application's foreground, not at
    // interrupt time. TickCount must continue changing while they animate or
    // wait. Inside Macintosh I (1985), I-260 and I-415.
    fn callback_suspends_guest_clock(&self) -> bool {
        self.active_interrupt_callback.is_some_and(|callback| {
            !matches!(
                callback.source,
                ActiveInterruptCallbackSource::DialogDrawProc
                    | ActiveInterruptCallbackSource::DialogFilterProc
            )
        })
    }

    fn charge_tick_budget(&mut self, units: i32, tick_cap: Option<u32>) -> bool {
        if units <= 0 {
            return false;
        }
        if self.callback_suspends_guest_clock() {
            return false;
        }

        self.tick_budget -= units;
        while self.tick_budget <= 0 && self.frozen_ticks.is_none() {
            if let Some(cap) = tick_cap {
                if self.guest_tick() >= cap {
                    return true;
                }
            }
            self.advance_guest_tick();
            self.tick_budget += self.instructions_per_tick as i32;
            if self.callback_suspends_guest_clock() {
                return false;
            }
            if let Some(cap) = tick_cap {
                if self.guest_tick() >= cap {
                    return true;
                }
            }
        }
        false
    }

    fn advance_guest_tick(&mut self) -> u32 {
        // A same-tick cycle proof is invalid as soon as any ordinary clock
        // advancement or interrupt work occurs during the observed cycle.
        self.cancel_idle_cycle_detector();
        self.dispatcher
            .refresh_menu_bar_policy_from_guest(&self.bus);
        // `Ticks` is writable guest state. Resolve the shared semantic clock
        // from its current low-memory bytes before applying the machine's
        // next vertical-retrace advancement, so a direct guest mutation is
        // visible to both ABI entry points without creating an adapter copy.
        self.dispatcher.read_tick_count(&self.bus);
        let new_tick = self.dispatcher.advance_tick();
        // Low-memory Ticks ($016A) is the guest-visible process clock. It is
        // imported above at the VBL boundary and published again after the
        // canonical advancement. Inside Macintosh Volume I (1985), p. I-260.
        self.bus.write_long(0x016A, new_tick);
        let sys_evt_mask = self
            .bus
            .read_word(crate::memory::globals::addr::SYS_EVT_MASK);
        self.dispatcher.with_process_state(|d| {
            d.post_auto_key_if_due(sys_evt_mask);
        });

        // Sync MBState ($0172) from the internal button state.
        // On real hardware the VBL interrupt handler reads the ADB mouse
        // state and writes $0172 at each retrace. In our HLE, the button
        // state and the event queue are updated together by push_mouse_down;
        // keep $0172 at "pressed" while either the button is physically
        // held OR an unconsumed mouseDown is still pending in the queue
        // WITHOUT a later mouseUp pairing it off. This ensures code that
        // polls $0172 directly (rather than calling GetNextEvent) can
        // detect clicks injected before polling started, while a
        // mouse_up queued behind the mouse_down still flips MBState back
        // to 0x80 even when no GetNextEvent ever drains the queue —
        // critical for polling-only games (Bonkheads-Deluxe class titles)
        // that would otherwise see the button as "held forever".
        let has_pending_unmatched_down = self
            .dispatcher
            .with_process_state(|d| d.has_unmatched_queued_mouse_down());
        let pressed = self.dispatcher.input_state.mouse_button_pressed()
            || has_pending_unmatched_down;
        let mb_state: u8 = if pressed { 0x00 } else { 0x80 };
        self.bus.write_byte(0x0172, mb_state);
        if input_tick_trace_enabled() {
            eprintln!(
                "{}",
                format_input_tick_trace(
                    new_tick,
                    self.total_instructions,
                    self.ppc_input_snapshot(),
                    self.process_context.event_queue().len(),
                    mb_state,
                )
            );
        }

        // Advance the real-time clock ($020C) once per second.
        // On a real Mac the IOP or VIA increments Time every second;
        // we approximate this by incrementing every 60 ticks (~1 s at
        // 60.15 Hz VBL). Games that read $020C directly (e.g. for
        // PRNG seeding or save-file timestamps) need a changing value.
        // Inside Macintosh Volume II, II-378
        if new_tick.is_multiple_of(60) {
            let time = self.bus.read_long(0x020C);
            self.bus.write_long(0x020C, time.wrapping_add(1));
        }

        // Fire the cursor task and vertical-retrace tasks before Time Manager
        // tasks. Games commonly drive screen/audio housekeeping from VBL, so
        // letting those callbacks run first avoids starving them behind
        // unrelated timer traffic.
        self.fire_cursor_task();
        self.fire_vbl_tasks();
        self.fire_timer_tasks(new_tick);
        new_tick
    }

    fn deliver_pending_wait_next_event_if_available(&mut self) -> bool {
        let Some(pending) = self.dispatcher.pending_wait_next_event_return.take() else {
            if !self.process_context.event_queue().is_empty()
                || self.dispatcher.has_pending_native_menu_event()
            {
                self.dispatcher.pending_wait_sleep_ticks = 0;
                return true;
            }
            return false;
        };

        if let (Some(resume_pc), Some(resume_sp)) = (pending.resume_pc, pending.resume_sp) {
            let current_pc = self.m68k.cpu.read_reg(Register::PC);
            let current_sp = self.m68k.cpu.read_reg(Register::A7);
            if current_pc != resume_pc || current_sp != resume_sp {
                self.dispatcher.pending_wait_sleep_ticks = 0;
                if crate::trap::dispatch::trace_input_enabled() {
                    eprintln!(
                        "[INPUT] dropping stale WaitNextEvent sleep return parked pc=${:08X} sp=${:08X}; current pc=${:08X} sp=${:08X}",
                        resume_pc, resume_sp, current_pc, current_sp
                    );
                }
                return false;
            }
        }

        let (
            mut what,
            mut message,
            mut when,
            mut where_v,
            mut where_h,
            mut modifiers,
            mut has_event,
        ) = self.dispatcher.with_process_state(|dispatcher| {
            dispatcher.dequeue_toolbox_event(&mut self.m68k.cpu, &mut self.bus, pending.event_mask)
        });
        if !has_event {
            if let Some(event) = self.dispatcher.mouse_moved_event_for_region(
                &self.bus,
                pending.event_mask,
                pending.mouse_rgn,
            ) {
                what = event.what;
                message = event.message;
                when = event.when;
                where_v = event.where_v;
                where_h = event.where_h;
                modifiers = event.modifiers;
                has_event = true;
                self.dispatcher.debug_mouse_moved_event_count = self
                    .dispatcher
                    .debug_mouse_moved_event_count
                    .saturating_add(1);
            }
        }
        if !has_event {
            self.dispatcher.pending_wait_next_event_return = Some(pending);
            return false;
        }

        self.dispatcher.write_event_record(
            &mut self.bus,
            pending.event_ptr,
            what,
            message,
            when,
            where_v,
            where_h,
            modifiers,
        );
        self.bus.write_word(pending.result_ptr, 0xFFFF);
        self.dispatcher.pending_wait_sleep_ticks = 0;
        if crate::trap::dispatch::trace_input_enabled() {
            eprintln!(
                "[INPUT] WaitNextEvent sleep woke with input event what={} message=${:08X}",
                what, message
            );
        }
        true
    }

    fn wake_pending_wait_next_event_if_input_available(&mut self) -> bool {
        if self.active_interrupt_callback.is_some() {
            return false;
        }
        if self.dispatcher.pending_wait_sleep_ticks == 0
            || self.dispatcher.pending_wait_next_event_return.is_none()
        {
            return false;
        }
        // Event Manager sleep is interrupted as soon as a matching input event
        // is available; callers injecting input between run slices should not
        // have to wait for the next foreground CPU step to observe the wake.
        // Macintosh Toolbox Essentials 1992, p. 2-22.
        self.deliver_pending_wait_next_event_if_available()
    }

    fn wake_foreground_after_input(&mut self) {
        if self.tick_budget <= 0 {
            self.refill_foreground_budget_after_async_return();
        }
    }

    fn refill_foreground_budget_after_async_return(&mut self) {
        if self.tick_budget <= 0 {
            self.tick_budget = self.instructions_per_tick.max(2) as i32;
        }
    }

    fn service_wait_sleep_ticks(&mut self, tick_cap: Option<u32>) -> bool {
        if self.dispatcher.pending_wait_sleep_ticks == 0 || self.active_interrupt_callback.is_some()
        {
            return false;
        }

        if self.frozen_ticks.is_some() {
            self.dispatcher.pending_wait_sleep_ticks = 0;
            self.dispatcher.pending_wait_next_event_return = None;
            return false;
        }

        // On a real Mac, WaitNextEvent returns immediately when an event
        // is available, regardless of the requested sleep duration.
        // Macintosh Toolbox Essentials 1992, 2-22
        if self.deliver_pending_wait_next_event_if_available() {
            return false;
        }

        // If a dialog is being handled by ModalDialog, or a ModalDialog-owned
        // dialog is visibly retained between ModalDialog calls, treat
        // WaitNextEvent sleep as an app-yield hint rather than a wall-clock
        // delay. App-owned visible dialogs created with GetNewDialog can run
        // their own WaitNextEvent loops before ever entering ModalDialog; those
        // must still honor the requested sleep interval.
        let retained_modal_dialog_snapshot = self
            .dispatcher
            .dialog_visible_snapshots
            .keys()
            .any(|dialog_ptr| self.dispatcher.dialog_modal_entered.contains(dialog_ptr));
        let app_owned_visible_dialog_snapshot = self
            .dispatcher
            .dialog_visible_snapshots
            .keys()
            .any(|dialog_ptr| !self.dispatcher.dialog_modal_entered.contains(dialog_ptr));
        if tick_cap.is_some()
            && (self.dispatcher.is_dialog_tracking() || retained_modal_dialog_snapshot)
        {
            self.dispatcher.pending_wait_sleep_ticks = 0;
            self.dispatcher.pending_wait_next_event_return = None;
            return false;
        }

        // In GUI mode (tick_cap present), suspend foreground guest code until
        // either the requested WaitNextEvent sleep expires or this host frame's
        // tick cap is reached. The Process Manager makes the process eligible
        // to run again only after an event arrives or the sleep time expires;
        // if the time expires with no event pending, the app receives a null
        // event. Inside Macintosh: Processes 1994, p. 2-8.
        if let Some(cap) = tick_cap {
            while self.dispatcher.pending_wait_sleep_ticks > 0 && self.guest_tick() < cap {
                self.dispatcher.pending_wait_sleep_ticks -= 1;
                self.advance_guest_tick();
                self.tick_budget = self.instructions_per_tick as i32;
                if self.active_interrupt_callback.is_some() {
                    break;
                }
            }

            // Yield to the host if the frame tick cap was reached while the
            // process is still suspended; the next frame will continue draining
            // the remaining sleep without delivering another null event early.
            if self.dispatcher.pending_wait_sleep_ticks > 0 && self.guest_tick() >= cap {
                return true;
            }
            self.dispatcher.pending_wait_next_event_return = None;
            return false;
        }

        // Headless mode (no tick_cap from caller).
        //
        // Default: drain all pending sleep ticks at once (faster wall-clock).
        // Opt-in cap (set via `FixtureRunner::set_wait_sleep_cap_in_headless`):
        // honor the cap as a per-WNE-call ceiling, mirroring GUI mode's
        // 1-tick cap. Used by scripted harnesses to prevent Systemless's tick rate
        // from rocketing ahead of Basilisk's during event-loop-heavy
        // gameplay. App-owned visible dialogs keep the real sleep even when a
        // script sets the cap to zero; otherwise headless probes can run modal
        // background work that Basilisk is still sleeping through.
        if let Some(cap) = self.wait_sleep_cap_in_headless {
            let advance = if app_owned_visible_dialog_snapshot {
                self.dispatcher.pending_wait_sleep_ticks
            } else {
                self.dispatcher.pending_wait_sleep_ticks.min(cap)
            };
            self.dispatcher.pending_wait_sleep_ticks = 0;
            self.dispatcher.pending_wait_next_event_return = None;
            for _ in 0..advance {
                self.advance_guest_tick();
                self.tick_budget = self.instructions_per_tick as i32;
                if self.active_interrupt_callback.is_some() {
                    break;
                }
            }
            return false;
        }

        while self.dispatcher.pending_wait_sleep_ticks > 0 {
            self.dispatcher.pending_wait_sleep_ticks -= 1;
            self.advance_guest_tick();
            self.tick_budget = self.instructions_per_tick as i32;

            if self.active_interrupt_callback.is_some() {
                break;
            }
        }
        self.dispatcher.pending_wait_next_event_return = None;
        false
    }

    fn service_delay_ticks(&mut self, tick_cap: Option<u32>) -> bool {
        if self.dispatcher.pending_delay_ticks == 0 || self.callback_suspends_guest_clock() {
            return false;
        }

        if self.frozen_ticks.is_some() {
            self.dispatcher.pending_delay_ticks = 0;
            return false;
        }

        // Drain delay ticks one at a time, firing VBL/timer callbacks each tick.
        // In GUI mode with a tick_cap, yield if we reach the cap.
        while self.dispatcher.pending_delay_ticks > 0 {
            if let Some(cap) = tick_cap {
                if self.guest_tick() >= cap {
                    return true;
                }
            }
            self.dispatcher.pending_delay_ticks -= 1;
            self.advance_guest_tick();
            self.tick_budget = self.instructions_per_tick as i32;
            if self.callback_suspends_guest_clock() {
                break;
            }
        }

        if self.dispatcher.pending_delay_ticks == 0 {
            let final_ticks = self.guest_tick();
            self.m68k.cpu.write_reg(Register::D0, final_ticks);
        }

        false
    }

    fn service_gui_retained_idle_tick(&mut self, tick_cap: Option<u32>) -> bool {
        if self.active_interrupt_callback.is_some() || self.frozen_ticks.is_some() {
            return true;
        }

        if let Some(cap) = tick_cap {
            if self.guest_tick() >= cap {
                return true;
            }
        }

        self.advance_guest_tick();
        self.tick_budget = self.instructions_per_tick as i32;

        tick_cap.map(|cap| self.guest_tick() >= cap).unwrap_or(true)
    }

    fn unfreeze_ticks_to(&mut self, target_tick: Option<u32>) {
        self.frozen_ticks = None;
        if let Some(target_tick) = target_tick {
            self.bus.write_long(0x016A, target_tick);
            self.dispatcher.read_tick_count(&self.bus);
        }
    }

    fn install_cursor_task(&mut self) {
        use crate::memory::globals::addr;
        // Apple Technical Note DV520, "How the Macintosh mouse/cursor
        // mechanism works": an absolute warp sets MTemp = RawMouse and
        // CrsrNew = CrsrCouple. Cursor VBL maintenance publishes Mouse.
        // https://developer.apple.com/library/archive/technotes/dv/dv_520.html
        // Keep a real callable routine for direct calls and wrappers that
        // defer the original task (Inside Macintosh: Processes, 1994, 6-9).
        let words = [
            0x4A38,
            addr::CRSR_NEW as u16, // TST.B CrsrNew.W
            0x6710,                // BEQ.S done
            0x21F8,
            addr::M_TEMP as u16,
            addr::MOUSE_LOC as u16, // MOVE.L MTemp.W,RawMouse.W
            0x21F8,
            addr::M_TEMP as u16,
            addr::MOUSE_LOC2 as u16, // MOVE.L MTemp.W,Mouse.W
            0x4238,
            addr::CRSR_NEW as u16, // CLR.B CrsrNew.W
            0x4E75,                // done: RTS
        ];
        if self.default_cursor_task == 0 {
            self.default_cursor_task = self.bus.alloc_synthetic((words.len() * 2) as u32);
        }
        for (index, word) in words.into_iter().enumerate() {
            self.bus
                .write_word(self.default_cursor_task + index as u32 * 2, word);
        }
        self.bus
            .write_long(addr::J_CRSR_TASK, self.default_cursor_task);
        self.bus.write_byte(addr::CRSR_NEW, 0);
        self.bus.write_byte(addr::CRSR_COUPLE, 1);
    }

    fn sync_guest_mouse_position(&mut self, previous_mouse: u32) {
        let mouse = self.bus.read_long(crate::memory::globals::addr::MOUSE_LOC2);
        if mouse != previous_mouse {
            self.dispatcher
                .set_mouse_position((mouse >> 16) as i16, mouse as i16);
        }
    }

    /// Fire the low-memory cursor task vector, if an app has installed one.
    ///
    /// JCrsrTask runs from interrupt-time cursor/VBL maintenance. MPW
    /// Interfaces/AIncludes/LowMemEqu.a names the ProcPtr at $08EE.
    fn fire_cursor_task(&mut self) {
        if self.callback_suspends_guest_clock() {
            return;
        }
        if (self.m68k.cpu.core.get_sr() & 0x0700) >= 0x0100 {
            if trace_vbl_enabled() {
                eprintln!(
                    "[VBL] defer JCrsrTask masked sr=${:04X} pc=${:08X}",
                    self.m68k.cpu.core.get_sr(),
                    self.m68k.cpu.read_reg(Register::PC)
                );
            }
            return;
        }

        let callback_addr = self
            .bus
            .read_long(crate::memory::globals::addr::J_CRSR_TASK);
        if callback_addr == 0 {
            return;
        }
        if callback_addr == self.default_cursor_task {
            use crate::memory::globals::addr;
            // Same work as the callable routine, without injecting a guest
            // interrupt on every tick when the vector has not been patched.
            if self.bus.read_byte(addr::CRSR_NEW) != 0 {
                let previous_mouse = self.bus.read_long(addr::MOUSE_LOC2);
                let point = self.bus.read_long(addr::M_TEMP);
                self.bus.write_long(addr::MOUSE_LOC, point);
                self.bus.write_long(addr::MOUSE_LOC2, point);
                self.bus.write_byte(addr::CRSR_NEW, 0);
                self.sync_guest_mouse_position(previous_mouse);
            }
            return;
        }

        if self.cursor_task_trampoline == 0 {
            // JCrsrTask is a no-argument ProcPtr. Invoke it from interrupt
            // context and preserve the same volatile register set as VBL and
            // Time Manager callbacks. MPW Interfaces/AIncludes/LowMemEqu.a:
            // `JCrsrTask EQU $8EE`.
            let tramp = self.bus.alloc(16);
            self.bus.write_word(tramp, 0x48E7); // MOVEM.L D0-D3/A0-A3,-(SP)
            self.bus.write_word(tramp + 2, 0xF0F0);
            self.bus.write_word(tramp + 4, 0x4EB9); // JSR abs.L
                                                    // +6..+9: callback_addr (patched per-fire)
            self.bus.write_word(tramp + 10, 0x4CDF); // MOVEM.L (SP)+,D0-D3/A0-A3
            self.bus.write_word(tramp + 12, 0x0F0F);
            self.bus.write_word(tramp + 14, 0x4E75); // RTS
            self.cursor_task_trampoline = tramp;
        }

        let tramp = self.cursor_task_trampoline;
        self.bus.write_long(tramp + 6, callback_addr);
        if trace_vbl_enabled() {
            eprintln!(
                "[VBL] fire JCrsrTask addr=${:08X} interrupted_pc=${:08X} interrupted_sp=${:08X}",
                callback_addr,
                self.m68k.cpu.read_reg(Register::PC),
                self.m68k.cpu.read_reg(Register::A7)
            );
        }
        self.inject_interrupt_callback(ActiveInterruptCallbackSource::CursorTask, tramp);
    }

    /// Fire the next due Vertical Retrace Manager task.
    ///
    /// VBL tasks run at interrupt time with A0 pointing at the task record.
    /// Processes 1994, 4-6 to 4-7; executor src/time/vbl.cpp
    fn fire_vbl_tasks(&mut self) {
        if self.callback_suspends_guest_clock() {
            return;
        }
        if (self.m68k.cpu.core.get_sr() & 0x0700) >= 0x0100 {
            if trace_vbl_enabled() {
                eprintln!(
                    "[VBL] defer masked sr=${:04X} pc=${:08X}",
                    self.m68k.cpu.core.get_sr(),
                    self.m68k.cpu.read_reg(Register::PC)
                );
            }
            return;
        }

        // Decrement every queue element before choosing a callback. A task
        // earlier in the queue may run every retrace; stopping at that task
        // would starve all later elements. Preserve tasks that became due
        // while another callback was delivered and service those first on
        // the next opportunity.
        let vbl_tasks = self.dispatcher.vbl_tasks.shared_handle();
        let pending_before: Vec<bool> = vbl_tasks.iter().map(|task| task.pending).collect();
        let due_task = vbl_tasks.with_mut(|vbl_tasks| {
            for task in vbl_tasks
                .iter_mut()
                .filter(|task| task.architecture == CallbackTaskArchitecture::M68k)
            {
                let count = self.bus.read_word(task.task_ptr + 10) as i16;
                if count <= 0 {
                    continue;
                }
                let new_count = count - 1;
                self.bus.write_word(task.task_ptr + 10, new_count as u16);
                if new_count == 0 {
                    task.pending = true;
                }
            }

            let due_index = pending_before
                .iter()
                .enumerate()
                .position(|(index, pending)| {
                    *pending
                        && vbl_tasks[index].architecture == CallbackTaskArchitecture::M68k
                })
                .or_else(|| {
                    vbl_tasks.iter().position(|task| {
                        task.architecture == CallbackTaskArchitecture::M68k && task.pending
                    })
                });
            due_index.map(|index| {
                let task = &mut vbl_tasks[index];
                task.pending = false;
                task.task_ptr
            })
        });

        let Some(task_ptr) = due_task else {
            return;
        };

        let callback_addr = self.bus.read_long(task_ptr + 6);
        if callback_addr == 0 {
            return;
        }

        if self.vbl_trampoline == 0 {
            let tramp = self.bus.alloc_synthetic(22);
            self.bus.write_word(tramp, 0x48E7); // MOVEM.L D0-D3/A0-A3,-(SP)
            self.bus.write_word(tramp + 2, 0xF0F0);
            self.bus.write_word(tramp + 4, 0x207C); // MOVEA.L #imm,A0
            self.bus.write_word(tramp + 10, 0x4EB9); // JSR abs.L
            self.bus.write_word(tramp + 16, 0x4CDF); // MOVEM.L (SP)+,D0-D3/A0-A3
            self.bus.write_word(tramp + 18, 0x0F0F);
            self.bus.write_word(tramp + 20, 0x4E75); // RTS
            self.vbl_trampoline = tramp;
        }

        let tramp = self.vbl_trampoline;
        self.bus.write_long(tramp + 6, task_ptr);
        self.bus.write_long(tramp + 12, callback_addr);

        let current_pc = self.m68k.cpu.read_reg(Register::PC);
        let sp = self.m68k.cpu.read_reg(Register::A7);
        let d_regs = [
            self.m68k.cpu.read_reg(Register::D0),
            self.m68k.cpu.read_reg(Register::D1),
            self.m68k.cpu.read_reg(Register::D2),
            self.m68k.cpu.read_reg(Register::D3),
            self.m68k.cpu.read_reg(Register::D4),
            self.m68k.cpu.read_reg(Register::D5),
            self.m68k.cpu.read_reg(Register::D6),
            self.m68k.cpu.read_reg(Register::D7),
        ];
        let a_regs = [
            self.m68k.cpu.read_reg(Register::A0),
            self.m68k.cpu.read_reg(Register::A1),
            self.m68k.cpu.read_reg(Register::A2),
            self.m68k.cpu.read_reg(Register::A3),
            self.m68k.cpu.read_reg(Register::A4),
            self.m68k.cpu.read_reg(Register::A5),
            self.m68k.cpu.read_reg(Register::A6),
            sp,
        ];
        let ccr = self.m68k.cpu.core.get_ccr();
        let sr = self.m68k.cpu.core.get_sr();
        let new_sp = sp.wrapping_sub(4);
        self.bus.write_long(new_sp, current_pc);
        self.m68k.cpu.write_reg(Register::A7, new_sp);
        let source = ActiveInterruptCallbackSource::Vbl;
        self.suspend_dialog_callback_for_interrupt();
        self.active_interrupt_callback = Some(ActiveInterruptCallback {
            source,
            resume_pc: current_pc,
            resume_sp: sp,
            d_regs,
            a_regs,
            sr,
            ccr,
            restore_port: None,
        });
        self.m68k
            .cpu
            .core
            .set_sr_noint_nosp(interrupt_callback_sr(source, sr));
        self.m68k.cpu.write_reg(Register::PC, tramp);

        if trace_vbl_enabled() {
            eprintln!(
                "[VBL] fire task=${:08X} addr=${:08X} interrupted_pc=${:08X} interrupted_sp=${:08X} count={}",
                task_ptr,
                callback_addr,
                current_pc,
                sp,
                self.bus.read_word(task_ptr + 10) as i16
            );
        }
    }

    /// Fire any expired Time Manager tasks by injecting a call to their callback.
    ///
    /// On a real Mac, timer callbacks execute at interrupt time — the 68K hardware
    /// saves the entire CPU state (SR + PC + all registers via the exception frame)
    /// before dispatching the interrupt handler. The callback may freely clobber
    /// A0-A3 and D0-D3 (Processes 1994, 3-22).
    ///
    /// We simulate this by writing a small native 68K trampoline at a fixed
    /// low-memory address ($0110) that:
    ///   1. Saves D0-D3/A0-A3 via MOVEM.L to the stack
    ///   2. Loads A1 with the task record pointer (from inline data)
    ///   3. JSR's to the callback address (from inline data)
    ///   4. Restores D0-D3/A0-A3 via MOVEM.L from the stack
    ///   5. RTS back to the interrupted code
    fn fire_timer_tasks(&mut self, current_tick: u32) {
        self.fire_timer_tasks_at(current_tick as u64 * 1_000_000);
    }

    fn current_timer_subtick(&self) -> u64 {
        const SUBTICKS_PER_TICK: u64 = 1_000_000;
        let tick_base = self.guest_tick() as u64 * SUBTICKS_PER_TICK;
        if self.tick_budget <= 0 {
            return tick_base;
        }
        let instructions_per_tick = self.instructions_per_tick.max(1) as i64;
        let remaining = (self.tick_budget as i64).clamp(0, instructions_per_tick);
        let elapsed = instructions_per_tick - remaining;
        tick_base + (elapsed as u64 * SUBTICKS_PER_TICK) / instructions_per_tick as u64
    }

    fn fire_timer_tasks_at(&mut self, current_subtick: u64) {
        if self.callback_suspends_guest_clock() {
            return;
        }

        const SUBTICKS_PER_TICK: u64 = 1_000_000;
        let current_tick = (current_subtick / SUBTICKS_PER_TICK) as u32;
        // Fire at most one task at a time to avoid nested callbacks.
        let timer_tasks = self.dispatcher.timer_tasks.shared_handle();
        let due_task = timer_tasks.with_mut(|timer_tasks| {
            timer_tasks
                .iter_mut()
                .filter(|task| {
                    task.architecture == CallbackTaskArchitecture::M68k
                        && task.active
                        && current_subtick >= task.fire_at_subtick
                })
                .min_by_key(|task| task.fire_at_subtick)
                .map(|task| {
                    // Mark only the task being delivered as fired. Other tasks that
                    // expire on the same tick must remain active for a later interrupt.
                    task.active = false;
                    task.last_fired_tick = Some(current_tick);
                    (task.task_ptr, task.callback)
                })
        });
        if let Some((task_ptr, tm_addr)) = due_task {
            self.dispatcher
                .callback_scheduling
                .with_mut(|scheduling| scheduling.current_subtick = current_subtick);
            // The revised Time Manager clears the qType active bit when the
            // delay expires, before invoking tmAddr. A callback can therefore
            // observe that its task is inactive and safely PrimeTime it again.
            // Inside Macintosh: Processes (1994), pp. 3-6 and 3-20.
            let q_type = self.bus.read_word(task_ptr + 4);
            self.bus.write_word(task_ptr + 4, q_type & 0x7FFF);

            if tm_addr == 0 {
                return;
            }

            // Allocate trampoline code in guest heap on first use.
            // Layout (22 bytes):
            //   +0:  MOVEM.L D0-D3/A0-A3,-(SP)  ; 48E7 F0F0
            //   +4:  MOVEA.L #task_ptr,A1         ; 227C xxxx xxxx
            //   +10: JSR     tm_addr              ; 4EB9 xxxx xxxx
            //   +16: MOVEM.L (SP)+,D0-D3/A0-A3   ; 4CDF 0F0F
            //   +20: RTS                          ; 4E75
            if self.timer_trampoline == 0 {
                let tramp = self.bus.alloc(24); // 22 bytes + 2 padding
                self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
                self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
                self.bus.write_word(tramp + 4, 0x227C); // MOVEA.L #imm32,A1
                                                        // +6..+9: task_ptr (patched per-fire)
                self.bus.write_word(tramp + 10, 0x4EB9); // JSR abs.L
                                                         // +12..+15: tm_addr (patched per-fire)
                self.bus.write_word(tramp + 16, 0x4CDF); // MOVEM.L (SP)+,regs
                self.bus.write_word(tramp + 18, 0x0F0F); // D0-D3/A0-A3
                self.bus.write_word(tramp + 20, 0x4E75); // RTS
                self.timer_trampoline = tramp;
            }

            // Patch the inline data for this specific fire
            let tramp = self.timer_trampoline;
            self.bus.write_long(tramp + 6, task_ptr);
            self.bus.write_long(tramp + 12, tm_addr);

            // Snapshot the interrupted CPU state before mutating A7 for the
            // synthetic return address. The Time Manager callback should resume
            // with the guest stack exactly as it was when interrupted.
            let current_pc = self.m68k.cpu.read_reg(Register::PC);
            let sp = self.m68k.cpu.read_reg(Register::A7);
            let d_regs = [
                self.m68k.cpu.read_reg(Register::D0),
                self.m68k.cpu.read_reg(Register::D1),
                self.m68k.cpu.read_reg(Register::D2),
                self.m68k.cpu.read_reg(Register::D3),
                self.m68k.cpu.read_reg(Register::D4),
                self.m68k.cpu.read_reg(Register::D5),
                self.m68k.cpu.read_reg(Register::D6),
                self.m68k.cpu.read_reg(Register::D7),
            ];
            let a_regs = [
                self.m68k.cpu.read_reg(Register::A0),
                self.m68k.cpu.read_reg(Register::A1),
                self.m68k.cpu.read_reg(Register::A2),
                self.m68k.cpu.read_reg(Register::A3),
                self.m68k.cpu.read_reg(Register::A4),
                self.m68k.cpu.read_reg(Register::A5),
                self.m68k.cpu.read_reg(Register::A6),
                sp,
            ];
            let ccr = self.m68k.cpu.core.get_ccr();
            let sr = self.m68k.cpu.core.get_sr();

            // Inject: push current PC, jump to trampoline
            let new_sp = sp.wrapping_sub(4);
            self.bus.write_long(new_sp, current_pc);
            self.m68k.cpu.write_reg(Register::A7, new_sp);
            self.suspend_dialog_callback_for_interrupt();
            self.active_interrupt_callback = Some(ActiveInterruptCallback {
                source: ActiveInterruptCallbackSource::Timer,
                resume_pc: current_pc,
                resume_sp: sp,
                d_regs,
                a_regs,
                sr,
                ccr,
                restore_port: None,
            });
            if trace_timer_enabled() {
                eprintln!(
                    "[TIMER] fire task=${:08X} tm_addr=${:08X} interrupted_pc=${:08X} interrupted_sp=${:08X} ccr=${:02X}",
                    task_ptr, tm_addr, current_pc, sp, ccr
                );
            }
            self.m68k.cpu.write_reg(Register::PC, tramp);
        } else {
            self.dispatcher
                .callback_scheduling
                .with_mut(|scheduling| scheduling.current_subtick = current_subtick);
        }
    }

    /// Check all channels with active double-buffers: if a channel is not
    /// currently playing but its current_buffer is ready in guest memory,
    /// load the samples so mix_frame() can produce audio.
    fn try_load_pending_double_buffers(&mut self) {
        if self.callback_suspends_guest_clock() {
            return;
        }

        let queued_doublebacks = self
            .dispatcher
            .sound_manager
            .pending_callbacks
            .iter()
            .map(|cb| (cb.chan_ptr, cb.exhausted_buffer_index))
            .collect::<Vec<_>>();

        let sound_manager = self.dispatcher.sound_manager.shared_handle();
        sound_manager.with_mut(|manager| {
            for chan in &mut manager.channels {
                if chan.is_playing() {
                    continue; // already has data
                }
                let (header_ptr, buf_idx, sample_rate, num_channels, sample_size) =
                    match chan.double_buffer {
                        Some(ref db) if !db.last_buffer_seen => (
                            db.header_ptr,
                            db.current_buffer,
                            db.sample_rate,
                            db.num_channels,
                            db.sample_size,
                        ),
                        _ => continue,
                    };
                let mut load_idx = buf_idx;
                let mut buf_ptr = self.bus.read_long(header_ptr + 12 + (buf_idx as u32) * 4);
                let mut can_load = buf_ptr != 0
                    && self.bus.read_long(buf_ptr + 4) & 0x01 != 0
                    && !queued_doublebacks
                        .iter()
                        .any(|&(pending_chan, pending_idx)| {
                            pending_chan == chan.guest_ptr && pending_idx == buf_idx
                        });
                let original_idx = load_idx;
                if !can_load {
                    let other_idx = buf_idx ^ 1;
                    let other_ptr = self.bus.read_long(header_ptr + 12 + (other_idx as u32) * 4);
                    if other_ptr == 0 {
                        continue;
                    }
                    let other_flags = self.bus.read_long(other_ptr + 4);
                    let other_pending = queued_doublebacks.iter().any(
                        |&(pending_chan, pending_idx)| {
                            pending_chan == chan.guest_ptr && pending_idx == other_idx
                        },
                    );
                    if other_flags & 0x01 == 0 || other_pending {
                        continue; // neither available buffer is ready yet
                    }
                    load_idx = other_idx;
                    buf_ptr = other_ptr;
                    can_load = true;
                    if let Some(ref mut db) = chan.double_buffer {
                        db.current_buffer = other_idx;
                    }
                }
                if !can_load {
                    continue;
                }
                let flags = self.bus.read_long(buf_ptr + 4);
                if trace_sound_runner_enabled() {
                    let preview = self
                        .bus
                        .read_bytes(buf_ptr + 16, 16)
                        .iter()
                        .map(|byte| format!("{:02X}", byte))
                        .collect::<Vec<_>>()
                        .join(" ");
                    eprintln!(
                        "[SOUND-DB] load-ready chan=${:08X} header=${:08X} requested_idx={} load_idx={} buf=${:08X} frames={} flags=${:08X} pending={:?} first={}",
                        chan.guest_ptr,
                        header_ptr,
                        original_idx,
                        load_idx,
                        buf_ptr,
                        self.bus.read_long(buf_ptr),
                        flags,
                        chan.double_buffer
                            .as_ref()
                            .map(|db| db.pending_callback_buffers)
                            .unwrap_or([false; 2]),
                        preview
                    );
                }
                crate::trap::TrapDispatcher::load_double_buffer_samples(
                    &mut self.bus,
                    chan,
                    buf_ptr,
                    sample_rate,
                    num_channels,
                    sample_size,
                );
                if flags & 0x01 != 0 {
                    if let Some(ref mut db) = chan.double_buffer {
                        db.current_buffer = load_idx;
                        db.complete_callback_for(load_idx);
                    }
                }
            }
        });
    }

    fn dump_invalid_pc_state(&self) {
        let d_regs = [
            self.m68k.cpu.read_reg(Register::D0),
            self.m68k.cpu.read_reg(Register::D1),
            self.m68k.cpu.read_reg(Register::D2),
            self.m68k.cpu.read_reg(Register::D3),
            self.m68k.cpu.read_reg(Register::D4),
            self.m68k.cpu.read_reg(Register::D5),
            self.m68k.cpu.read_reg(Register::D6),
            self.m68k.cpu.read_reg(Register::D7),
        ];
        let a_regs = [
            self.m68k.cpu.read_reg(Register::A0),
            self.m68k.cpu.read_reg(Register::A1),
            self.m68k.cpu.read_reg(Register::A2),
            self.m68k.cpu.read_reg(Register::A3),
            self.m68k.cpu.read_reg(Register::A4),
            self.m68k.cpu.read_reg(Register::A5),
            self.m68k.cpu.read_reg(Register::A6),
            self.m68k.cpu.read_reg(Register::A7),
        ];
        eprintln!(
            "[RUN_STEPS]   D0-D7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
            d_regs[0], d_regs[1], d_regs[2], d_regs[3], d_regs[4], d_regs[5], d_regs[6], d_regs[7]
        );
        eprintln!(
            "[RUN_STEPS]   A0-A7: {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X}",
            a_regs[0], a_regs[1], a_regs[2], a_regs[3], a_regs[4], a_regs[5], a_regs[6], a_regs[7]
        );
        eprintln!("[RUN_STEPS]   CCR=${:02X}", self.m68k.cpu.core.get_ccr());
        if let Some(active) = self.active_interrupt_callback {
            eprintln!(
                "[RUN_STEPS]   active_callback={:?} resume_pc=${:08X} resume_sp=${:08X}",
                active.source, active.resume_pc, active.resume_sp
            );
        }
        self.bus.dump_stack(a_regs[7], "invalid PC");
    }

    fn suspend_dialog_callback_for_interrupt(&mut self) {
        debug_assert!(!self.callback_suspends_guest_clock());
        if let Some(callback) = self.active_interrupt_callback.take() {
            debug_assert!(self.suspended_dialog_callback.is_none());
            self.suspended_dialog_callback = Some(callback);
        }
    }

    fn inject_interrupt_callback(
        &mut self,
        source: ActiveInterruptCallbackSource,
        trampoline: u32,
    ) {
        self.suspend_dialog_callback_for_interrupt();
        let current_pc = self.m68k.cpu.read_reg(Register::PC);
        let sp = self.m68k.cpu.read_reg(Register::A7);
        let d_regs = [
            self.m68k.cpu.read_reg(Register::D0),
            self.m68k.cpu.read_reg(Register::D1),
            self.m68k.cpu.read_reg(Register::D2),
            self.m68k.cpu.read_reg(Register::D3),
            self.m68k.cpu.read_reg(Register::D4),
            self.m68k.cpu.read_reg(Register::D5),
            self.m68k.cpu.read_reg(Register::D6),
            self.m68k.cpu.read_reg(Register::D7),
        ];
        let a_regs = [
            self.m68k.cpu.read_reg(Register::A0),
            self.m68k.cpu.read_reg(Register::A1),
            self.m68k.cpu.read_reg(Register::A2),
            self.m68k.cpu.read_reg(Register::A3),
            self.m68k.cpu.read_reg(Register::A4),
            self.m68k.cpu.read_reg(Register::A5),
            self.m68k.cpu.read_reg(Register::A6),
            sp,
        ];
        let ccr = self.m68k.cpu.core.get_ccr();
        let sr = self.m68k.cpu.core.get_sr();
        let new_sp = sp.wrapping_sub(4);
        self.bus.write_long(new_sp, current_pc);
        self.m68k.cpu.write_reg(Register::A7, new_sp);
        self.active_interrupt_callback = Some(ActiveInterruptCallback {
            source,
            resume_pc: current_pc,
            resume_sp: sp,
            d_regs,
            a_regs,
            sr,
            ccr,
            restore_port: None,
        });
        self.m68k
            .cpu
            .core
            .set_sr_noint_nosp(interrupt_callback_sr(source, sr));
        self.m68k.cpu.write_reg(Register::PC, trampoline);
    }

    /// Publish and optionally deliver one completed asynchronous File Manager
    /// request.
    ///
    /// A File Manager completion procedure receives A0 pointing at the
    /// parameter block and D0 equal to its final `ioResult`.
    /// Inside Macintosh: Files (1992), 2-238.
    fn fire_file_completion_callback(&mut self) -> bool {
        if self.callback_suspends_guest_clock() {
            return false;
        }

        let Some(completion) = self.dispatcher.pending_file_completions.pop_front() else {
            return false;
        };

        self.bus
            .write_word(completion.parameter_block + 16, completion.result as u16);
        if completion.completion_addr == 0 {
            return false;
        }

        if self.file_completion_trampoline == 0 {
            let tramp = self.bus.alloc_synthetic(8);
            self.bus.write_word(tramp, 0x4EB9); // JSR abs.L
            self.bus.write_word(tramp + 6, 0x4E75); // RTS
            self.file_completion_trampoline = tramp;
        }

        let tramp = self.file_completion_trampoline;
        self.bus.write_long(tramp + 2, completion.completion_addr);
        self.inject_interrupt_callback(ActiveInterruptCallbackSource::FileCompletion, tramp);
        self.m68k
            .cpu
            .write_reg(Register::A0, completion.parameter_block);
        self.m68k
            .cpu
            .write_reg(Register::D0, completion.result as i32 as u32);
        true
    }

    /// Deliver one pending ADB Talk-register-0 packet to the service routine
    /// installed through SetADBInfo.
    ///
    /// A0 points to the Pascal-string packet, A1 to the service routine,
    /// A2 to its registered data area, and D0 contains the command byte.
    /// Inside Macintosh Volume V (1986), pp. V-367 to V-371.
    fn fire_adb_callback(&mut self) -> bool {
        if self.callback_suspends_guest_clock() {
            return false;
        }
        let Some(packet) = self.dispatcher.adb.pop_pending_packet() else {
            return false;
        };
        if packet.service_routine == 0 {
            return false;
        }

        if self.adb_callback_trampoline == 0 {
            let tramp = self.bus.alloc_synthetic(8);
            self.bus.write_word(tramp, 0x4EB9); // JSR abs.L
            self.bus.write_word(tramp + 6, 0x4E75); // RTS
            self.adb_callback_trampoline = tramp;
        }
        if self.adb_packet_buffer == 0 {
            self.adb_packet_buffer = self.bus.alloc_synthetic(4);
        }

        let tramp = self.adb_callback_trampoline;
        self.bus.write_long(tramp + 2, packet.service_routine);
        for (offset, byte) in packet.packet.into_iter().enumerate() {
            self.bus
                .write_byte(self.adb_packet_buffer + offset as u32, byte);
        }
        self.inject_interrupt_callback(ActiveInterruptCallbackSource::Adb, tramp);
        self.m68k
            .cpu
            .write_reg(Register::A0, self.adb_packet_buffer);
        self.m68k
            .cpu
            .write_reg(Register::A1, packet.service_routine);
        self.m68k.cpu.write_reg(Register::A2, packet.data_area);
        self.m68k
            .cpu
            .write_reg(Register::D0, u32::from(packet.command));
        true
    }

    /// Fire pending Sound Manager callback procedures and file completion routines.
    fn fire_sound_callbacks(&mut self) -> bool {
        if self.callback_suspends_guest_clock() {
            return false;
        }

        let Some(cb) = self.dispatcher.sound_manager.with_mut(|sound| {
            sound
                .pending_sound_callbacks
                .iter()
                .position(|callback| {
                    matches!(
                        callback,
                        crate::sound::PendingSoundCallback::Command {
                            architecture: CallbackTaskArchitecture::M68k,
                            ..
                        } | crate::sound::PendingSoundCallback::FileCompletion {
                            architecture: CallbackTaskArchitecture::M68k,
                            ..
                        }
                    )
                })
                .map(|index| sound.pending_sound_callbacks.remove(index))
        }) else {
            return false;
        };
        match cb {
            crate::sound::PendingSoundCallback::Command {
                architecture: _,
                callback_addr,
                chan_ptr,
                cmd,
            } => {
                if callback_addr == 0 {
                    return false;
                }

                // Sound 1994, 2-152
                if self.sound_callback_trampoline == 0 {
                    // Sound callback:
                    //   PROCEDURE MyCallBack(chan: SndChannelPtr; cmd: SndCommand);
                    //
                    // In practice shipped apps commonly receive `cmd` as a
                    // pointer-sized argument and differ on how much stack they
                    // pop on return. Push cmdPtr nearest SP and chan beneath it,
                    // then reset SP to the saved-register frame after JSR so
                    // one-arg, two-arg, and C-style cleanup all resume safely.
                    let tramp = self.bus.alloc_synthetic(42);
                    self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
                    self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
                    self.bus.write_word(tramp + 4, 0x2F3C); // MOVE.L #chan,-(SP)
                    self.bus.write_word(tramp + 10, 0x2F3C); // MOVE.L #cmdPtr,-(SP)
                    self.bus.write_word(tramp + 16, 0x4EB9); // JSR abs.L
                    self.bus.write_word(tramp + 22, 0x2E7C); // MOVEA.L #savedSP,A7
                    self.bus.write_word(tramp + 28, 0x4CDF); // MOVEM.L (SP)+,regs
                    self.bus.write_word(tramp + 30, 0x0F0F); // D0-D3/A0-A3
                    self.bus.write_word(tramp + 32, 0x4E75); // RTS
                    self.sound_callback_trampoline = tramp;
                }

                let tramp = self.sound_callback_trampoline;
                let cmd_ptr = tramp + 34;
                let interrupted_sp = self.m68k.cpu.read_reg(Register::A7);
                let saved_regs_sp = interrupted_sp.wrapping_sub(4 + 32);
                self.bus.write_long(tramp + 6, chan_ptr);
                self.bus.write_long(tramp + 12, cmd_ptr);
                self.bus.write_long(tramp + 18, callback_addr);
                self.bus.write_long(tramp + 24, saved_regs_sp);
                self.bus.write_word(cmd_ptr, cmd.cmd);
                self.bus.write_word(cmd_ptr + 2, cmd.param1 as u16);
                self.bus.write_long(cmd_ptr + 4, cmd.param2);
                self.inject_interrupt_callback(ActiveInterruptCallbackSource::SoundCallback, tramp);
                true
            }
            crate::sound::PendingSoundCallback::FileCompletion {
                architecture: _,
                callback_addr,
                chan_ptr,
            } => {
                if callback_addr == 0 {
                    return false;
                }

                // Sound 1994, 2-151
                if self.sound_file_completion_trampoline == 0 {
                    let tramp = self.bus.alloc_synthetic(28);
                    self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
                    self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
                    self.bus.write_word(tramp + 4, 0x2F3C); // MOVE.L #chan,-(SP)
                    self.bus.write_word(tramp + 10, 0x4EB9); // JSR abs.L
                    self.bus.write_word(tramp + 16, 0x2E7C); // MOVEA.L #savedSP,A7
                    self.bus.write_word(tramp + 22, 0x4CDF); // MOVEM.L (SP)+,regs
                    self.bus.write_word(tramp + 24, 0x0F0F); // D0-D3/A0-A3
                    self.bus.write_word(tramp + 26, 0x4E75); // RTS
                    self.sound_file_completion_trampoline = tramp;
                }

                let tramp = self.sound_file_completion_trampoline;
                let interrupted_sp = self.m68k.cpu.read_reg(Register::A7);
                let saved_regs_sp = interrupted_sp.wrapping_sub(4 + 32);
                self.bus.write_long(tramp + 6, chan_ptr);
                self.bus.write_long(tramp + 12, callback_addr);
                self.bus.write_long(tramp + 18, saved_regs_sp);
                self.inject_interrupt_callback(
                    ActiveInterruptCallbackSource::SoundFileCompletion,
                    tramp,
                );
                true
            }
        }
    }

    /// Fire pending SndPlayDoubleBuffer doubleback callbacks.
    ///
    /// When mix_frame() exhausts a double buffer, it queues a callback request.
    /// Here we clear dbBufferReady on the exhausted buffer and inject a
    /// trampoline to call the game's doubleback proc to refill it.
    ///
    /// The doubleback procedure signature (Sound 1994, 2-146):
    ///   PROCEDURE MyDoubleBackProc(chan: SndChannelPtr;
    ///                              exhaustedBuffer: SndDoubleBufferPtr);
    fn fire_sound_doubleback_callbacks(&mut self) -> bool {
        if self.callback_suspends_guest_clock() {
            return false;
        }

        // Take one callback at a time (like timer tasks).
        let Some(cb) = self.dispatcher.sound_manager.with_mut(|sound| {
            (!sound.pending_callbacks.is_empty()).then(|| sound.pending_callbacks.remove(0))
        }) else {
            return false;
        };

        // Read the exhausted buffer pointer from the header.
        // dbhBufferPtr[0] at header+12, dbhBufferPtr[1] at header+16
        let exhausted_buf_ptr = self
            .bus
            .read_long(cb.header_ptr + 12 + (cb.exhausted_buffer_index as u32) * 4);

        // Clear dbBufferReady on the exhausted buffer.
        if exhausted_buf_ptr != 0 {
            let flags = self.bus.read_long(exhausted_buf_ptr + 4);
            if trace_sound_runner_enabled() {
                let preview = self
                    .bus
                    .read_bytes(exhausted_buf_ptr + 16, 16)
                    .iter()
                    .map(|byte| format!("{:02X}", byte))
                    .collect::<Vec<_>>()
                    .join(" ");
                eprintln!(
                    "[SOUND-DB] fire-doubleback tick={} chan=${:08X} header=${:08X} idx={} buf=${:08X} frames={} flags_before=${:08X} callback=${:08X} sr=${:04X} first={}",
                    self.guest_tick(),
                    cb.chan_ptr,
                    cb.header_ptr,
                    cb.exhausted_buffer_index,
                    exhausted_buf_ptr,
                    self.bus.read_long(exhausted_buf_ptr),
                    flags,
                    cb.callback_addr,
                    self.m68k.cpu.core.get_sr(),
                    preview
                );
            }
            self.bus.write_long(exhausted_buf_ptr + 4, flags & !0x01);
        }

        if cb.callback_addr == 0 {
            return false;
        }

        // Allocate trampoline on first use.
        // The doubleback proc is a Pascal procedure (callee pops params):
        //   PROCEDURE MyDoubleBackProc(chan: SndChannelPtr;
        //                              exhaustedBuffer: SndDoubleBufferPtr);
        //
        // Trampoline layout (34 bytes):
        //   +0:  MOVEM.L D0-D3/A0-A3,-(SP)  ; 48E7 F0F0 (save regs)
        //   +4:  MOVE.L  #chanPtr,-(SP)       ; 2F3C xxxx xxxx (push param1 first)
        //   +10: MOVE.L  #exhaustedBuf,-(SP)  ; 2F3C xxxx xxxx (param2 nearest return)
        //   +16: JSR     callback             ; 4EB9 xxxx xxxx
        //   +22: MOVEA.L #savedRegsSP,A7      ; ignore guest callback cleanup convention
        //   +28: MOVEM.L (SP)+,D0-D3/A0-A3   ; 4CDF 0F0F (restore regs)
        //   +32: RTS                          ; 4E75
        if self.sound_doubleback_trampoline == 0 {
            let tramp = self.bus.alloc_synthetic(34);
            self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
            self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
            self.bus.write_word(tramp + 4, 0x2F3C); // MOVE.L #imm,-(SP)
                                                    // +6..+9: chan ptr (patched)
            self.bus.write_word(tramp + 10, 0x2F3C); // MOVE.L #imm,-(SP)
                                                     // +12..+15: exhausted buf ptr (patched)
            self.bus.write_word(tramp + 16, 0x4EB9); // JSR abs.L
                                                     // +18..+21: callback addr (patched)
            self.bus.write_word(tramp + 22, 0x2E7C); // MOVEA.L #savedSP,A7
                                                     // +24..+27: saved regs SP (patched)
            self.bus.write_word(tramp + 28, 0x4CDF); // MOVEM.L (SP)+,regs
            self.bus.write_word(tramp + 30, 0x0F0F); // D0-D3/A0-A3
            self.bus.write_word(tramp + 32, 0x4E75); // RTS
            self.sound_doubleback_trampoline = tramp;
        }

        let tramp = self.sound_doubleback_trampoline;
        let interrupted_sp = self.m68k.cpu.read_reg(Register::A7);
        let saved_regs_sp = interrupted_sp.wrapping_sub(4 + 32);
        // Classic Pascal pushes parameters left-to-right. At callback entry,
        // after JSR has stacked the return address, the exhausted buffer is at
        // SP+4 and chan is at SP+8. Sound 1994, 2-153.
        self.bus.write_long(tramp + 6, cb.chan_ptr);
        self.bus.write_long(tramp + 12, exhausted_buf_ptr);
        self.bus.write_long(tramp + 18, cb.callback_addr);
        self.bus.write_long(tramp + 24, saved_regs_sp);

        // Doubleback procedures execute at interrupt time, so the interrupted
        // guest CPU state must be restored after the callback unwinds.
        // Sound 1994, 2-72
        self.inject_interrupt_callback(ActiveInterruptCallbackSource::SoundDoubleBack, tramp);
        true
    }

    fn suspend_parent_dialog_call(&mut self) {
        let Some(callback) = self.active_interrupt_callback.take() else {
            return;
        };
        debug_assert!(matches!(
            callback.source,
            ActiveInterruptCallbackSource::DialogDrawProc
                | ActiveInterruptCallbackSource::DialogFilterProc
        ));
        self.nested_dialog_calls.push(SuspendedDialogCall {
            callback,
            scratch: (0..DIALOG_CALLBACK_SCRATCH_SIZE)
                .map(|offset| {
                    self.bus
                        .read_byte(self.dialog_callback_scratch_base + offset)
                })
                .collect(),
            addresses: [self.dialog_draw_trampoline, self.dialog_filter_trampoline],
            draw_port: self.dialog_draw_port_snapshot.take(),
            modeless_draw: self.dispatcher.active_modeless_dialog_draw_proc.take(),
        });
    }

    fn resume_parent_dialog_call(&mut self, filter_completed: bool) {
        let Some(parent) = self.nested_dialog_calls.pop() else {
            return;
        };
        let result = filter_completed.then(|| {
            self.bus
                .read_word(self.dispatcher.dialog_filter_result_addr)
        });
        self.bus
            .write_bytes(self.dialog_callback_scratch_base, &parent.scratch);
        if let Some(result) = result {
            self.bus
                .write_word(self.dispatcher.dialog_filter_result_addr, result);
        }
        [self.dialog_draw_trampoline, self.dialog_filter_trampoline] = parent.addresses;
        self.dialog_draw_port_snapshot = parent.draw_port;
        self.dispatcher.active_modeless_dialog_draw_proc = parent.modeless_draw;
        self.active_interrupt_callback = Some(parent.callback);
    }

    fn dialog_callback_scratch_base(&self) -> u32 {
        self.dialog_callback_scratch_base
    }

    fn looks_like_dialog_proc_entry(&self, addr: u32) -> bool {
        if addr == 0 {
            return false;
        }
        let entry = self.bus.read_word(addr);
        entry == 0x4E56 || entry == 0x48E7 || entry == 0x4EF9 || entry == 0x4EFA
    }

    fn resolve_dialog_draw_proc_addr(&self, proc_addr: u32) -> Option<u32> {
        if self.looks_like_dialog_proc_entry(proc_addr) {
            return Some(proc_addr);
        }
        let a5_relative = self.m68k.cpu.read_reg(Register::A5).wrapping_add(proc_addr);
        if self.looks_like_dialog_proc_entry(a5_relative) {
            Some(a5_relative)
        } else {
            None
        }
    }

    fn inject_dialog_draw_proc(
        &mut self,
        proc_addr: u32,
        item_no: i16,
        dialog_ptr: u32,
        modeless: bool,
    ) -> bool {
        if self.callback_suspends_guest_clock() {
            return false;
        }

        if proc_addr == 0 {
            return false;
        }

        // Many dialogs stuff non-code placeholders into userItem proc fields.
        // Only fire callbacks that look like real 68K entry points.
        let Some(call_addr) = self.resolve_dialog_draw_proc_addr(proc_addr) else {
            if trace_dialog_procs_enabled() {
                let a5_relative = self.m68k.cpu.read_reg(Register::A5).wrapping_add(proc_addr);
                eprintln!(
                    "[DIALOG-PROC] skip dialog=${:08X} item={} proc=${:08X} a5rel=${:08X} entry=${:04X} a5entry=${:04X}",
                    dialog_ptr,
                    item_no,
                    proc_addr,
                    a5_relative,
                    self.bus.read_word(proc_addr),
                    self.bus.read_word(a5_relative),
                );
            }
            return false;
        };

        // Allocate trampoline on first use (32 bytes):
        //   +0:  MOVEM.L D0-D3/A0-A3,-(SP)   ; 48E7 F0F0
        //   +4:  MOVE.L  #dialogPtr,-(SP)      ; 2F3C xxxx xxxx
        //   +10: MOVE.W  #itemNo,-(SP)         ; 3F3C xxxx
        //   +14: JSR     proc_addr              ; 4EB9 xxxx xxxx
        //   +20: MOVEA.L #savedRegsSP,A7       ; 4FF9 xxxx xxxx
        //   +26: MOVEM.L (SP)+,D0-D3/A0-A3    ; 4CDF 0F0F
        //   +30: RTS                            ; 4E75
        self.suspend_parent_dialog_call();

        if self.dialog_draw_trampoline == 0 {
            let tramp = self.dialog_callback_scratch_base() + DIALOG_DRAW_TRAMPOLINE_OFFSET;
            self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
            self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
            self.bus.write_word(tramp + 4, 0x2F3C); // MOVE.L #imm,-(SP)
                                                    // +6..+9: dialogPtr (patched per-fire)
            self.bus.write_word(tramp + 10, 0x3F3C); // MOVE.W #imm,-(SP)
                                                     // +12..+13: itemNo (patched per-fire)
            self.bus.write_word(tramp + 14, 0x4EB9); // JSR abs.L
                                                     // +16..+19: proc_addr (patched per-fire)
            self.bus.write_word(tramp + 20, 0x4FF9); // MOVEA.L #imm,A7
                                                     // +22..+25: savedRegsSP (patched per-fire)
            self.bus.write_word(tramp + 26, 0x4CDF); // MOVEM.L (SP)+,regs
            self.bus.write_word(tramp + 28, 0x0F0F); // D0-D3/A0-A3
            self.bus.write_word(tramp + 30, 0x4E75); // RTS
            self.dialog_draw_trampoline = tramp;
        }

        let tramp = self.dialog_draw_trampoline;
        self.bus.write_long(tramp + 6, dialog_ptr);
        self.bus.write_word(tramp + 12, item_no as u16);
        self.bus.write_long(tramp + 16, call_addr);

        // The Dialog Manager sets the current port to the dialog before a
        // userItem draw proc. Preserve the dialog's clean baseline around
        // application drawing so one item cannot contaminate later redraws.
        self.dialog_draw_port_snapshot = Some(self.dispatcher.prepare_dialog_user_item_port_state(
            &mut self.bus,
            &mut self.m68k.cpu,
            dialog_ptr,
        ));

        // Inject: push current PC, jump to trampoline
        let current_pc = self.m68k.cpu.read_reg(Register::PC);
        let sp = self.m68k.cpu.read_reg(Register::A7);
        let d_regs = [
            self.m68k.cpu.read_reg(Register::D0),
            self.m68k.cpu.read_reg(Register::D1),
            self.m68k.cpu.read_reg(Register::D2),
            self.m68k.cpu.read_reg(Register::D3),
            self.m68k.cpu.read_reg(Register::D4),
            self.m68k.cpu.read_reg(Register::D5),
            self.m68k.cpu.read_reg(Register::D6),
            self.m68k.cpu.read_reg(Register::D7),
        ];
        let a_regs = [
            self.m68k.cpu.read_reg(Register::A0),
            self.m68k.cpu.read_reg(Register::A1),
            self.m68k.cpu.read_reg(Register::A2),
            self.m68k.cpu.read_reg(Register::A3),
            self.m68k.cpu.read_reg(Register::A4),
            self.m68k.cpu.read_reg(Register::A5),
            self.m68k.cpu.read_reg(Register::A6),
            sp,
        ];
        let ccr = self.m68k.cpu.core.get_ccr();
        let sr = self.m68k.cpu.core.get_sr();
        let new_sp = sp.wrapping_sub(4);
        let saved_regs_sp = new_sp.wrapping_sub(32);
        self.bus.write_long(tramp + 22, saved_regs_sp);
        self.bus.write_long(new_sp, current_pc);
        self.m68k.cpu.write_reg(Register::A7, new_sp);
        self.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::DialogDrawProc,
            resume_pc: current_pc,
            resume_sp: sp,
            d_regs,
            a_regs,
            sr,
            ccr,
            restore_port: None,
        });
        if modeless {
            self.dispatcher.active_modeless_dialog_draw_proc = Some(dialog_ptr);
        }
        if trace_dialog_procs_enabled() {
            eprintln!(
                "[DIALOG-PROC] fire {} dialog=${:08X} item={} proc=${:08X} call=${:08X} return_pc=${:08X}",
                if modeless { "modeless" } else { "modal" },
                dialog_ptr,
                item_no,
                proc_addr,
                call_addr,
                current_pc,
            );
        }
        self.m68k.cpu.write_reg(Register::PC, tramp);
        true
    }

    fn fire_modeless_dialog_draw_proc(&mut self) -> bool {
        if self.callback_suspends_guest_clock() {
            return false;
        }

        while let Some((dialog_ptr, proc_addr, item_no)) =
            self.dispatcher.modeless_dialog_draw_proc_queue.pop_front()
        {
            if self.inject_dialog_draw_proc(proc_addr, item_no, dialog_ptr, true) {
                return true;
            }
        }
        while let Some(dialog_ptr) = self.dispatcher.modeless_dialog_cdef_draw_queue.pop_front() {
            if self.dispatcher.arm_dialog_control_def_draws(
                &mut self.m68k.cpu,
                &mut self.bus,
                dialog_ptr,
            ) {
                return true;
            }
        }
        false
    }

    /// Fire the next pending dialog userItem draw proc by injecting a trampoline.
    ///
    /// On a real Mac, ModalDialog calls each userItem's draw proc during
    /// the update pass. The draw proc is a Pascal callback:
    ///   PROCEDURE MyItem (theWindow: WindowPtr; itemNo: INTEGER);
    /// Inside Macintosh Volume I, I-405
    ///
    /// We simulate this by writing a small 68K trampoline that:
    ///   1. Saves D0-D3/A0-A3 via MOVEM.L to the stack
    ///   2. Pushes params so MPW-style Pascal prologues see itemNo at
    ///      8(A6) and theWindow at 10(A6), matching Pascal's stack layout
    ///   3. JSR to draw proc address
    ///   4. Resets A7 to the saved-register frame, tolerating callbacks
    ///      that return with either `RTD #6` or plain `RTS`
    ///   5. Restores D0-D3/A0-A3
    ///   6. RTS back to interrupted code (the ModalDialog A-line)
    fn fire_dialog_draw_procs(&mut self) -> bool {
        if self.callback_suspends_guest_clock() {
            return false;
        }

        if let Some(tracking) = self
            .dispatcher
            .dialog_tracking
            .as_mut()
            .filter(|tracking| !tracking.draw_procs_done)
        {
            let Some((proc_addr, item_no)) = tracking.draw_proc_queue.pop_front() else {
                // All draw procs fired and returned
                tracking.draw_procs_done = true;
                return false;
            };
            let dialog_ptr = tracking.dialog_ptr;
            return self.inject_dialog_draw_proc(proc_addr, item_no, dialog_ptr, false);
        }

        self.fire_modeless_dialog_draw_proc()
    }

    fn fire_menu_hook_proc(&mut self, opcode: u16) -> bool {
        if self.active_interrupt_callback.is_some() || (opcode & !0x0400) != 0xa93d {
            return false;
        }
        let Some(key) = self
            .dispatcher
            .menu_tracking
            .request_menu_hook(self.bus.read_byte(0x0172) == 0)
        else {
            return false;
        };
        let pointer = self.bus.read_long(0x0a30);
        let Some(procedure) = crate::guest_procedure::resolve_guest_procedure(
            &mut self.bus,
            pointer,
            0,
            None,
            GuestIsa::M68k,
            GuestIsa::M68k,
        ) else {
            return false;
        };
        if procedure.proc_info != 0 {
            return false;
        }
        let target = crate::guest_call::GuestCallTarget {
            isa: procedure.isa,
            entry: procedure.entry,
            rtoc: procedure.rtoc,
        };
        let sp = self.m68k.cpu.read_reg(Register::A7);
        let operation = crate::guest_call::MenuHookOperation::pending(key);
        if procedure.isa == GuestIsa::PowerPc {
            if !self
                .dispatcher
                .guest_calls
                .begin_m68k_to_powerpc_with_operation(
                    target,
                    crate::guest_call::PowerPcArguments::from_slice(&[]).unwrap(),
                    self.m68k.cpu.read_reg(Register::PC),
                    sp,
                    None,
                    crate::guest_call::ManagerContinuation::Menu(
                        crate::guest_call::MenuManagerContinuation::Hook(operation.clone()),
                    ),
                )
            {
                return false;
            }
            assert!(self
                .dispatcher
                .menu_tracking
                .bind_menu_hook(key, operation.completion.clone()));
            self.dispatcher.preserve_menu_callback_port(&self.bus);
            return true;
        }
        let Some(frame) = crate::execution_m68k::M68kMenuHookFrame::new(procedure.entry, sp) else {
            return false;
        };
        if !self.bus.is_guest_address_writable(frame.entry - 66, 114) {
            return false;
        }
        let return_pc = frame.entry + 28;
        if !self.dispatcher.guest_calls.begin_m68k_with_operation(
            target,
            return_pc,
            sp,
            Some(frame.entry),
            Some(crate::guest_call::ManagerContinuation::Menu(
                crate::guest_call::MenuManagerContinuation::Hook(operation.clone()),
            )),
        ) {
            return false;
        }
        assert!(self
            .dispatcher
            .menu_tracking
            .bind_menu_hook(key, operation.completion.clone()));
        self.dispatcher.preserve_menu_callback_port(&self.bus);
        self.bus.write_bytes(frame.entry, &frame.image);
        self.bus.write_long(frame.entry - 4, return_pc);
        self.m68k.cpu.write_reg(Register::A7, frame.entry - 4);
        self.m68k.cpu.write_reg(Register::PC, frame.entry);
        true
    }

    fn dialog_filter_has_real_event_pending(&self, dialog_ptr: u32) -> bool {
        self.process_context
            .event_queue()
            .iter()
            .any(|event| matches!(event.what, 1 | 2 | 3 | 4 | 6))
            || self
                .dispatcher
                .pending_update_event(&self.bus, 1u16 << 6)
                .is_some_and(|event| {
                    event.message == dialog_ptr
                        && !self.dialog_filter_update_event_already_sent_this_tick(
                            dialog_ptr,
                            event.message,
                        )
                })
    }

    fn dialog_filter_null_event_already_sent_this_tick(&self, dialog_ptr: u32) -> bool {
        self.dialog_filter_last_null_event_tick
            .is_some_and(|(sent_dialog, sent_tick)| {
                sent_dialog == dialog_ptr && sent_tick == self.guest_tick()
            })
    }

    fn dialog_filter_update_event_already_sent_this_tick(
        &self,
        dialog_ptr: u32,
        update_window: u32,
    ) -> bool {
        self.dialog_filter_last_update_event_tick.is_some_and(
            |(sent_dialog, sent_window, sent_tick)| {
                sent_dialog == dialog_ptr
                    && sent_window == update_window
                    && sent_tick == self.guest_tick()
            },
        )
    }

    fn should_fire_dialog_filter_proc(&self) -> bool {
        let Some(tracking) = self.dispatcher.dialog_tracking.as_ref() else {
            return false;
        };

        if tracking.filter_proc == 0
            || !tracking.draw_procs_done
            || tracking.last_filter_event.is_some()
        {
            return false;
        }

        let dialog_ptr = tracking.dialog_ptr;
        let has_real_event = self.dialog_filter_has_real_event_pending(dialog_ptr);

        // A queued mouseDown on a standard dialog item is still a real event:
        // ModalDialog passes it to the filter first, then handles it itself if
        // the filter returns FALSE. Only suppress idle/null callbacks while
        // the mouse is physically held over a dialog item. IM:I 1985 I-415.
        if !has_real_event
            && (self.dispatcher.mouse_down_over_dialog_button()
                || self.dispatcher.mouse_down_over_dialog_plain_user_item()
                || self.dispatcher.pending_dialog_plain_user_item_mouse_down())
        {
            return false;
        }

        // ModalDialog gets events through GetNextEvent and passes them to the
        // filter proc. A null event means there was no real event to dequeue;
        // pace those synthetic idle callbacks to one per guest tick so the HLE
        // refire loop does not manufacture hundreds of thousands of no-input
        // filter calls between VBLs. Mouse/key/update events still bypass this
        // gate and are delivered immediately. IM:I 1985 I-415; MTE 1992 6-136.
        if !has_real_event && self.dialog_filter_null_event_already_sent_this_tick(dialog_ptr) {
            return false;
        }

        true
    }

    /// Fire the ModalDialog filter proc for game-managed dialogs.
    ///
    /// On a real Mac, ModalDialog's internal loop calls GetNextEvent (consuming
    /// the event) and then passes it to the filter proc. If the filter returns
    /// TRUE, ModalDialog returns immediately with the itemHit value the filter
    /// wrote. If FALSE, ModalDialog processes the event itself.
    /// Inside Macintosh Volume I, I-415
    ///
    /// We simulate this by:
    /// 1. Consuming the next actionable event from our queue (like GetNextEvent)
    /// 2. Writing it to a scratch EventRecord in guest memory
    /// 3. Injecting a 68K trampoline that calls the filter proc with correct
    ///    Pascal calling convention (Boolean result space + 3 params)
    /// 4. The trampoline saves the Boolean return value to a scratch location
    ///    so the ModalDialog re-fire path can read it
    fn fire_dialog_filter_proc(&mut self) -> bool {
        if self.callback_suspends_guest_clock() {
            return false;
        }

        let (filter_proc, dialog_ptr, item_hit_ptr) = {
            let tracking = match self.dispatcher.dialog_tracking.as_ref() {
                Some(t) => t,
                None => return false,
            };
            (
                tracking.filter_proc,
                tracking.dialog_ptr,
                tracking.item_hit_ptr,
            )
        };

        if filter_proc == 0 {
            return false;
        }

        // Only fire the filter if the proc address contains recognisable 68K
        // function entry code. Some games pass a non-nil but invalid filterProc
        // (e.g. Marathon passes a stack address reused as a Rect buffer by
        // GetDItem, leaving it full of coordinate data, not instructions).
        // Executing garbage code would halt the CPU; skip the call instead.
        // Standard 68K function preambles: LINK A6 (0x4E56),
        //   MOVEM.L regs,-(SP) (0x48E7), JMP abs (0x4EF9), JMP PC+n (0x4EFA).
        // Inside Macintosh Volume I, I-415
        let entry = self.bus.read_word(filter_proc);
        if entry != 0x4E56 && entry != 0x48E7 && entry != 0x4EF9 && entry != 0x4EFA {
            if trace_dialog_filter_enabled() {
                eprintln!(
                    "[DIALOG-FILTER] skip invalid-entry dialog=${:08X} proc=${:08X} entry=${:04X}",
                    dialog_ptr, filter_proc, entry
                );
            }
            return false;
        }

        self.suspend_parent_dialog_call();

        // Allocate EventRecord scratch space on first use.
        // EventRecord = what(2), message(4), when(4), where(4), modifiers(2)
        if self.dialog_filter_event == 0 {
            self.dialog_filter_event =
                self.dialog_callback_scratch_base() + DIALOG_FILTER_EVENT_OFFSET;
        }
        let evt = self.dialog_filter_event;

        // Allocate the 2-byte Boolean result scratch on first use.
        if self.dispatcher.dialog_filter_result_addr == 0 {
            self.dispatcher.dialog_filter_result_addr =
                self.dialog_callback_scratch_base() + DIALOG_FILTER_RESULT_OFFSET;
        }
        let result_addr = self.dispatcher.dialog_filter_result_addr;

        // Clear the filter result before each invocation.
        self.bus.write_word(result_addr, 0);

        let ticks = self.guest_tick();

        // Consume the next actionable event from the queue, mirroring the real
        // Mac ModalDialog which calls GetNextEvent before invoking the filter.
        // Inside Macintosh Volume I, I-415
        let idx = self
            .process_context
            .event_queue()
            .iter()
            .position(|e| matches!(e.what, 1 | 2 | 3 | 4 | 6));
        let next_event = idx.map(|i| self.process_context.shared_event_queue().remove(i).unwrap());

        let filter_event = if let Some(e) = next_event {
            e
        } else if let Some(update_event) = self
            .dispatcher
            .pending_update_event(&self.bus, 1u16 << 6)
            .filter(|event| {
                event.message == dialog_ptr
                    && !self.dialog_filter_update_event_already_sent_this_tick(
                        dialog_ptr,
                        event.message,
                    )
            })
        {
            // `GetNextEvent` normally obtains updateEvt records from the
            // Window Manager's invalid region state. The queued-event path is
            // a one-shot approximation, but apps can flush that queued event
            // before entering a nested ModalDialog filter. If the active dialog
            // itself is still invalid, deliver that real pending update to the
            // filter rather than falling through to a null event. Restrict this
            // to the current dialog so unrelated behind-window invalid regions
            // cannot flood modal filters. Pace this synthetic update source to
            // once per guest tick: IM:I I-8433 says GetNextEvent returns the
            // next available event subject to priority rules, and IM:I I-9079
            // describes update events as generated from the Window Manager's
            // accumulated update region. Re-offering the same still-invalid
            // region in a tight ModalDialog filter loop can otherwise starve
            // queued user input.
            self.dialog_filter_last_update_event_tick =
                Some((dialog_ptr, update_event.message, ticks));
            update_event
        } else {
            // Modal filters are called on null events too; many apps render
            // their dialog content from this path (e.g., idle redraw).
            let (v, h) = self.dispatcher.mouse_position();
            crate::trap::dispatch::QueuedEvent {
                what: 0,
                message: 0,
                when: ticks,
                where_v: v,
                where_h: h,
                modifiers: self.dispatcher.current_event_modifiers(),
            }
        };
        if let Some(tracking) = self.dispatcher.dialog_tracking.as_mut() {
            tracking.last_filter_event = Some(filter_event.clone());
        }
        let what = filter_event.what;
        let message = filter_event.message;
        let when = filter_event.when;
        let where_v = filter_event.where_v;
        let where_h = filter_event.where_h;
        let modifiers = filter_event.modifiers;
        if what == 0 {
            self.dialog_filter_last_null_event_tick = Some((dialog_ptr, ticks));
        } else {
            self.dialog_filter_last_null_event_tick = None;
        }
        self.dispatcher.write_event_record(
            &mut self.bus,
            evt,
            what,
            message,
            when,
            where_v,
            where_h,
            modifiers,
        );
        if trace_dialog_filter_enabled() {
            eprintln!(
                "[DIALOG-FILTER] call dialog=${:08X} proc=${:08X} event=what:{} message=${:08X} where=({}, {}) mods=${:04X}",
                dialog_ptr, filter_proc, what, message, where_v, where_h, modifiers
            );
        }

        // Trampoline (48 bytes) with correct Pascal calling convention:
        //
        // FUNCTION MyFilter(theDialog: DialogPtr; VAR theEvent: EventRecord;
        //                   VAR itemHit: INTEGER): BOOLEAN;
        // Inside Macintosh Volume I, I-415
        //
        // Pascal convention: caller pushes 2-byte result space, then params
        // left-to-right. Callee pops params; result is left on stack.
        //
        //   +0:  MOVEM.L D0-D3/A0-A3,-(SP)     ; 48E7 F0F0
        //   +4:  CLR.W   -(SP)                   ; 4267 — Boolean result space
        //   +6:  MOVE.L  #dialogPtr,-(SP)         ; 2F3C xxxx xxxx
        //   +12: MOVE.L  #eventPtr,-(SP)          ; 2F3C xxxx xxxx
        //   +18: MOVE.L  #itemHitPtr,-(SP)        ; 2F3C xxxx xxxx
        //   +24: JSR     filter_proc              ; 4EB9 xxxx xxxx
        //        ; callee popped 12 bytes of params; SP → 2-byte Boolean result
        //   +30: MOVE.W  (SP),(result_addr).L     ; 33D7 xxxx xxxx
        //   +36: MOVEA.L #savedSP,A7              ; 2E7C xxxx xxxx
        //   +42: MOVEM.L (SP)+,D0-D3/A0-A3       ; 4CDF 0F0F
        //   +46: RTS                              ; 4E75
        if self.dialog_filter_trampoline == 0 {
            let tramp = self.dialog_callback_scratch_base() + DIALOG_FILTER_TRAMPOLINE_OFFSET;
            self.bus.write_word(tramp, 0x48E7); // MOVEM.L regs,-(SP)
            self.bus.write_word(tramp + 2, 0xF0F0); // D0-D3/A0-A3
            self.bus.write_word(tramp + 4, 0x4267); // CLR.W -(SP) — result space
            self.bus.write_word(tramp + 6, 0x2F3C); // MOVE.L #imm,-(SP)
                                                    // +8..+11: dialogPtr
            self.bus.write_word(tramp + 12, 0x2F3C); // MOVE.L #imm,-(SP)
                                                     // +14..+17: eventPtr
            self.bus.write_word(tramp + 18, 0x2F3C); // MOVE.L #imm,-(SP)
                                                     // +20..+23: itemHitPtr
            self.bus.write_word(tramp + 24, 0x4EB9); // JSR abs.L
                                                     // +26..+29: filter_proc
            self.bus.write_word(tramp + 30, 0x33D7); // MOVE.W (SP),(abs).L
                                                     // +32..+35: result_addr
            self.bus.write_word(tramp + 36, 0x2E7C); // MOVEA.L #imm,A7
                                                     // +38..+41: savedSP
            self.bus.write_word(tramp + 42, 0x4CDF); // MOVEM.L (SP)+,regs
            self.bus.write_word(tramp + 44, 0x0F0F); // D0-D3/A0-A3
            self.bus.write_word(tramp + 46, 0x4E75); // RTS
            self.dialog_filter_trampoline = tramp;
        }

        let tramp = self.dialog_filter_trampoline;
        self.bus.write_long(tramp + 8, dialog_ptr);
        self.bus.write_long(tramp + 14, evt);
        self.bus.write_long(tramp + 20, item_hit_ptr);
        self.bus.write_long(tramp + 26, filter_proc);
        self.bus.write_long(tramp + 32, result_addr);

        // ModalDialog handles events through DialogSelect, which selects the
        // dialog port before event handling. Leave that port current when the
        // filter returns so application follow-up drawing/invalidations target
        // the active dialog.
        self.dispatcher
            .set_current_port_state(&mut self.bus, &mut self.m68k.cpu, dialog_ptr, None);

        // Inject callback execution.
        let current_pc = self.m68k.cpu.read_reg(Register::PC);
        let sp = self.m68k.cpu.read_reg(Register::A7);
        let d_regs = [
            self.m68k.cpu.read_reg(Register::D0),
            self.m68k.cpu.read_reg(Register::D1),
            self.m68k.cpu.read_reg(Register::D2),
            self.m68k.cpu.read_reg(Register::D3),
            self.m68k.cpu.read_reg(Register::D4),
            self.m68k.cpu.read_reg(Register::D5),
            self.m68k.cpu.read_reg(Register::D6),
            self.m68k.cpu.read_reg(Register::D7),
        ];
        let a_regs = [
            self.m68k.cpu.read_reg(Register::A0),
            self.m68k.cpu.read_reg(Register::A1),
            self.m68k.cpu.read_reg(Register::A2),
            self.m68k.cpu.read_reg(Register::A3),
            self.m68k.cpu.read_reg(Register::A4),
            self.m68k.cpu.read_reg(Register::A5),
            self.m68k.cpu.read_reg(Register::A6),
            sp,
        ];
        let ccr = self.m68k.cpu.core.get_ccr();
        let sr = self.m68k.cpu.core.get_sr();
        let new_sp = sp.wrapping_sub(4);
        let saved_sp = new_sp.wrapping_sub(32); // SP after MOVEM save at trampoline entry

        // Zero the stack region the filter proc will use as local variables.
        //
        // On a real Mac, ModalDialog's internal event loop calls GetNextEvent
        // and DialogSelect between filter proc invocations, which naturally
        // overwrites the stack area with fresh data. In our HLE, the filter
        // proc is called directly without these intermediate calls, so stale
        // local variables from the previous invocation persist. This causes
        // bugs when the filter proc's code reads uninitialized locals that
        // happen to contain residual data (e.g., a stale Pascal string length
        // byte interpreted as a large count, overflowing a buffer).
        //
        // Clear 2KB below the filter proc's entry SP to simulate the stack
        // hygiene that ModalDialog's real event loop provides.
        let filter_entry_sp = saved_sp.wrapping_sub(50); // after MOVEM+params+JSR
        let clear_size: u32 = 2048;
        let clear_start = filter_entry_sp.wrapping_sub(clear_size);
        self.bus.fill_zeros(clear_start, clear_size);

        self.bus.write_long(tramp + 38, saved_sp);
        self.bus.write_long(new_sp, current_pc);
        self.m68k.cpu.write_reg(Register::A7, new_sp);
        self.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::DialogFilterProc,
            resume_pc: current_pc,
            resume_sp: sp,
            d_regs,
            a_regs,
            sr,
            ccr,
            restore_port: None,
        });
        self.m68k.cpu.write_reg(Register::PC, tramp);

        // Mark rendered_pixels stale while the filter proc is executing so
        // redraw_chrome skips restoration (which would erase the filter's
        // framebuffer output). After the filter returns and ModalDialog refires,
        // the re-snapshot path captures the filter's drawing into rendered_pixels.
        if let Some(tracking) = self.dispatcher.dialog_tracking.as_mut() {
            tracking.filter_presentation_epoch = tracking
                .rendered_pixels_final
                .then(|| self.bus.presentation_epoch())
                .flatten();
            tracking.rendered_pixels_final = false;
        }
        true
    }

    /// Run the 68k guest until it halts or [`FixtureRunnerConfig::max_instructions`]
    /// is reached. Returns:
    /// - `Ok(())` on a clean halt (`Stopped`, ExitToShell, or invalid PC).
    /// - `Err(Error::Halted)` is *not* returned here — halt-via-trap maps
    ///   to `Ok(())`. Trap dispatch errors (other than `Halted`) propagate.
    /// - [`Error::Timeout`] when the instruction count cap is reached
    ///   before any halt condition fires.
    ///
    /// Most embedders should prefer [`FixtureRunner::run_steps`], which
    /// gives you per-call budget control, returns whether the CPU is
    /// still running, and exposes per-halt detail via the
    /// [`halted_pc`](Self::halted_pc) / [`halted_trap`](Self::halted_trap)
    /// accessors.
    pub fn run(&mut self) -> Result<()> {
        if self.guest_work_is_suspended() {
            return Ok(());
        }
        let mut count = 0;

        if trace_load_enabled() {
            eprintln!("========================================");
            eprintln!("        FIXTURE RUNNER STARTING         ");
            eprintln!("========================================");

            eprintln!(
                "[RUN] Starting at PC=${:08X}, A5=${:08X}, A7=${:08X}",
                self.m68k.cpu.read_reg(Register::PC),
                self.m68k.cpu.read_reg(Register::A5),
                self.m68k.cpu.read_reg(Register::A7)
            );
        }

        while count < self.config.max_instructions {
            if self.m68k.cpu.is_stopped() {
                if trace_load_enabled() {
                    eprintln!(
                        "[RUN] Stopped after {} instructions, PC=${:08X}",
                        count,
                        self.m68k.cpu.read_reg(Register::PC)
                    );
                }
                return Ok(());
            }

            let pc = self.m68k.cpu.read_reg(Register::PC);

            // Safety Trigger: If PC jumps outside RAM or to Low Mem, stop immediately
            // Allow $60+ since CRT relocation installs trampolines in low memory
            let translated_pc = self.bus.translate_guest_address(pc);
            if translated_pc >= self.bus.ram_size() || (translated_pc < 0x60 && pc > 0) {
                eprintln!(
                    "[RUN] CRITICAL: PC jumped to invalid address ${:08X}! Halting trace.",
                    pc
                );
                self.dump_trace();
                return Ok(());
            }

            // Trace: Push current PC/Opcode/Regs (gated on env var).
            if trace_buffer_enabled() {
                let opcode = self.bus.read_word(pc);
                let a0 = self.m68k.cpu.read_reg(Register::A0);
                let sp = self.m68k.cpu.read_reg(Register::A7);
                let a6 = self.m68k.cpu.read_reg(Register::A6);
                let a5 = self.m68k.cpu.read_reg(Register::A5);
                if self.trace_buffer.len() >= 200 {
                    self.trace_buffer.pop_front();
                }
                self.trace_buffer.push_back((pc, opcode, a0, sp, a6, a5));
            }

            let step_result = self.m68k.cpu.step(&mut self.bus);
            self.dispatcher
                .retire_returned_native_trap_call(&mut self.m68k.cpu);
            match step_result {
                StepResult::Blocked => return Ok(()),
                StepResult::Ok => {}
                StepResult::Stopped => {
                    if trace_load_enabled() {
                        let stopped_pc = self.m68k.cpu.read_reg(Register::PC);
                        let opcode = self.bus.read_word(stopped_pc);
                        eprintln!(
                            "[RUN] Step returned Stopped after {} instructions, PC=${:08X}, Opcode=${:04X}",
                            count, stopped_pc, opcode
                        );
                    }
                    self.dump_trace();
                    return Ok(());
                }
                StepResult::Aline(opcode) => {
                    if !self.dispatcher.aline_vector_is_default(&self.bus) {
                        self.m68k.cpu.core.take_aline_exception(&mut self.bus);
                        count += 1;
                        continue;
                    }
                    let dispatch_result = self.dispatch_classic_with_process_services(opcode);
                    match dispatch_result {
                        Ok(()) => {
                            // Smart PC Advance:
                            // Only advance PC if the trap didn't change it
                            // (auto-pop traps set PC to return address)
                            let pc_after = self.m68k.cpu.read_reg(Register::PC);
                            if pc_after == pc {
                                self.m68k.cpu.write_reg(Register::PC, pc + 2);
                            }

                            // Log traps to stderr, but don't dump trace unless it's suspicious
                            // eprintln!("[RUN] Trap ${:04X} handled...", opcode);
                        }
                        Err(Error::Halted) => {
                            if trace_load_enabled() {
                                eprintln!("[RUN] Halted via trap after {} instructions", count);
                            }
                            self.dump_trace();
                            return Ok(());
                        }
                        Err(e) => {
                            self.dump_trace();
                            return Err(e);
                        }
                    }
                }
                StepResult::Fline(_opcode) => {
                    if !self.dispatcher.fline_vector_is_default(&self.bus) {
                        self.m68k.cpu.core.take_fline_exception(&mut self.bus);
                    }
                }
            }
            count += 1;
        }
        if trace_load_enabled() {
            eprintln!("[RUN] Timeout after {} instructions", count);
        }
        self.dump_trace();
        Err(Error::Timeout(count))
    }

    /// Print the last N executed instructions to stderr in PC/Op/Reg
    /// form. Used by halt paths in `run` / `run_steps_internal` to
    /// surface the run-up to a crash. Early-exits when the trace
    /// buffer is empty (the default — `SYSTEMLESS_TRACE_BUFFER=1`
    /// must be set to populate the buffer in the first place).
    pub fn dump_trace(&self) {
        if self.trace_buffer.is_empty() {
            return;
        }
        eprintln!(
            "[TRACE] Last {} executed instructions:",
            self.trace_buffer.len()
        );
        eprintln!("  PC        Op    A0       SP       A6       D0");
        for (pc, opcode, a0, sp, a6, d0) in &self.trace_buffer {
            eprintln!(
                "  {:08X}  {:04X}  {:08X} {:08X} {:08X} {:08X}",
                pc, opcode, a0, sp, a6, d0
            );
        }
    }
}

/// Dump the diagnostic histograms when the runner is dropped. Each
/// `print_*_histogram` already early-returns when its env-var gate
/// isn't set, so this is a no-op for normal runs (including tests).
/// Investigate interactive-mode behavior with
/// `SYSTEMLESS_TRACE_TRAP_COUNTS=1`, `SYSTEMLESS_TRACE_OPCODE_COUNTS=1`,
/// `SYSTEMLESS_TRACE_HOT_PC=1`, `SYSTEMLESS_TRACE_PPC_FETCH_COUNTS=1`, or
/// `SYSTEMLESS_PPC_IMPORT_HIST=1`, `SYSTEMLESS_PPC_UNIMPL_HIST=1`,
/// `SYSTEMLESS_PPC_GWORLD_DUMP=1`, or `SYSTEMLESS_TRACE_TRAP_TIMING=1`.
impl Drop for FixtureRunner {
    fn drop(&mut self) {
        self.dispatcher.print_trap_histogram(40);
        self.print_opcode_histogram(40);
        self.print_pc_histogram(40);
        self.print_ppc_fetch_histogram(40);
        self.print_ppc_import_histogram(usize::MAX);
        self.print_ppc_unimpl_histogram(40);
        self.print_ppc_gworld_dump(8);
        self.dispatcher.print_trap_timing_histogram(40);
    }
}

// =============================================================================
// Loader Implementation
// =============================================================================

fn apply_retro68_rela_relocations<M: MemoryBus>(
    bus: &mut M,
    target_base: u32,
    target_size: usize,
    rela: &[u8],
    displacements: [u32; 4],
) -> std::result::Result<usize, Retro68RelocationError> {
    let relocations = decode_retro68_relocations(rela, target_size)?;
    let addresses = relocations
        .iter()
        .map(|relocation| {
            target_base.checked_add(relocation.offset).ok_or(
                Retro68RelocationError::GuestAddressOverflow {
                    base: target_base,
                    offset: relocation.offset,
                },
            )
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;

    for (relocation, address) in relocations.iter().zip(addresses) {
        let mut value = bus
            .read_long(address)
            .wrapping_add(displacements[relocation.base_index]);
        if relocation.pc_relative {
            value = value.wrapping_sub(address);
        }
        bus.write_long(address, value);
    }

    Ok(relocations.len())
}

fn update_far_segment_runtime_addresses<M: MemoryBus>(
    bus: &mut M,
    segment_addr: u32,
    a5_base: u32,
) {
    bus.write_long(
        segment_addr + MpwFarSegmentHeader::CURRENT_A5_OFFSET,
        a5_base,
    );
    bus.write_long(
        segment_addr + MpwFarSegmentHeader::LOAD_ADDRESS_OFFSET,
        segment_addr,
    );
}

fn apply_mpw_far_segment_relocations<M: MemoryBus>(
    bus: &mut M,
    segment_id: i16,
    segment_addr: u32,
    data: &[u8],
    a5_base: u32,
) {
    let Some(header) = MpwFarSegmentHeader::parse(data) else {
        return;
    };

    let mut a5_relocation_count = 0usize;
    match header.a5_relocation_offsets(data) {
        Some(offsets) => {
            a5_relocation_count = offsets.len();
            for offset in offsets {
                let addr = segment_addr.wrapping_add(offset);
                let relocated = bus.read_long(addr).wrapping_add(a5_base);
                bus.write_long(addr, relocated);
            }
        }
        None => {
            if trace_load_enabled() {
                eprintln!(
                    "[LOAD] CODE {} has invalid MPW far A5 relocation data at ${:08X}",
                    segment_id, header.a5_relocation_data_offset
                );
            }
        }
    }

    let mut pc_relocation_count = 0usize;
    match header.pc_relocation_offsets(data) {
        Some(offsets) => {
            pc_relocation_count = offsets.len();
            for offset in offsets {
                let addr = segment_addr.wrapping_add(offset);
                // Intrasegment addresses are relative to the code following
                // the far header, unlike jump-table entry offsets.
                // Mac OS Runtime Architectures, 10-20, 10-24..10-25.
                let relocated = bus
                    .read_long(addr)
                    .wrapping_add(segment_addr)
                    .wrapping_add(MpwFarSegmentHeader::SIZE as u32);
                bus.write_long(addr, relocated);
            }
        }
        None => {
            if trace_load_enabled() {
                eprintln!(
                    "[LOAD] CODE {} has invalid MPW far PC relocation data at ${:08X}",
                    segment_id, header.pc_relocation_data_offset
                );
            }
        }
    }

    update_far_segment_runtime_addresses(bus, segment_addr, a5_base);

    if trace_load_enabled() {
        eprintln!(
            "[LOAD] Applied MPW far relocations for CODE {}: a5={}, pc={}",
            segment_id, a5_relocation_count, pc_relocation_count
        );
    }
}

fn load_app_generic<M: MemoryBus>(
    fork: &ResourceFork,
    bus: &mut M,
    configured_load_address: u32,
) -> Option<LoadedApp> {
    // 1. Load CODE 0 Header
    let code0 = fork.get_code(0)?;
    let header = Code0Header::parse(&code0.data)?;
    let size_resource = [0, -1].into_iter().find_map(|id| {
        fork.get(*b"SIZE", id)
            .and_then(|res| ApplicationSizeResource::parse(&res.data))
            .filter(|size| size.preferred_partition_size().is_some())
            .map(|size| (id, size))
    });
    let (size_resource_id, size_resource) = size_resource
        .map(|(id, size)| (Some(id), Some(size)))
        .unwrap_or((None, None));
    let load_address = load_address_for_size_partition(
        configured_load_address,
        &header,
        size_resource,
        bus.application_memory_limit(),
    );
    if trace_load_enabled() {
        eprintln!(
            "[LOAD] CODE 0 header: above_a5={}, below_a5={}, jt_size={}, jt_offset={}",
            header.above_a5, header.below_a5, header.jump_table_size, header.jump_table_offset
        );
        if load_address != configured_load_address {
            if let Some(size) = size_resource {
                eprintln!(
                    "[LOAD] Relocated load base for SIZE partition: configured=${:08X} effective=${:08X} preferred={} minimum={}",
                    configured_load_address,
                    load_address,
                    size.preferred_size,
                    size.minimum_size
                );
            }
        }
        if let (Some(id), Some(size)) = (size_resource_id, size_resource) {
            eprintln!(
                "[LOAD] SIZE {} flags=${:04X} highLevelEventAware={} preferred={} minimum={}",
                id,
                size.flags,
                size.is_high_level_event_aware(),
                size.preferred_size,
                size.minimum_size
            );
        }
    }

    let a5_base = load_address + header.below_a5;
    let mut all_codes = fork.get_all_code();
    all_codes.sort_by_key(|code| code.id);
    let is_retro68_multisegment = all_codes.iter().any(|code| {
        code.id != 0
            && matches!(
                CodeSegmentHeader::parse(&code.data),
                Some(CodeSegmentHeader::MpwFar)
            )
            && fork.get(*b"RELA", code.id).is_some()
    });
    // For classic Mac apps, above_a5 defines the space needed above A5.
    // However, some apps place QuickDraw globals at higher offsets (e.g., A5+39KB).
    // Add 48KB reserve to accommodate most classic apps.
    let globals_end = a5_base + header.above_a5 + APP_QD_GLOBALS_RESERVE;

    // Clear A5 world
    let globals_zero_end = globals_end + APP_LOADER_CLEAR_RESERVE;
    bus.fill_zeros(load_address, globals_zero_end.saturating_sub(load_address));
    bus.write_long(0x0904, a5_base); // CurrentA5
    bus.write_word(0x0934, header.jump_table_offset as u16); // CurJTOffset - Inside Macintosh Volume II, II-62
    bus.write_word(0x028E, 0x0000); // ROM85

    // Write RTS stubs at low-memory jump vectors that some runtimes
    // (Think C, CodeWarrior) call directly instead of via A-line traps.
    // On a real Mac, these contain ROM routine addresses. In our HLE,
    // we place RTS instructions so JSRs to these addresses return safely.
    // Only cover $0060-$00FF to avoid corrupting system globals in $0100+
    // (e.g., $012D is a debugger presence flag that must remain 0).
    // The CRT's relocation pass will populate the real runtime trampolines.
    for addr in (0x0060..0x0100).step_by(2) {
        bus.write_word(addr, 0x4E75); // RTS
    }

    // Populate the 68k exception vector table the way a booted Mac leaves
    // it. Vector 0 holds the initial interrupt stack pointer and vector 1 the
    // initial program counter; vectors 2-63 hold the handler addresses the
    // ROM installs during startup, nearly all of them inside the ROM image.
    // Inside Macintosh Volume I, I-103 (Exception Vector Table);
    // M68000PRM, section 6.2 ("Exception Vectors").
    //
    // Leaving the table zeroed is not a neutral choice. Applications that
    // dereference an uninitialised pointer read address $0000, and a zero
    // there turns a stray write into low-memory corruption — SimCity 2000's
    // splash-screen colour animator runs before its CTabHandle is set and
    // writes through `*(long *)0`, which lands on `Ticks` ($016A) and stops
    // the clock. On real hardware the same write lands in ROM and is
    // discarded, which is why the bug stays latent there. Addresses past the
    // end of RAM are dropped by the bus, so the values below reproduce that.
    //
    // Observed in System 7.5.3 running in BasiliskII with a Quadra 650 ROM:
    // four entries point to RAM-resident handlers and the rest point into the
    // ROM image at $4080xxxx.
    const BOOT_EXCEPTION_VECTORS: [u32; 64] = [
        0x40810000, 0x40810000, 0x0001EAD6, 0x0001EAD8, 0x0001EADA, 0x0001EADC, 0x408026F8,
        0x408026FA, 0x408026FC, 0x408026FE, 0x408099B0, 0x4088D9FE, 0x40802704, 0x40802704,
        0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704,
        0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x00083C16, 0x40809B40, 0x4080A1B0,
        0x00006436, 0x40809B00, 0x40809B00, 0x000737D6, 0x40802704, 0x40802704, 0x40802704,
        0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704,
        0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x4088D252,
        0x40802704, 0x40802704, 0x4088D856, 0x4088D28C, 0x4088D544, 0x4088D68E, 0x4088DAB0,
        0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704, 0x40802704,
        0x40802704,
    ];
    for (vector, &handler) in BOOT_EXCEPTION_VECTORS.iter().enumerate() {
        bus.write_long((vector as u32) * 4, handler);
    }

    // Install default RTE stubs for the "post-instruction" exception
    // vectors that real Mac OS would route to SysError. Because these
    // exceptions all stack the PC of the *next* instruction (per
    // M68000PRM, "Group 2 — internal" — vectors 5/6/7 advance PC past
    // the offending op before taking the trap), an RTE simply resumes
    // execution at the next instruction without re-entering the fault.
    // Inside Macintosh Volume I, I-103 (Exception Vector Table).
    //
    //   vector 5 ($14): Zero Divide  — DIVU/DIVS with src == 0
    //   vector 6 ($18): CHK          — bounds-check trap
    //   vector 7 ($1C): TRAPV        — programmed overflow trap
    //
    // Bus error (vector 2) and address error (vector 3) are deliberately
    // NOT installed: they stack PPC (the start of the faulting
    // instruction), so RTE-ing would re-execute it and loop forever.
    // Properly handling those requires a skip-the-instruction stub
    // which is a separate undertaking.
    bus.write_word(0x00FE, 0x4E73); // RTE
    bus.write_long(0x0014, 0x0000_00FE); // ZeroDivide vector
    bus.write_long(0x0018, 0x0000_00FE); // CHK vector
    bus.write_long(0x001C, 0x0000_00FE); // TRAPV vector

    // Load DATA 0 into A5 world (initialized globals)
    // DATA goes below A5 at address (A5 - below_a5) = load_address
    if let Some(data) = fork.get(*b"DATA", 0) {
        // DATA resource starts at offset 0 from load_address and fills up to A5
        let data_dest = load_address;
        if trace_load_enabled() {
            eprintln!(
                "[LOAD] Writing DATA 0 ({} bytes) to ${:08X}",
                data.data.len(),
                data_dest
            );
        }
        bus.write_bytes(data_dest, &data.data);

        if is_retro68_multisegment {
            if let Some(rela) = fork.get(*b"RELA", 0) {
                // Retro68 MultiSegApp.c relocates initialized globals with
                // {code, data, bss, jump table} displacements of
                // {0, A5, A5, A5}.
                match apply_retro68_rela_relocations(
                    bus,
                    data_dest,
                    data.data.len(),
                    &rela.data,
                    [0, a5_base, a5_base, a5_base],
                ) {
                    Ok(count) => {
                        if trace_load_enabled() {
                            eprintln!("[LOAD] Applied Retro68 RELA 0: count={count}");
                        }
                    }
                    Err(error) => {
                        tracing::warn!("invalid Retro68 RELA 0 resource: {error:?}");
                        return None;
                    }
                }
            }
        }
    }

    // 2. Parse Jump Table from CODE 0
    let mut jump_table = Vec::new();
    let jt_data = &code0.data[16..];

    for i in 0..header.num_entries() {
        let entry_offset = i * 8;
        if entry_offset + 8 > jt_data.len() {
            break;
        }

        let word_2_3 = u16::from_be_bytes([jt_data[entry_offset + 2], jt_data[entry_offset + 3]]);
        let (offset, segment) = if word_2_3 == 0xA9F0 {
            // FAR format: segment.w, LoadSeg.w, offset.l.
            let seg = i16::from_be_bytes([jt_data[entry_offset], jt_data[entry_offset + 1]]);
            let off = u32::from_be_bytes([
                jt_data[entry_offset + 4],
                jt_data[entry_offset + 5],
                jt_data[entry_offset + 6],
                jt_data[entry_offset + 7],
            ]);
            (off, seg)
        } else if word_2_3 == 0xFFFF {
            // NULL
            (0u32, 0i16)
        } else {
            // NEAR Format
            let off = u16::from_be_bytes([jt_data[entry_offset], jt_data[entry_offset + 1]]);
            let seg = i16::from_be_bytes([jt_data[entry_offset + 4], jt_data[entry_offset + 5]]);
            (u32::from(off), seg)
        };

        jump_table.push(JumpTableEntry {
            offset,
            segment,
            loaded: false,
            address: 0,
        });
        if trace_load_enabled() {
            eprintln!(
                "[LOAD] Parsed JT[{}]: segment={}, offset=0x{:04X}, raw=[{:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}]",
                i, segment, offset,
                jt_data[entry_offset], jt_data[entry_offset+1],
                jt_data[entry_offset+2], jt_data[entry_offset+3],
                jt_data[entry_offset+4], jt_data[entry_offset+5],
                jt_data[entry_offset+6], jt_data[entry_offset+7]
            );
        }
    }

    // 3. Setup Layout
    let code0_base = globals_end;
    let code0_size = code0.data.len() as u32;
    let code0_user = code0_base + 4;
    let jt_base = a5_base + header.jump_table_offset;
    if trace_load_enabled() {
        eprintln!("[LOAD] Memory layout: a5_base=${:08X}, globals_end=${:08X}, code0_user=${:08X}, code0_size={}, jt_base=${:08X} (jt_offset={})",
                  a5_base, globals_end, code0_user, code0_size, jt_base, header.jump_table_offset);
    }

    // 4. Load CODE 0 (Resident)
    bus.write_long(code0_base, code0_size);
    bus.write_bytes(code0_user, &code0.data);

    // Copy the JT data from CODE 0 to the actual JT area at A5+jt_offset.
    // On a real Mac, the system writes CODE 0's JT content (bytes after the
    // 16-byte header) to A5+CurJTOffset. This populates the initial JT
    // entries with unloaded-format stubs (offset, MOVE.W #seg, _LoadSeg).
    // Inside Macintosh Volume II, II-60
    let jt_content = &code0.data[16..];
    if !jt_content.is_empty() {
        bus.write_bytes(jt_base, jt_content);
    }

    // 5. Load all other CODE resources
    let mut segment_bases = HashMap::new();
    segment_bases.insert(0, code0_user);
    crate::trap::dispatch::record_segment_base(0, code0_user);

    // Load CODE segments into memory but do NOT pre-patch jump table entries.
    // Think C / CodeWarrior apps populate the JT at runtime via their startup
    // code (crt0). Our LoadSeg trap handler patches entries on demand when
    // segments are first called, matching real Mac Segment Loader behavior.
    // Inside Macintosh Volume II, II-60; Executor segment.cpp
    //
    // Reserve space: scan CODE headers to find max JT extent so CODE segments
    // are placed above the JT area.
    let mut max_jt_end: u32 = jt_base + (jump_table.len() as u32 * 8);
    for code_res in &all_codes {
        if code_res.id == 0 || code_res.data.len() < 4 {
            continue;
        }
        let Some(segment_header) = CodeSegmentHeader::parse(&code_res.data) else {
            continue;
        };
        let Some(tab_off) = segment_header.jump_table_start_offset() else {
            continue;
        };
        let Some(n_entries) = segment_header.jump_table_entry_count() else {
            continue;
        };
        let end = jt_base + tab_off + n_entries * 8;
        if end > max_jt_end {
            max_jt_end = end;
        }

        // Pre-populate unloaded-JT-entry stubs for entries this segment
        // owns that are still ALL ZERO (i.e. not yet populated by CODE 0's
        // jt_content write). Per Inside Macintosh Volume II, II-60, an
        // unloaded entry is `offset(2) + \$3F3C(2) + seg(2) + \$A9F0(2)`
        // — JSR-ing to entry+2 fires LoadSeg via the trap. Real System 7
        // writes these stubs at app-launch time; without them, segments
        // not represented in CODE 0's jt_content stay zeroed, so a
        // guest JSR-through-JT walks zeros (or falls into the next
        // patched entry) and faults (Centaurian 1.2.1 hits this).
        //
        // Skip entries with non-zero content — they've already been
        // initialised by CODE 0's load (as stubs) or pre-patched as
        // loaded JMP.L. Stomping either of those would break MPW-style
        // fixtures where CODE 0 carries the canonical layout.
        for i in 0..n_entries {
            let entry = jt_base + tab_off + i * 8;
            let is_empty = bus.read_long(entry) == 0 && bus.read_long(entry + 4) == 0;
            let is_null_placeholder = bus.read_word(entry + 2) == 0xFFFF;
            if !is_empty && !is_null_placeholder {
                continue;
            }
            if is_null_placeholder {
                // Some near-model apps leave segment-owned CODE 0 entries as
                // `offset, FFFF, FFFF, FFFF` placeholders and call the slot at
                // entry+0. Materialize a Think-style unload stub so that first
                // call enters LoadSeg instead of executing the placeholder.
                let routine_offset = bus.read_word(entry);
                bus.write_word(entry, 0xA9F0);
                bus.write_word(entry + 2, 0);
                bus.write_word(entry + 4, routine_offset);
                bus.write_word(entry + 6, code_res.id as u16);
            } else {
                bus.write_word(entry, 0);
                bus.write_word(entry + 2, 0x3F3C);
                bus.write_word(entry + 4, code_res.id as u16);
                bus.write_word(entry + 6, 0xA9F0);
            }
        }
    }

    let reserved_boundary = std::cmp::max(code0_user + code0_size, max_jt_end);
    let mut current_load_ptr = (reserved_boundary + 4) & !3;

    for code_res in all_codes {
        if code_res.id == 0 {
            continue;
        }

        let size = code_res.data.len() as u32;
        let phys_addr = current_load_ptr;
        let user_addr = current_load_ptr + 4;

        // Dump segment header info
        let segment_header = CodeSegmentHeader::parse(&code_res.data);
        let hdr_info = match segment_header {
            Some(CodeSegmentHeader::MpwFar) => "mpw-far-model".to_string(),
            Some(CodeSegmentHeader::Near {
                table_offset,
                entry_count,
            }) => format!("near-model taboff={} n={}", table_offset, entry_count),
            Some(CodeSegmentHeader::ThinkFar {
                has_relocations,
                first_entry_index,
                entry_count,
            }) => format!(
                "think-far-model first_jt={} n={} relocs={}",
                first_entry_index, entry_count, has_relocations
            ),
            None => "unknown".to_string(),
        };
        if trace_load_enabled() {
            eprintln!(
                "[LOAD] Loading CODE {} ({} bytes) to ${:08X} [{}]",
                code_res.id, size, user_addr, hdr_info
            );
        }

        bus.write_long(phys_addr, size);
        bus.write_bytes(user_addr, &code_res.data);

        segment_bases.insert(code_res.id, user_addr);
        crate::trap::dispatch::record_segment_base(code_res.id, user_addr);

        // Only patch JT for CODE 0's entries (far-model segments from the
        // original CODE 0 parse). Near-model segments get their JT entries
        // populated by the app's startup code and patched by LoadSeg.
        if matches!(segment_header, Some(CodeSegmentHeader::MpwFar)) {
            if let Some(rela) = fork.get(*b"RELA", code_res.id) {
                // Retro68 MultiSegApp.c relocates the body after the 40-byte
                // far header with {segment, A5, A5, A5} displacements.
                let target_base = match user_addr.checked_add(MpwFarSegmentHeader::SIZE as u32) {
                    Some(address) => address,
                    None => {
                        tracing::warn!(
                            "Retro68 CODE {} relocation target address overflowed",
                            code_res.id
                        );
                        return None;
                    }
                };
                let target_size = code_res.data.len() - MpwFarSegmentHeader::SIZE;
                match apply_retro68_rela_relocations(
                    bus,
                    target_base,
                    target_size,
                    &rela.data,
                    [user_addr, a5_base, a5_base, a5_base],
                ) {
                    Ok(count) => {
                        update_far_segment_runtime_addresses(bus, user_addr, a5_base);
                        if trace_load_enabled() {
                            eprintln!("[LOAD] Applied Retro68 RELA {}: count={count}", code_res.id);
                        }
                    }
                    Err(error) => {
                        tracing::warn!("invalid Retro68 RELA {} resource: {error:?}", code_res.id);
                        return None;
                    }
                }
            } else {
                apply_mpw_far_segment_relocations(
                    bus,
                    code_res.id,
                    user_addr,
                    &code_res.data,
                    a5_base,
                );
            }

            for (i, entry) in jump_table.iter_mut().enumerate() {
                if entry.segment == code_res.id {
                    entry.loaded = true;
                    // MPW far-model jump-table offsets are measured from the
                    // beginning of the CODE segment. The first externally
                    // callable routine can therefore sit at offset $28, just
                    // after the 40-byte far header.
                    entry.address = user_addr + entry.offset;

                    let jt_addr = jt_base + (i as u32 * 8);
                    bus.write_word(jt_addr, code_res.id as u16);
                    bus.write_word(jt_addr + 2, 0x4EF9); // JMP
                    bus.write_long(jt_addr + 4, entry.address);
                    if trace_load_enabled() {
                        eprintln!(
                            "[LOAD] JT[{}] -> CODE {} @ ${:08X} (far-model, off=${:04X})",
                            i, code_res.id, entry.address, entry.offset
                        );
                    }
                }
            }
        }

        current_load_ptr = (user_addr + size + 4 + 3) & !3;
    }

    let loaded_image_end = align4(globals_zero_end.max(current_load_ptr));
    if trace_load_enabled() {
        eprintln!("[LOAD] Loaded image end=${:08X}", loaded_image_end);
    }

    // Keep the application stack below Systemless-owned callback code and
    // the framebuffer reservation at the top of RAM.
    let stack_top = bus.application_memory_limit() - 16;

    Some(LoadedApp {
        ppc: None,
        code0_header: header,
        a5_base,
        jump_table,
        segment_bases,
        loaded_image_end,
        initial_sp: stack_top,
        size_resource,
    })
}

#[cfg(test)]
mod tests;
