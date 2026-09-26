//! Trap Dispatcher - routes Mac OS traps to per-manager handler modules.
//!
//! The TrapDispatcher struct holds all emulator state. Each sub-module adds
//! `impl TrapDispatcher` blocks with `dispatch_*` methods that return
//! `Option<Result<()>>` — `Some` if the trap was handled, `None` to pass through.

use crate::memory::SavedPixels;
pub(crate) use super::gateways::TrapTableProfile;
#[cfg(test)]
use super::gateways::{M68K_68040_COME_FROM_TRAPS, POWERPC_604_COME_FROM_TRAPS};
pub(crate) use super::manager::{
    raw_trap_route, OsRoutineVariant, OS_TRAP_TABLE_BASE, OS_TRAP_TABLE_SLOTS,
    TOOLBOX_TRAP_TABLE_BASE, TOOLBOX_TRAP_TABLE_SLOTS,
};
#[cfg(test)]
use super::manager::{resolve_trap_table_target, TrapTableTarget, COME_FROM_PATCH_SIGNATURE};
use super::manager::{
    TrapManager, TrapManagerMemoryOp, TrapManagerMemoryResult, TrapManagerSetError, TrapTableKind,
};
use super::types::UnderlineInfo;
use crate::cpu::{CpuOps, Register};
use crate::display::CursorImage;
use crate::guest_call::SharedGuestCallStack;
use crate::list_manager::ProcessListRecord;
use crate::machine_profile::reference_machine_profile;
use crate::managers::resource::ResourceFork;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::menu_manager::SharedNativeMenuSelection;
use crate::process_context::{
    MigratedProcessHandles, PendingFileCompletion, ProcessContext, ProcessForkMap,
    ProcessLoadedResources, ProcessResourceFileMap, ProcessResourceManagerState,
    ProcessVfsDirectory, ProcessVfsMetadata, ProcessVfsVolumeRecord, ProcessWorkingDirectory,
    SharedProcessAppleEventDescriptors, SharedProcessAppleEventHandlers,
    SharedProcessAppleEventLaunchState,
    SharedProcessCollectionManager, SharedProcessControlManager, SharedProcessCursorState,
    SharedProcessDialogText, SharedProcessDisplayClut,
    SharedProcessEventQueue, SharedProcessFileSystem, SharedProcessGraphicsDevice,
    SharedProcessGraphicsPort, SharedProcessInputState,
    SharedProcessListManager, SharedProcessMemoryManager, SharedProcessMenuTracking,
    SharedProcessOpenFilePositions, SharedProcessOpenFiles, SharedProcessQuickDrawError,
    SharedProcessQuickDrawHiliteColors,
    SharedProcessQuickDrawOpColors, SharedProcessQuickDrawPixelStates, SharedProcessScrapState,
    SharedProcessSoundManager, SharedProcessTextEditManager, SharedProcessTickState,
    SharedProcessValue,
};
use crate::process_manager::{
    resolve_process_application_metadata, ProcessApplicationMetadata,
};
use crate::trace::{TraceEvent, TraceSink, TraceSource};
use crate::ui_theme::{UiTheme, UiThemeId};
use crate::{Error, Result};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::PathBuf;

pub use crate::event_queue::{
    EventProbeResult, EventQueueProbeSnapshot, EventRecordSnapshot, QueuedEvent,
};

pub(crate) const BOOT_VOLUME_NAME: &str = "MacintoshHD";
pub(crate) const BOOT_VOLUME_REF_NUM: i16 = -1;
const VFS_HFS_LITERAL_SLASH: char = '\u{F02F}';

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SelectorOperationRoute {
    pub(crate) selector: u32,
    pub(crate) operation_id: &'static str,
    pub(crate) routine_name: &'static str,
}

impl SelectorOperationRoute {
    pub(crate) const fn new(
        selector: u32,
        operation_id: &'static str,
        routine_name: &'static str,
    ) -> Self {
        Self {
            selector,
            operation_id,
            routine_name,
        }
    }
}

pub(crate) fn selector_operation_route(
    routes: &'static [SelectorOperationRoute],
    selector: u32,
) -> Option<&'static SelectorOperationRoute> {
    routes
        .binary_search_by_key(&selector, |route| route.selector)
        .ok()
        .map(|index| &routes[index])
}

const POWER_MANAGER_OPERATION_ROUTES: &[SelectorOperationRoute] =
    &include!("generated_power_manager_operations.rs");

fn power_manager_operation_route(
    trap_word: u16,
    selector: u16,
) -> Option<&'static SelectorOperationRoute> {
    if trap_word != 0xA09E {
        return None;
    }
    selector_operation_route(POWER_MANAGER_OPERATION_ROUTES, u32::from(selector))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenCopyBitsRect {
    pub src_top: i16,
    pub src_left: i16,
    pub src_bottom: i16,
    pub src_right: i16,
    pub dst_top: i16,
    pub dst_left: i16,
    pub dst_bottom: i16,
    pub dst_right: i16,
}

fn screen_copybits_rect_is_valid(rect: ScreenCopyBitsRect) -> bool {
    rect.src_right > rect.src_left
        && rect.src_bottom > rect.src_top
        && rect.dst_right > rect.dst_left
        && rect.dst_bottom > rect.dst_top
}

#[derive(Clone, Debug)]
pub(crate) struct RecentFileRead {
    pub(crate) ref_num: u16,
    pub(crate) filename: String,
    pub(crate) buffer: u32,
    pub(crate) start: usize,
    pub(crate) bytes_read: usize,
}

// Env-var lookups are cached via OnceLock. Tests/diagnostics that want
// to toggle these at runtime cannot — values are read ONCE at first call.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

static TRACE_GUEST_PC_TRAPS: OnceLock<bool> = OnceLock::new();
static TRACE_DIALOG_TRAPS: OnceLock<bool> = OnceLock::new();
static TRACE_INPUT: OnceLock<bool> = OnceLock::new();
static TRACE_DELIVERED_EVENTS: OnceLock<bool> = OnceLock::new();
static TRACE_SOUND: OnceLock<bool> = OnceLock::new();
static TRACE_RESFILE: OnceLock<bool> = OnceLock::new();
static TRACE_QUICKTIME: OnceLock<bool> = OnceLock::new();
static TRACE_PC_TARGET: OnceLock<Option<u32>> = OnceLock::new();
static TRACE_NATIVE_TRAPS: OnceLock<bool> = OnceLock::new();
static TRACE_TRAP_SP: OnceLock<bool> = OnceLock::new();
static GUI_CAPTURE_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
static GUI_CAPTURE_LIMIT: OnceLock<Option<u64>> = OnceLock::new();
static GUI_CAPTURE_LABEL: OnceLock<Option<String>> = OnceLock::new();
static GUI_CAPTURE_FRAME: AtomicU64 = AtomicU64::new(0);

/// File-backed sink for `SYSTEMLESS_TRACE_TRAP_PCS=<filepath>`. When set,
/// every A-line trap dispatch appends a `<pc:08X> <trap:04X>\n` line to
/// the named file. When unset, this resolves to `None` and the trace
/// path is a branch-predicted no-op.
static TRACE_TRAP_PCS_SINK: OnceLock<Option<Mutex<std::io::BufWriter<std::fs::File>>>> =
    OnceLock::new();

fn trace_trap_pcs_sink() -> Option<&'static Mutex<std::io::BufWriter<std::fs::File>>> {
    TRACE_TRAP_PCS_SINK
        .get_or_init(|| {
            let path = std::env::var_os("SYSTEMLESS_TRACE_TRAP_PCS")?;
            let path = std::path::PathBuf::from(path);
            let file = std::fs::File::create(&path).ok()?;
            let mut writer = std::io::BufWriter::new(file);
            use std::io::Write;
            let _ = writeln!(
                writer,
                "# runtime trap-PC trace (SYSTEMLESS_TRACE_TRAP_PCS)"
            );
            let _ = writeln!(
                writer,
                "# format: B <segment_id> <base_addr_hex>  (segment load)"
            );
            let _ = writeln!(
                writer,
                "# format: T <pc_hex> <trap_word_hex>      (trap dispatch)"
            );
            Some(Mutex::new(writer))
        })
        .as_ref()
}

/// Append a segment-load record to the `SYSTEMLESS_TRACE_TRAP_PCS` file
/// so a downstream cross-reference can convert runtime trap PCs back
/// to (CODE id, offset) pairs. No-op when the env var is unset.
pub fn record_segment_base(segment_id: i16, base_addr: u32) {
    if let Some(sink) = trace_trap_pcs_sink() {
        use std::io::Write;
        if let Ok(mut w) = sink.lock() {
            let _ = writeln!(w, "B {} {:08X}", segment_id, base_addr);
        }
    }
}

/// Read-only watcher for sound-gating globals at `(A5+$BFCC)` byte and
/// `(A5+$BFBA)` word. When `SYSTEMLESS_LOG_M1_GATES=<path>` is set, every
/// trap dispatch writes a row when either value changes from the last
/// snapshot. Direct (unbuffered) `File` so the change-only log survives
/// timeouts; logs are rare so per-write syscall cost is fine.
static LOG_M1_GATES_SINK: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

fn log_m1_gates_sink() -> Option<&'static Mutex<std::fs::File>> {
    LOG_M1_GATES_SINK
        .get_or_init(|| {
            let path = std::env::var_os("SYSTEMLESS_LOG_M1_GATES")?;
            let path = std::path::PathBuf::from(path);
            let mut file = std::fs::File::create(&path).ok()?;
            use std::io::Write;
            let _ = writeln!(file, "# sound-gate watcher (SYSTEMLESS_LOG_M1_GATES)");
            let _ = writeln!(
                file,
                "# Snapshots A5+$BFCC byte + A5+$BFBA word on each trap dispatch"
            );
            let _ = writeln!(
                file,
                "# format: M1-GATE trap=$XXXX pc=$XXXXXXXX a5=$XXXXXXXX BFCC.B=$XX BFBA.W=$XXXX"
            );
            Some(Mutex::new(file))
        })
        .as_ref()
}

/// Track the last-seen values so we only log when they change.
static M1_GATES_LAST_BFCC: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0xFF); // start with sentinel
static M1_GATES_LAST_BFBA: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0xFFFF); // start with sentinel

fn trace_guest_pc_traps_enabled() -> bool {
    *TRACE_GUEST_PC_TRAPS.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_PC_TRAPS").is_some())
}

pub(crate) fn trace_dialog_traps_enabled() -> bool {
    *TRACE_DIALOG_TRAPS.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DIALOG_TRAPS").is_some())
}

pub(crate) fn trace_input_enabled() -> bool {
    *TRACE_INPUT.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_INPUT").is_some())
}

pub(crate) fn trace_delivered_events_enabled() -> bool {
    *TRACE_DELIVERED_EVENTS
        .get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_DELIVERED_EVENTS").is_some())
}

// GetKeys returns a 16-byte KeyMap (`PACKED ARRAY[0..127] OF Boolean`;
// Universal Interfaces also exposes the byte-level representation as
// `KeyMapByteArray[16]`). Inside Macintosh Volume I (1985), pp. I-259–I-260
// says each array index is its key's virtual key code. In the byte-level ABI,
// the first logical element occupies the low-order bit of each byte.
pub(crate) fn key_map_byte_mask(key_code: u8) -> Option<(usize, u8)> {
    if key_code >= 128 {
        return None;
    }
    let byte_idx = (key_code >> 3) as usize;
    if byte_idx >= 16 {
        return None;
    }
    let mask = 1u8 << (key_code & 0x07);
    Some((byte_idx, mask))
}

pub(crate) fn key_map_key_is_down(key_map: &[u8; 16], key_code: u8) -> bool {
    let Some((byte_idx, mask)) = key_map_byte_mask(key_code) else {
        return false;
    };
    (key_map[byte_idx] & mask) != 0
}

fn trace_sound_enabled() -> bool {
    *TRACE_SOUND.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_SOUND").is_some())
}

fn trace_native_traps_enabled() -> bool {
    *TRACE_NATIVE_TRAPS.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_NATIVE_TRAPS").is_some())
}

fn trace_trap_sp_enabled() -> bool {
    *TRACE_TRAP_SP.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_TRAP_SP").is_some())
}

fn gui_capture_dir() -> Option<&'static PathBuf> {
    GUI_CAPTURE_DIR
        .get_or_init(|| {
            let path = std::env::var_os("SYSTEMLESS_GUI_CAPTURE_DIR")?;
            let path = PathBuf::from(path);
            if path.as_os_str().is_empty() {
                None
            } else {
                Some(path)
            }
        })
        .as_ref()
}

fn gui_capture_limit() -> Option<u64> {
    *GUI_CAPTURE_LIMIT.get_or_init(|| {
        std::env::var("SYSTEMLESS_GUI_CAPTURE_LIMIT")
            .ok()
            .and_then(|value| value.parse().ok())
    })
}

fn gui_capture_label() -> Option<&'static str> {
    GUI_CAPTURE_LABEL
        .get_or_init(|| {
            let label = std::env::var("SYSTEMLESS_GUI_CAPTURE_LABEL").ok()?;
            if label.is_empty() {
                None
            } else {
                Some(label)
            }
        })
        .as_deref()
}

fn sanitize_gui_capture_label(label: &str) -> String {
    let mut safe = String::with_capacity(label.len().min(96));
    for ch in label.chars().take(96) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            safe.push(ch);
        } else {
            safe.push('_');
        }
    }
    if safe.is_empty() {
        safe.push_str("frame");
    }
    safe
}

/// `SYSTEMLESS_TRACE_RESFILE=1` enables verbose tracing of resource-file open
/// traps (`OpenResFile`/`OpenRFPerm`/`HOpenResFile`/`FSpOpenResFile`).
/// Off by default — games that poll resource forks each frame (e.g.
/// Bonkheads Deluxe re-opens `BDX_Data` every iteration of its main loop)
/// would otherwise drown stderr in dedup-log lines.
pub(crate) fn trace_resfile_enabled() -> bool {
    *TRACE_RESFILE.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_RESFILE").is_some())
}

/// `SYSTEMLESS_TRACE_QUICKTIME=1` enables logging of the first 100
/// Movie Toolbox dispatch (`$AAAA`) selectors fired by the guest.
/// Off by default; the trace is diagnostic for identifying the
/// QuickTime calls a title makes.
pub(crate) fn trace_quicktime_enabled() -> bool {
    *TRACE_QUICKTIME.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_QUICKTIME").is_some())
}

static TRACE_ATRAPS_WINDOW: OnceLock<Option<(u32, u32)>> = OnceLock::new();
static TRACE_ALL_TRAPS: OnceLock<bool> = OnceLock::new();
static TRAP_HISTOGRAM_ENABLED: OnceLock<bool> = OnceLock::new();

/// When `SYSTEMLESS_TRACE_TRAP_COUNTS` is set, every A-line dispatch
/// increments `TrapDispatcher::trap_histogram` (indexed by `trap & 0xFFF`).
/// Dump via `TrapDispatcher::print_trap_histogram`.
fn trap_histogram_enabled() -> bool {
    *TRAP_HISTOGRAM_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_TRAP_COUNTS").is_some())
}

static TRAP_TIMING_ENABLED: OnceLock<bool> = OnceLock::new();

/// When `SYSTEMLESS_TRACE_TRAP_TIMING` is set, every dispatched trap
/// accumulates wall-clock nanoseconds into `TrapDispatcher::trap_time_ns`.
/// Adds ~20-30ns measurement overhead per trap. Dump via
/// `print_trap_timing_histogram`.
fn trap_timing_enabled() -> bool {
    *TRAP_TIMING_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_TRAP_TIMING").is_some())
}

fn trace_all_traps_enabled() -> bool {
    *TRACE_ALL_TRAPS.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_ALL_TRAPS").is_some())
}

/// Cached LO-HI window for `SYSTEMLESS_TRACE_ATRAPS_WINDOW`.
fn trace_atraps_window() -> Option<(u32, u32)> {
    *TRACE_ATRAPS_WINDOW.get_or_init(|| {
        let win = std::env::var("SYSTEMLESS_TRACE_ATRAPS_WINDOW").ok()?;
        let (lo_s, hi_s) = win.split_once('-')?;
        let lo = lo_s.parse::<u32>().ok()?;
        let hi = hi_s.parse::<u32>().ok()?;
        Some((lo, hi))
    })
}

/// `SYSTEMLESS_TRACE_PC=0xADDR` target — when a trap fires from this PC,
/// trap dispatch logs registers + return address.
fn trace_pc_target() -> Option<u32> {
    *TRACE_PC_TARGET.get_or_init(|| {
        let v = std::env::var_os("SYSTEMLESS_TRACE_PC")?;
        let s = v.to_str()?.trim();
        let s = s
            .strip_prefix("0x")
            .or_else(|| s.strip_prefix("0X"))
            .unwrap_or(s);
        u32::from_str_radix(s, 16).ok()
    })
}

fn apply_os_trap_dispatcher_ccr<C: CpuOps>(cpu: &mut C) {
    // The Mac trap dispatcher updates CCR for Operating System traps by
    // testing the low-order word of D0 before returning to the caller.
    // Inside Macintosh: Operating System Utilities (1994), p. 8-13.
    let mut ccr = cpu.get_ccr() & 0x10;
    let low_word = cpu.read_reg(Register::D0) as u16;
    if low_word == 0 {
        ccr |= 0x04;
    } else if (low_word & 0x8000) != 0 {
        ccr |= 0x08;
    }
    cpu.set_ccr(ccr);
}

fn capture_os_trap_dispatch_frame<C: CpuOps>(cpu: &C, trap_word: u16) -> OsTrapDispatchFrame {
    OsTrapDispatchFrame {
        trap_word,
        d1: cpu.read_reg(Register::D1),
        d2: cpu.read_reg(Register::D2),
        a0: cpu.read_reg(Register::A0),
        a1: cpu.read_reg(Register::A1),
        a2: cpu.read_reg(Register::A2),
    }
}

fn deliver_os_trap_word<C: CpuOps>(cpu: &mut C, trap_word: u16) {
    let d1 = cpu.read_reg(Register::D1);
    cpu.write_reg(Register::D1, (d1 & 0xFFFF_0000) | u32::from(trap_word));
}

fn restore_os_trap_dispatch_frame<C: CpuOps>(cpu: &mut C, frame: OsTrapDispatchFrame) {
    cpu.write_reg(Register::D1, frame.d1);
    cpu.write_reg(Register::D2, frame.d2);
    cpu.write_reg(Register::A1, frame.a1);
    cpu.write_reg(Register::A2, frame.a2);
    if !raw_trap_route(frame.trap_word).os_returns_a0 {
        cpu.write_reg(Register::A0, frame.a0);
    }
}

/// A parsed dialog item from a DITL resource.
/// Inside Macintosh Volume I, I-439
#[derive(Clone, Debug, Default)]
pub struct DialogItem {
    /// Item type byte from DITL (4=button, 8=statText, 16=editText, etc.)
    pub item_type: u8,
    /// Display rectangle in dialog-local coordinates (top, left, bottom, right)
    pub rect: (i16, i16, i16, i16),
    /// Text content (button title, static/edit text, or empty)
    pub text: String,
    /// Resource ID for icon/picture items
    pub resource_id: i16,
    /// For userItem (type 0): 68K procedure pointer installed via SetDItem.
    /// PROCEDURE MyItem (theWindow: WindowPtr; itemNo: INTEGER);
    /// Inside Macintosh Volume I, I-405
    pub proc_ptr: u32,
    /// For editText items (type 16): selection start byte offset
    /// (clamped to text.len()). Set by SelectDialogItemText
    /// ($A97E). Defaults to 0 (caret at start). The (start, end)
    /// pair encodes the user's text selection within the editText
    /// field; ModalDialog's redraw path can highlight bytes
    /// `start..end` per IM:I I-414.
    pub sel_start: i16,
    /// For editText items (type 16): selection end byte offset
    /// (clamped to text.len(); always ≥ sel_start after
    /// SelectDialogItemText normalization). Defaults to 0
    /// (caret at start, no selection). The IM-canonical "select
    /// all" pair `(0, -1)` is normalized to `(0, text.len())` at
    /// SelectDialogItemText time.
    pub sel_end: i16,
}

/// Candidate popup-menu association observed while a dialog is being
/// initialized. Some apps create custom popup controls by inserting a MENU,
/// querying a userItem with GetDItem, then installing a userItem draw proc via
/// SetDItem. Keep this pending until the SetDItem proc installation confirms it;
/// arbitrary userItem grids also call GetDItem heavily and must not be promoted
/// to popup controls merely because a menu was inserted earlier.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PendingDialogPopupMenu {
    pub dialog_ptr: u32,
    pub item_no: i16,
    pub menu_id: i16,
    pub rect: (i16, i16, i16, i16),
}

#[derive(Clone, Debug)]
pub struct DialogPopupDraw {
    pub rect: (i16, i16, i16, i16),
    pub title: String,
    pub enabled: bool,
    pub pressed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PendingWaitNextEventReturn {
    pub event_ptr: u32,
    pub result_ptr: u32,
    pub event_mask: u16,
    pub mouse_rgn: u32,
    pub resume_pc: Option<u32>,
    pub resume_sp: Option<u32>,
}

/// State for ModalDialog mouse/key tracking across frames.
/// Mirrors MenuTrackingState; follows the same re-fire pattern.
/// Inside Macintosh Volume I, I-415
#[derive(Default)]
pub struct DialogTrackingState {
    /// Guest pointer to the DialogRecord
    pub dialog_ptr: u32,
    /// Dialog window bounds in screen coordinates (top, left, bottom, right)
    pub bounds: (i16, i16, i16, i16),
    /// Dialog window title
    pub title: String,
    /// Window definition ID (0=documentProc, 1=dBoxProc, 2=plainDBox, etc.)
    pub proc_id: i16,
    /// Parsed DITL items (1-indexed in Mac convention; stored 0-indexed here)
    pub items: Vec<DialogItem>,
    /// Default button item number (1-based, 0=none)
    pub default_item: i16,
    /// Cancel button item number (1-based, 0=none)
    pub cancel_item: i16,
    /// Current text in the active editText field
    pub edit_text: String,
    /// Active editText item index (1-based, 0=none)
    pub edit_item: i16,
    /// Framebuffer pixels saved under the dialog (for restore on dismiss)
    pub saved_pixels: SavedPixels,
    /// Saved stack pointer from ModalDialog's first call
    pub stack_ptr: u32,
    /// Pointer to the itemHit variable (where to write the result)
    pub item_hit_ptr: u32,
    /// Snapshot of the fully-rendered dialog pixels (including pictures).
    /// Used by redraw_chrome to restore the dialog without re-parsing PICTs.
    pub rendered_pixels: SavedPixels,
    /// Remaining flash toggles (6 = 3 flashes). 0 = not flashing.
    pub flash_remaining: u8,
    /// Frames left in the current flash toggle phase
    pub flash_delay: u8,
    /// Which button item is flashing (1-based)
    pub flash_item: i16,
    /// Whether the user has typed in the edit text field (transitions from all-selected to cursor)
    pub edit_text_modified: bool,
    /// Queue of userItem draw procs to call (68K proc address, 1-based item number).
    /// Populated when ModalDialog first creates tracking state.
    /// Drained one-at-a-time via trampoline injection in runner.rs.
    pub draw_proc_queue: VecDeque<(u32, i16)>,
    /// Whether the initial draw procs have all been called.
    pub draw_procs_done: bool,
    /// Whether rendered_pixels has been re-snapshotted after draw procs completed.
    pub rendered_pixels_final: bool,
    pub(crate) filter_presentation_epoch: Option<u64>,
    /// Optional ModalDialog filter procedure pointer.
    /// FUNCTION MyFilter(dialog: DialogPtr; VAR event: EventRecord; VAR itemHit: INTEGER): BOOLEAN;
    /// Inside Macintosh Volume I, I-417
    pub filter_proc: u32,
    /// True when all DITL items are userItem, meaning the app owns dialog drawing.
    pub game_managed: bool,
    /// Most recent event passed to the ModalDialog filter proc.
    /// If the filter returns FALSE, ModalDialog must still process this event.
    pub last_filter_event: Option<QueuedEvent>,
    /// HLE popup draw data. Stored so updateEvt re-snapshots can redraw popups
    /// on top of the game's narrow indicator rendering while preserving the
    /// enabled/pressed state captured from the original dialog/control record.
    pub popup_draws: Vec<DialogPopupDraw>,
    /// Active popup-menu control tracking inside ModalDialog.
    pub active_popup: Option<DialogPopupTrackingState>,
    /// Active push-button tracking inside ModalDialog.
    pub active_button: Option<DialogButtonTrackingState>,
    /// Active plain userItem tracking inside ModalDialog.
    pub active_user_item: Option<DialogUserItemTrackingState>,
}

/// Retained state for the Standard File Package save dialogs.
///
/// StandardPutFile/CustomPutFile are modal package routines rather than
/// Dialog Manager calls, but they still run an internal event loop and return
/// only after Save or Cancel. The runner refires `_Pack3` while this state is
/// present, mirroring the existing ModalDialog/MenuSelect HLE pattern.
/// Inside Macintosh: Files (1992), pp. 3-13, 3-45 to 3-47.
#[derive(Clone, Debug)]
pub(crate) struct StandardFilePutTrackingState {
    pub modern_reply: bool,
    pub reply_ptr: u32,
    pub stack_ptr: u32,
    pub pop_total: u32,
    pub entries: Vec<StandardFileGetEntry>,
    pub current_dir_id: u32,
    pub selected: Option<usize>,
    pub prompt: String,
    pub name: String,
    pub sel_start: i16,
    pub sel_end: i16,
    pub bounds: (i16, i16, i16, i16),
    pub saved_pixels: SavedPixels,
}

/// Candidate file shown by a retained Standard File get dialog.
#[derive(Clone, Debug)]
pub(crate) struct StandardFileGetEntry {
    pub name: Vec<u8>,
    pub display_name: String,
    pub vref: i16,
    pub wd_ref: i16,
    pub dir_id: u32,
    pub file_type: u32,
    pub finder_flags: u16,
    pub is_directory: bool,
}

/// Retained state for the Standard File Package open dialogs.
///
/// StandardGetFile/CustomGetFile are modal package routines like the save
/// variants. In browser/UI-yield mode this lets `_Pack3` refire until the user
/// picks a visible file or cancels.
#[derive(Clone, Debug)]
pub(crate) struct StandardFileGetTrackingState {
    pub modern_reply: bool,
    pub reply_ptr: u32,
    pub stack_ptr: u32,
    pub pop_total: u32,
    pub entries: Vec<StandardFileGetEntry>,
    pub current_dir_id: u32,
    pub file_types: Option<Vec<u32>>,
    pub selected: usize,
    pub bounds: (i16, i16, i16, i16),
    pub saved_pixels: SavedPixels,
}

/// Popup-menu control state owned by an active ModalDialog loop.
pub struct DialogPopupTrackingState {
    pub item_no: i16,
    pub ctrl_handle: u32,
    pub ctrl_ptr: u32,
    pub active_menu: usize,
    pub highlighted_item: i16,
    pub saved_pixels: SavedPixels,
    pub dropdown_rect: (i16, i16, i16, i16),
}

/// Push-button tracking owned by an active ModalDialog loop.
pub struct DialogButtonTrackingState {
    /// The initiating event retained so a delayed release cannot consume an
    /// unrelated queued mouse-down if the dialog is disposed while tracking.
    pub mouse_down: QueuedEvent,
    pub item_no: i16,
    pub rect: (i16, i16, i16, i16),
    pub title: String,
    pub is_default: bool,
    pub highlighted: bool,
}

/// Push-button/click tracking for a front modal dialog. ModalDialog-retained
/// clicks consume both mouse events; app-owned modal clicks pass mouseDown to
/// the app and use this state to finish the visible button press on mouseUp.
pub struct RetainedModalDialogClickState {
    pub dialog_ptr: u32,
    pub item_no: i16,
    pub rect: (i16, i16, i16, i16),
    pub title: String,
    pub is_default: bool,
    pub highlighted: bool,
    pub delivered_to_app: bool,
}

/// Plain userItem tracking owned by an active ModalDialog loop.
pub struct DialogUserItemTrackingState {
    pub item_no: i16,
    pub rect: (i16, i16, i16, i16),
}

/// Rendered pixels for a dialog window after ModalDialog has returned
/// an item hit but before the app disposes the dialog.
#[derive(Clone, Debug)]
pub(crate) struct PersistentDialogSnapshot {
    pub bounds: (i16, i16, i16, i16),
    pub pixels: SavedPixels,
}

/// State for controls tracked through TrackControl.
/// TrackControl blocks until mouse-up, so HLE keeps the trap active across
/// refires in the same style as MenuSelect and ModalDialog.
pub(crate) struct ControlTrackingState {
    pub ctrl_handle: u32,
    pub ctrl_ptr: u32,
    pub popup_tracking: bool,
    pub active_menu: usize,
    pub highlighted_item: i16,
    pub saved_pixels: SavedPixels,
    pub dropdown_rect: (i16, i16, i16, i16),
    pub popup_content_top: i16,
    pub popup_scroll_direction: Option<crate::menu_manager::MenuScrollDirection>,
    pub simple_part: u16,
    pub simple_screen_rect: (i16, i16, i16, i16),
    pub simple_highlighted: bool,
    pub saved_hilite: u8,
    pub stack_ptr: u32,
    pub scrollbar_action_proc: u32,
    pub scrollbar_part: u16,
    pub scrollbar_last_action_tick: u32,
    pub scrollbar_idle_refires: u8,
    pub scrollbar_callback_pending: bool,
}

/// Retained state for TrackControl while dragging a scrollbar indicator thumb.
#[derive(Clone, Debug)]
pub(crate) struct ScrollbarThumbTrackingState {
    pub _ctrl_handle: u32,
    pub ctrl_ptr: u32,
    pub stack_ptr: u32,
    pub start_mouse: (i16, i16),
    pub start_thumb_pos: i16,
    pub start_value: i16,
    pub min: i16,
    pub max: i16,
    pub track_start: i16,
    pub travel: i16,
    pub cross_start: i16,
    pub cross_end: i16,
    pub is_vertical: bool,
    pub thumb_size: i16,
    pub slop_rect: (i16, i16, i16, i16),
    pub outline_rect: Option<(i16, i16, i16, i16)>,
    pub saved_pixels: Vec<(i16, i16, i16, i16, SavedPixels)>,
}

/// Retained state for TrackBox while the mouse button remains down.
#[derive(Clone, Debug)]
pub(crate) struct ZoomBoxTrackingState {
    pub _window_ptr: u32,
    pub stack_ptr: u32,
    pub hit_rect: (i16, i16, i16, i16),
}

/// Retained state for DragWindow while the mouse button remains down.
/// DragWindow owns a mouse-tracking loop and does not return until release;
/// the GUI runner therefore refires the trap at presentation boundaries.
/// Macintosh Toolbox Essentials (1992), pp. 4-94 to 4-95.
#[derive(Clone, Debug)]
pub(crate) struct WindowTrackingState {
    pub window_ptr: u32,
    pub stack_ptr: u32,
    pub start_mouse: (i16, i16),
    pub original_port_origin: (i16, i16),
    pub bounds_rect: (i16, i16, i16, i16),
    pub original_outline_rect: (i16, i16, i16, i16),
    pub outline_rect: (i16, i16, i16, i16),
    pub outline_saved_pixels: Vec<(i16, i16, i16, i16, SavedPixels)>,
    pub command_down: bool,
}

/// Retained state for TrackGoAway while the mouse button remains down.
/// The Window Manager keeps control, toggles the close-box highlight as the
/// cursor crosses its region, and returns only after mouse-up.
/// Macintosh Toolbox Essentials (1992), pp. 4-103 to 4-104.
#[derive(Clone, Debug)]
pub(crate) struct GoAwayTrackingState {
    pub window_ptr: u32,
    pub stack_ptr: u32,
    pub hit_rect: (i16, i16, i16, i16),
    pub highlight_rect: (i16, i16, i16, i16),
    pub highlighted: bool,
}

/// Retained state for GrowWindow while the mouse button remains down.
/// The Window Manager tracks a gray proposed structure outline and returns
/// packed dimensions only after mouse-up.
#[derive(Clone, Debug)]
pub(crate) struct GrowWindowTrackingState {
    pub window_ptr: u32,
    pub stack_ptr: u32,
    pub screen_mode: (u32, u32, u16, u16, u16),
    pub original_content_rect: (i16, i16, i16, i16),
    pub original_outline_rect: (i16, i16, i16, i16),
    pub start_point: (i16, i16),
    pub size_rect: (i16, i16, i16, i16),
    pub outline_rect: (i16, i16, i16, i16),
    pub outline_saved_pixels: Vec<(i16, i16, i16, i16, SavedPixels)>,
}

/// Retained state shared by DragGrayRgn and DragTheRgn while the mouse
/// button remains down. Both routines own a synchronous tracking loop and
/// return only after release.
/// Macintosh Toolbox Essentials (1992), pp. 4-95 to 4-98.
#[derive(Clone, Debug)]
pub(crate) struct RegionTrackingState {
    pub stack_ptr: u32,
    pub start_mouse: (i16, i16),
    pub port_bounds_origin: (i16, i16),
    pub limit_rect: Option<(i16, i16, i16, i16)>,
    pub slop_rect: (i16, i16, i16, i16),
    pub axis: i16,
    pub original_outline_rect: (i16, i16, i16, i16),
    pub outline_rect: Option<(i16, i16, i16, i16)>,
    pub outline_saved_pixels: Vec<(i16, i16, i16, i16, SavedPixels)>,
    pub outline_pattern: [u8; 8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PortDrawState {
    pub fg_color: (u16, u16, u16),
    pub bg_color: (u16, u16, u16),
    pub pm_fg_color: Option<(u32, i16)>,
    pub pm_bg_color: Option<(u32, i16)>,
    pub bk_pat: [u8; 8],
    pub pn_loc: (i16, i16),
    pub pn_size: (i16, i16),
    pub pn_mode: i16,
    pub pn_pat: [u8; 8],
    pub pn_vis: i16,
    pub tx_font: i16,
    pub tx_face: i16,
    pub tx_mode: i16,
    pub tx_size: i16,
}

impl Default for PortDrawState {
    fn default() -> Self {
        Self {
            fg_color: (0, 0, 0),
            bg_color: (0xFFFF, 0xFFFF, 0xFFFF),
            pm_fg_color: None,
            pm_bg_color: None,
            bk_pat: [0x00; 8],
            pn_loc: (0, 0),
            pn_size: (1, 1),
            pn_mode: 8,
            pn_pat: [0xFF; 8],
            pn_vis: 0,
            tx_font: 0,
            tx_face: 0,
            tx_mode: 1,
            tx_size: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PortRegionSnapshot {
    pub handle: u32,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PortStateSnapshot {
    pub port: u32,
    pub gdevice: u32,
    pub draw_state: PortDrawState,
    pub port_state_bytes: [u8; 56],
    pub resolved_color_fields: Option<u8>,
    pub vis_region: Option<PortRegionSnapshot>,
    pub clip_region: Option<PortRegionSnapshot>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CachedCopyBitmapInfo {
    pub base: u32,
    pub row_bytes: u32,
    pub bounds_top: i16,
    pub bounds_left: i16,
    pub bounds_bottom: i16,
    pub bounds_right: i16,
    pub pixel_size: u32,
    pub ctab_handle: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DrawOldState {
    pub structure: Option<(i16, i16, i16, i16)>,
    pub content: Option<(i16, i16, i16, i16)>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RecentColorTableFetch {
    pub ct_id: i16,
    pub ctab_handle: u32,
    pub port: u32,
    pub tick: u32,
}

pub use crate::callback_manager::{ProcessTimerTask as TimerTask, ProcessVblTask as VblTask};

pub(crate) const LOADSEG_GETRESOURCE_SENTINEL: u16 = 0x51F0;

/// In-flight Segment Loader native GetResource call.
///
/// Some protected/THINK-era apps install a native `_GetResource` hook that
/// decodes `CODE` resources when the real Segment Loader asks for them.
/// Systemless keeps CODE segments resident, so `_LoadSeg` has to explicitly
/// route through that hook and then resume HLE jump-table patching.
#[derive(Clone, Debug)]
pub(crate) struct LoadSegGetResourceState {
    pub seg_num: i16,
    pub entry_addr: u32,
    pub result_sp: u32,
    pub d_regs: [u32; 8],
    pub a_regs: [u32; 8],
}

/// Trap Dispatcher state retained while a handler installed through
/// `SetTrapAddress` runs. A handler may call the old trap address later, after
/// changing its stack frame, so old-trap recovery cannot infer this state
/// from A6 or from the handler's instruction shape.
#[derive(Clone, Debug)]
pub(crate) struct NativeTrapCallState {
    pub return_pc: u32,
    pub argument_sp: u32,
    pub os_dispatch_frame: Option<OsTrapDispatchFrame>,
    pub preserved_d_regs: [u32; 5],
    pub preserved_a_regs: [u32; 5],
}

/// Registers saved by the OS Trap Dispatcher around one routine invocation.
///
/// The complete A-line word is delivered in the low word of D1. On return,
/// D1, D2, A1, A2 and (when bit 8 is clear) A0 are restored; D0 and an A0
/// result selected by bit 8 remain visible. Inside Macintosh: Operating
/// System Utilities (1994), pp. 8-11--8-13.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OsTrapDispatchFrame {
    trap_word: u16,
    d1: u32,
    d2: u32,
    a0: u32,
    a1: u32,
    a2: u32,
}

/// In-flight AppleEvent handler call. Built by Pack8 routine 27
/// (`AEProcessAppleEvent`) when it dispatches a registered handler;
/// consumed by the trampoline trap when the handler `RTD`s back.
#[derive(Clone, Debug)]
pub(crate) struct AeCallState {
    /// PC the m68k would have continued at after `_Pack8` returned to
    /// the original `AEProcessAppleEvent` caller. Restored after the
    /// trampoline cleans up.
    pub return_pc: u32,
    /// SP that the trampoline expects to see when the handler `RTD`s.
    /// Used as a sanity check; the trampoline restores SP to this
    /// value (which is the result-slot address — the original caller
    /// pushed an OSErr slot before `_Pack8`, and `RTD #12` lands SP
    /// pointing right at it).
    pub expected_sp_after_rtd: u32,
    /// Optional result code to report to the original Pack8 caller after
    /// the handler returns. AEProcessAppleEvent reports the handler's
    /// OSErr; AESend reports delivery status, so same-process sends use
    /// noErr here while the handler result remains a reply-event concern.
    pub result_override: Option<i16>,
    /// Descriptor records created by the Apple Event Manager solely for this
    /// handler invocation. The manager disposes these application-heap
    /// objects after the handler returns. Interapplication Communication
    /// (1993), pp. 4-33 and 4-39.
    pub(crate) owned_descriptors: Option<(u32, u32)>,
    /// Optional Object Support Library continuation. When AEResolve calls a
    /// guest object accessor, the accessor returns through the same Pack8
    /// trampoline as AE handlers; this state tells the trampoline whether to
    /// resume another accessor level or finish the original AEResolve call.
    pub resolve_state: Option<AeResolveState>,
}

pub(crate) use crate::process_context::{
    ProcessAeDescriptor as AeDescriptor,
    ProcessSyntheticAppleEvent as SyntheticAppleEvent,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct AeObjectAccessor {
    pub accessor_ptr: u32,
    pub refcon: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct AePrivateHashTable {
    pub key_size: usize,
    pub value_size: usize,
    pub entries: HashMap<Vec<u8>, Vec<u8>>,
}

#[derive(Clone, Debug)]
pub(crate) struct AeResolveLevel {
    pub desired_class: u32,
    pub key_form: u32,
    pub key_data: AeDescriptor,
}

#[derive(Clone, Debug)]
pub(crate) struct AeResolveState {
    pub return_pc: u32,
    pub result_slot: u32,
    pub final_token_desc: u32,
    pub levels: Vec<AeResolveLevel>,
    pub next_level: usize,
    pub current_token_desc: u32,
    pub container_class: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AeCoercionHandler {
    pub handler_ptr: u32,
    pub refcon: u32,
    pub from_type_is_desc: bool,
}

pub(crate) type ListState = ProcessListRecord;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ControlAuxRecordState {
    /// Guest AuxCtlHandle returned by GetAuxCtl.
    pub handle: u32,
}

pub(crate) type VfsMetadata = ProcessVfsMetadata;
pub(crate) type VfsDirectory = ProcessVfsDirectory;
pub(crate) type VfsVolume = ProcessVfsVolumeRecord;

pub(crate) type WorkingDirectory = ProcessWorkingDirectory;

#[derive(Clone, Debug)]
pub(crate) struct PendingLaunchApplication {
    pub path: String,
    pub after_event_yield: bool,
    pub after_caller_exit: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct VfsCatalogEntry {
    pub path: String,
    pub name: String,
    pub is_directory: bool,
}

/// Polygon recording state for OpenPoly/ClosePoly.
/// Inside Macintosh Volume I, I-189
pub(crate) struct PolygonRecording {
    /// Guest handle for the PolyRec being built.
    pub handle: u32,
    /// Vertices as (v, h) pairs.
    pub vertices: Vec<(i16, i16)>,
}

/// Region recording state for OpenRgn/CloseRgn.
/// Imaging With QuickDraw 1994, 3-87..3-89.
#[derive(Debug, Default)]
pub(crate) struct RegionRecording {
    /// Outline segments collected from Line/LineTo and framed shapes.
    /// Endpoints are (v, h) pairs in local QuickDraw coordinates.
    pub outline_segments: Vec<((i16, i16), (i16, i16))>,
    /// Filled row spans contributed by existing regions or fallback shape
    /// paths. Each row stores sorted x endpoint pairs.
    pub filled_rows: BTreeMap<i16, Vec<i16>>,
    /// Mathematical bounds of all recorded input geometry.
    pub bbox: Option<(i16, i16, i16, i16)>,
}

/// Small LRU cache for Color Manager inverse-table payloads.
///
/// `MakeITable` still writes each caller's ITab header and target handle, but
/// identical CLUT/resolution pairs do not need to rerun the expensive
/// RGB-nearest-match scan.
pub(crate) const INVERSE_TABLE_CACHE_LIMIT: usize = 8;

#[derive(Clone)]
pub(crate) struct InverseTableCacheEntry {
    pub res: u16,
    pub clut: [[u16; 3]; 256],
    pub bytes: Vec<u8>,
}

/// Rust adapter identities allowed for one canonical A-line operation row.
/// `Nonterminal` is a declared registry state, distinct from an accidental
/// omission: its gateway remains callable and reports the exact raw word until
/// source-backed semantics are implemented.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum TrapAdapterId {
    Memory,
    Event,
    Resource,
    QuickDraw,
    Menu,
    Window,
    Control,
    Dialog,
    Sound,
    Toolbox,
    Sane,
    Unimplemented,
    Nonterminal,
    Collection,
}

impl TrapAdapterId {
    const fn mask(self) -> u16 {
        1 << self as u8
    }
}

/// Generated canonical operation identity and its allowed dispatch adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DefaultTrapRoute {
    pub(crate) operation_id: u16,
    adapter_mask: u16,
}

impl DefaultTrapRoute {
    const fn new(operation_id: u16, adapter_mask: u16) -> Self {
        Self {
            operation_id,
            adapter_mask,
        }
    }

    const fn allows(self, adapter: TrapAdapterId) -> bool {
        self.adapter_mask & adapter.mask() != 0
    }
}

const DEFAULT_TRAP_ROUTES: [DefaultTrapRoute; 1280] = include!("generated_default_routes.rs");

pub(crate) fn default_trap_route(trap_word: u16) -> &'static DefaultTrapRoute {
    &DEFAULT_TRAP_ROUTES[usize::from(raw_trap_route(trap_word).table_index)]
}

/// Application-context portion of the writable Trap Manager topology.
///
/// The Process Manager saves process-specific system globals when it switches
/// applications, and application-installed patches are available only while
/// that application's context is active. Inside Macintosh: Processes (1994),
/// pp. 1-3, 1-7--1-8, and 1-12. Keep the raw cells here rather than a decoded
/// handler map: direct writes and protected daisy chains must round-trip
/// without losing their guest-visible representation.
pub(crate) struct TrapTableProcessContext {
    profile: TrapTableProfile,
    raw_entries: Vec<u32>,
    raw_exception_vectors: [u32; 2],
    default_exception_vectors: [u32; 2],
    pending_native_trap_calls: HashMap<u16, Vec<NativeTrapCallState>>,
    current_trap_caller: Option<u32>,
}

/// Trap dispatcher with resource fork access and emulator state.
pub struct TrapDispatcher {
    /// Synthetic keyboard and mouse entries exposed by the ADB Manager.
    pub(crate) adb: crate::adb::AdbManager,
    /// Process-owned File Manager and Resource Manager state shared by the
    /// attached CPU adapters. Inside Macintosh: Files (1992), pp. 1-7--1-9;
    /// Inside Macintosh Volume I (1985), pp. I-109--I-110.
    process_file_system: SharedProcessFileSystem,
    /// Per-page hold refcounts for `HoldMemory`/`UnholdMemory`.
    /// Keys are 4 KiB page numbers in logical address space.
    /// Inside Macintosh: Memory (1992), 3-25 to 3-27.
    pub(crate) vm_held_page_counts: HashMap<u32, u16>,
    /// Pages that have ever been held by `HoldMemory`. `UnholdMemory`
    /// treats a previously-held page span as idempotent when callers
    /// release it again after the count reaches zero.
    pub(crate) vm_held_page_history: HashSet<u32>,
    /// Per-page lock refcounts for `LockMemory`/`UnlockMemory` and
    /// `LockMemoryContiguous`. `GetPhysical` requires all queried pages to
    /// be present in this map.
    /// Inside Macintosh: Memory (1992), 3-28 to 3-32.
    pub(crate) vm_locked_page_counts: HashMap<u32, u16>,
    /// Simulated instruction-cache enabled state for `_HWPriv`
    /// selector $0000 (`SwapInstructionCache`). The trap returns the
    /// previous state and installs the requested new state.
    /// Inside Macintosh: Memory (1992), p. 4-29.
    pub(crate) instruction_cache_enabled: bool,
    /// Simulated data-cache enabled state for `_HWPriv`
    /// selector $0002 (`SwapDataCache`). The trap returns the
    /// previous state and installs the requested new state.
    /// Inside Macintosh: Memory (1992), p. 4-30.
    pub(crate) data_cache_enabled: bool,
    /// Safely shared process Memory Manager used by both CPU adapters.
    process_memory_manager: Option<SharedProcessMemoryManager>,
    /// Process Memory Manager retained by a standalone 68K adapter until it
    /// attaches to the runner-owned process context.
    standalone_memory_manager: SharedProcessMemoryManager,
    /// Movie Toolbox handles returned by NewMovieFromFile/NewMovie-style traps.
    pub(crate) movie_states: HashMap<u32, MovieState>,
    /// Maps a movie-controller component instance to the Movie it drives, set
    /// by MCNewAttachedController so MCDoAction can start the right movie.
    pub(crate) movie_by_controller: HashMap<u32, u32>,
    /// Movie Toolbox current error value for GetMoviesError.
    pub(crate) movie_error: i16,
    /// Movie Toolbox sticky error value for GetMoviesStickyError.
    pub(crate) movie_sticky_error: i16,
    /// Dialogs the application has already painted itself with DrawDialog.
    /// ModalDialog gets and handles events; it does not repaint the dialog on
    /// entry. Inside Macintosh Volume I, I-415 (ModalDialog) and I-411
    /// (DrawDialog). Repainting would erase whatever the application drew
    /// into the dialog between its own DrawDialog and the ModalDialog call.
    pub(crate) dialogs_drawn_by_app: std::collections::HashSet<u32>,
    /// Map of Segment ID -> Loaded Address (for LoadSeg)
    pub(crate) segment_map: HashMap<i16, u32>,
    /// Process-owned application and system AppleEvent dispatch tables.
    pub(crate) ae_handlers: SharedProcessAppleEventHandlers,
    /// Process-owned launch awareness and one-shot synthetic OAPP state.
    pub(crate) apple_event_launch_state: SharedProcessAppleEventLaunchState,
    /// Process-owned AppleEvent event, descriptor, and shared-handle backing.
    pub(crate) ae_descriptor_state: SharedProcessAppleEventDescriptors,
    /// Object accessor dispatch table entries registered through
    /// AEInstallObjectAccessor. Key is `(isSysHandler, desiredClass,
    /// containerType)`.
    pub(crate) ae_object_accessors: HashMap<(bool, u32, u32), AeObjectAccessor>,
    /// Private Object Support Library hash tables created through Pack8
    /// selector $092E and accessed through selectors $0831/$0833/$0632.
    pub(crate) ae_private_hash_tables: HashMap<u32, AePrivateHashTable>,
    /// Special AppleEvent handlers registered through
    /// AEInstallSpecialHandler or AESetObjectCallbacks. Key is
    /// `(isSysHandler, functionClass)`.
    pub(crate) ae_special_handlers: HashMap<(bool, u32), u32>,
    /// Coercion handlers registered through AEInstallCoercionHandler. Key is
    /// `(isSysHandler, fromType, toType)`.
    pub(crate) ae_coercion_handlers: HashMap<(bool, u32, u32), AeCoercionHandler>,
    /// Gestalt selectors registered at runtime via `_NewGestalt` ($A3AD)
    /// or replaced via `_ReplaceGestalt` ($A5AD). Key is the OSType
    /// selector code packed big-endian; value is the guest-side selector
    /// function pointer. Systemless records these for duplicate-/undefined-
    /// selector accounting but cannot execute the guest function from a
    /// trap handler, so a subsequent `Gestalt` query of a registry-only
    /// selector still returns `gestaltUndefSelectorErr`. Operating
    /// System Utilities 1994, 1-34/1-35.
    pub(crate) gestalt_registry: HashMap<u32, u32>,
    /// State stashed across an AE handler invocation. When a Pack8
    /// `AEProcessAppleEvent` (routine 27) call dispatches an installed
    /// handler, the trap pushes a trampoline return address onto the
    /// guest stack and jumps to the handler. The handler's `RTD` lands
    /// back on the trampoline (a tiny `MOVE.W #$FEFE, D0; _Pack8`
    /// stub at `ae_trampoline_addr`); the matching Pack8 selector
    /// `$FEFE` dispatch finalises the AE call by resuming at the saved
    /// post-`_Pack8` PC. `None` means no AE call is currently in
    /// flight.
    pub(crate) ae_call_state: Option<AeCallState>,
    /// Outer AE handler states suspended by nested same-process AppleEvent
    /// dispatches.
    pub(crate) ae_call_state_stack: Vec<AeCallState>,
    /// Address of the lazily-allocated 6-byte trampoline used for AE
    /// handler returns. Holds `30 3C FE FE A8 16` (`MOVE.W #$FEFE, D0;
    /// _Pack8`) — the matching Pack8 dispatch with selector `$FEFE`
    /// finalises the AE call. `None` until the first
    /// `AEProcessAppleEvent` allocates it via `bus.alloc(8)`.
    pub(crate) ae_trampoline_addr: Option<u32>,
    /// Address of the lazily-allocated 96-byte QuickDraw mask table
    /// returned by `_GetMaskTable` ($A836). Three 16-word sub-tables
    /// (right masks, left masks, bit masks) per IM:IV IV-25..IV-26.
    /// `None` until the first `_GetMaskTable` call.
    pub(crate) mask_table_addr: Option<u32>,
    /// State stashed while `_LoadSeg` is routing `GetResource('CODE', seg)`
    /// through a native guest hook.
    pub(crate) loadseg_getresource_state: Option<LoadSegGetResourceState>,
    /// Address of the lazily-allocated 8-byte trampoline used to resume
    /// HLE `_LoadSeg` after that native `_GetResource` hook returns.
    pub(crate) loadseg_getresource_trampoline_addr: Option<u32>,
    /// One-shot flag for auto-pop traps whose HLE handler deliberately
    /// sets PC. `_LoadSeg` uses this when a guest native LoadSeg handler
    /// jumps to its saved old `$ADF0` trap: the real trap patches the
    /// original jump-table entry and resumes at that patched entry, not
    /// at the auto-pop return address.
    pub(crate) preserve_auto_pop_pc_once: bool,
    /// Address of the lazily-allocated trampoline used by DeviceLoop
    /// to call a guest drawing procedure for the current device.
    pub(crate) device_loop_trampoline: u32,
    /// Address of the lazily-allocated trampoline template used by the
    /// List Manager to call a guest LDEF drawing procedure.
    pub(crate) list_def_trampoline: u32,
    /// Address of the lazily-allocated trampoline template used by the
    /// Window Manager to call a guest WDEF procedure.
    pub(crate) window_def_trampoline: u32,
    /// Address of the lazily-allocated trampoline template used by the
    /// Control Manager to call a guest CDEF procedure.
    pub(crate) control_def_trampoline: u32,
    /// Reusable trampoline cells for multi-control CDEF callback chains.
    pub(crate) control_def_trampoline_chain: Vec<u32>,
    /// Address of the lazily-allocated trampoline used by DeferUserFn
    /// to call a callable userFunction immediately. Holds
    /// `48E7 F0F0 207C xxxx xxxx 4EB9 xxxx xxxx 4CDF 0F0F 7000 4E75`.
    pub(crate) defer_user_fn_trampoline: u32,
    /// Notification Manager requests in queue order. Each entry is the guest
    /// address of its static NMRec; qLink mirrors this order in guest memory.
    pub(crate) notification_requests: Vec<u32>,
    pub(crate) collection_callback_stack: Vec<super::collection::CollectionCallbackState>,
    pub(crate) collection_callback_trampoline: u32,
    /// Ports that have already been queried through QDDone. BasiliskII
    /// reports TRUE for each query against a live port, so this state is
    /// currently unused by the HLE path.
    pub(crate) qddone_seen_ports: std::collections::HashSet<u32>,
    /// Live Picture Utilities survey IDs minted by NewPictInfo and
    /// cleared by DisposPictInfo.
    pub(crate) pict_info_ids: HashSet<u32>,
    /// Whether the PPC Toolbox has been initialized via selector $0000.
    /// Most PPC selectors gate on this bit; selector $000A (`IPCListPorts`)
    /// on the zero-request local path is allowed before init in the baked
    /// fixture.
    pub(crate) ppc_initialized: bool,
    /// Guest trampoline entered when a ThreadEntryProc returns.
    pub(crate) thread_return_trampoline: u32,
    /// Custom `ThreadSchedulerProcPtr` installed by `SetThreadScheduler`.
    pub(crate) cooperative_thread_scheduler: u32,
    /// Default cooperative stack size reported by
    /// `GetDefaultThreadStackSize` and used when `NewThread` is passed 0.
    /// Synthetic Component Manager instances opened for HLE-provided
    /// components such as the QuickTime movie controller.
    pub(crate) synthetic_component_instances: HashSet<u32>,
    /// Next opaque ComponentInstance value returned by OpenComponent.
    pub(crate) next_synthetic_component_instance: u32,
    /// Saved old structure/content regions keyed by window pointer.
    /// SaveOld snapshots this state and DrawNew consumes it.
    pub(crate) saved_draw_old_regions: HashMap<u32, DrawOldState>,
    /// Whether the registered `kAEOpenApplication` handler has already
    /// been fired via an `AEProcessAppleEvent` dispatch. Distinct from
    /// the process launch state's one-shot bit (which tracks the synthetic
    /// OAPP queued for `WaitNextEvent` delivery): an app may call
    /// `AEProcessAppleEvent` directly without ever pumping events
    /// through WNE, and vice versa, so the two state bits cannot
    /// share a flag.
    pub(crate) fired_oapp_handler: bool,
    /// Cache of allocated synthetic system `'STR '` resource pointers.
    /// Lazily populated by [`Self::synthesize_system_str`] when an
    /// app calls `GetString` (or `Get1Resource('STR ', id)`) for a
    /// well-known System-file ID — for example `-16096` (Owner Name,
    /// Sharing Setup) or `-16413` (Macintosh Name) — that no loaded
    /// resource fork provides. The pointer is held permanently so
    /// repeat calls return the same handle. Networking 1994, 2-799.
    pub(crate) system_str_cache: HashMap<i16, u32>,
    /// Cache of synthetic System-file `'INTL'` resource pointers. Classic
    /// International Utilities expose U.S. numeric/time settings as ID 0 and
    /// U.S. day/month names as ID 1. Systemless does not mount a System file,
    /// so these records are synthesized on demand. Inside Macintosh Volume I
    /// (1985), pp. I-495..I-505.
    pub(crate) system_intl_cache: HashMap<i16, u32>,
    /// Cache of the standard System-file `'PAT#'` ID 0 resource. Classic
    /// applications use this 38-entry list for the MacPaint pattern palette.
    /// Systemless does not mount a System file, so the list is synthesized
    /// after the loaded resource chain misses. Inside Macintosh Volume I
    /// (1985), pp. I-475..I-476.
    pub(crate) system_pattern_list_cache: HashMap<i16, u32>,
    /// Cache of synthesized built-in system cursor blocks for
    /// GetCursor ($A9B9). On real Mac the standard cursor IDs (1
    /// iBeamCursor, 2 crossCursor, 3 plusCursor, 4 watchCursor per
    /// IM:I I-475..I-477) are CURS resources baked into the System
    /// file's resource fork; Systemless doesn't load that fork and
    /// instead synthesizes the bitmap+mask via [`Self::system_cursor`].
    /// Stable handles matter because apps cache the GetCursor result
    /// at boot and pass it to SetCursor every frame: a fresh
    /// allocation per call would leak a 68-byte block per frame.
    /// Inside Macintosh Volume I, I-474.
    pub(crate) system_cursor_cache: HashMap<i16, u32>,
    /// Cache of synthetic System-file `'ICON'` resources used by standard
    /// dialogs. Systemless does not mount a System file, so well-known icons
    /// are synthesized only after the application resource chain misses.
    /// Macintosh Toolbox Essentials (1992), pp. 6-153 and 7-63.
    pub(crate) system_icon_cache: HashMap<i16, u32>,
    /// Cache of synthetic System-file `'clut'` resource pointers for
    /// standard indexed depths. Systemless does not mount the System
    /// resource fork, but some installers call `GetResource('clut', depth)`
    /// directly instead of `GetCTable`.
    pub(crate) system_clut_cache: HashMap<i16, u32>,
    /// Cache of the synthetic System-file `'wctb'` ID 0 resource. The
    /// Window Manager loads this standard table during initialization, and
    /// applications may also retrieve and duplicate it directly.
    pub(crate) system_wctb_cache: HashMap<i16, u32>,
    /// Cache of synthetic System-file `'KCHR'` resource pointers. The
    /// U.S. Roman keyboard-layout resource ID 0 is present in every
    /// System file and is used directly by apps that call KeyTranslate.
    pub(crate) system_kchr_cache: HashMap<i16, u32>,
    /// Cache of the synthetic standard `'KMAP'` ID 0 resource. Classic apps
    /// may read its 128-byte hardware-to-virtual-key map directly when
    /// implementing configurable controls.
    pub(crate) system_kmap_cache: HashMap<i16, u32>,
    /// Cache of synthetic ROM `'WDEF'` resource pointers. WDEF IDs 0 and 1
    /// are the standard document and rounded-window definition functions.
    /// Their behavior is implemented by the Window Manager HLE, but callers
    /// may still fetch the resources directly through GetResource.
    pub(crate) system_wdef_cache: HashMap<i16, u32>,
    /// Cache of the synthetic ROM `'MDEF'` resource used by standard menus.
    /// The Menu Manager HLE owns standard drawing and hit testing, but
    /// MenuInfo.menuProc remains guest-visible and some applications invoke
    /// the procedure directly.
    pub(crate) system_mdef_cache: HashMap<i16, u32>,
    /// Protected callable nonterminal entry returned as the standard `StdPix`
    /// procedure by `SetStdCProcs`. QuickTime (1993), pp. 3-137--3-139 defines
    /// the distinct eight-argument routine; until that operation is complete,
    /// its unique gateway jumps to the source-backed `_Unimplemented` routine
    /// instead of exposing a noncallable host marker.
    pub(crate) std_pix_gateway: u32,
    /// Substitution strings most recently set via `ParamText`. Indices
    /// 0..3 correspond to `^0`..`^3` placeholders in any subsequently
    /// drawn dialog/alert static-text item. Inside Macintosh Volume I,
    /// I-422 (ParamText).
    pub(crate) param_text: SharedProcessDialogText,
    /// Selected UI rendering provider. The default preserves the legacy
    /// System 7 renderer; explicit non-classic providers are allowed to change
    /// chrome pixels without changing guest-visible Toolbox behavior.
    pub(crate) ui_theme_id: UiThemeId,
    /// Virtual filesystem: filename -> data fork contents
    pub vfs: SharedProcessValue<ProcessForkMap>,
    /// Virtual filesystem: filename -> resource fork contents
    pub vfs_rsrc: SharedProcessValue<ProcessForkMap>,
    /// Finder metadata and catalog IDs for VFS file entries.
    pub(crate) vfs_metadata: SharedProcessValue<HashMap<String, VfsMetadata>>,
    /// Canonical process directory catalogue shared with the native adapter.
    /// Files (1992), pp. 2-27--2-29 and 2-190--2-192.
    pub(crate) vfs_directories: SharedProcessValue<Vec<VfsDirectory>>,
    /// Read-only disk-image volumes mounted alongside the synthetic boot volume.
    pub(crate) vfs_volumes: SharedProcessValue<Vec<VfsVolume>>,
    /// Open working directories keyed by working directory reference number.
    pub(crate) working_directories: SharedProcessValue<HashMap<i16, WorkingDirectory>>,
    /// Open file table: refnum -> filename
    pub(crate) open_files: SharedProcessOpenFiles,
    /// Synthetic Device Manager drivers opened by name via PBOpen/OpenDriver.
    pub(crate) synthetic_drivers: HashMap<u16, String>,
    /// Guest SndChannel storage used by writes to the ROM Sound Driver
    /// reference number (-4). Allocated lazily on the first StartSound write.
    pub(crate) legacy_sound_driver_channel: Option<u32>,
    /// Process-owned refnums whose access paths grant write permission.
    /// Files 1992, pp. 2-7--2-8 and 2-121 defines permission as access-path
    /// state and `wrPermErr` (-61) for writes through a read-only path.
    pub(crate) write_refnums: SharedProcessValue<std::collections::HashSet<u16>>,
    /// File position table: refnum -> current byte offset
    pub(crate) file_positions: SharedProcessOpenFilePositions,
    /// Most recent successful PBRead/FSRead from a data fork.
    pub(crate) recent_file_read: Option<RecentFileRead>,
    /// Completed asynchronous File Manager requests awaiting `ioResult`
    /// publication and optional completion-procedure delivery.
    pub(crate) pending_file_completions: SharedProcessValue<VecDeque<PendingFileCompletion>>,
    /// Set of VFS keys whose `ioFlAttrib` lock bit is set.
    /// Maintained by SetFilLock/HSetFLock ($A041/$A241) and
    /// RstFilLock/HRstFLock ($A042/$A242); read by
    /// `fill_file_catalog_info` to set bit 0 of `ioFlAttrib`.
    /// Files 1992, 2-205 (`ioFlAttrib` field), 9302..9352 (HSetFLock/HRstFLock).
    /// Public to mirror `vfs`/`vfs_rsrc` so frontends and tests can
    /// inspect or seed lock state directly.
    pub locked_files: SharedProcessValue<std::collections::HashSet<String>>,
    /// Current MMU addressing mode (0=24-bit, 1=32-bit)
    /// Inside Macintosh Volume V, V-593
    pub(crate) mmu_mode: u8,
    /// Start Manager default video parameter-block bytes
    /// (`DefVideoRec.sdSlot`, `DefVideoRec.sdSResource`) returned by
    /// GetVideoDefault and updated by SetVideoDefault.
    /// Inside Macintosh Volume V, V-354 to V-355.
    pub(crate) default_video_rec: u16,
    /// Start Manager default OS parameter-block bytes returned by
    /// GetOSDefault and updated by SetOSDefault. High byte is the
    /// reserved field (reported as 0), low byte is `sdOSType`.
    /// Inside Macintosh Volume V, V-355.
    pub(crate) default_os_rec: u16,
    /// Start Manager default startup parameter-block bytes returned by
    /// GetDefaultStartup and updated by SetDefaultStartup. Stored as
    /// the raw 4-byte DefStartRec payload.
    /// Inside Macintosh Volume V, p. V-529.
    pub(crate) default_startup_rec: u32,
    /// Next synthetic catalog directory ID for VFS directories.
    pub(crate) next_vfs_dir_id: SharedProcessValue<u32>,
    /// Next stable negative volume reference for an extracted read-only volume.
    pub(crate) next_vfs_volume_ref_num: SharedProcessValue<i16>,
    /// Next synthetic file ID for VFS files.
    pub(crate) next_vfs_file_id: SharedProcessValue<u32>,
    /// Monotonic source for VFS creation and modification timestamps.
    pub(crate) next_vfs_timestamp: SharedProcessValue<u32>,
    /// Next working directory reference number.
    pub(crate) next_working_dir_refnum: SharedProcessValue<i16>,
    /// Foreground application launch queued by LaunchApplication. When
    /// `after_event_yield` is set, the runner starts it after the current
    /// app next yields through WaitNextEvent/EventAvail/GetNextEvent.
    pub(crate) pending_launch_app: Option<PendingLaunchApplication>,
    /// Current default directory.
    pub(crate) default_dir_id: SharedProcessValue<u32>,
    /// Working directory reference number for the application's folder.
    pub(crate) app_wd_refnum: SharedProcessValue<i16>,
    /// Host directory to write output files to (if set)
    pub output_dir: Option<std::path::PathBuf>,
    /// Current foreground color (RGBColor: R, G, B)
    pub(crate) fg_color: (u16, u16, u16),
    /// Current background color (RGBColor: R, G, B)
    pub(crate) bg_color: (u16, u16, u16),
    /// Palette handle and entry retained by PmForeColor/RestoreFore.
    /// SaveFore serializes these GrafVars fields through ColorSpec when set.
    pub(crate) pm_fg_color: Option<(u32, i16)>,
    /// Palette handle and entry retained by PmBackColor/RestoreBack.
    /// SaveBack serializes these GrafVars fields through ColorSpec when set.
    pub(crate) pm_bg_color: Option<(u32, i16)>,
    /// Requested colors for PixPats initialized by MakeRGBPat, keyed by
    /// PixPatHandle. The ROM expands these into depth-specific pattern data;
    /// HLE keeps the source RGB so color fills can resolve it for the current
    /// destination depth at draw time.
    pub(crate) makergbpat_colors: HashMap<u32, (u16, u16, u16)>,
    /// Extra horizontal pixels added to each non-space character
    /// when drawing text, expressed as a Fixed16.16 value. Set by
    /// CharExtra ($AA23) per IM:V V-149.
    pub char_extra: i32,
    /// Current background pattern
    pub bk_pat: [u8; 8],
    /// Current pen location (v, h)
    pub(crate) pn_loc: (i16, i16),
    /// Current pen size (v, h)
    pub(crate) pn_size: (i16, i16),
    /// Current pen mode
    pub(crate) pn_mode: i16,
    /// Current pen pattern
    pub pn_pat: [u8; 8],
    /// Pen visibility counter (negative = hidden). IM:I I-169.
    pub(crate) pn_vis: i16,
    /// Current text font ID
    pub(crate) tx_font: i16,
    /// Current text face/style
    pub(crate) tx_face: i16,
    /// Current text mode
    pub(crate) tx_mode: i16,
    /// Current text size
    pub(crate) tx_size: i16,
    /// Font Manager outline preference (`SetOutlinePreferred` / `GetOutlinePreferred`).
    pub(crate) outline_preferred: bool,
    /// Font Manager glyph-preservation preference (`SetPreserveGlyph` / `GetPreserveGlyph`).
    pub(crate) preserve_glyph: bool,
    /// Process-scoped pacing state for the wrapping Macintosh clock.
    ///
    /// The guest-visible low-memory `Ticks` bytes are authoritative. This
    /// handle is retained only so host scheduling and manager bookkeeping can
    /// share the last value observed at an ABI boundary; it is never a source
    /// from which guest bytes are projected.
    tick_state: SharedProcessTickState,
    /// Tick at which IdleUpdate last reset the Power Manager activity timer.
    /// Inside Macintosh: Devices (1994), p. 6-29.
    pub(crate) power_idle_last_update_tick: u32,
    /// Unbalanced DisableIdle calls. EnableIdle cancels at most one call.
    /// Inside Macintosh: Devices (1994), pp. 6-15 and 6-29--6-30.
    pub(crate) power_idle_disable_count: u32,
    /// Logical serial-port power selected through `_SerialPower`.
    /// Inside Macintosh: Devices (1994), pp. 6-33--6-35.
    pub(crate) serial_port_a_powered: bool,
    pub(crate) serial_port_b_powered: bool,
    pub(crate) fade_trace_remaining: u32,
    /// Total guest instructions retired so far.
    pub(crate) instruction_count: u64,
    /// Front window pointer
    /// Keep activation independent of the shared WindowList's stacking order:
    /// BringToFront does not activate, and an invisible frontmost NewWindow
    /// can be active. Macintosh Toolbox Essentials (1992), pp. 4-76 and 4-90.
    pub(crate) front_window: u32,
    /// Pointer to the Window Manager port (`WMgrPort` low-memory global).
    /// Inside Macintosh Volume I, I-282.
    pub(crate) window_manager_port: u32,
    /// Pointer to the color Window Manager port returned by GetCWMgrPort.
    pub(crate) window_manager_cport: u32,
    /// Counter for generating periodic update events
    pub(crate) event_counter: u32,
    /// Current window title (from WIND resource)
    pub(crate) window_title: String,
    /// Current window bounds (top, left, bottom, right) from WIND resource
    pub(crate) window_bounds: (i16, i16, i16, i16),
    /// Current window definition ID (procID) from WIND resource
    /// Inside Macintosh Volume I, I-299
    /// 0=documentProc, 1=dBoxProc, 2=plainDBox, 3=altDBoxProc, 4=noGrowDocProc
    pub(crate) window_proc_id: i16,
    /// Per-window procID map, keyed by window_ptr. Needed so that chrome
    /// redraws driven by ShowWindow / HideWindow / HiliteWindow can honor
    /// each window's actual procID instead of the globally-tracked
    /// front-window one — otherwise plainDBox (procID=2) windows get a
    /// document-style title bar. Inside Macintosh Volume I, I-274 / I-299.
    pub(crate) window_proc_ids: HashMap<u32, i16>,
    /// Windows whose `NewWindow` bounds lay entirely outside the screen.
    ///
    /// Real hardware draws such a window's frame where the application asked
    /// for it — off-screen, where it is never seen. Applications park a window
    /// there on purpose when they intend to drive its content themselves rather
    /// than let the Window Manager place it; synthesising chrome for one at a
    /// position the application never requested invents pixels the Mac would
    /// not have shown.
    pub(crate) windows_placed_offscreen: std::collections::HashSet<u32>,
    /// Aux-window handles keyed by WindowPtr. BasiliskII/System 7.5.3 gives
    /// each freshly created window a non-NIL AuxWin record, and SetWinColor
    /// mutates that record in place instead of allocating the first one on
    /// demand.
    pub(crate) window_aux_records: HashMap<u32, u32>,
    /// Original PixMapHandle installed when Systemless creates a CGrafPort
    /// window. If guest code later replaces portPixMap with SetPortPix, that
    /// handle describes scratch/offscreen pixels rather than the Window
    /// Manager-owned backing store.
    pub(crate) window_original_pixmaps: HashMap<u32, u32>,
    /// Saved framebuffer pixels under transient/non-document windows.
    /// Used to emulate Window Manager save-under behavior for dialog-like
    /// windows created through the Window Manager rather than Dialog Manager.
    pub(crate) window_saved_under_pixels: HashMap<u32, (i16, i16, i16, i16, SavedPixels)>,
    /// Aux-control state keyed by ControlHandle. On System 7.5.3 in 32-bit
    /// mode, each control has a stable AuxCtlRec even before custom colors are
    /// installed, so HLE GetAuxCtl currently treats aux-record presence as the
    /// caller-visible success bit.
    pub(crate) control_aux_records: HashMap<u32, ControlAuxRecordState>,
    /// Head of the guest-visible AuxCtlRec linked list (`AuxCtlHead`).
    pub(crate) control_aux_head: u32,
    /// Whether the current front window has a close box (goAwayFlag)
    pub(crate) go_away_flag: bool,
    /// Window list in front-to-back order.
    /// Macintosh Toolbox Essentials 1992, p. 4-65
    ///
    /// Invariant the idle-cycle prover relies on: every mutation of this
    /// host mirror is also written into the guest window chain
    /// (`sync_window_list_links`), so a proof's write journal sees it;
    /// the parked-cycle host snapshot carries a copy besides. FrontWindow
    /// and FindWindow are admitted to proofs on that basis
    /// (`runner::idle_cycle_trap_is_journal_complete`).
    pub(crate) window_list: crate::process_context::SharedProcessWindowList,
    /// Whether `window_list` is the process-owned registry rather than a
    /// standalone dispatcher fixture. The classic frame renderer leaves
    /// native-owned windows in this shared list to the native renderer.
    pub(crate) process_window_list_attached: bool,
    /// Set once the game has entered fullscreen (window covers entire screen
    /// and MBarHeight was 0). While set, the menu bar is suppressed even if
    /// the game temporarily restores MBarHeight (e.g. on cursor-at-top).
    pub fullscreen_locked: bool,
    /// Host presentation policy for the classic Mac menu bar.
    pub(crate) menu_bar_policy: crate::runner::MenuBarPolicy,
    /// Whether an initial-kiosk frontend has observed the guest genuinely hide
    /// the menu bar after creating a window. A later reveal releases the kiosk
    /// suppression back to guest control; an explicit DrawMenuBar request does
    /// so immediately.
    pub(crate) initial_kiosk_guest_hide_observed: bool,
    /// Effective host suppression bit used by rendering and hit-testing paths.
    /// Guest-controlled runners default to `false`; explicit frontend policy
    /// may set it while leaving `fullscreen_locked` to model guest state.
    pub menu_bar_hidden: bool,
    /// Sound Manager state (channels, playback buffers).
    pub(crate) sound_manager: SharedProcessSoundManager,
    /// Menus loaded from MENU resources, in order of insertion
    pub(crate) menus: Vec<super::menu::Menu>,
    /// Active menu tracking state (non-None while MenuSelect is tracking the mouse)
    pub(crate) menu_tracking: SharedProcessMenuTracking,
    /// Process-owned nested guest-procedure continuations shared by both CPUs.
    pub(crate) guest_calls: SharedGuestCallStack,
    /// A host-native menu selection waiting for the guest's normal
    /// FindWindow -> MenuSelect event path.  It is consumed only by
    /// MenuSelect and revalidated against the live menu list there.
    ///
    /// Invariant the idle-cycle prover relies on: this is only ever staged
    /// together with `pending_native_menu_event`, so a parked cycle sees a
    /// deliverable event and resumes instead of reusing a FindWindow
    /// answer that would now say `inMenuBar`; the parked-cycle host
    /// snapshot carries a copy besides.
    pub(crate) pending_native_menu_selection: SharedNativeMenuSelection,
    /// Latched menu-bar mouseDown corresponding to
    /// `pending_native_menu_selection`. Unlike an ordinary queued event, this
    /// survives an Event Manager consumer that fetches but ignores menu-bar
    /// clicks during an animation. It is cleared only when MenuSelect accepts
    /// or invalidates the native command.
    pub(crate) pending_native_menu_event: Option<QueuedEvent>,
    /// Guest tick on which the latched native event was most recently
    /// returned. Limit redelivery to once per tick so an animation loop that
    /// ignores mouseDown events can still make forward progress.
    pub(crate) pending_native_menu_event_tick: Option<u32>,
    /// Active control tracking state (currently popup-menu TrackControl).
    pub(crate) control_tracking: Option<ControlTrackingState>,
    /// Active scrollbar thumb indicator tracking state.
    pub(crate) scrollbar_thumb_tracking: Option<ScrollbarThumbTrackingState>,
    /// Active DragWindow tracking state.
    pub(crate) window_tracking: Option<WindowTrackingState>,
    /// Active TrackGoAway close-box tracking state.
    pub(crate) go_away_tracking: Option<GoAwayTrackingState>,
    /// Active TrackBox zoom-box tracking state.
    pub(crate) zoom_box_tracking: Option<ZoomBoxTrackingState>,
    /// Active GrowWindow size tracking state.
    pub(crate) grow_window_tracking: Option<GrowWindowTrackingState>,
    /// Active DragGrayRgn / DragTheRgn tracking state.
    pub(crate) region_tracking: Option<RegionTrackingState>,
    /// Underline info for continuous underline across a string (set by draw_string)
    pub(crate) underline_info: Option<UnderlineInfo>,
    /// Process-owned live mouse, keyboard, and key-repeat state.
    pub(crate) input_state: SharedProcessInputState,
    /// Debug counter for GetKeys calls that observed at least one held key.
    pub debug_getkeys_nonzero_count: u64,
    /// Last non-zero KeyMap returned by GetKeys. Used by regression tests to
    /// prove games are polling the same key state a frontend injected.
    pub debug_last_getkeys_nonzero_key_map: [u8; 16],
    /// Debug counter for keyDown/keyUp records delivered through Event Manager.
    pub debug_key_event_delivery_count: u64,
    /// Last keyDown/keyUp EventRecord.message delivered through Event Manager.
    pub debug_last_key_event_message: u32,
    /// Most recent EventRecord exposed to 68K guest code, retaining all
    /// fields including the posting timestamp for architecture-neutral tests.
    pub debug_last_event_record: Option<EventRecordSnapshot>,
    /// Most recent results from the direct mouse-state traps used by the
    /// showcase's Event Manager page.
    pub debug_last_button_result: Option<bool>,
    pub debug_last_still_down_result: Option<bool>,
    pub debug_last_wait_mouse_up_result: Option<bool>,
    /// Most recent Event Manager post/peek/take results, retained after the
    /// queue entry has been consumed by a later call.
    pub debug_event_queue_probe: EventQueueProbeSnapshot,
    /// Whether an activateEvt/updateEvt has been delivered to the guest.
    pub debug_activation_event_seen: bool,
    pub debug_update_event_seen: bool,
    /// Debug counter for WaitNextEvent calls observed by scripted probes.
    pub debug_wait_next_event_count: u64,
    /// Debug counter for GetNextEvent calls observed by scripted probes.
    pub debug_get_next_event_count: u64,
    /// Debug counter for mouse-moved OS events synthesized by WaitNextEvent.
    pub debug_mouse_moved_event_count: u64,
    /// Debug counter for GetMouse calls observed by scripted probes.
    pub debug_get_mouse_count: u64,
    /// Debug snapshots for GetMouse coordinate conversion.
    pub debug_get_mouse_local_change_count: u64,
    pub debug_get_mouse_last_local: (i16, i16),
    pub debug_get_mouse_last_global: (i16, i16),
    pub debug_get_mouse_last_port: u32,
    pub debug_get_mouse_last_port_bounds_top_left: (i16, i16),
    /// Debug counters for StillDown return values observed by scripted probes.
    pub debug_still_down_true_count: u64,
    pub debug_still_down_false_count: u64,
    /// Debug counters for Button return values observed by scripted probes.
    pub debug_button_true_count: u64,
    pub debug_button_false_count: u64,
    /// Debug counters for WaitMouseUp return values observed by scripted probes.
    pub debug_wait_mouse_up_true_count: u64,
    pub debug_wait_mouse_up_false_count: u64,
    /// Debug counters for QuickDraw activity during scripted probes.
    pub debug_set_origin_count: u64,
    pub debug_copy_bits_count: u64,
    pub debug_scroll_rect_count: u64,
    pub debug_scroll_rect_nonzero_delta_count: u64,
    pub debug_scroll_rect_changed_byte_count: u64,
    pub debug_scroll_rect_last_changed_bytes: u64,
    pub debug_scroll_rect_last_rect: (i16, i16, i16, i16),
    pub debug_scroll_rect_last_delta: (i16, i16),
    pub debug_scroll_rect_last_port: u32,
    pub debug_scroll_rect_last_base: u32,
    pub debug_scroll_rect_last_row_bytes: u16,
    pub debug_scroll_rect_last_port_bounds_top_left: (i16, i16),
    pub debug_scroll_rect_last_is_color: bool,
    /// Deterministic input trace, enabled through
    /// `TrapDispatcher::enable_input_trace_capture`; normal execution leaves
    /// this off so dialog/menu/control hot paths do not allocate.
    pub(crate) input_trace_enabled: bool,
    pub(crate) input_trace_log: Vec<String>,
    /// Queued events (mouseDown, mouseUp, etc.) to deliver via GetNextEvent
    pub(crate) event_queue: SharedProcessEventQueue,
    /// A mouseDown consumed by ModalDialog can return to the application
    /// before the physical release arrives. Keep ownership of that release
    /// even if the application disposes the dialog in the meantime.
    pub(crate) pending_modal_dialog_mouse_up: bool,
    /// Event record for the ModalDialog-owned press. Some application-owned
    /// handlers leave a copy of that mouseDown queued while disposing the
    /// dialog, so retain its identity instead of discarding an arbitrary
    /// earlier mouseDown.
    pub(crate) pending_modal_dialog_mouse_down: Option<QueuedEvent>,
    /// One-shot update events recovered after FlushEvents drops queue entries
    /// while the Window Manager update region remains dirty.
    pub(crate) flushed_update_events: VecDeque<QueuedEvent>,
    /// Full trap word currently being dispatched. Some OS traps share the
    /// low 8-bit trap number and require bit 8 to distinguish variants.
    pub(crate) current_trap_word: u16,
    /// Generated canonical operation row and the actual first-match adapter
    /// selected for the current default dispatch.
    pub(crate) current_trap_operation: u16,
    pub(crate) current_trap_adapter: TrapAdapterId,
    /// Generated selector-operation row selected by a dispatcher, when that
    /// selector family has been joined to the runtime registry.
    pub(crate) current_selector_operation: Option<&'static str>,
    /// When an auto-pop trap fires (bit 10 set in toolbox trap word),
    /// dispatch.rs pops the JSR return address and stores it here BEFORE
    /// calling the sub-dispatcher. Sub-dispatchers (e.g. SANE handlers) can
    /// read this for diagnostics — it identifies the actual game-side caller,
    /// not the JUMP TABLE entry where the trap word lives. None for non-auto-pop
    /// traps. Cleared back to None after the trap returns.
    pub(crate) current_trap_caller: Option<u32>,
    /// Elapsed null-event sleep requested by WaitNextEvent and waiting to be
    /// applied by the runner before guest execution resumes.
    /// Macintosh Toolbox Essentials 1992, p. 2-22
    pub(crate) pending_wait_sleep_ticks: u32,
    /// Return slots for a WaitNextEvent null result whose sleep has not yet
    /// expired. If input arrives during that sleep, the runner rewrites the
    /// EventRecord/result before foreground guest code resumes.
    pub(crate) pending_wait_next_event_return: Option<PendingWaitNextEventReturn>,
    /// Extra instruction-budget units reported by HLE traps that completed
    /// sizeable manager work inside Rust rather than through guest 68k code.
    pub(crate) pending_hle_tick_cost: i32,
    /// True while the runner is servicing a GUI/realtime frontend slice.
    /// Direct/headless stepping leaves this false so package calls that used
    /// to be immediate remain deterministic in non-interactive tests.
    pub(crate) yield_for_ui: bool,
    /// Remaining ticks for the Delay ($A03B) trap to consume.
    /// On a real Mac, Delay blocks the application for numTicks; in our HLE
    /// the runner drains these one-at-a-time via advance_guest_tick().
    /// Inside Macintosh Volume II, II-384
    pub pending_delay_ticks: u32,
    /// Process-owned cursor image and signed visibility level.
    pub(crate) cursor_state: SharedProcessCursorState,
    /// Total number of A-line trap dispatches since emulator start.
    pub trap_count: u64,
    /// A-line traps dispatched from game code only (PC < 0x800000).
    /// Excludes ROM/system traps for cross-emulator deterministic sync.
    pub game_trap_count: u64,
    /// Per-trap dispatch counter, populated only when
    /// `SYSTEMLESS_TRACE_TRAP_COUNTS=1` is set. Indexed by the low 12 bits of
    /// the trap word. Dump via `print_trap_histogram`.
    pub trap_histogram: Box<[u64; 4096]>,
    /// Per-trap accumulated wall-clock time (ns), populated only when
    /// `SYSTEMLESS_TRACE_TRAP_TIMING=1` is set. The Instant::now() call adds
    /// ~20-30ns measurement overhead per trap when enabled. Dump via
    /// `print_trap_timing_histogram`.
    pub trap_time_ns: Box<[u64; 4096]>,
    /// Number of copybits_screen events emitted (screen-affecting draws).
    pub copybits_screen_count: u64,
    /// Most recent sizeable CopyBits blit into the screen framebuffer.
    pub last_screen_copybits_rect: Option<ScreenCopyBitsRect>,
    /// Largest non-fullscreen FrameRect drawn into the screen framebuffer in
    /// the most recent guest tick that drew one. A matching retained CPort can
    /// use this explicit guest geometry to locate its framed presentation
    /// without assuming it is centered.
    pub(crate) last_screen_frame_rect: Option<ScreenCopyBitsRect>,
    pub(crate) last_screen_frame_rect_tick: u32,
    /// Count of all screen-affecting trace events captured so far.
    pub screen_event_count: u64,
    /// `screen_event_count` values where the recorded event was specifically
    /// a `copybits_screen` (framebuffer-mutating blit), in emission order.
    /// Used by the trace interpreter to rebind checkpoints away from
    /// non-CopyBits screen events (e.g. SetEntries CLUT updates) so the
    /// captured snapshot reflects a settled framebuffer rather than a
    /// transient mid-fade palette.
    pub copybits_screen_secs: Vec<u64>,
    /// Optional trace sink for deterministic event/snapshot capture.
    pub(crate) trace_sink: Option<Box<dyn TraceSink>>,
    /// Main GDevice handle in guest memory (0 = not yet allocated)
    pub(crate) main_gdevice_handle: u32,
    /// Current GDevice handle
    pub(crate) current_gdevice: SharedProcessGraphicsDevice,
    /// Current GrafPort/GWorld pointer
    pub(crate) current_port: SharedProcessGraphicsPort,
    /// Error from the last applicable Color QuickDraw or Color Manager call.
    pub(crate) quickdraw_error: SharedProcessQuickDrawError,
    /// Process-owned fallback for ports whose guest record has no
    /// allocator-managed GrafVars handle. A valid guest GrafVars record is
    /// always preferred by OpColor reads and writes.
    pub(crate) quickdraw_op_colors: SharedProcessQuickDrawOpColors,
    /// Process-owned fallback for ports whose guest record has no
    /// allocator-managed GrafVars handle. A valid guest GrafVars record is
    /// always preferred by HiliteColor reads and writes.
    pub(crate) quickdraw_hilite_colors: SharedProcessQuickDrawHiliteColors,
    /// Whether the attached process's current CGrafPort record is canonical
    /// for draw state shared with the native QuickDraw adapter.
    pub(crate) process_quickdraw_port_state_attached: bool,
    /// Per-port pen/color/text state restored by SetPort and SetGWorld.
    pub(crate) port_draw_states: HashMap<u32, PortDrawState>,
    /// Bit 0/1 mark CGrafPort fgColor/bkColor fields that QuickDraw has
    /// resolved through a color-setting call. Once resolved, guest writes to
    /// those indexed pixel fields remain authoritative for drawing.
    pub(crate) resolved_port_color_fields: HashMap<u32, u8>,
    /// Associated GDevice handle for each offscreen GWorld port.
    pub(crate) gworld_devices: HashMap<u32, u32>,
    /// Compatibility map for `&port->portBits` addresses (key = `port + 2`)
    /// to their most recently known-good bitmap snapshot. Used to recover
    /// CopyBits calls when guest code passes a stale/clobbered cGrafPort
    /// portBits record whose live handle/pixmap fields are invalid.
    pub(crate) disposed_gworld_portbits: HashMap<u32, CachedCopyBitmapInfo>,
    /// Process-owned pixel-state flags keyed by offscreen PixMapHandle. The
    /// `keepLocal`, `pixelsPurgeable`, and `pixelsLocked` subset is surfaced by
    /// GetPixelsState / SetPixelsState and the direct LockPixels /
    /// UnlockPixels aliases; guest storage and adapter allocation records stay
    /// outside this non-owning registry. Imaging With QuickDraw 1994, 6-36..6-38.
    pub(crate) gworld_pixel_states: SharedProcessQuickDrawPixelStates,
    /// Non-GWorld CGrafPorts opened via OpenCPort/InitCPort, tracked so
    /// sync_canonical_offscreen_ctabs_to_clut can reach their pixmaps.
    pub(crate) cport_ports: HashSet<u32>,
    /// PixMapHandle installed when OpenCPort/InitCPort initialized each
    /// app-managed CGrafPort. SetPortPix can replace that handle with an
    /// offscreen scratch image; such a replacement is not an onscreen port.
    pub(crate) cport_original_pixmaps: HashMap<u32, u32>,
    /// Non-window CGrafPort selected for HLE fallback presentation.
    pub(crate) manual_cport_presented_port: u32,
    /// Sparse snapshot of the screen immediately after presenting the manual
    /// CPort. If the guest substantially changes those pixels before the next
    /// redraw, the physical framebuffer has become the authoritative display
    /// surface and the fallback presentation latch must yield.
    pub(crate) manual_cport_screen_witness: Vec<u8>,
    /// Polygon recording state. When `Some`, LineTo/MoveTo calls append
    /// vertices. Set by OpenPoly, consumed by ClosePoly.
    pub(crate) recording_polygon: Option<PolygonRecording>,
    /// Region recording state. Set by OpenRgn, consumed by CloseRgn.
    pub(crate) recording_region: Option<RegionRecording>,
    /// Screen mode: (screen_base, row_bytes, width, height, pixel_size)
    /// Defaults to 800x600 8bpp.
    pub screen_mode: (u32, u32, u16, u16, u16),
    /// Initial host/user-selected logical geometry. Guest Display Manager
    /// switches may temporarily change `screen_mode`; this remains the native
    /// mode advertised for restoration and explicit override semantics.
    pub(crate) native_screen_geometry: (u16, u16),
    /// Runtime device CLUT for 8bpp mode. 256 entries of [R, G, B] in 16-bit Mac values.
    /// Initialized to the standard Mac 8-bit system palette. Updated by SetEntries trap
    /// and low-level video driver cscSetEntries. Used for DISPLAY rendering only.
    pub device_clut: SharedProcessDisplayClut,
    /// Per-channel transfer tables installed by the video driver's
    /// `cscSetGamma` control call. These affect presentation only; the device
    /// and Color Manager CLUTs retain the guest's uncorrected 16-bit values.
    pub(crate) display_gamma: crate::process_context::SharedProcessDisplayGamma,
    /// Color Manager CLUT for 8bpp mode. Updated only by high-level SetEntries ($AA3F)
    /// and ActivatePalette — NOT by low-level video driver palette fades.
    /// Used by QuickDraw shape drawing (PaintRect, etc.) for RGB→index mapping,
    /// mirroring the real Mac OS ITable which is derived from the Color Manager palette.
    /// Imaging With QuickDraw 1994, p. 4-82
    pub color_manager_clut: SharedProcessDisplayClut,
    /// Cached inverse-table payloads keyed by actual CLUT contents and
    /// resolution. Used by MakeITable and bounded to avoid retaining arbitrary
    /// game palettes indefinitely.
    pub(crate) inverse_table_cache: Vec<InverseTableCacheEntry>,
    /// Per-entry protection bits for the device CLUT, set by ProtectEntry
    /// ($AA3D) and cleared by ProtectEntry(false). When `clut_protected[i]`
    /// is true, SetEntries refuses to overwrite `device_clut[i]`.
    /// Inside Macintosh Volume V, V-145
    pub clut_protected: [bool; 256],
    /// Per-entry reservation bits for the device CLUT, set by ReserveEntry
    /// ($AA3E) and cleared by ReserveEntry(false). When `clut_reserved[i]`
    /// is true the entry is excluded from Color2Index / RGBForeColor
    /// matching (palette-animation slots), and SetEntries refuses to
    /// overwrite it from a different client.
    /// Inside Macintosh Volume V, V-145
    pub clut_reserved: [bool; 256],
    /// Tick until which a screen-backed DrawPicture-seeded palette should be
    /// preserved against unrelated system-palette restore traffic.
    pub(crate) seeded_picture_palette_until_tick: u32,
    /// Palette captured from a screen-backed DrawPicture during title/logo
    /// startup. While the seed window is active, canonical full-table
    /// SetEntries fades are applied as brightness changes over this palette
    /// instead of clobbering it back to the system CLUT.
    pub(crate) seeded_picture_palette: [[u16; 3]; 256],
    /// True while the Palette Manager has left the screen hardware CLUT on a
    /// transient full-table fade frame while retaining the prior logical
    /// GDevice table for inverse-table lookups.
    pub(crate) screen_palette_fade_active: bool,
    /// Most recent non-system GetCTable resource fetch. Some games fetch a
    /// CLUT immediately before drawing a screen-backed PICT and expect that
    /// table to drive the initial palette seed for the picture.
    pub(crate) recent_resource_ctable_fetch: Option<RecentColorTableFetch>,
    /// Window palette associations keyed by WindowPtr. A key of `0xFFFF_FFFF`
    /// acts as the application/default palette sentinel.
    pub(crate) window_palettes: HashMap<u32, (u32, i16)>,
    /// Palette update flags keyed by PaletteHandle.
    pub(crate) palette_updates: HashMap<u32, i16>,
    /// Device indices assigned to palette entries by the most recent
    /// activation. Ordinary tolerant entries are not tied to their palette
    /// positions, so Entry2Index must consult this allocation rather than
    /// treating the entry number as a pixel value.
    pub(crate) palette_device_indices: HashMap<(u32, u16), u8>,
    /// Saved menu-bar pixels and retained text coverage for unchanged chrome.
    pub(crate) menu_bar_cache: std::cell::RefCell<Option<super::framebuffer::MenuBarCache>>,
    /// Recently painted classic indexed title bars, including outline coverage.
    pub(crate) window_title_cache: std::cell::RefCell<Vec<super::framebuffer::WindowTitleCache>>,
    /// The menu mark's device indices for the current main-device colour
    /// table; see `MenuMarkIndexCache`.
    pub(crate) menu_mark_indices: std::cell::Cell<Option<super::framebuffer::MenuMarkIndexCache>>,
    /// Host-side copy of the main device's colour table; see `ColorTableMirror`.
    pub(crate) color_mirror: std::cell::RefCell<super::framebuffer::ColorTableMirror>,
    /// Set for the duration of a chrome pass, whose lookups reuse the mirror.
    pub(crate) color_mirror_fresh: std::cell::Cell<bool>,
    /// Themed chrome rendered earlier, replayed while its inputs hold; see
    /// `ThemeChromeCacheEntry`.
    pub(crate) theme_chrome_cache: std::cell::RefCell<Vec<super::framebuffer::ThemeChromeCacheEntry>>,
    /// Color tables produced from palettes whose entries are all pmExplicit.
    /// Their pixel values are literal device indices, so indexed CopyBits
    /// must preserve those values instead of color-matching duplicate RGBs.
    pub(crate) explicit_palette_ctabs: HashSet<u32>,
    /// Transform supplied by an Icon Utilities handle call while it routes
    /// through the legacy icon renderer. Zero for ordinary PlotCIcon calls.
    pub(crate) icon_transform_override: i16,
    /// Printing Manager error code surfaced by `PrError` and set by
    /// `PrSetError`. Inside Macintosh Volume II 1985, p. II-161;
    /// Inside Macintosh Volume V 1986, p. V-408.
    pub(crate) printing_error: i16,
    /// Monotonic source for Color Manager `ctSeed` values.
    pub(crate) next_ct_seed: u32,
    /// Optional override pattern for FillRect when the game passes the QD `black`
    /// global as the fill pattern. Used to work around games that should use a
    /// dithered city/object pattern but were compiled with `black` instead.
    pub fill_black_override: Option<[u8; 8]>,
    /// Active picture recording state:
    /// (pic_handle, frame top, left, bottom, right, encoded PICT v2 commands).
    /// Set by OpenPicture, cleared by ClosePicture.
    pub(crate) recording_picture: Option<(u32, i16, i16, i16, i16, Vec<u8>)>,
    /// Complete bitmap PICT captured by CopyBits during OpenPicture.
    pub(crate) recording_picture_bitmap: Option<Vec<u8>>,
    /// Machine profile belonging to the currently installed process table.
    /// `None` means no application trap context is active.
    pub(crate) trap_table_profile: Option<TrapTableProfile>,
    /// Generated default handlers for exception vectors 10 (`$28`) and 11
    /// (`$2C`) in the active process context. The writable low-memory vector
    /// cells remain authoritative: a different value delegates the fault to
    /// guest 68k code instead of the HLE path.
    pub(crate) trap_exception_vector_defaults: Option<[u32; 2]>,
    /// Original calls retained for each active native trap handler. The value
    /// is a LIFO stack because a patch can re-enter the same A-line trap before
    /// the outer invocation follows its saved daisy-chain link. Inside
    /// Macintosh: Operating System Utilities (1994), pp. 8-8 and 8-23--8-24.
    pub(crate) pending_native_trap_calls: HashMap<u16, Vec<NativeTrapCallState>>,
    /// Re-entrancy guard for the CopyBits `grafProcs.bitsProc` bottleneck:
    /// `(bitsProc address, stack pointer at the tail call)`. A custom bitsProc
    /// normally reaches the real transfer by calling CopyBits again; without
    /// this guard that second call would be handed back to the same proc
    /// forever. While the stack pointer is still at or below the recorded value
    /// we are nested inside the proc, so CopyBits performs the blit itself.
    pub(crate) bits_proc_reentry: Option<(u32, u32)>,
    /// Installed Time Manager tasks.
    /// Processes 1994, 3-14
    pub(crate) timer_tasks: crate::process_context::SharedProcessTimerTasks,
    /// Deferred Task Manager queue in FIFO order, with the first guest tick
    /// on which each entry may run. Guest qLink and DTQueue mirror this queue.
    pub(crate) deferred_tasks: VecDeque<(u32, u32)>,
    /// A task installed by a hardware callback may run at that interrupt's end.
    pub(crate) deferred_install_in_interrupt: bool,
    /// Process-owned callback scheduling metadata.
    pub(crate) callback_scheduling: crate::process_context::SharedProcessCallbackScheduling,
    /// Ordered Power Manager sleep queue. Each entry is a guest SleepQRec;
    /// its first longword remains the guest-visible next link.
    pub(crate) sleep_queue: Vec<u32>,
    /// Installed Vertical Retrace Manager tasks.
    /// Processes 1994, 4-6 to 4-7
    pub(crate) vbl_tasks: crate::process_context::SharedProcessVblTasks,
    /// Active dialog tracking state (non-None while ModalDialog is tracking input)
    pub dialog_tracking: Option<DialogTrackingState>,
    pub(crate) suspended_modal_dialogs: Vec<DialogTrackingState>,
    /// Active Standard File Package save dialog tracking state.
    pub(crate) standard_file_put_tracking: Option<StandardFilePutTrackingState>,
    /// Active Standard File Package open dialog tracking state.
    pub(crate) standard_file_get_tracking: Option<StandardFileGetTrackingState>,
    /// Bounds owned by retained host overlays in an attached CPU adapter.
    /// These surfaces draw directly into the framebuffer without WindowRecords.
    pub(crate) external_host_overlay_rects: Vec<(i16, i16, i16, i16)>,
    /// Parsed dialog items keyed by dialog pointer, for GetDItem/ModalDialog
    pub dialog_items: HashMap<u32, Vec<DialogItem>>,
    /// Original rects for items hidden via HideDialogItem,
    /// keyed by (dialog_ptr, 1-based item_no). Restored by ShowDialogItem.
    pub(crate) hidden_dialog_item_rects: HashMap<(u32, i16), (i16, i16, i16, i16)>,
    /// Maps guest handle address → (dialog_ptr, 0-based item index) for SetDialogItemText
    pub(crate) dialog_item_handles: HashMap<u32, (u32, usize)>,
    /// Control values for dialog items: (dialog_ptr, 1-based item_no) → value (0/1 for checkboxes)
    /// Inside Macintosh Volume I, I-327
    pub(crate) dialog_control_values: HashMap<(u32, i16), i16>,
    /// Maps guest ControlHandle address → (dialog_ptr, 1-based item_no) for Get/SetControlValue
    pub(crate) dialog_control_handles: HashMap<u32, (u32, i16)>,
    /// Guest-resident shim returned by DialogDispatch selector $03
    /// GetStdFilterProc. Lazily allocated on first use; 0 = not yet
    /// allocated.
    pub(crate) dialog_std_filter_proc: u32,
    /// Host-side per-dialog cancel-item overrides set before ModalDialog
    /// creates a tracking state.
    pub(crate) dialog_cancel_items: HashMap<u32, i16>,
    /// Guest-memory address of the 2-byte scratch location where the filter
    /// proc trampoline writes its Boolean return value. Set by the runner
    /// when the trampoline is first allocated; 0 = not yet allocated.
    pub(crate) dialog_filter_result_addr: u32,
    /// Saved background pixels for dialogs that returned a non-dismissing item
    /// (e.g., checkbox click). Keyed by dialog_ptr. Reused when ModalDialog re-enters.
    pub(crate) dialog_saved_pixels: HashMap<u32, SavedPixels>,
    /// Rendered front-dialog pixels retained after a visible dialog draw,
    /// including first-show shells and ModalDialog returns before DisposDialog
    /// closes the window.
    pub(crate) dialog_visible_snapshots: HashMap<u32, PersistentDialogSnapshot>,
    /// Dialogs for which ModalDialog has completed its first-call setup (drew
    /// controls, snapshotted pixels). On re-entry we skip draw_dialog to
    /// preserve game-drawn custom content (e.g. PICT titles, group boxes).
    pub(crate) dialog_modal_entered: std::collections::HashSet<u32>,
    /// Dialogs whose application CDEF draw callbacks have just completed and
    /// whose next ModalDialog re-fire must snapshot those pixels without an
    /// intervening HLE standard-item redraw.
    pub(crate) dialog_cdef_draw_pending_snapshot: HashSet<u32>,
    /// Dialogs whose application CDEF controls have completed at least one
    /// visible whole-control draw pass.
    pub(crate) dialog_cdefs_initially_drawn: HashSet<u32>,
    /// Editable dialog items whose initial all-selected text state has already
    /// been replaced by typed input. Keyed by (dialog_ptr, 1-based item number)
    /// so ModalDialog re-entry keeps appending instead of replacing again.
    pub(crate) dialog_edit_text_modified_items: HashSet<(u32, i16)>,
    /// Visible dialogs whose initial NewDialog/GetNewDialog draw was deferred
    /// because one or more in-bounds userItem draw procs had not yet been
    /// installed. If such a dialog is disposed before DrawDialog/ModalDialog
    /// paints it, there are no dialog pixels to erase from the screen.
    pub(crate) dialog_initial_draw_deferred: HashSet<u32>,
    /// userItem draw procs queued by modeless/dialog-show paths outside
    /// ModalDialog. Drained through the same runner trampoline as modal
    /// draw procs.
    pub(crate) modeless_dialog_draw_proc_queue: VecDeque<(u32, u32, i16)>,
    /// Dialogs whose application CDEF draw callbacks must run after any
    /// modeless userItem callbacks queued by the same Dialog Manager redraw.
    pub(crate) modeless_dialog_cdef_draw_queue: VecDeque<u32>,
    /// Dialog currently executing a modeless userItem draw proc.
    pub(crate) active_modeless_dialog_draw_proc: Option<u32>,
    /// Mouse click currently captured by a front modal dialog. This includes
    /// ModalDialog-retained clicks and app-owned modal button presses.
    pub(crate) retained_modal_dialog_click: Option<RetainedModalDialogClickState>,
    /// One-shot recovery for the common ModalDialog button-return pattern.
    /// Real applications normally call DisposDialog with the dialog pointer
    /// immediately after a button item is returned. If HLE callback/stack
    /// interleaving leaves the app passing a stale non-dialog pointer, this
    /// lets the next DisposDialog target the front retained modal dialog
    /// without translating arbitrary userItem ProcPtr arguments.
    pub(crate) pending_modal_button_dispose_dialog: Option<u32>,
    /// Stack of saved window state for restoring front_window/bounds when
    /// dialogs are disposed. Each GetNewDialog pushes the current state;
    /// DisposDialog pops it. Tuple shape:
    /// `(front_window_ptr, bounds_rect, proc_id, title)`.
    /// Inside Macintosh Volume I, I-274 (Window List)
    #[allow(clippy::type_complexity)] // 4-element tuple — narrower than a 4-field struct alias
    pub(crate) window_stack: Vec<(u32, (i16, i16, i16, i16), i16, String)>,
    /// Saved visRgn for active BeginUpdate/EndUpdate pairs, keyed by window.
    /// Inside Macintosh Volume I, I-292 to I-293
    pub(crate) saved_vis_regions: HashMap<u32, (i16, i16, i16, i16)>,
    /// Process-owned List Manager state shared with native execution.
    pub(crate) list_states: SharedProcessListManager,
    pub(crate) collections: SharedProcessCollectionManager,
    /// Process-owned TextEdit feature state shared with native execution.
    pub(crate) textedit_states: SharedProcessTextEditManager,
    /// Process-owned Control Manager metadata shared with native execution.
    pub(crate) control_manager: SharedProcessControlManager,
    /// The menu ID of the most recently inserted menu (via InsertMenu).
    /// Cleared when a type-0 userItem GetDItem is called immediately after.
    pub(crate) last_inserted_menu_id: Option<i16>,
    /// Pending InsertMenu → GetDItem popup association. Confirmed only when
    /// the app installs a draw proc for that same userItem with SetDItem.
    pub(crate) pending_dialog_popup_menu: Option<PendingDialogPopupMenu>,
    /// Associates type-0 (userItem) dialog slots with popup menu IDs.
    /// Established by the InsertMenu → GetDItem → SetDItem pattern that games
    /// use when setting up custom popup controls in dialogs.
    /// Key: (dialog_ptr, 1-based item_no), Value: menu_id
    pub(crate) dialog_item_popup_menus: HashMap<(u32, i16), i16>,
    /// Original DITL rects for popup userItems, saved before SetDItem narrows them.
    /// Key: (dialog_ptr, 1-based item_no), Value: (top, left, bottom, right)
    pub(crate) dialog_popup_original_rects: HashMap<(u32, i16), (i16, i16, i16, i16)>,
    /// Popup-like userItems detected by geometry narrowing rather than a draw
    /// ProcPtr install. Some apps query a full-width userItem, shrink it to a
    /// small arrow hit rect with SetDItem, and draw the menu title separately.
    pub(crate) dialog_popup_candidate_items: HashSet<(u32, i16)>,
    /// Process-owned desk scrap shared by classic and native gateways.
    pub(crate) scrap: SharedProcessScrapState,
    /// Most recent pack ID passed to InitPack.
    /// Kept as lightweight bookkeeping for future pack-specific heuristics.
    pub last_init_pack_id: Option<i16>,
}

pub(crate) type ResourceFileMap = ProcessResourceFileMap;

/// Synthetic Movie Toolbox state for Movie handles returned by
/// NewMovieFromFile/NewMovie-style traps.
#[derive(Clone, Debug)]
pub(crate) struct MovieState {
    pub box_rect: (i16, i16, i16, i16),
    pub gworld_port: u32,
    pub gworld_gdh: u32,
    pub volume: i16,
    pub preferred_rate: i32,
    pub rate: i32,
    pub current_time: i32,
    pub duration: i32,
    pub time_scale: i32,
    pub active: bool,
    /// Parsed video track (sample tables + codec), if the movie carries one.
    pub media: Option<super::movie_media::VideoTrack>,
    pub music: Option<Vec<super::movie_media::MusicNote>>,
    pub audio_time: f64,
    pub time_base_flags: u32,
    /// The movie's data-fork bytes; `media` sample offsets index into this.
    pub data_fork: Vec<u8>,
    /// Lazily-created Cinepak decoder, retained so inter frames composite on
    /// the prior reconstructed frame.
    pub decoder: Option<super::cinepak::CinepakDecoder>,
    /// Lazily-created QuickTime Animation (`rle `) decoder, retained across
    /// frames for the same reason.
    pub rle_decoder: Option<super::qtrle::QtRleDecoder>,
    /// Index of the sample most recently decoded and blitted, to avoid
    /// redundant re-decodes while the timeline sits on one frame.
    pub rendered_sample: Option<usize>,
    /// Guest tick at which playback was last serviced, used to advance the
    /// movie clock by real elapsed time rather than jumping to the end.
    pub last_service_tick: Option<u32>,
}

impl MovieState {
    pub(crate) fn new(
        _res_refnum: u16,
        _res_id: i16,
        _flags: u16,
        box_rect: (i16, i16, i16, i16),
        duration: i32,
        time_scale: i32,
    ) -> Self {
        Self {
            box_rect,
            gworld_port: 0,
            gworld_gdh: 0,
            volume: 0x0100,
            preferred_rate: 0x0001_0000,
            rate: 0,
            current_time: 0,
            duration: duration.max(1),
            time_scale: time_scale.max(1),
            active: true,
            media: None,
            music: None,
            audio_time: 0.0,
            time_base_flags: 0,
            data_fork: Vec::new(),
            decoder: None,
            rle_decoder: None,
            rendered_sample: None,
            last_service_tick: None,
        }
    }
}

pub(crate) type LoadedResources = ProcessLoadedResources;

impl std::ops::Deref for TrapDispatcher {
    type Target = ProcessResourceManagerState;

    fn deref(&self) -> &Self::Target {
        &self.process_file_system.resource_manager
    }
}

impl TrapDispatcher {
    pub(crate) fn current_process_application_metadata(&self) -> ProcessApplicationMetadata {
        resolve_process_application_metadata(
            &self.process_file_system.vfs_directories,
            &self.process_file_system.vfs_files,
            &self.process_file_system.resource_manager.vfs_resource_files,
            Some(&self.vfs_metadata),
            self.process_file_system.launched_app_path.as_deref(),
        )
    }

    /// Mutate process-owned Resource Manager state for one serialized trap
    /// operation without exposing a mutable reference through `DerefMut`.
    pub(crate) fn with_resource_manager_mut<R>(
        &mut self,
        operation: impl FnOnce(&mut ProcessResourceManagerState) -> R,
    ) -> R {
        let resource_manager = self.process_file_system.resource_manager.shared_handle();
        resource_manager.with_mut(operation)
    }

    #[cfg(test)]
    pub(crate) fn set_loaded_resources_for_test(&mut self, resources: LoadedResources) {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager.resources = Some(resources);
        });
    }

    #[cfg(test)]
    pub(crate) fn insert_loaded_resource_handle_for_test(
        &mut self,
        handle: u32,
        resource: (u32, [u8; 4], i16),
    ) {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager.loaded_handles.insert(handle, resource);
        });
    }

    #[cfg(test)]
    pub(crate) fn insert_resource_handle_file_for_test(&mut self, handle: u32, refnum: u16) {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager.resource_handle_files.insert(handle, refnum);
        });
    }

    #[cfg(test)]
    pub(crate) fn insert_detached_resource_handle_for_test(
        &mut self,
        handle: u32,
        resource: ([u8; 4], i16),
    ) {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager.detached_handles.insert(handle, resource);
        });
    }

    #[cfg(test)]
    pub(crate) fn insert_detached_resource_handle_file_for_test(
        &mut self,
        handle: u32,
        refnum: u16,
    ) {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager.detached_handle_files.insert(handle, refnum);
        });
    }

    #[cfg(test)]
    pub(crate) fn insert_resource_backing_data_for_test(
        &mut self,
        key: (u16, [u8; 4], i16),
        data: Vec<u8>,
    ) {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager.resource_backing_data.insert(key, data);
        });
    }

    #[cfg(test)]
    pub(crate) fn with_resource_file_mut_for_test<R>(
        &mut self,
        refnum: u16,
        operation: impl FnOnce(&mut ResourceFileMap) -> R,
    ) -> Option<R> {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager
                .resources
                .as_mut()
                .and_then(|resources| resources.files.get_mut(&refnum))
                .map(operation)
        })
    }

    #[cfg(test)]
    pub(crate) fn remove_resource_handle_file_for_test(&mut self, handle: u32) {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager.resource_handle_files.remove(&handle);
        });
    }

    #[cfg(test)]
    pub(crate) fn insert_resource_pointer_for_test(
        &mut self,
        refnum: u16,
        resource: ([u8; 4], i16),
        ptr: u32,
    ) {
        self.with_resource_file_mut_for_test(refnum, |file| {
            file.loaded.insert(resource, ptr);
        })
        .expect("test resource file");
    }

    #[cfg(test)]
    pub(crate) fn insert_named_resource_for_test(
        &mut self,
        refnum: u16,
        key: ([u8; 4], String),
        resource: (i16, u32),
    ) {
        self.with_resource_file_mut_for_test(refnum, |file| {
            file.named.insert(key, resource);
        })
        .expect("test resource file");
    }

    #[cfg(test)]
    pub(crate) fn insert_resource_name_for_test(
        &mut self,
        refnum: u16,
        resource: ([u8; 4], i16),
        name: String,
    ) {
        self.with_resource_file_mut_for_test(refnum, |file| {
            file.names_by_id.insert(resource, name);
        })
        .expect("test resource file");
    }

    #[cfg(test)]
    pub(crate) fn insert_resource_attrs_for_test(
        &mut self,
        refnum: u16,
        resource: ([u8; 4], i16),
        attrs: u8,
    ) {
        self.with_resource_file_mut_for_test(refnum, |file| {
            file.attrs.insert(resource, attrs);
        })
        .expect("test resource file");
    }

    #[cfg(test)]
    pub(crate) fn set_resource_map_attrs_for_test(&mut self, refnum: u16, attrs: u16) {
        self.with_resource_file_mut_for_test(refnum, |file| {
            file.map_attrs = attrs;
        })
        .expect("test resource file");
    }

    /// Copy the process display transfer table used for host presentation.
    pub fn device_gamma(&self) -> crate::display::DisplayGamma {
        self.display_gamma.table()
    }

    /// Access the process-owned Sound Manager state.
    pub fn sound_manager(&self) -> &crate::sound::SoundManager {
        &self.sound_manager
    }

    /// Add one process-owned Sound Manager channel without exposing the
    /// manager's shared mutable state.
    pub fn add_sound_channel(&self, channel: crate::sound::SndChannel) {
        self.sound_manager.add_channel(channel);
    }

    /// Queue one Sound Manager callback at the process ownership boundary.
    pub fn queue_sound_callback(&self, callback: crate::sound::PendingSoundCallback) {
        self.sound_manager.queue_sound_callback(callback);
    }

    /// Queue one classic double-buffer callback at the process boundary.
    pub fn queue_sound_doubleback_callback(
        &self,
        callback: crate::sound::PendingDoubleBackCallback,
    ) {
        self.sound_manager.queue_doubleback_callback(callback);
    }

    /// Attach process services outside the construction-owned migration pair.
    pub(crate) fn attach_unconverted_process_services(&mut self, context: &mut ProcessContext) {
        let mut memory_manager = None;
        context.attach_memory_manager(&mut memory_manager);
        let memory_manager = memory_manager.expect("process context supplies a Memory Manager");
        if let Some(attached) = &self.process_memory_manager {
            assert!(
                attached.ptr_eq(&memory_manager),
                "cannot attach two process Memory Managers"
            );
        } else if !self.standalone_memory_manager.ptr_eq(&memory_manager) {
            let target = memory_manager.borrow();
            let standalone = self.standalone_memory_manager.borrow();
            target.assert_can_adopt_process_memory_manager(&standalone);
        }
        context.attach_file_system(&mut self.process_file_system);
        self.open_files = self.process_file_system.files.shared_handle();
        self.write_refnums = self.process_file_system.writable_refnums.shared_handle();
        self.pending_file_completions =
            self.process_file_system.pending_completions.shared_handle();
        self.working_directories = self.process_file_system.working_directories.shared_handle();
        self.next_working_dir_refnum = self
            .process_file_system
            .next_working_directory_ref_num
            .shared_handle();
        self.app_wd_refnum = self
            .process_file_system
            .application_working_directory_ref_num
            .shared_handle();
        self.vfs_volumes = self.process_file_system.vfs_volumes.shared_handle();
        self.next_vfs_volume_ref_num = self
            .process_file_system
            .next_vfs_volume_ref_num
            .shared_handle();
        self.file_positions = self.process_file_system.files.positions();
        context.attach_sound_manager(&mut self.sound_manager);
        context.attach_callback_tasks(
            &mut self.timer_tasks,
            &mut self.vbl_tasks,
            &mut self.callback_scheduling,
        );
        context.attach_scrap_state(&mut self.scrap);
        context.attach_control_manager(&mut self.control_manager);
        context.attach_list_manager(&mut self.list_states);
        context.attach_collection_manager(&mut self.collections);
        context.attach_text_edit_manager(&mut self.textedit_states);
        context.attach_dialog_text(&mut self.param_text);
        context.attach_cursor_state(&mut self.cursor_state);
        context.attach_quickdraw_selection(&mut self.current_port, &mut self.current_gdevice);
        context.attach_quickdraw_error(&mut self.quickdraw_error);
        context.attach_quickdraw_op_colors(&mut self.quickdraw_op_colors);
        context.attach_quickdraw_hilite_colors(&mut self.quickdraw_hilite_colors);
        context.attach_quickdraw_pixel_states(&mut self.gworld_pixel_states);
        self.process_quickdraw_port_state_attached = true;
        context.attach_display_color_state(
            &mut self.device_clut,
            &mut self.color_manager_clut,
            &mut self.display_gamma,
        );
        context.attach_event_queue(&mut self.event_queue);
        context.attach_input_state(&mut self.input_state);
        context.attach_window_list(&mut self.window_list);
        self.process_window_list_attached = true;
        context.attach_classic_file_system(&mut self.vfs, &mut self.vfs_rsrc);
        context.attach_classic_vfs_catalogue(
            &mut self.vfs_directories,
            &mut self.vfs_metadata,
            &mut self.locked_files,
            &mut self.next_vfs_dir_id,
            &mut self.next_vfs_file_id,
            &mut self.next_vfs_timestamp,
            &mut self.default_dir_id,
        );
        self.attach_memory_manager_handle(memory_manager);
        context.attach_native_menu_selection(&mut self.pending_native_menu_selection);
        context.attach_apple_event_handlers(&mut self.ae_handlers);
        context.attach_apple_event_launch_state(&mut self.apple_event_launch_state);
        context.attach_apple_event_descriptors(&mut self.ae_descriptor_state);
    }

    pub(crate) fn process_memory_manager(&self) -> SharedProcessMemoryManager {
        self.process_memory_manager
            .as_ref()
            .unwrap_or(&self.standalone_memory_manager)
            .clone()
    }

    fn attach_memory_manager_handle(&mut self, memory_manager: SharedProcessMemoryManager) {
        if let Some(attached) = &self.process_memory_manager {
            assert!(
                attached.ptr_eq(&memory_manager),
                "cannot attach two process Memory Managers"
            );
            return;
        }
        if !self.standalone_memory_manager.ptr_eq(&memory_manager) {
            let mut target = memory_manager.borrow_mut();
            let mut standalone = self.standalone_memory_manager.borrow_mut();
            target.adopt_process_memory_manager(&mut standalone);
        }
        self.process_memory_manager = Some(memory_manager);
    }

    pub(crate) fn track_handle_ptr(&self, ptr: u32, handle: u32) -> Option<u32> {
        self.process_memory_manager().track_handle_ptr(ptr, handle)
    }

    pub(crate) fn untrack_handle_ptr(&self, ptr: u32) -> Option<u32> {
        self.process_memory_manager().untrack_handle_ptr(ptr)
    }

    pub(crate) fn handle_for_ptr(&self, ptr: u32) -> Option<u32> {
        self.process_memory_manager().handle_for_ptr(ptr)
    }

    #[cfg(test)]
    pub(crate) fn has_handle_ptr(&self, ptr: u32) -> bool {
        self.process_memory_manager().has_handle_ptr(ptr)
    }

    #[cfg(test)]
    pub(crate) fn set_handle_state_bits(&self, handle: u32, state: u8) {
        self.process_memory_manager()
            .set_handle_state(handle, state);
    }

    pub(crate) fn remove_handle_state_bits(&self, handle: u32) -> Option<u8> {
        self.process_memory_manager().remove_handle_state(handle)
    }

    pub(crate) fn handle_state_bits(&self, handle: u32) -> Option<u8> {
        self.process_memory_manager().handle_state(handle)
    }

    pub(crate) fn update_handle_state_bits(
        &self,
        handle: u32,
        update: impl FnOnce(Option<u8>) -> Option<u8>,
    ) {
        self.process_memory_manager()
            .update_handle_state(handle, update);
    }

    #[cfg(test)]
    pub(crate) fn has_handle_state_bits(&self, handle: u32) -> bool {
        self.process_memory_manager().has_handle_state(handle)
    }

    /// Replace bytes in a native relocatable block through the process-level
    /// Memory Manager attached for the current serialized 68K dispatch.
    pub(crate) fn replace_process_native_handle_bytes(
        &mut self,
        bus: &mut MacMemoryBus,
        handle: u32,
        expected_ptr: u32,
        bytes: &[u8],
    ) -> bool {
        let memory_manager = self.process_memory_manager();
        let result = memory_manager.borrow_mut().replace_native_handle_bytes(
            bus,
            handle,
            expected_ptr,
            bytes,
        );
        match result {
            Ok((old_ptr, new_ptr)) => {
                self.untrack_handle_ptr(old_ptr);
                self.track_handle_ptr(new_ptr, handle);
                bus.write_word(crate::memory::globals::addr::MEM_ERR, 0);
                true
            }
            Err(error) => {
                bus.write_word(crate::memory::globals::addr::MEM_ERR, error as u16);
                false
            }
        }
    }

    /// Run one 68K operation with every process manager continuously attached.
    pub(crate) fn with_process_state<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        f(self)
    }

    pub(crate) const AUTO_KEY_THRESHOLD_TICKS: u32 = 16;
    pub(crate) const AUTO_KEY_RATE_TICKS: u32 = 4;
    const CAPS_LOCK_KEY_CODE: u8 = 0x39;

    pub(crate) fn set_menu_bar_policy(&mut self, policy: crate::runner::MenuBarPolicy) {
        self.menu_bar_policy = policy;
        self.initial_kiosk_guest_hide_observed = false;
        self.menu_bar_hidden = !matches!(policy, crate::runner::MenuBarPolicy::GuestControlled);
    }

    pub(crate) fn release_initial_menu_bar_kiosk(&mut self) {
        if self.menu_bar_policy == crate::runner::MenuBarPolicy::InitialKiosk {
            self.menu_bar_policy = crate::runner::MenuBarPolicy::GuestControlled;
            self.initial_kiosk_guest_hide_observed = false;
            self.menu_bar_hidden = false;
        }
    }

    pub(crate) fn key_is_modifier(key_code: u8) -> bool {
        // Command, Shift, Caps Lock, Option, and Control (including the
        // right-side variants) update KeyMap/modifiers but generate no
        // keyDown or keyUp events. Inside Macintosh Volume I, I-246.
        matches!(
            key_code,
            0x37 | 0x38 | 0x39 | 0x3A | 0x3B | 0x3C | 0x3D | 0x3E
        )
    }

    pub(crate) fn key_generates_auto_key(key_code: u8) -> bool {
        !Self::key_is_modifier(key_code)
    }

    pub(crate) fn add_hle_tick_cost(&mut self, cost: u32) {
        if cost == 0 {
            return;
        }
        let cost = cost.min(i32::MAX as u32) as i32;
        self.pending_hle_tick_cost = self.pending_hle_tick_cost.saturating_add(cost);
    }

    pub(crate) fn take_hle_tick_cost(&mut self) -> i32 {
        let cost = self.pending_hle_tick_cost;
        self.pending_hle_tick_cost = 0;
        cost
    }

    pub(crate) fn resource_load_tick_cost(byte_len: u32) -> u32 {
        if byte_len == 0 {
            return 0;
        }
        64u32.saturating_add(byte_len.saturating_mul(16))
    }

    pub(crate) fn quickdraw_blit_tick_cost(
        width: u32,
        height: u32,
        src_pixel_size: u32,
        dst_pixel_size: u32,
        transformed: bool,
    ) -> u32 {
        let pixels = width.saturating_mul(height);
        if pixels == 0 {
            return 0;
        }
        let mut per_pixel = if transformed { 3u32 } else { 1u32 };
        if src_pixel_size != dst_pixel_size {
            per_pixel = per_pixel.saturating_add(2);
        }
        256u32.saturating_add(pixels.saturating_mul(per_pixel))
    }

    pub(crate) fn draw_picture_tick_cost(width: u32, height: u32, picture_bytes: u32) -> u32 {
        let pixels = width.saturating_mul(height);
        if pixels == 0 && picture_bytes == 0 {
            return 0;
        }
        256u32
            .saturating_add(pixels.saturating_mul(3))
            .saturating_add(picture_bytes / 4)
    }

    /// Number of menus currently loaded (added via InsertMenu, NewMenu,
    /// GetNewMBar, etc.). Used by ctx.json snapshots so observers can see
    /// whether the menu bar was populated at capture time without
    /// re-instrumenting.
    pub fn menu_count(&self) -> usize {
        self.menus.len()
    }

    /// Iterator over the loaded menu titles, in insertion order.
    /// Titles may include embedded bytes for Apple-menu icons etc.;
    /// callers should handle non-ASCII defensively.
    pub fn menu_titles(&self) -> impl Iterator<Item = &str> {
        self.menus.iter().map(|m| m.title.as_str())
    }

    /// Frontmost WindowPtr tracked by the Window Manager, or NIL.
    pub fn front_window(&self) -> u32 {
        self.front_window
    }

    /// Cached global bounds of the front window content rect.
    pub fn window_bounds(&self) -> (i16, i16, i16, i16) {
        self.window_bounds
    }

    /// Bounds of a retained visible dialog, if one is currently drawn.
    pub fn visible_dialog_bounds(&self) -> Option<(i16, i16, i16, i16)> {
        if let Some(tracking) = self.dialog_tracking.as_ref() {
            return Some(tracking.bounds);
        }
        if self.front_window != 0 && self.dialog_items.contains_key(&self.front_window) {
            return Some(self.window_bounds);
        }
        if let Some(snapshot) = self.dialog_visible_snapshots.get(&self.front_window) {
            return Some(snapshot.bounds);
        }
        self.dialog_visible_snapshots
            .values()
            .next()
            .map(|snapshot| snapshot.bounds)
    }

    /// Structure bounds of a retained visible dialog, including its WDEF
    /// frame. Frontends use this to keep transient dialogs visible when the
    /// application's normal presentation viewport is smaller than the guest
    /// screen.
    pub fn visible_dialog_structure_bounds(
        &self,
        bus: &MacMemoryBus,
    ) -> Option<(i16, i16, i16, i16)> {
        let dialog_ptr = if let Some(tracking) = self.dialog_tracking.as_ref() {
            tracking.dialog_ptr
        } else if self.front_window != 0
            && self.dialog_items.contains_key(&self.front_window)
            && self.window_visible(bus, self.front_window)
        {
            self.front_window
        } else if self
            .dialog_visible_snapshots
            .contains_key(&self.front_window)
        {
            self.front_window
        } else {
            *self.dialog_visible_snapshots.keys().next()?
        };
        self.window_structure_rect(bus, dialog_ptr).or_else(|| {
            self.dialog_tracking
                .as_ref()
                .filter(|tracking| tracking.dialog_ptr == dialog_ptr)
                .map(|tracking| tracking.bounds)
                .or_else(|| {
                    self.dialog_visible_snapshots
                        .get(&dialog_ptr)
                        .map(|snapshot| snapshot.bounds)
                })
                .or_else(|| {
                    (dialog_ptr == self.front_window && self.dialog_items.contains_key(&dialog_ptr))
                        .then_some(self.window_bounds)
                })
        })
    }

    /// Number of windows currently tracked by the Window Manager list.
    pub fn window_count(&self) -> usize {
        self.window_list.len()
    }

    pub(crate) fn capture_gui_frame(&self, bus: &MacMemoryBus, label: &str) {
        let Some(dir) = gui_capture_dir() else {
            return;
        };
        if let Some(required_label) = gui_capture_label() {
            if !label.contains(required_label) {
                return;
            }
        }
        let (_, _, width, height, _) = self.screen_mode;
        if width == 0 || height == 0 {
            return;
        }

        let frame = GUI_CAPTURE_FRAME.fetch_add(1, Ordering::Relaxed);
        if let Some(limit) = gui_capture_limit() {
            if frame >= limit {
                return;
            }
        }

        if let Err(err) = std::fs::create_dir_all(dir) {
            eprintln!("[GUI-CAPTURE] failed to create {}: {}", dir.display(), err);
            return;
        }

        let safe_label = sanitize_gui_capture_label(label);
        let filename = format!(
            "{:06}_t{:06}_tr{:08}_{}.png",
            frame,
            self.current_tick(),
            self.trap_count,
            safe_label
        );
        let path = dir.join(&filename);
        let mut rgba = crate::display::render_screen_with_gamma(
            bus,
            self.screen_mode,
            &self.device_clut,
            &self.display_gamma.table(),
        );
        if let Some(cursor) = self.cursor() {
            crate::display::render_cursor(
                &mut rgba,
                width as u32,
                height as u32,
                cursor,
                self.mouse_position(),
            );
        }
        let img = image::RgbImage::from_fn(width as u32, height as u32, |x, y| {
            let idx = ((y * width as u32 + x) * 4) as usize;
            image::Rgb([rgba[idx], rgba[idx + 1], rgba[idx + 2]])
        });
        if let Err(err) = img.save(&path) {
            eprintln!("[GUI-CAPTURE] failed to save {}: {}", path.display(), err);
            return;
        }

        let index_path = dir.join("frames.jsonl");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&index_path)
        {
            use std::io::Write;
            let _ = writeln!(
                file,
                "{{\"frame\":{},\"file\":\"{}\",\"label\":\"{}\",\"tick\":{},\"trap_count\":{},\"game_trap_count\":{},\"trap_word\":\"{:04X}\",\"front_window\":\"{:08X}\"}}",
                frame,
                filename,
                safe_label,
                self.current_tick(),
                self.trap_count,
                self.game_trap_count,
                self.current_trap_word,
                self.front_window
            );
        }
    }

    /// Number of items in the menu identified by `handle`, or
    /// `None` if no menu with that handle is registered. Used by
    /// tests to observe AppendMenu / DeleteMenuItem /
    /// InsertMenuItem / DeleteMenu effects on host-side state.
    pub fn menu_items_len(&self, handle: u32) -> Option<usize> {
        self.menus
            .iter()
            .find(|m| m.handle == handle)
            .map(|m| m.items.len())
    }

    /// Whether the dialog at `dialog_ptr` is currently registered
    /// with an item list. Used by tests to observe
    /// NewDialog / GetNewDialog / DisposDialog effects on
    /// dialog_items state.
    pub fn dialog_is_registered(&self, dialog_ptr: u32) -> bool {
        self.dialog_items.contains_key(&dialog_ptr)
    }

    /// Text of the 1-based item in the menu identified by
    /// `handle`. Returns `None` if the menu isn't registered or
    /// `item_one_based` is out of range. Used by tests to observe
    /// SetItem, AppendMenu-text, InsertMenuItem side effects.
    pub fn menu_item_text(&self, handle: u32, item_one_based: i16) -> Option<String> {
        if item_one_based < 1 {
            return None;
        }
        let idx = (item_one_based - 1) as usize;
        self.menus
            .iter()
            .find(|m| m.handle == handle)
            .and_then(|m| m.items.get(idx))
            .map(|it| it.text.clone())
    }

    /// Test-only: set the current port without going through SetPort.
    /// Used by integration test helpers like setup_with_cgraf_port().
    pub fn set_current_port_for_test(&mut self, port: u32) {
        self.current_port.set(port);
    }

    /// Test-only: invoke save_dialog_pixels for the byte-isomorphism gate.
    /// Used by tests asserting the bulk path returns the same bytes the
    /// per-pixel reference would have produced.
    pub fn save_dialog_pixels_for_test(
        &self,
        bus: &MacMemoryBus,
        rect: (i16, i16, i16, i16),
    ) -> Vec<u8> {
        self.save_dialog_pixels(bus, rect).into_vec()
    }

    /// Test-only: invoke restore_dialog_pixels for the byte-isomorphism
    /// gate. Used by tests asserting the bulk path writes the same bytes
    /// the per-pixel reference would have written.
    pub fn restore_dialog_pixels_for_test(
        &self,
        bus: &mut MacMemoryBus,
        rect: (i16, i16, i16, i16),
        saved: &[u8],
    ) {
        self.restore_dialog_pixels(bus, rect, &saved.to_vec().into());
    }

    /// Return the process-scoped wrapping Macintosh tick counter.
    ///
    /// Callers entering from a guest ABI must first use `read_tick_count` so
    /// direct stores to low-memory `Ticks` are imported before this accessor
    /// is used by manager policy.
    pub(crate) fn current_tick(&self) -> u32 {
        self.tick_state.current_tick()
    }

    /// Queue a one-shot deferred task and maintain the guest-visible DTQueue.
    /// Inside Macintosh: Processes (1994), pp. 6-11--6-13.
    pub(crate) fn enqueue_deferred_task(&mut self, bus: &mut MacMemoryBus, task: u32) {
        if self.deferred_tasks.iter().any(|(queued, _)| *queued == task) {
            return;
        }
        let queue = crate::memory::globals::addr::DT_QUEUE;
        bus.write_long(task, 0);
        if let Some((tail, _)) = self.deferred_tasks.back() {
            bus.write_long(*tail, task);
        } else {
            bus.write_long(queue + 2, task);
        }
        bus.write_long(queue + 6, task);
        let ready = self
            .current_tick()
            .wrapping_add(u32::from(!self.deferred_install_in_interrupt));
        self.deferred_tasks.push_back((task, ready));
    }

    pub(crate) fn pop_ready_deferred_task(
        &mut self,
        bus: &mut MacMemoryBus,
        tick: u32,
    ) -> Option<u32> {
        let (task, ready) = *self.deferred_tasks.front()?;
        if (tick.wrapping_sub(ready) as i32) < 0 {
            return None;
        }
        self.deferred_tasks.pop_front();
        let queue = crate::memory::globals::addr::DT_QUEUE;
        let next = self.deferred_tasks.front().map_or(0, |(ptr, _)| *ptr);
        bus.write_long(queue + 2, next);
        if next == 0 {
            bus.write_long(queue + 6, 0);
        }
        bus.write_long(task, 0);
        Some(task)
    }

    /// Resolve the architecture-neutral TickCount operation from the
    /// guest-visible low-memory value. A direct guest write is accepted at
    /// the ABI boundary and updates the host pacing snapshot; it is never
    /// overwritten by an adapter scalar.
    pub(crate) fn read_tick_count(&mut self, bus: &MacMemoryBus) -> u32 {
        let guest_ticks = bus.read_long(crate::memory::globals::addr::TICKS);
        self.tick_state.read_tick_count(guest_ticks);
        guest_ticks
    }

    /// Set low-memory `Ticks` for an explicit fixture synchronization and
    /// import the bytes into the host pacing snapshot. Production callers
    /// should write guest memory at their ABI boundary and use
    /// `read_tick_count` directly.
    #[cfg(test)]
    pub(crate) fn set_tick_count_for_test(&mut self, bus: &mut MacMemoryBus, tick: u32) {
        bus.write_long(crate::memory::globals::addr::TICKS, tick);
        self.read_tick_count(bus);
    }

    /// Advance the host pacing snapshot by one wrapping tick. The caller
    /// writes the returned value to guest low memory as part of the same VBL
    /// boundary.
    pub(crate) fn advance_tick(&mut self) -> u32 {
        let tick = self.tick_state.advance_ticks(1);
        tick
    }

    /// Test-only: mark the synthetic kAEOpenApplication event as
    /// already delivered so the next GetNextEvent/WaitNextEvent
    /// returns a real null event instead of the boot-time oapp stub.
    pub fn set_sent_open_app_event_for_test(&mut self, sent: bool) {
        self.apple_event_launch_state
            .set_open_application_event_sent(sent);
    }

    /// Test-only: set the screen mode (base, rowBytes, width, height, depth).
    /// Production code initializes screen_mode from the machine profile.
    pub fn set_screen_mode_for_test(
        &mut self,
        base: u32,
        row_bytes: u32,
        width: u16,
        height: u16,
        depth: u16,
    ) {
        self.screen_mode = (base, row_bytes, width, height, depth);
    }

    /// Test-only: install a resource into the current application file (refnum 0)
    /// without needing a parsed ResourceFork. Allocates `data` on the guest bus
    /// and registers it under (type, id). Returns the guest address of the data.
    ///
    /// Production code initializes resources by parsing a real fork via
    /// `load_resources`. Use this helper in integration tests that just need a
    /// resource visible to traps like GetResource, GetCursor, GetString, etc.
    pub fn install_test_resource(
        &mut self,
        bus: &mut MacMemoryBus,
        res_type: [u8; 4],
        id: i16,
        data: &[u8],
    ) -> u32 {
        self.install_test_resource_in_file(bus, 0, res_type, id, data)
    }

    /// Test-only: variant of `install_test_resource` that targets a specific
    /// `refnum`. Use when a test needs to assert current-file-vs-search-chain
    /// semantics (e.g. `Get1IndResource` $A80E vs `GetIndResource` $A99D —
    /// IM:IV-15). Refnums are appended to `search_order` in install order so
    /// the file becomes part of the chain; the current file is left
    /// unchanged so the test can drive `UseResFile` ($A998) explicitly.
    pub fn install_test_resource_in_file(
        &mut self,
        bus: &mut MacMemoryBus,
        refnum: u16,
        res_type: [u8; 4],
        id: i16,
        data: &[u8],
    ) -> u32 {
        let data_ptr = bus.alloc(data.len().max(1) as u32);
        bus.write_bytes(data_ptr, data);

        self.with_resource_manager_mut(|resource_manager| {
            let resources = resource_manager.resources.get_or_insert_with(|| LoadedResources {
                files: HashMap::from([(0u16, ResourceFileMap::default())]),
                names: HashMap::new(),
                search_order: vec![0],
                current_file: 0,
            });
            let file = resources.files.entry(refnum).or_default();
            file.loaded.insert((res_type, id), data_ptr);
            if !resources.search_order.contains(&refnum) {
                resources.search_order.push(refnum);
            }
        });
        self.remember_resource_backing_data(refnum, res_type, id, data.to_vec());
        data_ptr
    }

    /// Test-only: variant of `install_test_resource_in_file` that also
    /// records the resource name. Required by traps that walk the
    /// resource fork by NAME (AddResMenu / InsertResMenu / GetNamedResource)
    /// — without the named entry the resource is invisible to those
    /// callers even though the (type, id) entry exists.
    pub fn install_named_test_resource_in_file(
        &mut self,
        bus: &mut MacMemoryBus,
        refnum: u16,
        res_type: [u8; 4],
        id: i16,
        name: &str,
        data: &[u8],
    ) -> u32 {
        let data_ptr = self.install_test_resource_in_file(bus, refnum, res_type, id, data);
        self.with_resource_manager_mut(|resource_manager| {
            if let Some(resources) = resource_manager.resources.as_mut() {
                let file = resources.files.entry(refnum).or_default();
                file.named
                    .insert((res_type, name.to_string()), (id, data_ptr));
                file.names_by_id.insert((res_type, id), name.to_string());
            }
        });
        data_ptr
    }

    /// Install a trace sink to receive runtime events and screen
    /// snapshots. The sink (and where it persists output) is the host's
    /// concern; see [`crate::trace::TraceSink`].
    pub fn set_trace_sink(&mut self, sink: Box<dyn TraceSink>) {
        self.trace_sink = Some(sink);
        self.screen_event_count = 0;
        self.copybits_screen_secs.clear();
    }

    pub fn trace_source(&self) -> Option<TraceSource> {
        self.trace_sink.as_ref().map(|sink| sink.source())
    }

    pub(crate) fn trace_field_map(pairs: &[(&str, String)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect()
    }

    /// True when trace-event recording is active. Hot-path traps should
    /// gate `record_trace_event` callsites (which build a BTreeMap +
    /// string-formatted field values) behind this check — otherwise every
    /// call allocates + constructs the map even when it will be discarded
    /// by record_trace_event's own recorder-is-none early-return.
    #[inline]
    pub(crate) fn is_trace_recording(&self) -> bool {
        self.trace_sink.is_some()
    }

    pub(crate) fn trace_palette_field_map(
        bus: &MacMemoryBus,
        table_ptr: u32,
        start: i16,
        count: i16,
    ) -> BTreeMap<String, String> {
        let normalized_start = if start < 0 {
            0usize
        } else {
            (start as usize).min(255)
        };
        let safe_count = if count < 0 {
            255usize
        } else {
            (count as usize).min(255)
        };
        let last_index = normalized_start.saturating_add(safe_count).min(255);
        let mid_index = normalized_start + (last_index - normalized_start) / 2;
        let mut hash = 0x811C9DC5u32;
        let mut rgb_only_hash = 0x811C9DC5u32;
        for index in normalized_start..=last_index {
            let entry = table_ptr + (index as u32) * 8;
            for offset in [0u32, 2, 4, 6] {
                let word = bus.read_word(entry + offset);
                for byte in word.to_be_bytes() {
                    hash ^= u32::from(byte);
                    hash = hash.wrapping_mul(0x0100_0193);
                }
            }
            for offset in [2u32, 4, 6] {
                let word = bus.read_word(entry + offset);
                for byte in word.to_be_bytes() {
                    rgb_only_hash ^= u32::from(byte);
                    rgb_only_hash = rgb_only_hash.wrapping_mul(0x0100_0193);
                }
            }
        }
        // idx_245_rgb: RGB at CLUT index 245 when this call's range
        // covers it. Cross-emulator replay of the set_entries stream
        // can then reconstruct device_clut[245] at any tick without
        // touching BasiliskII's video.cpp. "-" is the out-of-range
        // sentinel (consumers skip it when walking the stream).
        let idx_245_rgb = if (normalized_start..=last_index).contains(&245) {
            Self::trace_palette_entry_rgb(bus, table_ptr, 245)
        } else {
            "-".to_string()
        };
        Self::trace_field_map(&[
            ("start", start.to_string()),
            ("count", safe_count.to_string()),
            ("first_index", normalized_start.to_string()),
            ("last_index", last_index.to_string()),
            ("mid_index", mid_index.to_string()),
            (
                "first_rgb",
                Self::trace_palette_entry_rgb(bus, table_ptr, normalized_start),
            ),
            (
                "mid_rgb",
                Self::trace_palette_entry_rgb(bus, table_ptr, mid_index),
            ),
            (
                "last_rgb",
                Self::trace_palette_entry_rgb(bus, table_ptr, last_index),
            ),
            ("idx_245_rgb", idx_245_rgb),
            ("table_hash", format!("{hash:08X}")),
            ("rgb_only_hash", format!("{rgb_only_hash:08X}")),
        ])
    }

    fn trace_palette_entry_rgb(bus: &MacMemoryBus, table_ptr: u32, index: usize) -> String {
        let entry = table_ptr + (index as u32) * 8;
        format!(
            "{:04X},{:04X},{:04X}",
            bus.read_word(entry + 2),
            bus.read_word(entry + 4),
            bus.read_word(entry + 6)
        )
    }

    pub(crate) fn record_trace_event(
        &mut self,
        bus: &MacMemoryBus,
        pc: u32,
        event: &str,
        fields: BTreeMap<String, String>,
        screen_affecting: bool,
    ) -> Result<()> {
        if self.trace_sink.is_none() {
            return Ok(());
        }
        if screen_affecting {
            self.screen_event_count = self.screen_event_count.wrapping_add(1);
            if event == "copybits_screen" {
                self.copybits_screen_secs.push(self.screen_event_count);
            }
            let tick = self.current_tick();
            self.trace_sink
                .as_mut()
                .expect("trace_sink checked above")
                .record_snapshot(
                    bus,
                    self.screen_mode,
                    &self.device_clut,
                    self.screen_event_count,
                    tick,
                    self.instruction_count,
                )
                .map_err(Error::Trace)?;
        }
        let source = self
            .trace_sink
            .as_ref()
            .expect("trace_sink checked above")
            .source();
        let trace_event = TraceEvent {
            source,
            tick: self.current_tick(),
            instructions: self.instruction_count,
            pc,
            trap_count: self.trap_count,
            game_trap_count: self.game_trap_count,
            screen_event_count: self.screen_event_count,
            event: event.to_string(),
            fields,
        };
        self.trace_sink
            .as_mut()
            .expect("trace_sink checked above")
            .record_event(&trace_event)
            .map_err(Error::Trace)?;
        Ok(())
    }

    pub(crate) fn key_is_down(&self, key_code: u8) -> bool {
        self.input_state.key_is_down(key_code)
    }

    pub(crate) fn key_map_bytes(&self) -> [u8; 16] {
        self.input_state.key_map_snapshot()
    }

    pub(crate) fn current_event_modifiers(&self) -> u16 {
        const BTN_STATE: u16 = 128;
        const CMD_KEY: u16 = 256;
        const SHIFT_KEY: u16 = 512;
        const ALPHA_LOCK: u16 = 1024;
        const OPTION_KEY: u16 = 2048;
        const CONTROL_KEY: u16 = 4096;

        let mut modifiers = 0u16;
        if !self.input_state.mouse_button_pressed() {
            modifiers |= BTN_STATE;
        }
        if self.key_is_down(0x37) {
            modifiers |= CMD_KEY;
        }
        if self.key_is_down(0x38) || self.key_is_down(0x3C) {
            modifiers |= SHIFT_KEY;
        }
        // EventRecord.modifiers exposes the logical Caps Lock latch through
        // alphaLock. Inside Macintosh Volume I (1985), p. I-263.
        if self.key_is_down(Self::CAPS_LOCK_KEY_CODE) {
            modifiers |= ALPHA_LOCK;
        }
        if self.key_is_down(0x3A) || self.key_is_down(0x3D) {
            modifiers |= OPTION_KEY;
        }
        if self.key_is_down(0x3B) || self.key_is_down(0x3E) {
            modifiers |= CONTROL_KEY;
        }
        modifiers
    }

    pub fn enable_input_trace_capture(&mut self) {
        self.input_trace_enabled = true;
        self.input_trace_log.clear();
        self.input_trace_log
            .push("# systemless deterministic input trace v1".to_string());
    }

    pub fn input_trace_text(&self) -> String {
        if self.input_trace_log.is_empty() {
            String::new()
        } else {
            let mut out = self.input_trace_log.join("\n");
            out.push('\n');
            out
        }
    }

    pub(crate) fn record_input_trace_line(&mut self, line: String) {
        if self.input_trace_enabled {
            self.input_trace_log.push(line);
        }
    }

    pub(crate) fn input_trace_state_fields(&self) -> String {
        let key_map_snapshot = self.input_state.key_map_snapshot();
        let (mouse_v, mouse_h) = self.input_state.mouse_position();
        let key_map = if key_map_snapshot.iter().any(|&byte| byte != 0) {
            key_map_snapshot
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join("")
        } else {
            "none".to_string()
        };
        format!(
            "state=mouse=({},{}) button={} live_modifiers=${:04X} key_map={} tracking=menu:{} dialog:{} control:{}",
            mouse_v,
            mouse_h,
            if self.input_state.mouse_button_pressed() {
                "down"
            } else {
                "up"
            },
            self.current_event_modifiers(),
            key_map,
            if self.is_menu_tracking() { "active" } else { "idle" },
            if self.is_dialog_tracking() {
                "active"
            } else {
                "idle"
            },
            if self.is_control_tracking() {
                "active"
            } else {
                "idle"
            },
        )
    }

    /// Dump the top-N traps by dispatch count in descending order. No-op
    /// when `SYSTEMLESS_TRACE_TRAP_COUNTS` was not set at startup. Format:
    ///   [TRAP-HIST]   100234  $A9ED PostEvent
    pub fn print_trap_histogram(&self, top_n: usize) {
        if !trap_histogram_enabled() {
            return;
        }
        let mut entries: Vec<(u16, u64)> = self
            .trap_histogram
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| if c > 0 { Some((i as u16, c)) } else { None })
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.1));
        let total: u64 = entries.iter().map(|(_, c)| c).sum();
        eprintln!(
            "[TRAP-HIST] top {} of {} distinct traps ({} total dispatches)",
            top_n.min(entries.len()),
            entries.len(),
            total
        );
        for (idx, count) in entries.iter().take(top_n) {
            // `idx` is the low-12-bit number; reconstruct a nominal
            // trap word so lookups make sense. Tool traps use 0xA800|idx,
            // OS traps use 0xA000|(idx & 0xFF). We can't distinguish
            // from the counter alone (toolbox/OS share the 12-bit space
            // via selector bits), so print both likely forms.
            let as_tool = 0xA800 | *idx;
            let as_os = 0xA000 | (*idx & 0xFF);
            eprintln!(
                "[TRAP-HIST]   {:>10}  idx=${:03X}  (tool ${:04X} / os ${:04X})",
                count, idx, as_tool, as_os
            );
        }
    }

    /// Dump the top-N traps by accumulated wall-clock time (descending).
    /// No-op when `SYSTEMLESS_TRACE_TRAP_TIMING` was not set at startup.
    /// Format:
    ///   [TRAP-TIME]    1234567 ns   12.5 ns/call (98765 calls)  idx=$xxx ...
    /// Pairs with `print_trap_histogram` to distinguish "hot because called
    /// a lot" from "hot because each call is slow".
    pub fn print_trap_timing_histogram(&self, top_n: usize) {
        if !trap_timing_enabled() {
            return;
        }
        let mut entries: Vec<(u16, u64, u64)> = self
            .trap_time_ns
            .iter()
            .enumerate()
            .filter_map(|(i, &ns)| {
                if ns == 0 {
                    return None;
                }
                let count = self.trap_histogram[i];
                Some((i as u16, ns, count))
            })
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.1));
        let total_ns: u64 = entries.iter().map(|(_, ns, _)| ns).sum();
        eprintln!(
            "[TRAP-TIME] top {} of {} distinct traps with timing data ({:.3} ms total wall-clock)",
            top_n.min(entries.len()),
            entries.len(),
            total_ns as f64 / 1_000_000.0,
        );
        for (idx, ns, count) in entries.iter().take(top_n) {
            let as_tool = 0xA800 | *idx;
            let as_os = 0xA000 | (*idx & 0xFF);
            let avg_ns = ns.checked_div(*count).unwrap_or(0);
            eprintln!(
                "[TRAP-TIME]   {:>11} ns total  {:>7} ns/call  ({:>10} calls)  idx=${:03X} (tool ${:04X} / os ${:04X})",
                ns, avg_ns, count, idx, as_tool, as_os
            );
        }
    }

    pub fn new() -> Self {
        *Self::new_inner_boxed(MigratedProcessHandles {
            ticks: SharedProcessTickState::default(),
            execution: SharedGuestCallStack::default(),
        })
    }

    #[cfg(test)]
    pub(crate) fn new_with_migrated_handles(handles: MigratedProcessHandles) -> Self {
        *Self::new_inner_boxed(handles)
    }

    pub(crate) fn new_boxed_with_migrated_handles(
        handles: MigratedProcessHandles,
    ) -> Box<Self> {
        Self::new_inner_boxed(handles)
    }

    #[cfg(test)]
    pub(crate) fn is_constructed_from_migrated_handles(
        &self,
        handles: &MigratedProcessHandles,
    ) -> bool {
        self.tick_state.ptr_eq(&handles.ticks)
            && self.guest_calls.ptr_eq(&handles.execution)
            && self.menu_tracking.is_view_of(&self.guest_calls)
    }

    fn new_inner_boxed(handles: MigratedProcessHandles) -> Box<Self> {
        let MigratedProcessHandles {
            ticks: tick_state,
            execution: guest_calls,
        } = handles;
        let menu_tracking = guest_calls.menu_tracking_view().into();
        let process_file_system = SharedProcessFileSystem::default();
        process_file_system.vfs_directories.replace(vec![ProcessVfsDirectory {
            dir_id: 2,
            parent_dir_id: 1,
            // The root directory's catalog name is the volume name.
            // Files 1992, 2-27 and 2-85.
            path: String::new(),
            creator: u32::from_be_bytes(*b"MACS"),
            file_type: u32::from_be_bytes(*b"fold"),
            finder_flags: 0,
            dirty: false,
        }]);
        let open_files = process_file_system.files.shared_handle();
        let write_refnums = process_file_system.writable_refnums.shared_handle();
        let pending_file_completions = process_file_system.pending_completions.shared_handle();
        let working_directories = process_file_system.working_directories.shared_handle();
        let next_working_dir_refnum = process_file_system
            .next_working_directory_ref_num
            .shared_handle();
        let app_wd_refnum = process_file_system
            .application_working_directory_ref_num
            .shared_handle();
        let vfs_volumes = process_file_system.vfs_volumes.shared_handle();
        let next_vfs_volume_ref_num = process_file_system.next_vfs_volume_ref_num.shared_handle();
        let vfs_directories = process_file_system.vfs_directories.shared_handle();
        let file_positions = process_file_system.files.positions();

        let mut dispatcher = Box::new(Self {
            adb: crate::adb::AdbManager::new(),
            process_file_system,
            vm_held_page_counts: HashMap::new(),
            vm_held_page_history: HashSet::new(),
            vm_locked_page_counts: HashMap::new(),
            instruction_cache_enabled: true,
            data_cache_enabled: true,
            process_memory_manager: None,
            standalone_memory_manager: SharedProcessMemoryManager::default(),
            movie_states: HashMap::new(),
            movie_by_controller: HashMap::new(),
            movie_error: 0,
            movie_sticky_error: 0,
            dialogs_drawn_by_app: std::collections::HashSet::new(),
            segment_map: HashMap::new(),
            ae_handlers: SharedProcessAppleEventHandlers::default(),
            apple_event_launch_state: SharedProcessAppleEventLaunchState::default(),
            ae_descriptor_state: SharedProcessAppleEventDescriptors::default(),
            ae_object_accessors: HashMap::new(),
            ae_private_hash_tables: HashMap::new(),
            ae_special_handlers: HashMap::new(),
            ae_coercion_handlers: HashMap::new(),
            gestalt_registry: HashMap::new(),
            ae_call_state: None,
            ae_call_state_stack: Vec::new(),
            ae_trampoline_addr: None,
            mask_table_addr: None,
            loadseg_getresource_state: None,
            loadseg_getresource_trampoline_addr: None,
            preserve_auto_pop_pc_once: false,
            device_loop_trampoline: 0,
            list_def_trampoline: 0,
            window_def_trampoline: 0,
            control_def_trampoline: 0,
            control_def_trampoline_chain: Vec::new(),
            defer_user_fn_trampoline: 0,
            notification_requests: Vec::new(),
            collection_callback_stack: Vec::new(),
            collection_callback_trampoline: 0,
            qddone_seen_ports: HashSet::new(),
            pict_info_ids: HashSet::new(),
            ppc_initialized: false,
            thread_return_trampoline: 0,
            cooperative_thread_scheduler: 0,
            synthetic_component_instances: HashSet::new(),
            next_synthetic_component_instance: 0x00C1_0001,
            saved_draw_old_regions: HashMap::new(),
            fired_oapp_handler: false,
            system_str_cache: HashMap::new(),
            system_intl_cache: HashMap::new(),
            system_pattern_list_cache: HashMap::new(),
            system_cursor_cache: HashMap::new(),
            system_icon_cache: HashMap::new(),
            system_clut_cache: HashMap::new(),
            system_wctb_cache: HashMap::new(),
            system_kchr_cache: HashMap::new(),
            system_kmap_cache: HashMap::new(),
            system_wdef_cache: HashMap::new(),
            system_mdef_cache: HashMap::new(),
            std_pix_gateway: 0,
            param_text: SharedProcessDialogText::default(),
            ui_theme_id: UiThemeId::ClassicSystem7,
            vfs: SharedProcessValue::default(),
            vfs_rsrc: SharedProcessValue::default(),
            vfs_metadata: SharedProcessValue::default(),
            vfs_directories,
            vfs_volumes,
            working_directories,
            open_files,
            synthetic_drivers: HashMap::new(),
            legacy_sound_driver_channel: None,
            write_refnums,
            file_positions,
            recent_file_read: None,
            pending_file_completions,
            locked_files: SharedProcessValue::default(),
            mmu_mode: 1,                      // true32b — 32-bit addressing by default
            default_video_rec: 0x0000,        // no default video device selected
            default_os_rec: 0x0001,           // Macintosh Operating System
            default_startup_rec: 0x0000_0000, // zero-filled first-device startup default
            next_vfs_dir_id: SharedProcessValue::from_value(16),
            next_vfs_volume_ref_num,
            next_vfs_file_id: SharedProcessValue::from_value(32),
            next_vfs_timestamp: SharedProcessValue::from_value(1),
            next_working_dir_refnum,
            pending_launch_app: None,
            default_dir_id: SharedProcessValue::from_value(2),
            app_wd_refnum,
            output_dir: None,
            fg_color: (0, 0, 0),
            bg_color: (0xFFFF, 0xFFFF, 0xFFFF),
            pm_fg_color: None,
            pm_bg_color: None,
            makergbpat_colors: HashMap::new(),
            char_extra: 0,
            bk_pat: [0x00; 8],
            pn_loc: (0, 0),
            pn_size: (1, 1),
            pn_mode: 8,
            pn_pat: [0xFF; 8],
            pn_vis: 0,
            tx_font: 0,
            tx_face: 0,
            tx_mode: 1,
            tx_size: 12,
            outline_preferred: false,
            preserve_glyph: false,
            tick_state,
            power_idle_last_update_tick: 0,
            power_idle_disable_count: 0,
            serial_port_a_powered: false,
            serial_port_b_powered: false,
            fade_trace_remaining: 0,
            instruction_count: 0,
            front_window: 0,
            window_manager_port: 0,
            window_manager_cport: 0,
            event_counter: 0,
            window_title: String::new(),
            window_bounds: (0, 0, 342, 512),
            window_proc_id: 0,
            window_proc_ids: HashMap::new(),
            windows_placed_offscreen: std::collections::HashSet::new(),
            window_aux_records: HashMap::new(),
            window_original_pixmaps: HashMap::new(),
            window_saved_under_pixels: HashMap::new(),
            control_aux_records: HashMap::new(),
            control_aux_head: 0,
            go_away_flag: false,
            window_list: Default::default(),
            process_window_list_attached: false,
            fullscreen_locked: false,
            menu_bar_policy: crate::runner::MenuBarPolicy::GuestControlled,
            initial_kiosk_guest_hide_observed: false,
            menu_bar_hidden: false,
            sound_manager: SharedProcessSoundManager::default(),
            menus: Vec::new(),
            menu_tracking,
            guest_calls,
            pending_native_menu_selection: SharedNativeMenuSelection::default(),
            pending_native_menu_event: None,
            pending_native_menu_event_tick: None,
            control_tracking: None,
            scrollbar_thumb_tracking: None,
            window_tracking: None,
            go_away_tracking: None,
            zoom_box_tracking: None,
            grow_window_tracking: None,
            region_tracking: None,
            underline_info: None,
            input_state: SharedProcessInputState::default(),
            debug_getkeys_nonzero_count: 0,
            debug_last_getkeys_nonzero_key_map: [0; 16],
            debug_key_event_delivery_count: 0,
            debug_last_key_event_message: 0,
            debug_last_event_record: None,
            debug_last_button_result: None,
            debug_last_still_down_result: None,
            debug_last_wait_mouse_up_result: None,
            debug_event_queue_probe: EventQueueProbeSnapshot::default(),
            debug_activation_event_seen: false,
            debug_update_event_seen: false,
            debug_wait_next_event_count: 0,
            debug_get_next_event_count: 0,
            debug_mouse_moved_event_count: 0,
            debug_get_mouse_count: 0,
            debug_get_mouse_local_change_count: 0,
            debug_get_mouse_last_local: (0, 0),
            debug_get_mouse_last_global: (0, 0),
            debug_get_mouse_last_port: 0,
            debug_get_mouse_last_port_bounds_top_left: (0, 0),
            debug_still_down_true_count: 0,
            debug_still_down_false_count: 0,
            debug_button_true_count: 0,
            debug_button_false_count: 0,
            debug_wait_mouse_up_true_count: 0,
            debug_wait_mouse_up_false_count: 0,
            debug_set_origin_count: 0,
            debug_copy_bits_count: 0,
            debug_scroll_rect_count: 0,
            debug_scroll_rect_nonzero_delta_count: 0,
            debug_scroll_rect_changed_byte_count: 0,
            debug_scroll_rect_last_changed_bytes: 0,
            debug_scroll_rect_last_rect: (0, 0, 0, 0),
            debug_scroll_rect_last_delta: (0, 0),
            debug_scroll_rect_last_port: 0,
            debug_scroll_rect_last_base: 0,
            debug_scroll_rect_last_row_bytes: 0,
            debug_scroll_rect_last_port_bounds_top_left: (0, 0),
            debug_scroll_rect_last_is_color: false,
            input_trace_enabled: false,
            input_trace_log: Vec::new(),
            event_queue: SharedProcessEventQueue::default(),
            pending_modal_dialog_mouse_up: false,
            pending_modal_dialog_mouse_down: None,
            flushed_update_events: VecDeque::new(),
            current_trap_word: 0,
            current_trap_operation: 0,
            current_trap_adapter: TrapAdapterId::Nonterminal,
            current_selector_operation: None,
            current_trap_caller: None,
            pending_wait_sleep_ticks: 0,
            pending_wait_next_event_return: None,
            pending_hle_tick_cost: 0,
            yield_for_ui: false,
            pending_delay_ticks: 0,
            cursor_state: SharedProcessCursorState::default(),
            trap_count: 0,
            game_trap_count: 0,
            trap_histogram: Box::new([0u64; 4096]),
            trap_time_ns: Box::new([0u64; 4096]),
            copybits_screen_count: 0,
            last_screen_copybits_rect: None,
            last_screen_frame_rect: None,
            last_screen_frame_rect_tick: 0,
            screen_event_count: 0,
            copybits_screen_secs: Vec::new(),
            trace_sink: None,
            main_gdevice_handle: 0,
            current_gdevice: SharedProcessGraphicsDevice::default(),
            current_port: SharedProcessGraphicsPort::default(),
            quickdraw_error: SharedProcessQuickDrawError::default(),
            quickdraw_op_colors: SharedProcessQuickDrawOpColors::default(),
            quickdraw_hilite_colors: SharedProcessQuickDrawHiliteColors::default(),
            process_quickdraw_port_state_attached: false,
            port_draw_states: HashMap::new(),
            resolved_port_color_fields: HashMap::new(),
            gworld_devices: HashMap::new(),
            disposed_gworld_portbits: HashMap::new(),
            gworld_pixel_states: SharedProcessQuickDrawPixelStates::default(),
            cport_ports: HashSet::new(),
            cport_original_pixmaps: HashMap::new(),
            manual_cport_presented_port: 0,
            manual_cport_screen_witness: Vec::new(),
            recording_polygon: None,
            recording_region: None,
            screen_mode: {
                let profile = reference_machine_profile();
                (
                    0,
                    profile.screen_row_bytes(),
                    profile.screen_width,
                    profile.screen_height,
                    profile.screen_depth,
                )
            },
            native_screen_geometry: {
                let profile = reference_machine_profile();
                (profile.screen_width, profile.screen_height)
            },
            device_clut: SharedProcessDisplayClut::from_value(Self::standard_mac_8bpp_clut()),
            display_gamma: crate::process_context::SharedProcessDisplayGamma::default(),
            color_manager_clut: SharedProcessDisplayClut::from_value(Self::standard_mac_8bpp_clut()),
            inverse_table_cache: Vec::new(),
            clut_protected: [false; 256],
            clut_reserved: [false; 256],
            seeded_picture_palette_until_tick: 0,
            seeded_picture_palette: Self::standard_mac_8bpp_clut(),
            screen_palette_fade_active: false,
            recent_resource_ctable_fetch: None,
            window_palettes: HashMap::new(),
            palette_updates: HashMap::new(),
            palette_device_indices: HashMap::new(),
            menu_bar_cache: std::cell::RefCell::new(None),
            window_title_cache: std::cell::RefCell::new(Vec::new()),
            menu_mark_indices: std::cell::Cell::new(None),
            color_mirror: std::cell::RefCell::new(Default::default()),
            color_mirror_fresh: std::cell::Cell::new(false),
            theme_chrome_cache: std::cell::RefCell::new(Vec::new()),
            explicit_palette_ctabs: HashSet::new(),
            icon_transform_override: 0,
            printing_error: 0,
            next_ct_seed: 1,
            fill_black_override: None,
            recording_picture: None,
            recording_picture_bitmap: None,
            trap_table_profile: None,
            trap_exception_vector_defaults: None,
            pending_native_trap_calls: HashMap::new(),
            bits_proc_reentry: None,
            timer_tasks: Default::default(),
            deferred_tasks: VecDeque::new(),
            deferred_install_in_interrupt: false,
            callback_scheduling: Default::default(),
            sleep_queue: Vec::new(),
            vbl_tasks: Default::default(),
            dialog_tracking: None,
            suspended_modal_dialogs: Vec::new(),
            standard_file_put_tracking: None,
            standard_file_get_tracking: None,
            external_host_overlay_rects: Vec::new(),
            dialog_items: HashMap::new(),
            hidden_dialog_item_rects: HashMap::new(),
            dialog_item_handles: HashMap::new(),
            dialog_control_values: HashMap::new(),
            dialog_control_handles: HashMap::new(),
            dialog_std_filter_proc: 0,
            dialog_cancel_items: HashMap::new(),
            dialog_filter_result_addr: 0,
            dialog_saved_pixels: HashMap::new(),
            dialog_visible_snapshots: HashMap::new(),
            dialog_modal_entered: std::collections::HashSet::new(),
            dialog_cdef_draw_pending_snapshot: HashSet::new(),
            dialog_cdefs_initially_drawn: HashSet::new(),
            dialog_edit_text_modified_items: HashSet::new(),
            dialog_initial_draw_deferred: HashSet::new(),
            modeless_dialog_draw_proc_queue: VecDeque::new(),
            modeless_dialog_cdef_draw_queue: VecDeque::new(),
            active_modeless_dialog_draw_proc: None,
            retained_modal_dialog_click: None,
            pending_modal_button_dispose_dialog: None,
            window_stack: Vec::new(),
            saved_vis_regions: HashMap::new(),
            list_states: SharedProcessListManager::default(),
            collections: SharedProcessCollectionManager::default(),
            textedit_states: SharedProcessTextEditManager::default(),
            control_manager: SharedProcessControlManager::default(),
            last_inserted_menu_id: None,
            pending_dialog_popup_menu: None,
            dialog_item_popup_menus: HashMap::new(),
            dialog_popup_original_rects: HashMap::new(),
            dialog_popup_candidate_items: HashSet::new(),
            scrap: SharedProcessScrapState::default(),
            last_init_pack_id: None,
        });
        dispatcher.ensure_vfs_directory("System Folder");
        dispatcher.ensure_vfs_directory("System Folder/Preferences");
        dispatcher
    }

    pub fn set_ui_theme_id(&mut self, ui_theme_id: UiThemeId) {
        self.ui_theme_id = ui_theme_id;
    }

    pub fn ui_theme_id(&self) -> UiThemeId {
        self.ui_theme_id
    }

    pub fn ui_theme(&self) -> &'static dyn UiTheme {
        self.ui_theme_id.provider()
    }

    /// Whether MenuSelect is actively tracking the mouse.
    pub fn is_menu_tracking(&self) -> bool {
        self.menu_tracking.is_some()
    }

    /// Whether ModalDialog is actively tracking user input.
    pub fn is_dialog_tracking(&self) -> bool {
        self.dialog_tracking.is_some()
    }

    /// Whether StandardPutFile/CustomPutFile is actively tracking input.
    pub fn is_standard_file_put_tracking(&self) -> bool {
        self.standard_file_put_tracking.is_some()
    }

    /// Whether StandardGetFile/CustomGetFile is actively tracking input.
    pub fn is_standard_file_get_tracking(&self) -> bool {
        self.standard_file_get_tracking.is_some()
    }

    /// Whether TrackControl is actively tracking a control.
    pub fn is_control_tracking(&self) -> bool {
        self.control_tracking.is_some()
    }

    /// Whether DragWindow is actively tracking the mouse.
    pub fn is_window_tracking(&self) -> bool {
        self.window_tracking.is_some()
    }

    /// Whether TrackGoAway is actively tracking the close box.
    pub fn is_go_away_tracking(&self) -> bool {
        self.go_away_tracking.is_some()
    }

    /// Whether GrowWindow is actively tracking a proposed size.
    pub fn is_grow_window_tracking(&self) -> bool {
        self.grow_window_tracking.is_some()
    }

    /// Whether DragGrayRgn or DragTheRgn is actively tracking the mouse.
    pub fn is_region_tracking(&self) -> bool {
        self.region_tracking.is_some()
    }

    /// Whether TrackControl has redirected execution into a guest scrollbar
    /// action procedure. The runner must let that callback return to the
    /// retained A968 trap instead of immediately rewinding over it.
    pub(crate) fn is_control_action_callback_pending(&self) -> bool {
        self.control_tracking
            .as_ref()
            .is_some_and(|tracking| tracking.scrollbar_callback_pending)
    }

    /// Whether retained menu tracking has entered an application MDEF and
    /// must let that guest callback return to the original menu trap.
    #[cfg(test)]
    pub(crate) fn is_menu_definition_callback_pending(&self) -> bool {
        self.is_menu_definition_callback_pending_with_tracking(&self.menu_tracking)
    }

    pub(crate) fn is_menu_definition_callback_pending_with_tracking(
        &self,
        menu_tracking: &SharedProcessMenuTracking,
    ) -> bool {
        self.guest_calls.menu_bar_build().is_some()
            || self
                .menu_tracking
                .as_ref()
                .or_else(|| menu_tracking.as_ref())
                .and_then(crate::menu_manager::MenuTrackingState::active_definition)
                .or(self.menu_tracking.context().definition.as_ref())
                .is_some_and(|tracking| tracking.pending_invocation().is_some())
    }

    /// Shared check used by both dispatch.rs (auto-pop push-back) and
    /// runner.rs (PC rewind for refire). Returns true when the given trap
    /// word should refire next frame because one of the synchronous Toolbox
    /// tracking loops is active and the trap is the matching routine. Strips
    /// the auto-pop bit (0x0400) so auto-pop variants match too.
    pub fn is_tracking_refire(&self, opcode: u16) -> bool {

        let trap_no_autopop = opcode & !0x0400;
        let is_dialog_refire =
            matches!(trap_no_autopop, 0xA991 | 0xA985 | 0xA986 | 0xA987 | 0xA988);
        let is_standard_file_refire = trap_no_autopop == 0xA9EA;
        let is_control_refire = trap_no_autopop == 0xA968;
        let is_window_refire = trap_no_autopop == 0xA925;
        let is_go_away_refire = trap_no_autopop == 0xA91E;
        let is_track_box_refire = trap_no_autopop == 0xA83B;
        let is_grow_window_refire = trap_no_autopop == 0xA92B;
        let is_region_refire = matches!(trap_no_autopop, 0xA905 | 0xA926);
        (is_dialog_refire && self.is_dialog_tracking())
            || (is_standard_file_refire
                && (self.is_standard_file_put_tracking() || self.is_standard_file_get_tracking()))
            || (is_control_refire
                && (self.is_control_tracking() || self.scrollbar_thumb_tracking.is_some()))
            || (is_window_refire && self.is_window_tracking())
            || (is_go_away_refire && self.is_go_away_tracking())
            || (is_track_box_refire && self.zoom_box_tracking.is_some())
            || (is_grow_window_refire && self.is_grow_window_tracking())
            || (is_region_refire && self.is_region_tracking())
            || (trap_no_autopop == 0xA9D4
                && self.textedit_states.has_classic_click_tracking())
    }

    /// Generate the standard Mac 8-bit system palette as 16-bit RGB values.
    pub(crate) fn standard_mac_8bpp_clut() -> [[u16; 3]; 256] {
        crate::display::standard_mac_8bpp_clut()
    }

    /// Return the canonical indexed Color QuickDraw table and entry count for
    /// a standard screen depth. The 4bpp values match the System 7.5.3
    /// `GetCTable(4)` oracle; in particular, the dark-green entry uses the ROM
    /// value rather than Executor's older 0x64AF green component.
    pub(crate) fn standard_mac_indexed_clut(depth: u16) -> Option<([[u16; 3]; 256], usize)> {
        let mut clut = [[0u16; 3]; 256];
        let entries = match depth {
            1 => {
                clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
                clut[1] = [0x0000, 0x0000, 0x0000];
                2
            }
            2 => {
                clut[0] = [0xFFFF, 0xFFFF, 0xFFFF];
                clut[1] = [0xAAAA, 0xAAAA, 0xAAAA];
                clut[2] = [0x5555, 0x5555, 0x5555];
                clut[3] = [0x0000, 0x0000, 0x0000];
                4
            }
            4 => {
                const COLORS: [[u16; 3]; 16] = [
                    [0xFFFF, 0xFFFF, 0xFFFF],
                    [0xFC00, 0xF37D, 0x052F],
                    [0xFFFF, 0x648A, 0x028C],
                    [0xDD6B, 0x08C2, 0x06A2],
                    [0xF2D7, 0x0856, 0x84EC],
                    [0x46E3, 0x0000, 0xA53E],
                    [0x0000, 0x0000, 0xD400],
                    [0x0241, 0xAB54, 0xEAFF],
                    [0x1F21, 0xB793, 0x1431],
                    [0x0000, 0x8000, 0x11B0],
                    [0x5600, 0x2C9D, 0x0524],
                    [0x90D7, 0x7160, 0x3A34],
                    [0xC000, 0xC000, 0xC000],
                    [0x8000, 0x8000, 0x8000],
                    [0x4000, 0x4000, 0x4000],
                    [0x0000, 0x0000, 0x0000],
                ];
                clut[..COLORS.len()].copy_from_slice(&COLORS);
                COLORS.len()
            }
            8 => return Some((Self::standard_mac_8bpp_clut(), 256)),
            _ => return None,
        };
        Some((clut, entries))
    }

    /// Return a standard color-device table (the depth plus 64 CTable IDs)
    /// with the current highlight color represented in the table.
    ///
    /// Inside Macintosh: Volume VI (1991), pp. 17-17..17-18 and 20-7,
    /// describes IDs 66, 68, and 72 as the standard 2-, 4-, and 8-bit
    /// color tables with the highlight color added. The 2-bit table reserves
    /// index 2 for that color. The 4-bit table replaces the standard entry
    /// nearest to it. The 8-bit table already has 254 color entries; the
    /// System 7.5.3 ROM's GetCTable(72) oracle returns that canonical table
    /// unchanged, so there is no spare entry to replace in that case.
    pub(crate) fn standard_mac_enhanced_clut(
        depth: u16,
        hilite: (u16, u16, u16),
    ) -> Option<([[u16; 3]; 256], usize)> {
        let (mut clut, entries) = Self::standard_mac_indexed_clut(depth)?;
        let hilite = [hilite.0, hilite.1, hilite.2];
        match depth {
            2 => clut[2] = hilite,
            4 => {
                // Index 0 is white and the last entry is black; both are
                // fixed endpoints of the standard table. Select the closest
                // interior entry using the Color Manager's RGB distance.
                let mut closest = 1;
                let mut closest_distance = u64::MAX;
                for index in 1..entries - 1 {
                    let color = clut[index];
                    let distance = color
                        .iter()
                        .zip(hilite.iter())
                        .map(|(&a, &b)| {
                            let delta = i64::from(a) - i64::from(b);
                            (delta * delta) as u64
                        })
                        .sum();
                    if distance < closest_distance {
                        closest = index;
                        closest_distance = distance;
                    }
                }
                clut[closest] = hilite;
            }
            8 => {}
            _ => return None,
        }
        Some((clut, entries))
    }

    /// Return the default 4-bit CTable installed by `NewGWorld` on
    /// System 7.5.3. This differs from the `GetCTable(4)` resource at
    /// dark-green entry 9.
    ///
    /// Inside Macintosh: Imaging With QuickDraw 1994, pp. 6-30..6-31
    pub(crate) fn standard_mac_4bpp_gworld_clut() -> [[u16; 3]; 256] {
        let (mut clut, _) = Self::standard_mac_indexed_clut(4).expect("standard 4-bit CTable");
        clut[9] = [0x0000, 0x64AF, 0x11B0];
        clut
    }

    /// Whether the representable entries match the default 4-bit GWorld
    /// CTable. Callers may carry inherited colors above index 15 because
    /// `read_ctab_handle_clut` overlays short CTables on the logical table.
    pub(crate) fn uses_standard_mac_4bpp_gworld_clut(clut: &[[u16; 3]; 256]) -> bool {
        let standard = Self::standard_mac_4bpp_gworld_clut();
        clut[..16] == standard[..16]
    }

    /// System 7.5.3's `MakeITable` result for the default 4-bit GWorld
    /// CTable at the default resolution of four bits per RGB component.
    /// Each hexadecimal nibble is one of the 4-bit destination indices.
    ///
    /// Color Manager inverse tables use ROM propagation and tie-breaking,
    /// not a fresh Euclidean nearest-color search. Keeping this oracle exact
    /// preserves `Color2Index`, `CopyBits`, and `DrawPicture` results.
    ///
    /// Inside Macintosh Volume V, pp. V-137 and V-142
    pub(crate) fn standard_mac_4bpp_gworld_itable() -> &'static [u8; 4096] {
        const NIBBLES: &str = concat!(
            "fffffff666666666fffffff666666666fffffff666666666f999eeee666666669999eeeee6666666999999999666667799999999997777779999999997777777",
            "99999999777777778888888777777777888888877777777788888888777777778888888877777777888888887777777788888888777777778888888877777777",
            "fffffff555566666fffffff555566666ffffeee555566666f99eeeee55566666999eeeeee55666669999eeeee5566677999999999957777799999999d7777777",
            "8888888dd77777778888888877777777888888887777777788888888877777778888888887777777888888888777777788888888877777778888888887777777",
            "ffffff5555556666ffffee5555556666aaaeeee555556666a9eeeeee5555666699eeeeeee5556666999eeeeee55566779999eeeed55577779999eeeddd577777",
            "8888eedddd7777778888888dd7777777888888887777777788888888877777778888888887777777888888888777777788888888877777778888888887777777",
            "ffffe55555555666aaaeee5555555666aaaeeee555555666aaeeeeee55555666aeeeeeeee555566699eeeeeee5555677999eeeeed5555777999eeeeddd557777",
            "888eeeddddd777778888eedddd7777778888888dd777777788888888d777777788888888d777777788888888d777777788888888d777777788888888d7777777",
            "aaaee55555555555aaaeee5555555555aaaeeee555555555aaeeeeee55555555aeeeeeeee5555555aeeeeeeee555555599eeeeeed555557799eeeeeddd555777",
            "88eeeedddddd7777888eeeddddd777778888eedddd7777778888888ddd7777778888888ddd7777778888888ddd7777778888888ddd7777778888888ddd777777",
            "aaaae55555555555aaaaee5555555555aaaaeee555555555aaaeeeee55555555aaeeeeeee5555555aaeeeeeed5555555a9eeeeeddd555577a9bbeeddddddd777",
            "a8bbedddddddd777888beddddddd77778888edddddd77777888888ddddd77777888888ddddccc777888888ddddccc777888888ddddccc777888888ddddccc770",
            "aaaae55555555555aaaaee5555555555aaaaeee555555555aaaeeeee55555555aaeeeeeed5555555aaeeeeeddd555555a9bbeedddddddd77abbbbbdddddddd77",
            "abbbbddddddddd7788bbbdddddddd777888bbddddddd777788888ddddddcc77788888dddddcccc7788888dddddcccc7788888dddddcccc7018888dddddcccc00",
            "aaaae55555555555aaaaee5555555555aaaaeee555555555aaaeeeedd5555555aaeeeeeddd555555aabbeeddddddddddabbbbbddddddddddbbbbbbdddddddddd",
            "bbbbbdddddddddddbbbbbddddddddd7788bbbdddddddc777888bbddddddccc77888bbdddddcccccc888bbdddddcccccc188bbdddddccccc0111bbdddddcccc00",
            "3333e55555555555aaaaee5555555555aaaaeeddd5555555aaabeedddd555555aabbeeddddddddddabbbbbddddddddddbbbbbbddddddddddbbbbbbdddddddddd",
            "bbbbbdddddddddddbbbbbdddddddddddbbbbbdddddddcc7788bbbddddddccccc88bbbdddddcccccc18bbbdddddcccccc11bbbdddddccccc0111bbdddddcccc00",
            "3333334445555555333bbb4445555555aaabbbbdd5555555aabbbbbddd555555abbbbbbdddddddddbbbbbbbdddddddddbbbbbbbdddddddddbbbbbbbddddddddd",
            "bbbbbbddddddddddbbbbbbddddddccccbbbbbbdddddcccccbbbbbbddddccccccbbbbbbdddccccccc1bbbbbdddccccccc11bbbbdddcccccc0111bbbdddccccc00",
            "33333344445555553333334444555555333bbb444455555533bbbbbddd5555552bbbbbbdddddcccc2bbbbbbdddddcccc2bbbbbbdddddccccbbbbbbbdddddcccc",
            "bbbbbbddddddccccbbbbbbdddddcccccbbbbbbddddccccccbbbbbbdddccccccc1bbbbbcccccccccc11bbbbcccccccccc111bbbccccccccc01111111ccccccc00",
            "333333444444444433333344444444443333334444444444333bbb444444cccc22bbbbbddddccccc22bbbbbddddccccc22bbbbbddddccccc2bbbbbbddddccccc",
            "2bbbbbdddddccccc2bbbbbddddcccccc2bbbbbdddccccccc1bbbbbcccccccccc11bbbccccccccccc111bbccccccccccc111111ccccccccc01111111ccccccc00",
            "333333444444444433333344444444443333334444444444333333444444cccc222bbb44444ccccc222bbbbdddcccccc222bbbbdddcccccc22bbbbbdddcccccc",
            "22bbbbddddcccccc22bbbbdddccccccc22bbbbcccccccccc11bbbccccccccccc111bcccccccccccc11111ccccccccccc111111ccccccccc01111111ccccccc00",
            "3333334444444444333333444444444433333344444444443333334444444444222222444444cccc22222224444ccccc2222222dddcccccc222bbbbdddcccccc",
            "222bbbddddcccccc222bbbdddccccccc222bbbcccccccccc111bbccccccccccc11111ccccccccccc111111ccccccccc01111111ccccccc0011111111ccccc000",
            "33333444444444443333344444444444333334444444444422222444444444442222224444444440222222244444ccc022222222444cccc02222222dddccccc0",
            "222222ddddccccc0222222dddcccccc0222222ccccccccc0111111ccccccccc0111111ccccccccc01111111ccccccc0011111111ccccc0001111111100000000",
            "333344444444444433334444444444442222444444444444222224444444444422222244444444402222222444444400222222224444cc0022222222444ccc00",
            "2222222dddcccc002222222ddccccc002222222ccccccc001111111ccccccc001111111ccccccc0011111111ccccc00011111111000000001111111100000000",
        );

        static TABLE: OnceLock<[u8; 4096]> = OnceLock::new();
        TABLE.get_or_init(|| {
            debug_assert_eq!(NIBBLES.len(), 4096);
            let mut table = [0u8; 4096];
            for (slot, byte) in table.iter_mut().zip(NIBBLES.bytes()) {
                *slot = match byte {
                    b'0'..=b'9' => byte - b'0',
                    b'a'..=b'f' => byte - b'a' + 10,
                    _ => unreachable!("inverse-table oracle contains only hexadecimal nibbles"),
                };
            }
            table
        })
    }

    /// Match an RGB color through the default 4-bit GWorld inverse table.
    /// Color2Index returns exact CTable entries before consulting the
    /// quantized inverse-table cell.
    pub(crate) fn standard_mac_4bpp_gworld_color2index(r: u16, g: u16, b: u16) -> u8 {
        let clut = Self::standard_mac_4bpp_gworld_clut();
        if let Some(index) = clut[..16].iter().position(|entry| *entry == [r, g, b]) {
            return index as u8;
        }
        let cell = (usize::from(r >> 12) << 8) | (usize::from(g >> 12) << 4) | usize::from(b >> 12);
        Self::standard_mac_4bpp_gworld_itable()[cell]
    }

    /// 4-bit-per-channel inverse table (16x16x16 = 4096 cells) for the
    /// standard 8-bit ColorTable. Each cell holds the CLUT index whose entry
    /// is closest (by Euclidean distance in 16-bit RGB) to the centre of that
    /// cube cell.
    ///
    /// QuickDraw associates an inverse table with a GDevice ColorTable. The
    /// table remains stable across hardware-only palette animation, but a
    /// logical ColorTable replacement requires a corresponding inverse table.
    /// This standard-palette table is therefore only an oracle for GDevices
    /// that still have the canonical System ColorTable.
    /// Imaging With QuickDraw 1994, p. 4-82 (MakeITable, default 4 bits)
    pub(crate) fn standard_mac_8bpp_itable() -> [u8; 4096] {
        let clut = Self::standard_mac_8bpp_clut();
        let mut table = [0u8; 4096];
        for cell in 0u32..4096 {
            let qr = (cell >> 8) & 0xF;
            let qg = (cell >> 4) & 0xF;
            let qb = cell & 0xF;
            // Cube cell centre (top 4 bits + 0x0800 mid-cell offset).
            let cr = ((qr << 12) | 0x0800) as i64;
            let cg = ((qg << 12) | 0x0800) as i64;
            let cb = ((qb << 12) | 0x0800) as i64;
            let mut best_idx = 0u8;
            let mut best_dist = i64::MAX;
            for (idx, entry) in clut.iter().enumerate() {
                let dr = cr - i64::from(entry[0]);
                let dg = cg - i64::from(entry[1]);
                let db = cb - i64::from(entry[2]);
                let d = dr * dr + dg * dg + db * db;
                if d < best_dist {
                    best_dist = d;
                    best_idx = idx as u8;
                }
            }
            table[cell as usize] = best_idx;
        }
        table
    }

    /// Look up `(r, g, b)` in the cached system 8bpp ITable.
    /// Inputs are quantised to top 4 bits per channel; the cell index
    /// is `qr<<8 | qg<<4 | qb`.
    pub(crate) fn standard_itable_lookup(r: u16, g: u16, b: u16) -> u8 {
        // Recompute on each call for now — a future iteration can cache
        // the table in a OnceCell when this becomes hot. 4096 entries
        // built in ~256k float ops is well under 1 ms on host hw.
        thread_local! {
            static CACHED: std::cell::OnceCell<[u8; 4096]> = const { std::cell::OnceCell::new() };
        }
        CACHED.with(|cell| {
            let table = cell.get_or_init(Self::standard_mac_8bpp_itable);
            let qr = ((r >> 12) as u32) & 0xF;
            let qg = ((g >> 12) as u32) & 0xF;
            let qb = ((b >> 12) as u32) & 0xF;
            table[(qr << 8 | qg << 4 | qb) as usize]
        })
    }

    /// Resolve an RGB color through the main 8-bit screen GDevice's logical
    /// inverse table.
    ///
    /// The logical ColorTable is deliberately distinct from the live video
    /// DAC palette: low-level fades change how existing pixels are displayed
    /// without changing which pixel value QuickDraw selects. A genuine
    /// ColorTable replacement, however, must change the inverse lookup. This
    /// mirrors the `ctSeed`/`iTabSeed` relationship described by Inside
    /// Macintosh: Advanced Color Imaging, "Inverse Tables".
    pub(crate) fn screen_itable_index(clut: &[[u16; 3]; 256], rgb: [u16; 3]) -> u8 {
        // Preserve the conventional endpoint pixels even when the ColorTable
        // contains duplicate white or black entries.
        if rgb == [0xFFFF; 3] && clut[0] == rgb {
            return 0;
        }
        if rgb == [0; 3] && clut[255] == rgb {
            return 255;
        }

        // Keep the exact System 7.5.3 oracle, including its propagation and
        // tie-breaking, while the logical GDevice table is canonical.
        static STANDARD_CLUT: OnceLock<[[u16; 3]; 256]> = OnceLock::new();
        let standard_clut = STANDARD_CLUT.get_or_init(Self::standard_mac_8bpp_clut);
        if clut == standard_clut {
            return Self::standard_itable_lookup(rgb[0], rgb[1], rgb[2]);
        }

        // Color2Index returns an exact ColorTable entry before consulting
        // the quantized inverse-table cell.
        if let Some(index) = clut.iter().position(|entry| *entry == rgb) {
            return index as u8;
        }

        // A custom table uses the same four-bit cell-centre rule as
        // `build_inverse_table_bytes`, but computing the one requested cell
        // avoids constructing a 4096-byte table for infrequent RGBForeColor
        // and RGBBackColor calls.
        let centre = |component: u16| i64::from((component & 0xF000) | 0x0800);
        let cr = centre(rgb[0]);
        let cg = centre(rgb[1]);
        let cb = centre(rgb[2]);
        let mut best_index = 0u8;
        let mut best_distance = i64::MAX;
        for (index, entry) in clut.iter().enumerate() {
            let dr = cr - i64::from(entry[0]);
            let dg = cg - i64::from(entry[1]);
            let db = cb - i64::from(entry[2]);
            let distance = dr * dr + dg * dg + db * db;
            if distance < best_distance {
                best_index = index as u8;
                best_distance = distance;
            }
        }
        best_index
    }

    /// Register loaded segments for LoadSeg trap.
    pub fn register_segments(&mut self, segments: HashMap<i16, u32>) {
        self.segment_map = segments;
    }

    fn normalize_vfs_path_components(path: &str) -> String {
        path.split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .collect::<Vec<_>>()
            .join("/")
    }

    pub(crate) fn is_unix_tmp_path(name: &str) -> bool {
        let path = name.strip_prefix("Unix:").unwrap_or(name);
        path == "/tmp" || path.starts_with("/tmp/")
    }

    pub(crate) fn normalize_vfs_path(name: &str) -> String {
        let path = name.strip_prefix("Unix:").unwrap_or(name);
        if Self::is_unix_tmp_path(path) {
            let tail = path.strip_prefix("/tmp").unwrap_or("");
            let tail = Self::normalize_vfs_path_components(&tail.replace(':', "/"));
            return if tail.is_empty() {
                "Temporary Items".to_string()
            } else {
                format!("Temporary Items/{tail}")
            };
        }

        let path = path.replace(':', "/");
        Self::normalize_vfs_path_components(&path)
    }

    pub(crate) fn encode_hfs_component_for_vfs(component: &str) -> String {
        component
            .chars()
            .map(|character| {
                if character == '/' {
                    VFS_HFS_LITERAL_SLASH
                } else {
                    character
                }
            })
            .collect()
    }

    pub(crate) fn normalize_hfs_path(name: &str) -> String {
        // MPW fixtures address the synthetic host mount as "Unix:", while
        // the VFS stores that volume's contents directly at its root.
        let path = name.strip_prefix("Unix:").unwrap_or(name);
        if Self::is_unix_tmp_path(path) {
            return Self::normalize_vfs_path(path);
        }
        path.split(':')
            .filter(|component| !component.is_empty() && *component != ".")
            .map(Self::encode_hfs_component_for_vfs)
            .collect::<Vec<_>>()
            .join("/")
    }

    pub(crate) fn normalize_hfs_lookup_path(name: &str) -> String {
        let normalized = Self::normalize_hfs_path(name);
        if name.starts_with(':') || !name.contains(':') {
            return normalized;
        }
        let Some((volume_name, remainder)) = normalized.split_once('/') else {
            return normalized;
        };
        if volume_name.eq_ignore_ascii_case(Self::boot_volume_name()) {
            remainder.to_string()
        } else {
            normalized
        }
    }

    pub(crate) fn hfs_name_from_vfs_component(component: &str) -> String {
        component
            .chars()
            .map(|character| {
                if character == VFS_HFS_LITERAL_SLASH {
                    '/'
                } else {
                    character
                }
            })
            .collect()
    }

    pub(crate) fn boot_volume_name() -> &'static str {
        BOOT_VOLUME_NAME
    }

    /// Fetch a file's data-fork bytes from the VFS, matching by normalized,
    /// case-insensitive path (the same rule OpenMovieFile uses). Used to feed
    /// QuickTime movie sample data that lives in the data fork.
    pub(crate) fn vfs_data_fork_bytes(&self, name: &str) -> Option<Vec<u8>> {
        let target = Self::normalize_vfs_path(name);
        self.vfs
            .iter()
            .find(|(key, _)| Self::normalize_vfs_path(key).eq_ignore_ascii_case(&target))
            .map(|(_, bytes)| bytes.to_vec())
    }

    pub(crate) fn boot_volume_ref_num() -> i16 {
        BOOT_VOLUME_REF_NUM
    }

    /// Mount an extracted disk-image root as a read-only File Manager volume.
    /// The root remains a normal top-level VFS directory for compatibility
    /// while File Manager calls receive a stable negative volume reference.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mount_vfs_volume(
        &mut self,
        name: &str,
        attributes: u16,
        file_count: u16,
        allocation_block_count: u16,
        allocation_block_size: u32,
        clump_size: u32,
        free_blocks: u16,
        bitmap_start: u16,
        allocation_pointer: u16,
        allocation_start: u16,
        next_catalog_id: u32,
        created_date: u32,
        modified_date: u32,
    ) -> i16 {
        let normalized = Self::normalize_vfs_path(name);
        if normalized.is_empty() || normalized.eq_ignore_ascii_case(BOOT_VOLUME_NAME) {
            return Self::boot_volume_ref_num();
        }
        if let Some(volume) = self
            .vfs_volumes
            .iter()
            .find(|volume| volume.name.eq_ignore_ascii_case(&normalized))
        {
            return volume.ref_num;
        }

        let root_dir_id = self.ensure_vfs_directory(&normalized);
        let mut ref_num = *self.next_vfs_volume_ref_num;
        while ref_num == 0
            || ref_num == Self::boot_volume_ref_num()
            || self
                .vfs_volumes
                .iter()
                .any(|volume| volume.ref_num == ref_num)
        {
            ref_num = ref_num.saturating_sub(1);
        }
        self.next_vfs_volume_ref_num
            .with_mut(|next_ref_num| *next_ref_num = ref_num.saturating_sub(1));
        self.vfs_volumes.push(VfsVolume {
            ref_num,
            name: normalized,
            root_dir_id,
            // Extracted images are immutable media. Report the VCB's
            // hardware-lock bit so PBHGetVInfo agrees with mutations,
            // which return wPrErr (hardware volume lock). Inside
            // Macintosh: Files, pp. 2-127, 2-144, and 2-329.
            attributes: attributes | 0x0080,
            file_count,
            allocation_block_count,
            allocation_block_size,
            clump_size,
            free_blocks,
            bitmap_start,
            allocation_pointer,
            allocation_start,
            next_catalog_id,
            created_date,
            modified_date,
        });
        ref_num
    }

    pub(crate) fn vfs_volume_by_name(&self, name: &str) -> Option<&VfsVolume> {
        let normalized = Self::normalize_vfs_path(name);
        self.vfs_volumes
            .iter()
            .find(|volume| volume.name.eq_ignore_ascii_case(&normalized))
    }

    pub(crate) fn vfs_volume_for_ref_num(&self, ref_num: i16) -> Option<&VfsVolume> {
        self.vfs_volumes
            .iter()
            .find(|volume| volume.ref_num == ref_num)
    }

    pub(crate) fn vfs_volume_for_path(&self, path: &str) -> Option<&VfsVolume> {
        let normalized = Self::normalize_vfs_path(path);
        let root = normalized.split('/').next()?;
        self.vfs_volume_by_name(root)
    }

    /// Return whether a VFS path belongs to an extracted disk-image volume.
    /// Resource-fork mirrors use a `__rsrc__` prefix, so strip it before
    /// resolving the volume root. The synthetic boot volume remains writable;
    /// extracted image volumes are immutable by construction.
    pub(crate) fn vfs_path_is_read_only(&self, path: &str) -> bool {
        let path = path.strip_prefix("__rsrc__").unwrap_or(path);
        self.vfs_volume_for_path(path).is_some()
    }

    pub(crate) fn boot_volume_ref_num_u16() -> u16 {
        BOOT_VOLUME_REF_NUM as u16
    }

    pub(crate) fn vfs_parent_path(path: &str) -> &str {
        path.rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("")
    }

    pub(crate) fn vfs_basename(path: &str) -> &str {
        path.rsplit('/').next().unwrap_or(path)
    }

    pub(crate) fn vfs_directory_name(path: &str) -> String {
        if path.is_empty() {
            BOOT_VOLUME_NAME.to_string()
        } else {
            Self::vfs_basename(path).to_string()
        }
    }

    fn allocate_vfs_timestamp(&mut self) -> u32 {
        let timestamp = *self.next_vfs_timestamp;
        self.next_vfs_timestamp
            .with_mut(|next_timestamp| *next_timestamp = next_timestamp.saturating_add(1));
        timestamp
    }

    fn find_case_insensitive_key<'a, I>(keys: I, target: &str) -> Option<String>
    where
        I: IntoIterator<Item = &'a String>,
    {
        // Sort keys for deterministic first-match when multiple case-different
        // forms of the same path coexist in VFS. Without the sort, HashMap
        // iteration order makes which form "wins" depend on hash randomisation.
        let normalized_target = Self::normalize_vfs_path(target);
        let mut sorted: Vec<&String> = keys.into_iter().collect();
        sorted.sort_unstable();
        sorted
            .into_iter()
            .find(|key| Self::normalize_vfs_path(key).eq_ignore_ascii_case(&normalized_target))
            .cloned()
    }

    pub(crate) fn find_case_insensitive_relative_key<'a, I>(keys: I, target: &str) -> Option<String>
    where
        I: IntoIterator<Item = &'a String>,
    {
        let normalized_target = Self::normalize_vfs_path(target);
        if !normalized_target.contains('/') {
            return None;
        }
        // A leading colon denotes a partial HFS pathname whose directory
        // components remain significant. Match the whole normalized suffix
        // on a component boundary before any basename-only compatibility
        // fallback can discard those components.
        // Inside Macintosh: Files (1992), pp. 2-27 to 2-30.
        let suffix = format!("/{normalized_target}").to_ascii_lowercase();
        let mut sorted: Vec<&String> = keys.into_iter().collect();
        sorted.sort_unstable();
        sorted
            .into_iter()
            .find(|key| {
                Self::normalize_vfs_path(key)
                    .to_ascii_lowercase()
                    .ends_with(&suffix)
            })
            .cloned()
    }

    pub(crate) fn ensure_vfs_directory(&mut self, path: &str) -> u32 {
        let normalized = Self::normalize_vfs_path(path);
        if normalized.is_empty() {
            return 2;
        }
        if let Some(directory) = self
            .vfs_directories
            .iter()
            .find(|directory| directory.path.eq_ignore_ascii_case(&normalized))
        {
            return directory.dir_id;
        }

        let parent_path = Self::vfs_parent_path(&normalized).to_string();
        let parent_dir_id = self.ensure_vfs_directory(&parent_path);
        let dir_id = *self.next_vfs_dir_id;
        self.next_vfs_dir_id
            .with_mut(|next_dir_id| *next_dir_id = next_dir_id.saturating_add(1));

        self.vfs_directories.push(VfsDirectory {
            dir_id,
            parent_dir_id,
            path: normalized,
            creator: u32::from_be_bytes(*b"MACS"),
            file_type: u32::from_be_bytes(*b"fold"),
            finder_flags: 0,
            dirty: true,
        });
        dir_id
    }

    pub(crate) fn ensure_vfs_file_metadata(&mut self, path: &str) {
        let normalized = Self::normalize_vfs_path(path);
        if normalized.is_empty() {
            return;
        }
        if self.vfs_metadata.contains_key(&normalized) {
            self.process_file_system.with_mut(|file_system| {
                file_system.publish_classic_vfs_metadata(&normalized);
            });
            return;
        }

        let parent_path = Self::vfs_parent_path(&normalized).to_string();
        let parent_dir_id = self.ensure_vfs_directory(&parent_path);
        let timestamp = self.allocate_vfs_timestamp();
        let file_id = *self.next_vfs_file_id;
        self.vfs_metadata.insert(
            normalized.clone(),
            VfsMetadata {
                file_id,
                parent_dir_id,
                file_type: u32::from_be_bytes(*b"????"),
                creator: u32::from_be_bytes(*b"????"),
                finder_flags: 0,
                created_date: timestamp,
                modified_date: timestamp,
            },
        );
        self.next_vfs_file_id
            .with_mut(|next_file_id| *next_file_id = next_file_id.saturating_add(1));
        self.process_file_system.with_mut(|file_system| {
            file_system.publish_classic_vfs_metadata(&normalized);
        });
    }

    pub(crate) fn ensure_vfs_catalog(&mut self) {
        // Sort keys before assigning dir_ids so the values assigned by
        // ensure_vfs_directory (which increments next_vfs_dir_id in insertion
        // order) are deterministic across runs. Without the sort, dir_id
        // assignments depend on HashMap hash randomisation.
        let mut keys: Vec<String> = self.vfs.keys().cloned().collect();
        for key in self.vfs_rsrc.keys() {
            if !keys.iter().any(|existing| existing == key) {
                keys.push(key.clone());
            }
        }
        keys.sort_unstable();
        for key in keys {
            let normalized = Self::normalize_vfs_path(&key);
            if normalized.is_empty() {
                continue;
            }
            let parent = Self::vfs_parent_path(&normalized).to_string();
            self.ensure_vfs_directory(&parent);
            self.ensure_vfs_file_metadata(&normalized);
        }
    }

    pub(crate) fn set_vfs_entry_metadata(
        &mut self,
        name: &str,
        file_type: [u8; 4],
        creator: [u8; 4],
        finder_flags: u16,
    ) {
        let normalized = Self::normalize_vfs_path(name);
        self.ensure_vfs_file_metadata(&normalized);
        self.vfs_metadata.update(&normalized, |metadata| {
            metadata.file_type = u32::from_be_bytes(file_type);
            metadata.creator = u32::from_be_bytes(creator);
            metadata.finder_flags = finder_flags;
        });
        self.process_file_system.with_mut(|file_system| {
            file_system.publish_classic_vfs_metadata(&normalized);
        });
    }

    pub(crate) fn set_vfs_entry_finfo(
        &mut self,
        name: &str,
        file_type: u32,
        creator: u32,
        finder_flags: u16,
    ) {
        let normalized = Self::normalize_vfs_path(name);
        self.ensure_vfs_file_metadata(&normalized);
        self.vfs_metadata.update(&normalized, |metadata| {
            metadata.file_type = file_type;
            metadata.creator = creator;
            metadata.finder_flags = finder_flags;
        });
        self.process_file_system.with_mut(|file_system| {
            file_system.publish_classic_vfs_metadata(&normalized);
        });
    }

    pub(crate) fn set_launched_app_path(&mut self, name: &str) {
        let normalized = Self::normalize_vfs_path(name);
        self.ensure_vfs_file_metadata(&normalized);
        if let Some(metadata) = self.vfs_metadata.get(&normalized).copied() {
            self.default_dir_id
                .with_mut(|default_dir_id| *default_dir_id = metadata.parent_dir_id);
            self.process_file_system
                .default_dir_id
                .with_mut(|default_dir_id| *default_dir_id = metadata.parent_dir_id);
            let app_volume_ref = self
                .vfs_volume_for_path(&normalized)
                .map(|volume| volume.ref_num)
                .unwrap_or(Self::boot_volume_ref_num());
            // Open a working directory for the app's parent folder so that
            // PBGetVol returns a WDRefNum and PBGetWDInfo resolves the correct dirID.
            // Inside Macintosh Volume IV, IV-72
            if let Some(wd_ref) =
                self.open_working_directory(app_volume_ref, metadata.parent_dir_id, 0)
            {
                self.app_wd_refnum
                    .with_mut(|app_ref_num| *app_ref_num = wd_ref);
            }
        }
        self.process_file_system
            .with_mut(|file_system| file_system.launched_app_path = Some(normalized));
    }

    pub fn launched_app_path(&self) -> Option<&str> {
        self.process_file_system.launched_app_path.as_deref()
    }

    pub fn materialize_quilt_resources(&mut self) -> usize {
        let (materialized_count, synthesized_files) = self.vfs_rsrc.with_mut(|vfs_rsrc| {
            crate::managers::resource::quilt::materialize_quilt_resources_for_vfs(
                &self.vfs, vfs_rsrc,
            )
        });
        for (synth_path, file_type, creator, finder_flags) in synthesized_files {
            self.set_vfs_entry_metadata(&synth_path, file_type, creator, finder_flags);
        }
        materialized_count
    }

    pub(crate) fn materialize_named_quilt_resource_file(&mut self, path: &str) -> Option<String> {
        let normalized = Self::normalize_vfs_path(path);
        let hfs_normalized = Self::normalize_hfs_path(path);
        let target_key = if self.vfs_rsrc.contains_key(&hfs_normalized) {
            return Some(hfs_normalized);
        } else if self.vfs_rsrc.contains_key(&normalized) {
            return Some(normalized);
        } else if !hfs_normalized.is_empty() {
            hfs_normalized
        } else {
            normalized
        };

        if let Some((source_file, mut quilt_entries)) =
            crate::managers::resource::quilt::quilt_named_resource_records(
                &self.vfs,
                &self.vfs_rsrc,
                &target_key,
            )
        {
            crate::managers::resource::quilt::synthesize_quilt_img_resource_if_missing(
                &target_key,
                &mut quilt_entries,
            );
            if let Some(fork_data) =
                crate::managers::resource::serialize_resource_fork(&quilt_entries)
            {
                let (creator, file_type, finder_flags) = self
                    .vfs_metadata
                    .get(&source_file)
                    .map(|m| (m.creator, m.file_type, m.finder_flags))
                    .unwrap_or((
                        u32::from_be_bytes(*b"Game"),
                        u32::from_be_bytes(*b"bits"),
                        0,
                    ));

                let materialized_path = target_key.clone();
                self.vfs_rsrc.insert(materialized_path.clone(), fork_data);
                self.set_vfs_entry_finfo(&materialized_path, file_type, creator, finder_flags);
                return Some(materialized_path);
            }
        }
        None
    }

    pub(crate) fn queue_pending_launch_application(&mut self, name: &str, after_event_yield: bool) {
        let normalized = Self::normalize_vfs_path(name);
        self.pending_launch_app = Some(PendingLaunchApplication {
            path: normalized,
            after_event_yield,
            after_caller_exit: false,
        });
    }

    pub(crate) fn queue_background_launch_application(&mut self, name: &str) {
        let normalized = Self::normalize_vfs_path(name);
        self.pending_launch_app = Some(PendingLaunchApplication {
            path: normalized,
            after_event_yield: false,
            after_caller_exit: true,
        });
    }

    pub(crate) fn take_pending_launch_application(
        &mut self,
        event_yield_reached: bool,
        caller_exited: bool,
    ) -> Option<String> {
        let ready = self.pending_launch_app.as_ref().is_some_and(|pending| {
            caller_exited
                || ((!pending.after_event_yield || event_yield_reached)
                    && !pending.after_caller_exit)
        });
        if ready {
            self.pending_launch_app.take().map(|pending| pending.path)
        } else {
            None
        }
    }

    pub(crate) fn touch_vfs_entry(&mut self, name: &str) {
        let normalized = Self::normalize_vfs_path(name);
        self.ensure_vfs_file_metadata(&normalized);
        let timestamp = self.allocate_vfs_timestamp();
        self.vfs_metadata.update(&normalized, |metadata| {
            metadata.modified_date = timestamp;
            if metadata.created_date == 0 {
                metadata.created_date = timestamp;
            }
        });
        self.process_file_system.with_mut(|file_system| {
            file_system.publish_classic_vfs_metadata(&normalized);
        });
    }

    pub(crate) fn remove_vfs_entry_metadata(&mut self, name: &str) {
        let normalized = Self::normalize_vfs_path(name);
        self.vfs_metadata.remove(&normalized);
    }

    pub(crate) fn publish_vfs_entry_to_process(&mut self, name: &str) {
        let normalized = Self::normalize_vfs_path(name);
        self.process_file_system.with_mut(|file_system| {
            file_system.publish_classic_vfs_metadata(&normalized);
        });
    }

    pub(crate) fn remove_vfs_entry_from_process(&mut self, name: &str) {
        let normalized = Self::normalize_vfs_path(name);
        self.process_file_system
            .with_mut(|file_system| file_system.remove_classic_vfs_path(&normalized));
    }

    pub fn remove_vfs_path(&mut self, name: &str) -> bool {
        let normalized = Self::normalize_vfs_path(name);
        if normalized.is_empty() {
            return false;
        }

        let prefix = format!("{}/", normalized);
        let mut removed = false;

        let data_keys: Vec<String> = self
            .vfs
            .keys()
            .filter(|key| *key == &normalized || key.starts_with(&prefix))
            .cloned()
            .collect();
        for key in data_keys {
            removed |= self.vfs.remove(&key).is_some();
            self.vfs_metadata.remove(&key);
        }

        let rsrc_keys: Vec<String> = self
            .vfs_rsrc
            .keys()
            .filter(|key| *key == &normalized || key.starts_with(&prefix))
            .cloned()
            .collect();
        for key in rsrc_keys {
            removed |= self.vfs_rsrc.remove(&key).is_some();
            self.vfs_metadata.remove(&key);
        }

        removed |= self.vfs_metadata.remove(&normalized).is_some();

        let directory_count = self.vfs_directories.len();
        self.vfs_directories.retain(|directory| {
            let path = directory.path.to_ascii_lowercase();
            let normalized = normalized.to_ascii_lowercase();
            let prefix = prefix.to_ascii_lowercase();
            !(path == normalized || path.starts_with(&prefix))
        });
        if self.vfs_directories.len() != directory_count {
            removed = true;
        }

        self.process_file_system
            .with_mut(|file_system| file_system.remove_classic_vfs_path(&normalized));

        removed
    }

    pub fn remove_vfs_path_relative_to_launched_app(&mut self, name: &str) -> bool {
        if self.remove_vfs_path(name) {
            return true;
        }

        let Some(app_path) = self.launched_app_path().map(str::to_owned) else {
            return false;
        };
        let parent = Self::vfs_parent_path(&app_path);
        if parent.is_empty() {
            return false;
        }

        let normalized = Self::normalize_vfs_path(name);
        self.remove_vfs_path(&format!("{}/{}", parent, normalized))
    }

    pub(crate) fn vfs_file_metadata(&mut self, name: &str) -> Option<VfsMetadata> {
        let normalized = Self::normalize_vfs_path(name);
        if self.vfs.contains_key(&normalized) || self.vfs_rsrc.contains_key(&normalized) {
            self.ensure_vfs_file_metadata(&normalized);
            return self.vfs_metadata.get(&normalized).copied();
        }
        None
    }

    pub(crate) fn directory_path_for_id(&self, dir_id: u32) -> Option<&str> {
        self.vfs_directories
            .iter()
            .find(|directory| directory.dir_id == dir_id)
            .map(|directory| directory.path.as_str())
    }

    pub(crate) fn directory_entry_for_id(&self, dir_id: u32) -> Option<&VfsDirectory> {
        self.vfs_directories
            .iter()
            .find(|directory| directory.dir_id == dir_id)
    }

    pub(crate) fn resolve_volume_ref_num(&self, vref: i16) -> i16 {
        if vref == 0 {
            return Self::boot_volume_ref_num();
        }
        if vref == Self::boot_volume_ref_num() {
            return vref;
        }
        if self.vfs_volumes.iter().any(|volume| volume.ref_num == vref) {
            return vref;
        }
        if let Some(working_directory) = self.working_directories.get(&vref) {
            return working_directory.volume_ref_num;
        }
        Self::boot_volume_ref_num()
    }

    pub(crate) fn resolve_directory_id(&self, vref: i16, dir_id: u32) -> u32 {
        // HFS lookups treat WD refnums as an implicit directory selector when
        // ioDirID is 0 or 1, and they treat vRefNum=0 + ioDirID=0 as the
        // current default directory. Files 1992, 2-151 to 2-153.
        if dir_id <= 1 {
            if let Some(volume) = self.vfs_volume_for_ref_num(vref) {
                return volume.root_dir_id;
            }
            if let Some(working_directory) = self.working_directories.get(&vref) {
                return working_directory.dir_id;
            }
            if dir_id == 0 && vref == 0 {
                return *self.default_dir_id;
            }
            if dir_id == 0 {
                return 2;
            }
            return dir_id;
        }
        if dir_id != 0 {
            return dir_id;
        }
        if vref == 0 {
            return *self.default_dir_id;
        }
        if vref == Self::boot_volume_ref_num() {
            return 2;
        }
        if let Some(working_directory) = self.working_directories.get(&vref) {
            return working_directory.dir_id;
        }
        2
    }

    pub(crate) fn resolve_volume_and_directory(&self, vref: i16, dir_id: u32) -> (i16, u32) {
        (
            self.resolve_volume_ref_num(vref),
            self.resolve_directory_id(vref, dir_id),
        )
    }

    pub(crate) fn hfs_lookup_directory_ids(&self, vref: i16, dir_id: u32) -> Vec<u32> {
        let primary_dir_id = self.resolve_directory_id(vref, dir_id);
        let mut dir_ids = vec![primary_dir_id];

        // Executor retries by-name HFS lookups with the directory implied by
        // the default volume or WD refnum when an explicit ioDirID fails.
        // Mirror that fallback so callers that leave ioDirID stale still find
        // files relative to the current working directory.
        let fallback_dir_id = if vref == 0 {
            Some(*self.default_dir_id)
        } else {
            self.working_directories.get(&vref).map(|wd| wd.dir_id)
        };

        if let Some(fallback_dir_id) = fallback_dir_id {
            if fallback_dir_id != primary_dir_id {
                dir_ids.push(fallback_dir_id);
            }
        }

        dir_ids
    }

    pub(crate) fn open_working_directory(
        &mut self,
        vref: i16,
        dir_id: u32,
        proc_id: u32,
    ) -> Option<i16> {
        let (volume_ref_num, effective_dir_id) = self.resolve_volume_and_directory(vref, dir_id);
        self.directory_path_for_id(effective_dir_id)?;
        if effective_dir_id == 2 {
            return Some(volume_ref_num);
        }

        if let Some(existing) = self
            .working_directories
            .values()
            .find(|entry| {
                entry.volume_ref_num == volume_ref_num
                    && entry.dir_id == effective_dir_id
                    && entry.proc_id == proc_id
            })
            .copied()
        {
            return Some(existing.ref_num);
        }

        let mut ref_num = *self.next_working_dir_refnum;
        while self.working_directories.contains_key(&ref_num) {
            ref_num = ref_num.saturating_add(1);
        }
        self.next_working_dir_refnum
            .with_mut(|next| *next = ref_num.saturating_add(1));
        self.working_directories.with_mut(|directories| {
            directories.insert(
                ref_num,
                WorkingDirectory {
                    ref_num,
                    volume_ref_num,
                    dir_id: effective_dir_id,
                    proc_id,
                },
            );
        });
        Some(ref_num)
    }

    pub(crate) fn close_working_directory(&mut self, wd_ref_num: i16) -> bool {
        let Some(record) = self
            .working_directories
            .with_mut(|directories| directories.remove(&wd_ref_num))
        else {
            return false;
        };
        if *self.app_wd_refnum == wd_ref_num {
            self.app_wd_refnum
                .with_mut(|app_ref_num| *app_ref_num = record.volume_ref_num);
        }
        true
    }

    pub(crate) fn working_directory_info(&self, wd_ref_num: i16) -> Option<WorkingDirectory> {
        if wd_ref_num == Self::boot_volume_ref_num() {
            return Some(WorkingDirectory {
                ref_num: wd_ref_num,
                volume_ref_num: Self::boot_volume_ref_num(),
                dir_id: 2,
                proc_id: 0,
            });
        }
        if let Some(volume) = self.vfs_volume_for_ref_num(wd_ref_num) {
            return Some(WorkingDirectory {
                ref_num: wd_ref_num,
                volume_ref_num: wd_ref_num,
                dir_id: volume.root_dir_id,
                proc_id: 0,
            });
        }
        self.working_directories.get(&wd_ref_num).copied()
    }

    pub(crate) fn working_directory_by_index(
        &self,
        index: i16,
        volume_spec: i16,
    ) -> Option<WorkingDirectory> {
        if index <= 0 {
            return None;
        }
        let target_volume = if volume_spec == 0 {
            None
        } else {
            Some(self.resolve_volume_ref_num(volume_spec))
        };
        let mut working_directories: Vec<WorkingDirectory> = self
            .working_directories
            .values()
            .copied()
            .filter(|entry| {
                target_volume
                    .map(|volume_ref_num| entry.volume_ref_num == volume_ref_num)
                    .unwrap_or(true)
            })
            .collect();
        working_directories.sort_by_key(|entry| entry.ref_num);
        working_directories.get(index as usize - 1).copied()
    }

    pub(crate) fn find_vfs_file_in_directory(&mut self, dir_id: u32, name: &str) -> Option<String> {
        self.ensure_vfs_catalog();
        let normalized = Self::normalize_hfs_lookup_path(name);
        if let Some(dir_path) = self.directory_path_for_id(dir_id) {
            let candidate = if dir_path.is_empty() {
                normalized.clone()
            } else {
                format!("{dir_path}/{normalized}")
            };
            if let Some(found) = Self::find_case_insensitive_key(self.vfs.keys(), &candidate) {
                return Some(found);
            }

            // Fallback: search inside subdirectories whose names start with
            // the requested filename.  StuffIt archives sometimes nest a file
            // in a folder whose name differs only by a trailing "s" or extra
            // suffix (e.g. "Physics Models/Standard" when the app asks for
            // "Physics Model").  Look for the first data-fork file inside any
            // matching subdirectory.
            let prefix = format!("{}/", candidate);
            let prefix_lower = prefix.to_ascii_lowercase();
            // Sort keys for deterministic "first match" when multiple
            // subdirectory entries share the same prefix. HashMap iteration
            // order is randomized so the first-match would otherwise vary
            // across runs.
            let mut sorted_keys: Vec<&String> = self.vfs.keys().collect();
            sorted_keys.sort_unstable();
            let mut subdir_match: Option<String> = None;
            for key in sorted_keys {
                let key_lower = key.to_ascii_lowercase();
                if key_lower.starts_with(&prefix_lower) {
                    // Skip resource-fork "Icon" files — prefer actual data files.
                    let basename = key.rsplit('/').next().unwrap_or(key);
                    if basename.eq_ignore_ascii_case("Icon") {
                        continue;
                    }
                    subdir_match = Some(key.clone());
                    break;
                }
            }
            if let Some(found) = subdir_match {
                return Some(found);
            }

            // Some archives flatten companion folders while the app still
            // asks for a partial pathname such as ":Resources:Settings".
            // Keep that compatibility fallback scoped to the explicitly
            // requested parent directory; do not degrade to a volume-wide
            // basename search when a concrete parent dirID was supplied.
            let basename = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
            if basename != normalized {
                let sibling = if dir_path.is_empty() {
                    basename.to_string()
                } else {
                    format!("{dir_path}/{basename}")
                };
                if let Some(found) = Self::find_case_insensitive_key(self.vfs.keys(), &sibling) {
                    return Some(found);
                }
            }
        }
        if normalized.contains('/') {
            if let Some(found) = Self::find_case_insensitive_key(self.vfs.keys(), &normalized) {
                return Some(found);
            }
        }
        None
    }

    pub(crate) fn find_vfs_rsrc_file_in_directory(
        &mut self,
        dir_id: u32,
        name: &str,
    ) -> Option<String> {
        self.ensure_vfs_catalog();
        let normalized = Self::normalize_hfs_lookup_path(name);
        if let Some(dir_path) = self.directory_path_for_id(dir_id) {
            let candidate = if dir_path.is_empty() {
                normalized.clone()
            } else {
                format!("{dir_path}/{normalized}")
            };
            if let Some(found) = Self::find_case_insensitive_key(self.vfs_rsrc.keys(), &candidate) {
                return Some(found);
            }
            if let Some(found) = self.materialize_named_quilt_resource_file(&candidate) {
                return Some(found);
            }
        }
        if normalized.contains('/') {
            if let Some(found) = Self::find_case_insensitive_key(self.vfs_rsrc.keys(), &normalized)
            {
                return Some(found);
            }
            if let Some(found) = self.materialize_named_quilt_resource_file(&normalized) {
                return Some(found);
            }
        }
        if let Some(dir_path) = self.directory_path_for_id(dir_id) {
            let basename = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
            if basename != normalized {
                let sibling = if dir_path.is_empty() {
                    basename.to_string()
                } else {
                    format!("{dir_path}/{basename}")
                };
                if let Some(found) = Self::find_case_insensitive_key(self.vfs_rsrc.keys(), &sibling)
                {
                    return Some(found);
                }
                if let Some(found) = self.materialize_named_quilt_resource_file(&sibling) {
                    return Some(found);
                }
            }
        }
        if let Some(found) = self.materialize_named_quilt_resource_file(name) {
            return Some(found);
        }
        None
    }

    pub(crate) fn find_vfs_directory_in_directory(
        &mut self,
        dir_id: u32,
        name: &str,
    ) -> Option<String> {
        self.ensure_vfs_catalog();
        let normalized = Self::normalize_hfs_lookup_path(name);
        if normalized.is_empty() {
            return None;
        }
        // HFS identifies a volume's root using its volume reference number,
        // reserved parent directory ID 1, and volume name. The root itself has
        // directory ID 2. Files 1992, 1-27 and 2-85.
        if dir_id == 1 && normalized.eq_ignore_ascii_case(BOOT_VOLUME_NAME) {
            return Some(String::new());
        }
        if dir_id == 1 {
            if let Some(volume) = self.vfs_volume_by_name(&normalized) {
                return Some(
                    self.directory_path_for_id(volume.root_dir_id)
                        .unwrap_or_default()
                        .to_string(),
                );
            }
        }
        // Sort paths before first-match .find() to keep directory resolution
        // deterministic when an archive contains case-different spellings.
        let mut sorted_directories: Vec<&VfsDirectory> = self.vfs_directories.iter().collect();
        sorted_directories.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        if let Some(dir_path) = self.directory_path_for_id(dir_id) {
            let candidate = if dir_path.is_empty() {
                normalized.clone()
            } else {
                format!("{dir_path}/{normalized}")
            };
            if let Some(found) = sorted_directories
                .iter()
                .copied()
                .find(|directory| directory.path.eq_ignore_ascii_case(&candidate))
            {
                return Some(found.path.clone());
            }
        }
        if normalized.contains('/') {
            return sorted_directories
                .iter()
                .copied()
                .find(|directory| directory.path.eq_ignore_ascii_case(&normalized))
                .map(|directory| directory.path.clone());
        }
        None
    }

    pub(crate) fn list_vfs_catalog_entries(&mut self, dir_id: u32) -> Vec<VfsCatalogEntry> {
        self.ensure_vfs_catalog();
        let mut entries = Vec::new();
        let effective_dir_id = if self.directory_entry_for_id(dir_id).is_some() {
            dir_id
        } else {
            2
        };

        // Iterate the canonical directory vector in path-sorted order so the
        // entries Vec is deterministic before the final name sort.
        let mut directories: Vec<&VfsDirectory> = self.vfs_directories.iter().collect();
        directories.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        for directory in directories {
            if directory.path.is_empty() || directory.parent_dir_id != effective_dir_id {
                continue;
            }
            entries.push(VfsCatalogEntry {
                path: directory.path.clone(),
                name: Self::vfs_directory_name(&directory.path),
                is_directory: true,
            });
        }

        let mut file_paths: Vec<String> = self.vfs_metadata.keys().cloned().collect();
        file_paths.sort_by_key(|path| path.to_ascii_lowercase());
        for path in file_paths {
            let Some(metadata) = self.vfs_metadata.get(&path).copied() else {
                continue;
            };
            if metadata.parent_dir_id != effective_dir_id {
                continue;
            }
            entries.push(VfsCatalogEntry {
                path: path.clone(),
                name: Self::vfs_basename(&path).to_string(),
                is_directory: false,
            });
        }

        entries.sort_by_key(|entry| entry.name.to_ascii_lowercase());
        entries
    }

    /// Classic Macintosh arrow cursor (from ROM).
    pub(crate) fn default_arrow_cursor() -> ([u8; 32], [u8; 32], i16, i16) {
        crate::display::default_arrow_cursor()
    }

    /// Get a built-in system cursor by ID.
    /// Standard Mac cursor IDs: 1=iBeam, 2=cross, 3=plus, 4=watch
    pub(crate) fn system_cursor(id: i16) -> Option<([u8; 32], [u8; 32], i16, i16)> {
        match id {
            // crossCursor (ID 2) - crosshair, hotspot at center (7,7)
            2 => {
                #[rustfmt::skip]
                let data: [u8; 32] = [
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x00, 0x00, // ................
                    0xFC, 0x7E, // XXXXXX...XXXXXX.
                    0x00, 0x00, // ................
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x00, 0x00, // ................
                ];
                #[rustfmt::skip]
                let mask: [u8; 32] = [
                    0x03, 0x80, // ......XXX.......
                    0x03, 0x80, // ......XXX.......
                    0x03, 0x80, // ......XXX.......
                    0x03, 0x80, // ......XXX.......
                    0x03, 0x80, // ......XXX.......
                    0x03, 0x80, // ......XXX.......
                    0xFF, 0xFE, // XXXXXXXXXXXXXXX.
                    0xFF, 0xFF, // XXXXXXXXXXXXXXXX
                    0xFF, 0xFE, // XXXXXXXXXXXXXXX.
                    0x03, 0x80, // ......XXX.......
                    0x03, 0x80, // ......XXX.......
                    0x03, 0x80, // ......XXX.......
                    0x03, 0x80, // ......XXX.......
                    0x03, 0x80, // ......XXX.......
                    0x03, 0x80, // ......XXX.......
                    0x00, 0x00, // ................
                ];
                Some((data, mask, 7, 7))
            }
            // iBeamCursor (ID 1) - text cursor, hotspot at (8,4)
            1 => {
                #[rustfmt::skip]
                let data: [u8; 32] = [
                    0x0E, 0xE0, // ....XXX.XXX.....
                    0x04, 0x40, // .....X...X......
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x01, 0x00, // .......X........
                    0x04, 0x40, // .....X...X......
                    0x0E, 0xE0, // ....XXX.XXX.....
                ];
                let mask = data; // iBeam: data == mask for simplicity
                Some((data, mask, 8, 4))
            }
            // plusCursor (ID 3) - fat plus, hotspot at (8,8)
            3 => {
                #[rustfmt::skip]
                let data: [u8; 32] = [
                    0x00, 0x00,
                    0x07, 0xC0,
                    0x07, 0xC0,
                    0x07, 0xC0,
                    0x07, 0xC0,
                    0x07, 0xC0,
                    0xFF, 0xFE,
                    0xFF, 0xFE,
                    0xFF, 0xFE,
                    0xFF, 0xFE,
                    0xFF, 0xFE,
                    0x07, 0xC0,
                    0x07, 0xC0,
                    0x07, 0xC0,
                    0x07, 0xC0,
                    0x07, 0xC0,
                ];
                let mask = data;
                Some((data, mask, 8, 8))
            }
            // watchCursor (ID 4) - watch, hotspot at (8,8)
            4 => {
                #[rustfmt::skip]
                let data: [u8; 32] = [
                    0x07, 0xC0,
                    0x07, 0xC0,
                    0x1F, 0xF0,
                    0x3F, 0xF8,
                    0x3F, 0xF8,
                    0x3F, 0xF8,
                    0x3E, 0x78,
                    0x3E, 0x18,
                    0x3F, 0x18,
                    0x3F, 0xF8,
                    0x3F, 0xF8,
                    0x3F, 0xF8,
                    0x3F, 0xF8,
                    0x1F, 0xF0,
                    0x07, 0xC0,
                    0x07, 0xC0,
                ];
                let mask = data;
                Some((data, mask, 8, 8))
            }
            _ => None,
        }
    }

    /// Update the current mouse position (called from GUI layer).
    /// Coordinates are in Mac screen space (0,0 = top-left of screen).
    pub fn set_mouse_position(&mut self, v: i16, h: i16) {
        self.input_state.set_mouse_position((v, h));
        self.adb
            .note_mouse_state((v, h), self.input_state.mouse_button_pressed());
    }

    pub(crate) fn has_unmatched_queued_mouse_down(&self) -> bool {
        let mut unmatched_mousedowns: i32 = 0;
        for event in self.event_queue.iter() {
            match event.what {
                1 => unmatched_mousedowns += 1,
                2 => unmatched_mousedowns -= 1,
                _ => {}
            }
        }
        unmatched_mousedowns > 0
    }

    /// Push a mouse-down event into the event queue.
    pub fn push_mouse_down(&mut self, v: i16, h: i16) {
        self.input_state.set_mouse_state((v, h), true);
        self.adb.note_mouse_state((v, h), true);
        let modifiers = self.current_event_modifiers();
        let tick = self.current_tick();
        self.event_queue.push_back(QueuedEvent {
            what: 1, // mouseDown
            message: 0,
            when: tick,
            where_v: v,
            where_h: h,
            modifiers,
        });
    }

    /// Push a mouse-up event into the event queue.
    /// Update the hardware button state immediately on release.
    /// Button() reflects the physical state, while StillDown()/WaitMouseUp()
    /// combine that state with pending mouse events to decide whether the
    /// original click is still in progress.
    pub fn push_mouse_up(&mut self, v: i16, h: i16) {
        self.input_state.set_mouse_state((v, h), false);
        self.adb.note_mouse_state((v, h), false);
        // The classic mouse has one button, so the first physical release
        // after a ModalDialog-owned press is its matching mouseUp. Consume it
        // at injection time so event masks or FlushEvents cannot leave stale
        // ownership behind to swallow a later, unrelated click.
        if self.pending_modal_dialog_mouse_up {
            self.pending_modal_dialog_mouse_up = false;
            self.pending_modal_dialog_mouse_down = None;
            return;
        }
        let modifiers = self.current_event_modifiers();
        let tick = self.current_tick();
        self.event_queue.push_back(QueuedEvent {
            what: 2, // mouseUp
            message: 0,
            when: tick,
            where_v: v,
            where_h: h,
            modifiers,
        });
    }

    /// Push a key-down event into the event queue.
    pub fn push_key_down(&mut self, key_code: u8, char_code: u8) {
        // A physical key remains down until keyUp. Host browsers/windowing
        // systems may emit repeated keydown callbacks while it is held, but
        // classic Event Manager represents those repeats as autoKey events.
        // Inside Macintosh Volume I, I-246. Ignore duplicate host callbacks
        // so they cannot enqueue extra keyDown records or restart autoKey.
        if key_code == Self::CAPS_LOCK_KEY_CODE {
            if !self.input_state.press_caps_lock() {
                return;
            }
            // Caps Lock latches on one physical press and releases on the
            // next. Inside Macintosh Volume I (1985), p. I-34.
            let latched = !self.key_is_down(key_code);
            self.input_state.set_key_down(key_code, latched);
        } else {
            if self.key_is_down(key_code) {
                return;
            }
            self.input_state.set_key_down(key_code, true);
        }
        let modifiers = self.current_event_modifiers();
        if trace_input_enabled() {
            eprintln!(
                "[INPUT] key_down key_code=${:02X} char_code=${:02X} ('{}')",
                key_code,
                char_code,
                char::from(char_code)
            );
        }
        if Self::key_is_modifier(key_code) {
            return;
        }
        let message = ((key_code as u32) << 8) | (char_code as u32);
        let tick = self.current_tick();
        let (mouse_v, mouse_h) = self.input_state.mouse_position();
        self.event_queue.push_back(QueuedEvent {
            what: 3, // keyDown
            message,
            when: tick,
            where_v: mouse_v,
            where_h: mouse_h,
            modifiers,
        });

        if Self::key_generates_auto_key(key_code) {
            // Auto-key timing defaults are 16 ticks for the first repeat and
            // 4 ticks thereafter. Inside Macintosh Volume I, I-246.
            let next_tick = self
                .current_tick()
                .wrapping_add(Self::AUTO_KEY_THRESHOLD_TICKS);
            self.input_state
                .arm_key_repeat(key_code, char_code, next_tick);
        }
    }

    /// Push a key-up event into the event queue.
    pub fn push_key_up(&mut self, key_code: u8, char_code: u8) {
        self.push_key_up_with_system_event_mask(
            crate::memory::globals::DEFAULT_SYS_EVT_MASK,
            key_code,
            char_code,
        );
    }

    pub(crate) fn push_key_up_with_system_event_mask(
        &mut self,
        system_event_mask: u16,
        key_code: u8,
        char_code: u8,
    ) {
        if key_code == Self::CAPS_LOCK_KEY_CODE {
            self.input_state.release_caps_lock();
        } else {
            self.input_state.set_key_down(key_code, false);
        }
        self.input_state.clear_key_repeat_for(key_code);
        let modifiers = self.current_event_modifiers();
        if trace_input_enabled() {
            eprintln!(
                "[INPUT] key_up key_code=${:02X} char_code=${:02X} ('{}')",
                key_code,
                char_code,
                char::from(char_code)
            );
        }
        if Self::key_is_modifier(key_code) {
            return;
        }
        let message = ((key_code as u32) << 8) | (char_code as u32);
        // The default per-process SysEvtMask excludes keyUp events. A key
        // release always updates the physical KeyMap above, but it enters the
        // OS event queue only when the application explicitly enables
        // keyUpMask through SetEventMask. Inside Macintosh Volume I, I-254;
        // Macintosh Toolbox Essentials 1992, pp. 2-28..2-29 and 2-99.
        if Self::posted_event_is_enabled(system_event_mask, 4) {
            let tick = self.current_tick();
            let (mouse_v, mouse_h) = self.input_state.mouse_position();
            self.event_queue.push_back(QueuedEvent {
                what: 4, // keyUp
                message,
                when: tick,
                where_v: mouse_v,
                where_h: mouse_h,
                modifiers,
            });
        }
    }

    /// Get the current cursor data for rendering overlay.
    pub fn cursor(&self) -> Option<&CursorImage> {
        self.cursor_state.visible_image()
    }

    /// Show the cursor (called by GUI on mouse move to undo ObscureCursor).
    pub fn show_cursor(&mut self) {
        // ObscureCursor is modeled as a transient no-op, while HideCursor
        // remains balanced exclusively by ShowCursor. Inside Macintosh
        // Volume I (1985), p. I-168.
    }

    /// Check if cursor is visible (for debug logging).
    pub fn cursor_visible(&self) -> bool {
        self.cursor_state.visible()
    }

    /// Current cursor hide/show nesting level.
    pub fn cursor_level(&self) -> i16 {
        self.cursor_state.level()
    }

    /// Whether a cursor image is installed, independent of visibility.
    pub fn cursor_data_present(&self) -> bool {
        self.cursor_state.has_image()
    }

    /// Explicit screen-space transform for frontends that need to map host
    /// mouse coordinates back into a fullscreen game's source playfield.
    ///
    /// This is derived only from a sizeable CopyBits call into the screen
    /// framebuffer, not from rendered pixels. It is active while a game is in
    /// fullscreen mode and has hidden the Mac cursor, which is the common
    /// contract for software-cursor playfields such as first-person or
    /// crosshair-driven games. Visible Mac cursor UI, including menu bars and
    /// title screens, keeps normal screen coordinates.
    pub fn fullscreen_input_transform(&self) -> Option<ScreenCopyBitsRect> {
        if !self.fullscreen_locked || self.cursor_state.visible() {
            return None;
        }
        let rect = self.last_screen_copybits_rect?;
        if !screen_copybits_rect_is_valid(rect) || !self.screen_copybits_rect_maps_input(rect) {
            return None;
        }
        Some(rect)
    }

    fn screen_copybits_rect_maps_input(&self, rect: ScreenCopyBitsRect) -> bool {
        let (_, _, screen_width, screen_height, _) = self.screen_mode;
        let screen_width = screen_width.min(i16::MAX as u16) as i16;
        let screen_height = screen_height.min(i16::MAX as u16) as i16;
        !(rect.src_top == rect.dst_top
            && rect.src_left == rect.dst_left
            && rect.src_bottom == rect.dst_bottom
            && rect.src_right == rect.dst_right
            && rect.dst_top <= 0
            && rect.dst_left <= 0
            && rect.dst_bottom >= screen_height
            && rect.dst_right >= screen_width)
    }

    /// Current cursor bitmap + mask + hotspot, as installed by
    /// SetCursor / InitCursor. Returns `(data[32], mask[32],
    /// hotSpot.v, hotSpot.h)`. `None` when no cursor has been
    /// installed and the dispatcher was never initialised (rare —
    /// `TrapDispatcher::new()` seeds the default arrow). Used by
    /// tests to observe SetCursor's bitmap-storage effect.
    pub fn cursor_data(&self) -> Option<([u8; 32], [u8; 32], i16, i16)> {
        self.cursor_state.mono_parts()
    }

    /// Get the current mouse position.
    pub fn mouse_position(&self) -> (i16, i16) {
        self.input_state.mouse_position()
    }

    /// Number of Time Manager tasks currently in the queue.
    /// Per IM:IV IV-300, InsTime adds a task and RmvTime removes
    /// one; this accessor lets tests observe the effect.
    pub fn timer_task_count(&self) -> usize {
        self.timer_tasks.len()
    }

    /// Whether the Time Manager task whose TMTask record lives at
    /// `task_ptr` has been activated (via PrimeTime). Returns
    /// `None` if no such task is installed, `Some(bool)` otherwise.
    /// Per IM:IV IV-301, PrimeTime sets the active flag + schedules
    /// `fire_at_tick`; this accessor lets tests observe both.
    pub fn timer_task_active(&self, task_ptr: u32) -> Option<bool> {
        self.timer_tasks
            .iter()
            .find(|t| t.task_ptr == task_ptr)
            .map(|t| t.active)
    }

    /// Scheduled fire tick for an installed Time Manager task.
    /// Paired with `timer_task_active` for PrimeTime assertions.
    pub fn timer_task_fire_at(&self, task_ptr: u32) -> Option<u32> {
        self.timer_tasks
            .iter()
            .find(|t| t.task_ptr == task_ptr)
            .map(|t| t.fire_at_tick)
    }

    /// Parse a hex digit character ('0'-'9', 'A'-'F', 'a'-'f') to its value.
    pub(crate) fn hex_digit(b: u8) -> u8 {
        match b {
            b'0'..=b'9' => b - b'0',
            b'A'..=b'F' => b - b'A' + 10,
            b'a'..=b'f' => b - b'a' + 10,
            _ => 0,
        }
    }

    pub(crate) fn normalize_ostype(res_type: [u8; 4]) -> [u8; 4] {
        if !res_type.contains(&0) {
            return res_type;
        }

        let non_nul: Vec<u8> = res_type.into_iter().filter(|byte| *byte != 0).collect();
        if non_nul.is_empty()
            || non_nul.len() >= 4
            || !non_nul
                .iter()
                .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
        {
            return res_type;
        }

        let mut normalized = [b' '; 4];
        for (index, byte) in non_nul.into_iter().enumerate() {
            normalized[index] = byte;
        }
        normalized
    }

    /// Check whether a resource of the given type exists in the loaded resources.
    pub fn has_resource_type(&self, res_type: &[u8; 4]) -> bool {
        self.count_resources(*res_type, false) > 0
    }

    fn allocate_resource_fork(
        &self,
        fork: &ResourceFork,
        bus: &mut MacMemoryBus,
    ) -> ResourceFileMap {
        const RES_PRELOAD_ATTR: u8 = 0x04;
        let mut loaded = HashMap::new();
        let mut named = HashMap::new();
        let mut names_by_id = HashMap::new();
        let mut attrs = HashMap::new();
        // Sort resources by (type, id) for deterministic heap layout across runs.
        let mut sorted_resources: Vec<_> = fork.resources().iter().collect();
        sorted_resources.sort_by_key(|((res_type, id), _)| (*res_type, *id));
        for ((res_type, id), res) in sorted_resources {
            // OpenResFile reads the resource map plus only resources carrying
            // resPreload; ordinary resource data stays on disk until requested.
            // SetResLoad(FALSE) also suppresses preloading.
            // Inside Macintosh Volume I (1985), I-111, I-115, I-118.
            let ptr = if Self::resource_loading_enabled(bus) && res.attrs & RES_PRELOAD_ATTR != 0 {
                let ptr = bus.alloc(res.data.len() as u32);
                if ptr != 0 {
                    bus.write_bytes(ptr, &res.data);
                    Self::zero_loaded_resource_padding(bus, ptr, res.data.len() as u32);
                }
                ptr
            } else {
                0
            };
            loaded.insert((*res_type, *id), ptr);
            attrs.insert((*res_type, *id), res.attrs);
            if let Some(ref name) = res.name {
                named.insert((*res_type, name.clone()), (*id, ptr));
                names_by_id.insert((*res_type, *id), name.clone());
            }
        }
        ResourceFileMap {
            loaded,
            named,
            names_by_id,
            attrs,
            map_attrs: 0,
        }
    }

    fn remember_preloaded_resource_residency(&mut self, refnum: u16, file: &ResourceFileMap) {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager.resident_resources.extend(
                file.loaded
                    .iter()
                    .filter(|(_, ptr)| **ptr != 0)
                    .map(|(&(res_type, res_id), _)| (refnum, res_type, res_id)),
            );
        });
    }

    fn resource_reference_order(fork: &ResourceFork) -> Vec<([u8; 4], i16)> {
        let mut resources: Vec<_> = fork.resources().values().collect();
        resources.sort_by_key(|resource| resource.reference_offset);
        resources
            .into_iter()
            .map(|resource| (resource.res_type, resource.id))
            .collect()
    }

    pub(crate) fn remember_resource_backing_data(
        &mut self,
        refnum: u16,
        res_type: [u8; 4],
        res_id: i16,
        data: Vec<u8>,
    ) {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager
                .resource_backing_data
                .insert((refnum, res_type, res_id), data);
        });
        if res_type == *b"FONT" || res_type == *b"NFNT" {
            self.register_resource_font_backing(refnum, res_id);
        } else if res_type == *b"FOND" {
            self.register_fond_associated_strikes(refnum, res_id);
        }
    }

    pub(crate) fn loaded_resource_handle_size(data_size: u32) -> u32 {
        data_size.saturating_add(3) & !3
    }

    pub(crate) fn zero_loaded_resource_padding(bus: &mut MacMemoryBus, ptr: u32, data_size: u32) {
        let handle_size = Self::loaded_resource_handle_size(data_size);
        if ptr != 0 && handle_size > data_size {
            bus.fill_zeros(ptr.wrapping_add(data_size), handle_size - data_size);
        }
    }

    pub(crate) fn forget_resource_backing_data(
        &mut self,
        refnum: u16,
        res_type: [u8; 4],
        res_id: i16,
    ) {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager
                .resource_backing_data
                .remove(&(refnum, res_type, res_id));
        });
    }

    pub(crate) fn remember_resource_fork_backing_data(&mut self, refnum: u16, fork: &ResourceFork) {
        self.with_resource_manager_mut(|resource_manager| {
            for ((res_type, res_id), resource) in fork.resources() {
                resource_manager
                    .resource_backing_data
                    .entry((refnum, *res_type, *res_id))
                    .or_insert_with(|| resource.data.clone());
            }
        });
        // Parse every FOND only after the complete resource fork is present.
        // HashMap iteration order is intentionally unspecified, while an
        // NFNT's arbitrary resource ID is meaningful only through its FOND
        // association. Inside Macintosh: Text (1993), pp. 4-13 and 4-95.
        for ((res_type, res_id), _) in fork.resources() {
            if res_type == b"FONT" || res_type == b"NFNT" {
                self.register_resource_font_backing(refnum, *res_id);
            }
        }
    }

    fn fond_associations_for_font_resource(
        &self,
        refnum: u16,
        font_resource_id: i16,
    ) -> Vec<crate::quickdraw::fonts::FondAssociation> {
        self.resource_backing_data
            .iter()
            .filter(|((entry_refnum, res_type, _), _)| {
                *entry_refnum == refnum && res_type == b"FOND"
            })
            .filter_map(|((_, _, fond_id), bytes)| {
                crate::quickdraw::fonts::parse_fond_associations(*fond_id, bytes)
            })
            .flatten()
            .filter(|association| association.font_resource_id == font_resource_id)
            .collect()
    }

    fn register_resource_font_backing(&self, refnum: u16, font_resource_id: i16) {
        let Some((res_type, bytes)) = [*b"NFNT", *b"FONT"].iter().find_map(|res_type| {
            self.resource_backing_data
                .get(&(refnum, *res_type, font_resource_id))
                .map(|bytes| (*res_type, bytes))
        }) else {
            return;
        };
        let associations = self.fond_associations_for_font_resource(refnum, font_resource_id);
        if associations.is_empty() {
            // Standalone old-style FONT resources retain the original
            // family*128+size convention. NFNT IDs are arbitrary and only
            // acquire a family and size through a FOND association, which may
            // not have been loaded yet.
            if res_type == *b"FONT" {
                let _ =
                    crate::quickdraw::fonts::register_resource_font_strike(font_resource_id, bytes);
            }
            return;
        }

        for association in associations {
            // The renderer currently synthesizes bold/italic/etc. from the
            // plain strike. Do not accidentally install an intrinsic styled
            // strike as the family's plain face when both share a point size.
            if association.style & 0x00FF != 0 {
                continue;
            }
            let registered = crate::quickdraw::fonts::register_resource_font_strike_for_family(
                association.family_id,
                association.size,
                bytes,
            );
            if registered && std::env::var_os("SYSTEMLESS_TRACE_FONT_TRAPS").is_some() {
                eprintln!(
                    "[FONT] FOND {} maps {}pt style ${:04X} to bitmap resource {}",
                    association.family_id,
                    association.size,
                    association.style,
                    association.font_resource_id,
                );
            }
        }
    }

    fn register_fond_associated_strikes(&self, refnum: u16, fond_resource_id: i16) {
        let Some(fond_bytes) =
            self.resource_backing_data
                .get(&(refnum, *b"FOND", fond_resource_id))
        else {
            return;
        };
        let Some(associations) =
            crate::quickdraw::fonts::parse_fond_associations(fond_resource_id, fond_bytes)
        else {
            return;
        };
        for font_resource_id in associations
            .iter()
            .map(|association| association.font_resource_id)
        {
            self.register_resource_font_backing(refnum, font_resource_id);
        }
    }

    pub(crate) fn clear_resource_file_backing_data(&mut self, refnum: u16) {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager
                .resource_backing_data
                .retain(|(entry_refnum, _, _), _| *entry_refnum != refnum);
        });
    }

    pub(crate) fn remember_resource_handle_index(
        &mut self,
        handle: u32,
        refnum: u16,
        res_type: [u8; 4],
        res_id: i16,
    ) {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager
                .resource_handles_by_key
                .insert((refnum, res_type, res_id), handle);
        });
    }

    pub(crate) fn forget_resource_handle_index_for_handle(&mut self, handle: u32) {
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
                .resource_handles_by_key
                .remove(&(refnum, res_type, res_id));
        });
    }

    pub(crate) fn unload_resource_live_map_entry_for_handle(&mut self, handle: u32) {
        self.with_resource_manager_mut(|resource_manager| {
            let Some((ptr, res_type, res_id)) =
                resource_manager.loaded_handles.get(&handle).copied()
            else {
                return;
            };
            let Some(refnum) = resource_manager.resource_handle_files.get(&handle).copied() else {
                return;
            };
            let Some(file) = resource_manager
                .resources
                .as_mut()
                .and_then(|resources| resources.files.get_mut(&refnum))
            else {
                return;
            };

            if file.loaded.get(&(res_type, res_id)).copied() == Some(ptr) {
                file.loaded.insert((res_type, res_id), 0);
            }
            for ((named_type, _), (named_id, named_ptr)) in &mut file.named {
                if *named_type == res_type && *named_id == res_id && *named_ptr == ptr {
                    *named_ptr = 0;
                }
            }
        });
    }

    pub(crate) fn clear_resource_file_handle_index(&mut self, refnum: u16) {
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager
                .resource_handles_by_key
                .retain(|(entry_refnum, _, _), _| *entry_refnum != refnum);
        });
    }

    pub(crate) fn resource_search_order(&self) -> Vec<u16> {
        let Some(resources) = self.resources.as_ref() else {
            return Vec::new();
        };
        let current_file = self.current_resource_refnum();

        // The Resource Manager searches the current file and only the files
        // opened before it, in reverse open order.
        // Inside Macintosh Volume I, I-125 to I-126
        let mut order = Vec::new();
        let mut include = false;
        for refnum in resources.search_order.iter().rev().copied() {
            if refnum == current_file {
                include = true;
            }
            if include && resources.files.contains_key(&refnum) {
                order.push(refnum);
            }
        }
        if order.is_empty() && resources.files.contains_key(&current_file) {
            order.push(current_file);
        }
        order
    }

    pub(crate) fn current_resource_refnum(&self) -> u16 {
        let process_current = (*self.current_resource_file).max(0) as u16;
        if process_current != 0 {
            return process_current;
        }
        self.resources.as_ref().map_or(0, |resources| {
            if resources.files.contains_key(&resources.current_file) {
                resources.current_file
            } else {
                0
            }
        })
    }

    pub(crate) fn set_current_resource_refnum(&mut self, bus: &mut MacMemoryBus, refnum: u16) {
        let selected = self.with_resource_manager_mut(|resource_manager| {
            let mut selected = 0;
            if let Some(resources) = resource_manager.resources.as_mut() {
                resources.current_file = if resources.files.contains_key(&refnum) {
                    refnum
                } else {
                    0
                };
                selected = resources.current_file;
            }
            selected
        });
        self.process_file_system.resource_manager.current_resource_file
            .with_mut(|current_file| *current_file = selected as i16);
        bus.write_word(0x0A5A, self.current_resource_refnum());
    }

    pub(crate) fn set_resource_file_name(&mut self, refnum: u16, name: impl Into<String>) {
        self.with_resource_manager_mut(|resource_manager| {
            if let Some(resources) = resource_manager.resources.as_mut() {
                resources.names.insert(refnum, name.into());
            }
        });
    }

    /// Allocate the next non-colliding process File Manager reference number.
    pub(crate) fn allocate_process_file_refnum(&mut self) -> u16 {
        let mut candidate = self.process_file_system.next_file_ref_num.max(100);
        loop {
            let refnum = u16::try_from(candidate).expect("positive File Manager refnum");
            let resource_refnum_in_use = self
                .resources
                .as_ref()
                .is_some_and(|resources| resources.files.contains_key(&refnum));
            if !self.open_files.contains_key(&refnum)
                && !self.synthetic_drivers.contains_key(&refnum)
                && !resource_refnum_in_use
            {
                let next_file_ref_num = candidate
                    .checked_add(1)
                    .expect("File Manager reference numbers exhausted");
                self.process_file_system.with_mut(|file_system| {
                    file_system.next_file_ref_num = next_file_ref_num;
                });
                return refnum;
            }
            candidate = candidate
                .checked_add(1)
                .expect("File Manager reference numbers exhausted");
        }
    }

    // Inside Macintosh: Files (1992), pp. 2-81–2-83: an HFS file
    // reference number is 2 + 94*n, an offset into the FCB buffer.
    // Resource Manager access paths share that namespace with data forks.
    fn allocate_file_fcb(
        &mut self,
        bus: &mut MacMemoryBus,
        path: &str,
        resource_fork: bool,
        writable: bool,
    ) -> std::result::Result<u16, i16> {
        use crate::memory::globals::addr;
        const FCB_SIZE: u16 = 94;
        const MAX_FCBS: u16 = 342;
        let old_buffer = bus.read_long(addr::FCB_S_PTR);
        let old_size = if old_buffer == 0 {
            0
        } else {
            bus.read_word(old_buffer)
        };
        let refnum = (0..MAX_FCBS)
            .map(|index| 2 + FCB_SIZE * index)
            .find(|refnum| {
                *refnum != bus.read_word(addr::CUR_APREF_NUM)
                    && !self.open_files.contains_key(refnum)
                    && !self.synthetic_drivers.contains_key(refnum)
                    && !self
                        .resources
                        .as_ref()
                        .is_some_and(|r| r.files.contains_key(refnum))
                    && (*refnum >= old_size || bus.read_long(old_buffer + *refnum as u32) == 0)
            })
            .ok_or(-42i16)?; // tmfoErr
        let required = refnum + FCB_SIZE;
        let buffer = if required > old_size {
            let new_buffer = bus.alloc(required as u32);
            if new_buffer == 0 {
                return Err(-108); // memFullErr
            }
            bus.fill_bytes(new_buffer, required as u32, 0);
            if old_buffer != 0 {
                let previous = bus.read_bytes(old_buffer, old_size as usize);
                bus.write_bytes(new_buffer, &previous);
            }
            bus.write_word(new_buffer, required);
            bus.write_long(addr::FCB_S_PTR, new_buffer);
            if old_buffer != 0 {
                bus.free(old_buffer);
            }
            new_buffer
        } else {
            old_buffer
        };
        bus.write_word(addr::FS_FCB_LEN, FCB_SIZE);

        // Files 1992, pp. 2-79–2-83: fcbVPtr identifies the volume's VCB;
        // vcbVRefNum is the signed word at byte 78 of that record.
        let volume_ref = self
            .vfs_volume_for_path(path)
            .map(|volume| volume.ref_num)
            .unwrap_or(BOOT_VOLUME_REF_NUM);
        let mut vcb = bus.read_long(addr::VCB_Q_HDR + 2);
        while vcb != 0 && bus.read_word(vcb + 78) != volume_ref as u16 {
            vcb = bus.read_long(vcb);
        }
        if vcb == 0 {
            vcb = bus.alloc(178);
            if vcb == 0 {
                return Err(-108);
            }
            bus.fill_bytes(vcb, 178, 0);
            bus.write_word(vcb + 8, 0x4244);
            bus.write_word(vcb + 78, volume_ref as u16);
            let volume_name = self
                .vfs_volume_for_ref_num(volume_ref)
                .map(|volume| volume.name.as_str())
                .unwrap_or(BOOT_VOLUME_NAME);
            Self::write_pstring(
                bus,
                vcb + 44,
                &volume_name.chars().take(27).collect::<String>(),
            );
            let tail = bus.read_long(addr::VCB_Q_HDR + 6);
            if tail == 0 {
                bus.write_long(addr::VCB_Q_HDR + 2, vcb);
            } else {
                bus.write_long(tail, vcb);
            }
            bus.write_long(addr::VCB_Q_HDR + 6, vcb);
        }
        let metadata = self.vfs_file_metadata(path);
        let len = if resource_fork {
            self.vfs_rsrc.get(path).map_or(0, |data| data.len() as u32)
        } else {
            self.vfs.get(path).map_or(0, |data| data.len() as u32)
        };
        let fcb = buffer + refnum as u32;
        bus.fill_bytes(fcb, FCB_SIZE as u32, 0);
        bus.write_long(fcb, metadata.map_or(0, |file| file.file_id));
        let flags = (if resource_fork { 0x0200 } else { 0 })
            | (if writable { 0x0100 } else { 0 });
        bus.write_word(fcb + 4, flags);
        bus.write_long(fcb + 8, len);
        bus.write_long(fcb + 12, len);
        bus.write_long(fcb + 20, vcb);
        bus.write_long(fcb + 50, metadata.map_or(0, |file| file.file_type));
        bus.write_long(fcb + 58, metadata.map_or(2, |file| file.parent_dir_id));
        let name = Self::hfs_name_from_vfs_component(Self::vfs_basename(path));
        Self::write_pstring(bus, fcb + 62, &name.chars().take(31).collect::<String>());
        Ok(refnum)
    }

    pub(crate) fn allocate_data_file_fcb(
        &mut self,
        bus: &mut MacMemoryBus,
        path: &str,
        writable: bool,
    ) -> std::result::Result<u16, i16> {
        self.allocate_file_fcb(bus, path, false, writable)
    }

    fn allocate_resource_file_fcb(
        &mut self,
        bus: &mut MacMemoryBus,
        path: &str,
        writable: bool,
    ) -> std::result::Result<u16, i16> {
        self.allocate_file_fcb(bus, path, true, writable)
    }

    pub(crate) fn clear_file_fcb(bus: &mut MacMemoryBus, refnum: u16) {
        let fcb_buffer = bus.read_long(crate::memory::globals::addr::FCB_S_PTR);
        if fcb_buffer != 0
            && refnum % 94 == 2
            && u32::from(refnum) + 94 <= u32::from(bus.read_word(fcb_buffer))
        {
            bus.fill_bytes(fcb_buffer + u32::from(refnum), 94, 0);
        }
    }

    /// Allocate a new loaded resource-file slot for the given VFS key.
    ///
    /// The caller is responsible for resolving duplicates before calling
    /// this helper. It merges an existing resource fork snapshot when one
    /// is present, otherwise it registers an empty resource file, then
    /// makes the new file current.
    pub(crate) fn open_resource_file_from_vfs_key(
        &mut self,
        bus: &mut MacMemoryBus,
        vfs_key: &str,
        wants_write: bool,
    ) -> u16 {
        let rsrc_data = self.vfs_rsrc.get(vfs_key).unwrap().clone();
        let refnum = match self.allocate_resource_file_fcb(bus, vfs_key, wants_write) {
            Ok(refnum) => refnum,
            Err(error) => {
                bus.write_word(0x0A60, error as u16);
                return u16::MAX;
            }
        };
        if let Some(fork) = ResourceFork::parse(&rsrc_data) {
            self.merge_resources_from_fork(&fork, bus, refnum);
        } else {
            self.register_empty_resource_file(refnum);
        }
        self.set_resource_file_name(refnum, vfs_key.to_owned());
        if wants_write {
            self.write_refnums.insert(refnum);
        }
        self.set_current_resource_refnum(bus, refnum);
        bus.write_word(0x0A60, 0);
        refnum
    }

    pub(crate) fn resource_file_name(&self, refnum: u16) -> Option<&str> {
        self.resources
            .as_ref()
            .and_then(|resources| resources.names.get(&refnum))
            .map(|name| name.as_str())
    }

    pub(crate) fn close_resource_file_refnum(
        &mut self,
        bus: &mut MacMemoryBus,
        refnum: u16,
    ) -> bool {
        if refnum == 0 {
            return false;
        }

        let _ = self.flush_resource_file_refnum(bus, refnum);

        let closing_current = self.current_resource_refnum() == refnum;
        let closed = self.with_resource_manager_mut(|resource_manager| {
            let resources = resource_manager.resources.as_mut()?;
            if !resources.files.contains_key(&refnum) {
                return None;
            }

            let mut file_ptrs: HashSet<u32> = HashSet::new();
            let mut externally_referenced_ptrs: HashSet<u32> = HashSet::new();
            if let Some(file) = resources.files.get_mut(&refnum) {
                for attr in file.attrs.values_mut() {
                    *attr &= !(Self::RES_CHANGED_ATTR as u8);
                }
                file.map_attrs &= !Self::RES_MAP_CHANGED_ATTR;
                file_ptrs.extend(file.loaded.values().copied().filter(|ptr| *ptr != 0));
            }

            externally_referenced_ptrs.extend(
                resources
                    .files
                    .iter()
                    .filter(|(other_refnum, _)| **other_refnum != refnum)
                    .flat_map(|(_, file)| file.loaded.values().copied())
                    .filter(|ptr| *ptr != 0),
            );

            if resources.current_file == refnum {
                resources.current_file = resources
                    .search_order
                    .iter()
                    .rev()
                    .find(|&&candidate| {
                        candidate != refnum && resources.files.contains_key(&candidate)
                    })
                    .copied()
                    .unwrap_or(0);
            }

            resources
                .search_order
                .retain(|&candidate| candidate != refnum);
            resources.files.remove(&refnum);
            let closed_name = resources.names.remove(&refnum);
            Some((
                file_ptrs,
                externally_referenced_ptrs,
                closed_name,
                resources.current_file,
            ))
        });
        let Some((file_ptrs, externally_referenced_ptrs, closed_name, surviving_classic_current)) =
            closed
        else {
            return false;
        };
        if closing_current {
            self.process_file_system
                .resource_manager
                .current_resource_file
                .with_mut(|current_file| {
                    *current_file = surviving_classic_current as i16;
                });
        }
        self.clear_resource_file_backing_data(refnum);
        self.clear_resource_file_handle_index(refnum);
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager.resource_file_order.remove(&refnum);
            resource_manager
                .resident_resources
                .retain(|(entry_refnum, _, _)| *entry_refnum != refnum);
        });

        let mut freed_ptrs = 0usize;
        for ptr in file_ptrs {
            self.untrack_handle_ptr(ptr);
            if !externally_referenced_ptrs.contains(&ptr) {
                bus.free(ptr);
                freed_ptrs += 1;
            }
        }

        let file_handles: Vec<u32> = self
            .resource_handle_files
            .iter()
            .filter_map(|(&handle, &handle_refnum)| (handle_refnum == refnum).then_some(handle))
            .collect();
        for handle in &file_handles {
            bus.write_long(*handle, 0);
            bus.free(*handle);
            self.remove_handle_state_bits(*handle);
        }
        self.with_resource_manager_mut(|resource_manager| {
            for handle in &file_handles {
                resource_manager.loaded_handles.remove(handle);
                resource_manager.resource_handle_files.remove(handle);
                resource_manager.detached_handle_files.remove(handle);
                resource_manager.detached_handles.remove(handle);
            }
        });

        let detached_handles: Vec<u32> = self
            .detached_handle_files
            .iter()
            .filter_map(|(&handle, &handle_refnum)| (handle_refnum == refnum).then_some(handle))
            .collect();
        self.with_resource_manager_mut(|resource_manager| {
            for handle in detached_handles {
                resource_manager.detached_handle_files.remove(&handle);
            }
        });

        self.write_refnums.remove(&refnum);
        Self::clear_file_fcb(bus, refnum);
        bus.write_word(0x0A5A, self.current_resource_refnum());

        if trace_resfile_enabled() {
            eprintln!(
                "[RSRC] close resource refnum={} name={:?} freed_ptrs={} freed_handles={}",
                refnum,
                closed_name,
                freed_ptrs,
                file_handles.len()
            );
        }

        true
    }

    /// Reverse of `resource_file_name`: returns the refnum a file with
    /// the given name was opened under, if any. Used by OpenRFPerm to
    /// dedupe repeated opens of the same resource fork — without this,
    /// games that re-open their own fork (Bonkheads opens it 16+ times
    /// during boot) re-allocate every resource on every open and exhaust
    /// the heap before the title even renders.
    pub(crate) fn refnum_for_resource_file_name(&self, name: &str) -> Option<u16> {
        self.resources.as_ref().and_then(|resources| {
            resources
                .names
                .iter()
                .find(|(_, n)| n.as_str() == name)
                .map(|(refnum, _)| *refnum)
        })
    }

    pub(crate) fn find_loaded_resource_any(
        &self,
        res_type: [u8; 4],
        res_id: i16,
    ) -> Option<(u16, u32)> {
        let res_type = Self::normalize_ostype(res_type);
        let resources = self.resources.as_ref()?;
        for refnum in self.resource_search_order() {
            if let Some(&ptr) = resources
                .files
                .get(&refnum)
                .and_then(|file| file.loaded.get(&(res_type, res_id)))
                .filter(|ptr| **ptr != 0)
            {
                return Some((refnum, ptr));
            }
        }
        None
    }

    /// Pascal-string body for a synthetic system `'STR '` resource ID,
    /// or `None` if the ID is not one we synthesize. These mirror the
    /// strings stored in the System file by the Sharing Setup
    /// control panel on a fresh System 7 install. Networking 1994,
    /// 2-799 (owner name surfaces here when Sharing Setup is unset).
    pub(crate) fn system_str_default_body(res_id: i16) -> Option<&'static [u8]> {
        match res_id {
            // Owner Name (Sharing Setup)
            -16096 => Some(b"\x0EMacintosh User\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"),
            // Macintosh Name (Sharing Setup, AppleTalk identity)
            -16413 => Some(b"\x09Macintosh\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"),
            // Owner Password (encrypted blob — empty placeholder)
            -16097 => Some(b"\x00"),
            _ => None,
        }
    }

    /// Allocate (and cache) a synthetic `'STR '` resource for one of
    /// the well-known System-file IDs returned by
    /// [`Self::system_str_default_body`]. Returns the byte pointer to
    /// the Pascal string in guest RAM, ready to be wrapped in a
    /// resource handle by `get_or_create_resource_handle_in_file`.
    pub(crate) fn synthesize_system_str(
        &mut self,
        bus: &mut MacMemoryBus,
        res_id: i16,
    ) -> Option<u32> {
        if let Some(&ptr) = self.system_str_cache.get(&res_id) {
            return Some(ptr);
        }
        let body = Self::system_str_default_body(res_id)?;
        let ptr = bus.alloc(body.len() as u32);
        bus.write_bytes(ptr, body);
        self.system_str_cache.insert(res_id, ptr);
        Some(ptr)
    }

    /// Allocate and cache the classic U.S. `'INTL'` resources used by
    /// `IUGetIntl` and direct Resource Manager lookups. ID 0 is an `Intl0Rec`
    /// (numeric, currency, short-date, and time settings); ID 1 is an
    /// `Intl1Rec` (long-date names and separators). Inside Macintosh Volume I
    /// (1985), pp. I-495..I-501.
    pub(crate) fn synthesize_system_intl(
        &mut self,
        bus: &mut MacMemoryBus,
        res_id: i16,
    ) -> Option<u32> {
        if let Some(&ptr) = self.system_intl_cache.get(&res_id) {
            return Some(ptr);
        }

        let body = match res_id {
            0 => vec![
                b'.', b',', b';', // decimalPt, thousSep, listSep
                b'$', 0, 0,    // currSym1..3
                0xF0, // symbol leads; minus sign; trailing and leading zeroes
                0,    // dateOrder: month, day, year
                0,    // shrtDateFmt: no leading zeroes or century
                b'/', 0xFF, // dateSep, 12-hour timeCycle
                0x60, // leading zeroes for minutes and seconds
                b' ', b'A', b'M', 0, // mornStr
                b' ', b'P', b'M', 0,    // eveStr
                b':', // timeSep
                0, 0, 0, 0, 0, 0, 0, 0, // time1Suff..time8Suff
                0, // non-metric
                0, 0, // U.S. region, version 0
            ],
            1 => {
                fn push_str15(body: &mut Vec<u8>, value: &[u8]) {
                    debug_assert!(value.len() <= 15);
                    body.push(value.len() as u8);
                    body.extend_from_slice(value);
                    body.resize(body.len() + 15 - value.len(), 0);
                }

                let mut body = Vec::with_capacity(332);
                for day in [
                    b"Sunday".as_slice(),
                    b"Monday",
                    b"Tuesday",
                    b"Wednesday",
                    b"Thursday",
                    b"Friday",
                    b"Saturday",
                ] {
                    push_str15(&mut body, day);
                }
                for month in [
                    b"January".as_slice(),
                    b"February",
                    b"March",
                    b"April",
                    b"May",
                    b"June",
                    b"July",
                    b"August",
                    b"September",
                    b"October",
                    b"November",
                    b"December",
                ] {
                    push_str15(&mut body, month);
                }
                body.extend_from_slice(&[
                    0, 0xFF, 0, 3, // include day; month/day/year; no leading 0; abbr 3
                    0, 0, 0, 0, // st0
                    b',', b' ', 0, 0, // st1
                    b' ', 0, 0, 0, // st2
                    b',', b' ', 0, 0, // st3
                    0, 0, 0, 0, // st4
                    0, 0, // U.S. region, version 0
                    0x4E, 0x75, // localRtn: RTS (no localization hook)
                ]);
                debug_assert_eq!(body.len(), 332);
                body
            }
            _ => return None,
        };

        let ptr = bus.alloc(body.len() as u32);
        bus.write_bytes(ptr, &body);
        self.system_intl_cache.insert(res_id, ptr);
        Some(ptr)
    }

    /// Allocate (and cache) the standard System-file `'PAT#'` ID 0 resource.
    /// Its big-endian count word is followed by the 38 eight-byte patterns in
    /// the MacPaint palette. Inside Macintosh Volume I (1985), pp. I-475..I-476.
    pub(crate) fn synthesize_system_pattern_list(
        &mut self,
        bus: &mut MacMemoryBus,
        res_id: i16,
    ) -> Option<u32> {
        if res_id != 0 {
            return None;
        }
        if let Some(&ptr) = self.system_pattern_list_cache.get(&res_id) {
            return Some(ptr);
        }

        const PATTERNS: [[u8; 8]; 38] = [
            [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            [0xDD, 0xFF, 0x77, 0xFF, 0xDD, 0xFF, 0x77, 0xFF],
            [0xDD, 0x77, 0xDD, 0x77, 0xDD, 0x77, 0xDD, 0x77],
            [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
            [0x55, 0xFF, 0x55, 0xFF, 0x55, 0xFF, 0x55, 0xFF],
            [0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA],
            [0xEE, 0xDD, 0xBB, 0x77, 0xEE, 0xDD, 0xBB, 0x77],
            [0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88],
            [0xB1, 0x30, 0x03, 0x1B, 0xD8, 0xC0, 0x0C, 0x8D],
            [0x80, 0x10, 0x02, 0x20, 0x01, 0x08, 0x40, 0x04],
            [0xFF, 0x88, 0x88, 0x88, 0xFF, 0x88, 0x88, 0x88],
            [0xFF, 0x80, 0x80, 0x80, 0xFF, 0x08, 0x08, 0x08],
            [0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            [0x80, 0x40, 0x20, 0x00, 0x02, 0x04, 0x08, 0x00],
            [0x82, 0x44, 0x39, 0x44, 0x82, 0x01, 0x01, 0x01],
            [0xF8, 0x74, 0x22, 0x47, 0x8F, 0x17, 0x22, 0x71],
            [0x55, 0xA0, 0x40, 0x40, 0x55, 0x0A, 0x04, 0x04],
            [0x20, 0x50, 0x88, 0x88, 0x88, 0x88, 0x05, 0x02],
            [0xBF, 0x00, 0xBF, 0xBF, 0xB0, 0xB0, 0xB0, 0xB0],
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            [0x80, 0x00, 0x08, 0x00, 0x80, 0x00, 0x08, 0x00],
            [0x88, 0x00, 0x22, 0x00, 0x88, 0x00, 0x22, 0x00],
            [0x88, 0x22, 0x88, 0x22, 0x88, 0x22, 0x88, 0x22],
            [0xAA, 0x00, 0xAA, 0x00, 0xAA, 0x00, 0xAA, 0x00],
            [0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00],
            [0x11, 0x22, 0x44, 0x88, 0x11, 0x22, 0x44, 0x88],
            [0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00],
            [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80],
            [0xAA, 0x00, 0x80, 0x00, 0x88, 0x00, 0x80, 0x00],
            [0xFF, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80],
            [0x08, 0x1C, 0x22, 0xC1, 0x80, 0x01, 0x02, 0x04],
            [0x88, 0x14, 0x22, 0x41, 0x88, 0x00, 0xAA, 0x00],
            [0x40, 0xA0, 0x00, 0x00, 0x04, 0x0A, 0x00, 0x00],
            [0x03, 0x84, 0x48, 0x30, 0x0C, 0x02, 0x01, 0x01],
            [0x80, 0x80, 0x41, 0x3E, 0x08, 0x08, 0x14, 0xE3],
            [0x10, 0x20, 0x54, 0xAA, 0xFF, 0x02, 0x04, 0x08],
            [0x77, 0x89, 0x8F, 0x8F, 0x77, 0x98, 0xF8, 0xF8],
            [0x00, 0x08, 0x14, 0x2A, 0x55, 0x2A, 0x14, 0x08],
        ];

        let ptr = bus.alloc(2 + PATTERNS.len() as u32 * 8);
        bus.write_word(ptr, PATTERNS.len() as u16);
        for (index, pattern) in PATTERNS.iter().enumerate() {
            bus.write_bytes(ptr + 2 + index as u32 * 8, pattern);
        }
        self.system_pattern_list_cache.insert(res_id, ptr);
        Some(ptr)
    }

    /// Allocate (and cache) a synthetic System-file `'clut'` resource for
    /// the standard indexed color-table IDs. The resource body is a
    /// ColorTable record, matching what `GetCTable(depth)` exposes through
    /// the Color Manager in Systemless.
    pub(crate) fn synthesize_system_clut(
        &mut self,
        bus: &mut MacMemoryBus,
        res_id: i16,
    ) -> Option<u32> {
        if let Some(&ptr) = self.system_clut_cache.get(&res_id) {
            return Some(ptr);
        }
        let (std_clut, entry_count) = Self::standard_mac_indexed_clut(res_id as u16)?;
        let ptr = bus.alloc(8 + entry_count as u32 * 8);
        bus.write_long(ptr, res_id as u32); // ctSeed follows the standard depth ID.
        bus.write_word(ptr + 4, 0); // ctFlags
        bus.write_word(ptr + 6, entry_count as u16 - 1); // ctSize
        for index in 0..entry_count as u32 {
            let entry = ptr + 8 + index * 8;
            let [r, g, b] = std_clut[index as usize];
            bus.write_word(entry, index as u16);
            bus.write_word(entry + 2, r);
            bus.write_word(entry + 4, g);
            bus.write_word(entry + 6, b);
        }
        self.system_clut_cache.insert(res_id, ptr);
        Some(ptr)
    }

    /// Allocate (and cache) the standard System 7 window color table.
    /// `InitWindows` searches the application, System file, and ROM for
    /// `'wctb'` ID 0, whose `WinCTab` contains the colors for the standard
    /// window-part identifiers. Inside Macintosh Volume V (1986), pp.
    /// V-201..V-203; Macintosh Toolbox Essentials (1992), pp. 4-71 and 4-127.
    pub(crate) fn synthesize_system_wctb(
        &mut self,
        bus: &mut MacMemoryBus,
        res_id: i16,
    ) -> Option<u32> {
        if res_id != 0 {
            return None;
        }
        if let Some(&ptr) = self.system_wctb_cache.get(&res_id) {
            return Some(ptr);
        }

        const COLORS: [(u16, u16, u16, u16); 13] = [
            (0, 0xFFFF, 0xFFFF, 0xFFFF),  // wContentColor
            (1, 0x0000, 0x0000, 0x0000),  // wFrameColor
            (2, 0x0000, 0x0000, 0x0000),  // wTextColor
            (3, 0x0000, 0x0000, 0x0000),  // wHiliteColor
            (4, 0xFFFF, 0xFFFF, 0xFFFF),  // wTitleBarColor
            (5, 0xFFFF, 0xFFFF, 0xFFFF),  // wHiliteColorLight
            (6, 0x0000, 0x0000, 0x0000),  // wHiliteColorDark
            (7, 0xFFFF, 0xFFFF, 0xFFFF),  // wTitleBarLight
            (8, 0x0000, 0x0000, 0x0000),  // wTitleBarDark
            (9, 0xCCCC, 0xCCCC, 0xFFFF),  // wDialogLight
            (10, 0x0000, 0x0000, 0x0000), // wDialogDark
            (11, 0xCCCC, 0xCCCC, 0xFFFF), // wTingeLight
            (12, 0x3333, 0x3333, 0x6666), // wTingeDark
        ];

        let ptr = bus.alloc(8 + COLORS.len() as u32 * 8);
        bus.write_long(ptr, 0); // wCSeed is reserved.
        bus.write_word(ptr + 4, 0); // wCReserved is reserved.
        bus.write_word(ptr + 6, COLORS.len() as u16 - 1);
        for (index, &(part, red, green, blue)) in COLORS.iter().enumerate() {
            let entry = ptr + 8 + index as u32 * 8;
            bus.write_word(entry, part);
            bus.write_word(entry + 2, red);
            bus.write_word(entry + 4, green);
            bus.write_word(entry + 6, blue);
        }
        self.system_wctb_cache.insert(res_id, ptr);
        Some(ptr)
    }

    /// Allocate (and cache) a callable resource shim for the standard ROM
    /// window definition functions. The Window Manager HLE implements their
    /// drawing and hit-testing behavior for built-in procIDs. A direct guest
    /// call still has to honor the Pascal WDEF ABI, however: four parameters
    /// occupy 12 bytes and the caller reserves a 4-byte result. The shim
    /// discards those parameters, clears the result to the documented default
    /// of zero, and returns through the saved JSR address. Macintosh Toolbox
    /// Essentials (1992), pp. 4-145..4-146; Inside Macintosh Volume V,
    /// V-31..V-32.
    pub(crate) fn synthesize_system_wdef(
        &mut self,
        bus: &mut MacMemoryBus,
        res_id: i16,
    ) -> Option<u32> {
        if let Some(&ptr) = self.system_wdef_cache.get(&res_id) {
            return Some(ptr);
        }
        if !matches!(res_id, 0 | 1) {
            return None;
        }

        let ptr = bus.alloc(10);
        bus.write_word(ptr, 0x205F); // MOVEA.L (SP)+,A0 — recover JSR return PC.
        bus.write_word(ptr + 2, 0xDEFC); // ADDA.W #12,SP — discard WDEF parameters.
        bus.write_word(ptr + 4, 12);
        bus.write_word(ptr + 6, 0x4297); // CLR.L (SP) — LongInt function result.
        bus.write_word(ptr + 8, 0x4ED0); // JMP (A0).
        self.system_wdef_cache.insert(res_id, ptr);
        Some(ptr)
    }

    /// Allocate (and cache) a callable shim for the standard ROM menu
    /// definition procedure. The Menu Manager HLE performs the built-in
    /// MDEF behavior, but direct guest calls still use the five-parameter,
    /// 18-byte Pascal procedure ABI declared by MPW Menus.h. Inside
    /// Macintosh Volume I, I-352 and I-365.
    pub(crate) fn synthesize_system_mdef(
        &mut self,
        bus: &mut MacMemoryBus,
        res_id: i16,
    ) -> Option<u32> {
        if let Some(&ptr) = self.system_mdef_cache.get(&res_id) {
            return Some(ptr);
        }
        if res_id != 0 {
            return None;
        }

        let ptr = bus.alloc(crate::menu_manager::STANDARD_MENU_DEFINITION_SHIM.len() as u32);
        bus.write_bytes(ptr, &crate::menu_manager::STANDARD_MENU_DEFINITION_SHIM);
        self.system_mdef_cache.insert(res_id, ptr);
        Some(ptr)
    }

    /// Allocate (and cache) the standard U.S. Roman keyboard-layout
    /// resource (`'KCHR'` ID 0). Inside Macintosh: Text 1993, C-18..C-19
    /// defines the resource as a version word, a 256-byte table-selection
    /// index, a table-count word, 128-byte character-mapping tables keyed by
    /// virtual key code, and a dead-key-count word.
    pub(crate) fn synthesize_system_kchr(
        &mut self,
        bus: &mut MacMemoryBus,
        res_id: i16,
    ) -> Option<u32> {
        if let Some(&ptr) = self.system_kchr_cache.get(&res_id) {
            return Some(ptr);
        }
        if res_id != 0 {
            return None;
        }

        const TABLES: usize = 2;
        const TABLE_COUNT_OFFSET: usize = 2 + 256;
        const TABLE_BASE: usize = TABLE_COUNT_OFFSET + 2;
        const DEAD_KEY_COUNT_OFFSET: usize = TABLE_BASE + TABLES * 128;
        const LEN: usize = DEAD_KEY_COUNT_OFFSET + 2;
        let mut body = vec![0u8; LEN];
        for modifier in 0..=255usize {
            body[2 + modifier] = if (modifier & 0x22) != 0 { 1 } else { 0 };
        }
        body[TABLE_COUNT_OFFSET..TABLE_COUNT_OFFSET + 2]
            .copy_from_slice(&(TABLES as u16).to_be_bytes());

        let normal = TABLE_BASE;
        let shifted = TABLE_BASE + 128;
        let keys: &[(usize, u8, u8)] = &[
            (0x00, b'a', b'A'),
            (0x01, b's', b'S'),
            (0x02, b'd', b'D'),
            (0x03, b'f', b'F'),
            (0x04, b'h', b'H'),
            (0x05, b'g', b'G'),
            (0x06, b'z', b'Z'),
            (0x07, b'x', b'X'),
            (0x08, b'c', b'C'),
            (0x09, b'v', b'V'),
            (0x0B, b'b', b'B'),
            (0x0C, b'q', b'Q'),
            (0x0D, b'w', b'W'),
            (0x0E, b'e', b'E'),
            (0x0F, b'r', b'R'),
            (0x10, b'y', b'Y'),
            (0x11, b't', b'T'),
            (0x12, b'1', b'!'),
            (0x13, b'2', b'@'),
            (0x14, b'3', b'#'),
            (0x15, b'4', b'$'),
            (0x16, b'6', b'^'),
            (0x17, b'5', b'%'),
            (0x18, b'=', b'+'),
            (0x19, b'9', b'('),
            (0x1A, b'7', b'&'),
            (0x1B, b'-', b'_'),
            (0x1C, b'8', b'*'),
            (0x1D, b'0', b')'),
            (0x1E, b']', b'}'),
            (0x1F, b'o', b'O'),
            (0x20, b'u', b'U'),
            (0x21, b'[', b'{'),
            (0x22, b'i', b'I'),
            (0x23, b'p', b'P'),
            (0x24, b'\r', b'\r'),
            (0x25, b'l', b'L'),
            (0x26, b'j', b'J'),
            (0x27, b'\'', b'"'),
            (0x28, b'k', b'K'),
            (0x29, b';', b':'),
            (0x2A, b'\\', b'|'),
            (0x2B, b',', b'<'),
            (0x2C, b'/', b'?'),
            (0x2D, b'n', b'N'),
            (0x2E, b'm', b'M'),
            (0x2F, b'.', b'>'),
            (0x31, b' ', b' '),
            (0x32, b'`', b'~'),
            (0x7B, 0x1C, 0x1C),
            (0x7C, 0x1D, 0x1D),
            (0x7D, 0x1F, 0x1F),
            (0x7E, 0x1E, 0x1E),
        ];
        for &(vk, unshifted, shifted_char) in keys {
            body[normal + vk] = unshifted;
            body[shifted + vk] = shifted_char;
        }

        let ptr = bus.alloc(body.len() as u32);
        if ptr == 0 {
            return None;
        }
        bus.write_bytes(ptr, &body);
        self.system_kchr_cache.insert(res_id, ptr);
        Some(ptr)
    }

    /// Allocate (and cache) the standard keycode-map resource (`'KMAP'` ID
    /// 0). Its four-byte ID/version header is followed by the 128-entry
    /// hardware-to-virtual-key map and a zero exception-array count. The
    /// standard map translates Control and the four cursor keys between the
    /// original and ADB virtual-key assignments; all other entries are
    /// identity-valued. Inside Macintosh: Text (1993), pp. C-11..C-15.
    pub(crate) fn synthesize_system_kmap(
        &mut self,
        bus: &mut MacMemoryBus,
        res_id: i16,
    ) -> Option<u32> {
        if let Some(&ptr) = self.system_kmap_cache.get(&res_id) {
            return Some(ptr);
        }
        if res_id != 0 {
            return None;
        }

        const HEADER_SIZE: usize = 4;
        const MAP_SIZE: usize = 128;
        const LEN: usize = HEADER_SIZE + MAP_SIZE + 2;
        let mut body = vec![0u8; LEN];
        for keycode in 0..MAP_SIZE {
            body[HEADER_SIZE + keycode] = keycode as u8;
        }
        for (raw, virtual_key) in [
            (0x36usize, 0x3Bu8),
            (0x3B, 0x7B),
            (0x3C, 0x7C),
            (0x3D, 0x7D),
            (0x3E, 0x7E),
            (0x7B, 0x3C),
            (0x7C, 0x3D),
            (0x7D, 0x3E),
            (0x7E, 0x36),
        ] {
            body[HEADER_SIZE + raw] = virtual_key;
        }

        let ptr = bus.alloc(body.len() as u32);
        if ptr == 0 {
            return None;
        }
        bus.write_bytes(ptr, &body);
        self.system_kmap_cache.insert(res_id, ptr);
        Some(ptr)
    }

    /// Synthesize (and cache) a 68-byte CURS-shaped block for one of
    /// the standard system cursor IDs (1 iBeamCursor, 2 crossCursor,
    /// 3 plusCursor, 4 watchCursor per IM:I I-475..I-477). Returns
    /// `None` for any other ID — callers (specifically
    /// [`Self::dispatch_dialog`] for `GetCursor` $A9B9) treat that as
    /// the IM:I I-474 "If the resource can't be read, GetCursor
    /// returns NIL" path. The block layout matches the Cursor record
    /// in IM:I I-475: 32 bytes of `data` bitmap + 32 bytes of `mask` +
    /// 4 bytes for the `hotSpot` Point (vertical word, horizontal
    /// word).
    pub(crate) fn synthesize_system_cursor(
        &mut self,
        bus: &mut MacMemoryBus,
        cursor_id: i16,
    ) -> Option<u32> {
        if let Some(&ptr) = self.system_cursor_cache.get(&cursor_id) {
            return Some(ptr);
        }
        let (data, mask, hot_v, hot_h) = Self::system_cursor(cursor_id)?;
        let ptr = bus.alloc(68);
        bus.write_bytes(ptr, &data);
        bus.write_bytes(ptr + 32, &mask);
        bus.write_word(ptr + 64, hot_v as u16);
        bus.write_word(ptr + 66, hot_h as u16);
        self.system_cursor_cache.insert(cursor_id, ptr);
        Some(ptr)
    }

    /// Allocate and cache the standard System-file `'ICON'` resource ID 1.
    ///
    /// Dialog `iconItem` records name an ICON resource, and Resource Manager
    /// searches the open resource chain through the System file when the
    /// application does not supply it. Systemless has no mounted System file,
    /// so this preserves that final lookup while leaving application resources
    /// authoritative. Macintosh Toolbox Essentials (1992), pp. 6-153 and 7-63.
    pub(crate) fn synthesize_system_icon(
        &mut self,
        bus: &mut MacMemoryBus,
        icon_id: i16,
    ) -> Option<u32> {
        if let Some(&ptr) = self.system_icon_cache.get(&icon_id) {
            return Some(ptr);
        }
        let body: &[u8] = match icon_id {
            1 => &[
                0xFF, 0xFF, 0xFF, 0xFF, 0x80, 0x7F, 0xFF, 0xFF, 0x80, 0x7F, 0xFF, 0xFF, 0x80, 0x7F,
                0xFF, 0xFF, 0x80, 0x7F, 0xFF, 0xFF, 0x80, 0x7F, 0xC0, 0xFF, 0x88, 0x7F, 0x00, 0x3F,
                0x88, 0x7E, 0x00, 0x1F, 0x88, 0x7C, 0x00, 0x0F, 0x80, 0x78, 0x00, 0x07, 0x80, 0x78,
                0x00, 0x07, 0x80, 0x70, 0x00, 0x03, 0x80, 0x71, 0xDD, 0xC3, 0x80, 0x70, 0x00, 0x03,
                0x80, 0x70, 0x00, 0x03, 0x80, 0x71, 0xDD, 0x43, 0x80, 0x70, 0x00, 0x03, 0x80, 0x70,
                0x00, 0x03, 0x80, 0x71, 0xD7, 0x03, 0x80, 0x70, 0x00, 0x03, 0x87, 0xF0, 0x00, 0x03,
                0x81, 0xF1, 0xEE, 0xC3, 0x81, 0xF0, 0x00, 0x07, 0x81, 0xF0, 0x00, 0x07, 0x81, 0xF0,
                0x00, 0x0F, 0x81, 0xE0, 0x00, 0x1F, 0x8F, 0x80, 0x00, 0x7F, 0x81, 0xFF, 0xFF, 0xFF,
                0x81, 0xFF, 0xFF, 0xFF, 0x81, 0xFF, 0xFF, 0xFF, 0x81, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                0xFF, 0xFF,
            ],
            _ => return None,
        };
        let ptr = bus.alloc(body.len() as u32);
        bus.write_bytes(ptr, body);
        self.system_icon_cache.insert(icon_id, ptr);
        Some(ptr)
    }

    /// Allocate (and cache) a tool-trap trampoline for the given
    /// trap word. Used by GetTrapAddress / GetToolTrapAddress when
    /// no native handler is installed. The returned address names a protected
    /// two-byte stub containing the auto-pop variant of the canonical
    /// tool-trap word.
    ///
    /// Stub layout — exactly 2 bytes:
    /// ```text
    ///   +0 trap_word | 0x0400   ; auto-pop bit set
    /// ```
    ///
    /// When the guest does `JSR (A0)` through this address:
    ///   1. CPU pushes return PC, jumps to trampoline
    ///   2. CPU reads `trap_word | 0x0400` at trampoline+0
    ///   3. Auto-pop dispatcher pops the return PC, runs the trap
    ///   4. Trap handler reads stack params at sp+0 (params
    ///      pre-pushed by caller) — same layout as an inline trap
    ///   5. Dispatcher sets PC = saved return PC
    ///   6. Caller resumes at the instruction after the JSR
    ///
    /// The auto-pop bit is only valid for tool traps. OS traps use a
    /// separate canonical-trap-plus-RTS gateway because their register
    /// convention leaves the JSR return address at the top of the stack.
    /// IM:II II-384 (NGetTrapAddress); IM:V V-577 (auto-pop bit).
    pub(crate) fn get_or_create_tool_trap_trampoline(
        &mut self,
        bus: &mut MacMemoryBus,
        trap_word: u16,
    ) -> u32 {
        bus.get_or_create_system_trap_gateway(0xA800 | (trap_word & 0x03FF))
    }

    fn canonical_trap_word(trap_word: u16) -> u16 {
        raw_trap_route(trap_word).canonical_word
    }

    #[cfg(test)]
    fn raw_trap_table_entry(trap_word: u16) -> u32 {
        raw_trap_route(trap_word).table_address
    }

    fn default_trap_gateway(&self, bus: &MacMemoryBus, trap_word: u16) -> Option<u32> {
        bus.default_system_trap_gateway(self.trap_table_profile?, trap_word)
    }

    #[cfg(test)]
    fn write_readonly_code_long(bus: &mut MacMemoryBus, address: u32, value: u32) {
        bus.write_readonly_code_word(address, (value >> 16) as u16);
        bus.write_readonly_code_word(address + 2, value as u16);
    }

    /// Create an inactive, profile-complete table for a new process.
    /// Permanent come-from heads belong to this context, while callable
    /// gateways remain system-owned and may be shared by every process.
    pub(crate) fn create_trap_table_process_context(
        &mut self,
        bus: &mut MacMemoryBus,
        profile: TrapTableProfile,
    ) -> Result<TrapTableProcessContext> {
        let image = bus
            .create_system_trap_table(profile)
            .ok_or(Error::TrapTableInitialization)?;
        Ok(TrapTableProcessContext {
            profile,
            raw_entries: image.raw_entries,
            raw_exception_vectors: image.exception_vectors,
            default_exception_vectors: image.exception_vectors,
            pending_native_trap_calls: HashMap::new(),
            current_trap_caller: None,
        })
    }

    /// Save the active application's trap context and restore another one.
    /// The returned context owns the exact raw cells and in-flight native
    /// patch frames of the process that was switched out.
    pub(crate) fn switch_trap_table_process_context(
        &mut self,
        bus: &mut MacMemoryBus,
        incoming: TrapTableProcessContext,
    ) -> Option<TrapTableProcessContext> {
        let outgoing = self.trap_table_profile.map(|profile| {
            let mut raw_entries =
                Vec::with_capacity(usize::from(OS_TRAP_TABLE_SLOTS + TOOLBOX_TRAP_TABLE_SLOTS));
            for slot in 0..OS_TRAP_TABLE_SLOTS {
                raw_entries.push(bus.read_long(OS_TRAP_TABLE_BASE + u32::from(slot) * 4));
            }
            for slot in 0..TOOLBOX_TRAP_TABLE_SLOTS {
                raw_entries.push(bus.read_long(TOOLBOX_TRAP_TABLE_BASE + u32::from(slot) * 4));
            }
            TrapTableProcessContext {
                profile,
                raw_entries,
                raw_exception_vectors: [bus.read_long(0x28), bus.read_long(0x2C)],
                default_exception_vectors: self
                    .trap_exception_vector_defaults
                    .expect("active trap profile must have exception-vector defaults"),
                pending_native_trap_calls: std::mem::take(&mut self.pending_native_trap_calls),
                current_trap_caller: self.current_trap_caller.take(),
            }
        });

        debug_assert_eq!(
            incoming.raw_entries.len(),
            usize::from(OS_TRAP_TABLE_SLOTS + TOOLBOX_TRAP_TABLE_SLOTS)
        );
        let toolbox_offset = usize::from(OS_TRAP_TABLE_SLOTS);
        for slot in 0..OS_TRAP_TABLE_SLOTS {
            bus.write_long(
                OS_TRAP_TABLE_BASE + u32::from(slot) * 4,
                incoming.raw_entries[usize::from(slot)],
            );
        }
        for slot in 0..TOOLBOX_TRAP_TABLE_SLOTS {
            bus.write_long(
                TOOLBOX_TRAP_TABLE_BASE + u32::from(slot) * 4,
                incoming.raw_entries[toolbox_offset + usize::from(slot)],
            );
        }
        bus.write_long(0x28, incoming.raw_exception_vectors[0]);
        bus.write_long(0x2C, incoming.raw_exception_vectors[1]);
        self.pending_native_trap_calls = incoming.pending_native_trap_calls;
        self.current_trap_caller = incoming.current_trap_caller;
        self.trap_table_profile = Some(incoming.profile);
        self.trap_exception_vector_defaults = Some(incoming.default_exception_vectors);
        outgoing
    }

    /// Discard the active application's trap context during process teardown.
    pub(crate) fn teardown_trap_table_process_context(&mut self) {
        self.pending_native_trap_calls.clear();
        self.current_trap_caller = None;
        self.trap_table_profile = None;
        self.trap_exception_vector_defaults = None;
    }

    /// Materialize the selected machine profile's complete raw Trap Manager
    /// tables. Every default entry is a stable, callable gateway into its HLE
    /// implementation. The table storage itself remains writable; generated
    /// gateways and permanent come-from heads are protected.
    pub(crate) fn materialize_trap_tables(
        &mut self,
        bus: &mut MacMemoryBus,
        profile: TrapTableProfile,
    ) -> Result<()> {
        if !bus.is_guest_address_writable(OS_TRAP_TABLE_BASE, usize::from(OS_TRAP_TABLE_SLOTS) * 4)
            || !bus.is_guest_address_writable(
                TOOLBOX_TRAP_TABLE_BASE,
                usize::from(TOOLBOX_TRAP_TABLE_SLOTS) * 4,
            )
            || !bus.is_guest_address_writable(0x28, 8)
        {
            return Err(Error::TrapTableInitialization);
        }
        let context = self.create_trap_table_process_context(bus, profile)?;
        let _ = self.switch_trap_table_process_context(bus, context);
        Ok(())
    }

    /// Establish standalone classic trap tables before lookup or patching.
    /// Repeated initialization preserves active cells and in-flight calls.
    /// Keep the dispatcher paired with its original code-memory owner.
    /// Inside Macintosh: Operating System Utilities (1994), pp. 8-4--8-9.
    pub fn initialize_trap_tables(&mut self, bus: &mut MacMemoryBus) -> Result<()> {
        if self.trap_table_profile.is_some() {
            return Ok(());
        }
        self.materialize_trap_tables(bus, TrapTableProfile::M68k68040)
    }

    /// Whether low-memory exception vector 10 still names this process's
    /// generated A-line dispatcher identity. An inactive context has no
    /// default vector identity.
    pub(crate) fn aline_vector_is_default(&self, bus: &MacMemoryBus) -> bool {
        self.trap_exception_vector_defaults
            .is_some_and(|defaults| bus.read_long(0x28) == defaults[0])
    }

    /// Whether low-memory exception vector 11 still names this process's
    /// generated line-F handler identity.
    pub(crate) fn fline_vector_is_default(&self, bus: &MacMemoryBus) -> bool {
        self.trap_exception_vector_defaults
            .is_some_and(|defaults| bus.read_long(0x2C) == defaults[1])
    }

    /// Return the logical address currently selected by a materialized raw
    /// table entry. Trap Manager getters use this view even when the address
    /// is the default gateway; dispatch uses [`Self::native_trap_handler`] to
    /// distinguish that default from an installed patch.
    pub(crate) fn trap_table_address(&self, bus: &MacMemoryBus, trap_word: u16) -> Option<u32> {
        self.trap_table_profile?;
        let canonical = Self::canonical_trap_word(trap_word);
        let kind = if raw_trap_route(canonical).is_toolbox {
            TrapTableKind::Toolbox
        } else {
            TrapTableKind::OperatingSystem
        };
        // The bus is borrowed immutably for the whole lookup, so the
        // protected-code check reads the live ranges; snapshotting them here
        // copied a Vec on every trap dispatch.
        TrapManager::get_address_with_provenance(
            canonical,
            kind,
            |operation| match operation {
                TrapManagerMemoryOp::ReadLong(address) => bus
                    .try_read_trap_manager_long(address)
                    .map(TrapManagerMemoryResult::Long),
                TrapManagerMemoryOp::WriteLong { .. }
                | TrapManagerMemoryOp::WriteProtectedLong { .. } => None,
            },
            |address| bus.protected_code_contains(address),
        )
    }

    /// Return the current non-default handler for a canonical trap slot.
    /// Once low-memory tables exist, their bytes are the source of truth so a
    /// guest can patch a trap with an ordinary longword store.
    pub(crate) fn native_trap_handler(&self, bus: &MacMemoryBus, trap_word: u16) -> Option<u32> {
        let canonical = Self::canonical_trap_word(trap_word);
        let logical = self.trap_table_address(bus, canonical)?;
        (self.default_trap_gateway(bus, canonical) != Some(logical)).then_some(logical)
    }

    pub(crate) fn install_trap_address(
        &mut self,
        bus: &mut MacMemoryBus,
        trap_word: u16,
        handler: u32,
    ) -> std::result::Result<(), TrapManagerSetError> {
        self.initialize_trap_tables(bus)
            .map_err(|_| TrapManagerSetError::UnreadableTable)?;
        let canonical = Self::canonical_trap_word(trap_word);
        let kind = if raw_trap_route(canonical).is_toolbox {
            TrapTableKind::Toolbox
        } else {
            TrapTableKind::OperatingSystem
        };
        let protected_code = bus.protected_code_ownership();
        TrapManager::set_address_with_provenance(
            canonical,
            kind,
            handler,
            |operation| match operation {
                TrapManagerMemoryOp::ReadLong(address) => bus
                    .try_read_trap_manager_long(address)
                    .map(TrapManagerMemoryResult::Long),
                TrapManagerMemoryOp::WriteLong { address, value } => bus
                    .try_write_long(address, value)
                    .then_some(TrapManagerMemoryResult::Written),
                TrapManagerMemoryOp::WriteProtectedLong { address, value } => bus
                    .try_write_protected_code_long(address, value)
                    .then_some(TrapManagerMemoryResult::Written),
            },
            move |address| protected_code.contains(address),
        )
    }

    fn retain_native_trap_call(&mut self, trap_word: u16, call: NativeTrapCallState) {
        self.pending_native_trap_calls
            .entry(trap_word)
            .or_default()
            .push(call);
    }

    pub(crate) fn take_latest_native_trap_call(
        &mut self,
        trap_word: u16,
    ) -> Option<NativeTrapCallState> {
        let (call, empty) = {
            let calls = self.pending_native_trap_calls.get_mut(&trap_word)?;
            let call = calls.pop();
            (call, calls.is_empty())
        };
        if empty {
            self.pending_native_trap_calls.remove(&trap_word);
        }
        call
    }

    /// Add every retained native-patch return PC to the current m68k batch's
    /// watch list. Reaching one stops the batch before the return-site
    /// instruction executes, so the runner can validate PC and SP and retire
    /// the exact invocation without single-stepping all intervening code.
    pub(crate) fn append_pending_native_trap_return_pcs(&self, pcs: &mut Vec<u32>) {
        for call in self.pending_native_trap_calls.values().flatten() {
            if !pcs.contains(&call.return_pc) {
                pcs.push(call.return_pc);
            }
        }
    }

    /// Whether the canonical OS or Toolbox slot selected by an A-line word
    /// currently has a guest patch. Runner fast paths must defer to normal
    /// dispatch whenever this is true.
    pub(crate) fn has_native_trap_patch(&self, bus: &MacMemoryBus, trap_word: u16) -> bool {
        self.native_trap_handler(bus, trap_word).is_some()
    }

    /// Retire a native trap invocation that returned directly instead of
    /// following its saved daisy-chain link. Both the return PC and the
    /// post-RTS stack pointer must match the frame synthesized at dispatch.
    pub(crate) fn retire_returned_native_trap_call<C: CpuOps>(&mut self, cpu: &mut C) {
        let pc = cpu.read_reg(Register::PC);
        let sp = cpu.read_reg(Register::A7);
        let returned_trap =
            self.pending_native_trap_calls
                .iter()
                .find_map(|(&trap_word, calls)| {
                    calls
                        .last()
                        .is_some_and(|call| call.return_pc == pc && call.argument_sp == sp)
                        .then_some(trap_word)
                });
        let Some(returned_trap) = returned_trap else {
            return;
        };
        let Some(call) = self.take_latest_native_trap_call(returned_trap) else {
            return;
        };
        if let Some(frame) = call.os_dispatch_frame {
            restore_os_trap_dispatch_frame(cpu, frame);
            apply_os_trap_dispatcher_ccr(cpu);
        }
    }

    pub(crate) fn find_named_resource_current(
        &self,
        res_type: [u8; 4],
        name: &str,
    ) -> Option<(u16, i16, u32)> {
        let res_type = Self::normalize_ostype(res_type);
        let resources = self.resources.as_ref()?;
        let refnum = self.current_resource_refnum();
        let file = resources.files.get(&refnum)?;
        let entry = file
            .named
            .get(&(res_type, name.to_string()))
            .copied()
            .or_else(|| {
                // Resource names compare without case; an application's
                // spelling need not match the resource map's spelling.
                // Inside Macintosh Volume I (1985), p. I-119.
                let needle = name.to_lowercase();
                file.named.iter().find_map(|((kind, candidate), entry)| {
                    (*kind == res_type && candidate.to_lowercase() == needle).then_some(*entry)
                })
            })?;
        Some((refnum, entry.0, entry.1))
    }

    /// Collect every named resource of `res_type` reachable through the
    /// current resource search order. The returned file identity and data
    /// pointer let AppendResMenu materialize every matching resource after it
    /// restores `SetResLoad(TRUE)`. Names are sorted alphabetically, and an
    /// ID found in a closer map shadows the same type/ID in later maps.
    /// Macintosh Toolbox Essentials (1992), pp. 3-101--3-104.
    pub(crate) fn named_resource_records_of_type(
        &self,
        res_type: [u8; 4],
    ) -> Vec<(u16, i16, String, u32)> {
        let res_type = Self::normalize_ostype(res_type);
        let Some(resources) = self.resources.as_ref() else {
            return Vec::new();
        };
        let mut seen_ids = std::collections::HashSet::new();
        let mut entries = Vec::new();
        for refnum in self.resource_search_order() {
            let Some(file) = resources.files.get(&refnum) else {
                continue;
            };
            for ((rt, name), (id, ptr)) in &file.named {
                if *rt != res_type {
                    continue;
                }
                if seen_ids.insert(*id) {
                    entries.push((refnum, *id, name.clone(), *ptr));
                }
            }
        }
        entries.sort_by(|(_, _, left, _), (_, _, right, _)| {
            left.to_lowercase()
                .cmp(&right.to_lowercase())
                .then_with(|| left.cmp(right))
        });
        entries
    }

    pub(crate) fn find_named_resource_any(
        &self,
        res_type: [u8; 4],
        name: &str,
    ) -> Option<(u16, i16, u32)> {
        let res_type = Self::normalize_ostype(res_type);
        let resources = self.resources.as_ref()?;
        for refnum in self.resource_search_order() {
            let Some(file) = resources.files.get(&refnum) else {
                continue;
            };
            // Try exact match first.
            if let Some((id, ptr)) = file.named.get(&(res_type, name.to_string())).copied() {
                return Some((refnum, id, ptr));
            }
            // Resource Manager name lookups are case-insensitive per
            // IM:I I-119. Keep the fallback generic: resource names
            // can differ by case between authoring tools and callers.
            let needle_lower = name.to_lowercase();
            for ((rt, n), (id, ptr)) in &file.named {
                if *rt == res_type && n.to_lowercase() == needle_lower {
                    return Some((refnum, *id, *ptr));
                }
            }
        }
        None
    }

    pub(crate) fn count_resources(&self, res_type: [u8; 4], current_only: bool) -> usize {
        let res_type = Self::normalize_ostype(res_type);
        let Some(resources) = self.resources.as_ref() else {
            return 0;
        };

        if current_only {
            return resources
                .files
                .get(&self.current_resource_refnum())
                .map_or(0, |file| {
                    file.loaded.keys().filter(|(t, _)| *t == res_type).count()
                });
        }

        resources
            .files
            .values()
            .map(|file| file.loaded.keys().filter(|(t, _)| *t == res_type).count())
            .sum()
    }

    pub(crate) fn resource_refnum_for_ptr(
        &self,
        res_type: [u8; 4],
        res_id: i16,
        ptr: u32,
    ) -> Option<u16> {
        let resources = self.resources.as_ref()?;
        // Sort refnums before searching so the number of HashMap probes
        // before find-match is deterministic across runs. Mac Resource
        // Manager search order (IM:Resource I-115) is by RscChain stack —
        // refnum order is a reasonable approximation since refnums
        // increment as files are opened.
        let mut refnums: Vec<u16> = resources.files.keys().copied().collect();
        refnums.sort_unstable();
        for refnum in refnums {
            let file = match resources.files.get(&refnum) {
                Some(f) => f,
                None => continue,
            };
            if let Some(file_ptr) = file
                .loaded
                .get(&(res_type, res_id))
                .copied()
                .filter(|&file_ptr| file_ptr == ptr)
            {
                let _ = file_ptr;
                return Some(refnum);
            }
        }
        None
    }

    /// Load resources into guest memory for trap access.
    /// Loads ALL resource types from the fork (not just a hardcoded whitelist).
    pub fn load_resources(&mut self, fork: &ResourceFork, bus: &mut MacMemoryBus) {
        // ResLoad is guest-visible as well as process policy. Applications
        // save it directly before temporarily disabling automatic loading.
        // Inside Macintosh Volume I (1985), p. I-118.
        bus.write_word(
            crate::memory::globals::addr::RES_LOAD,
            u16::from(self.policy.res_load()) << 8,
        );
        if let Some(app_path) = self.launched_app_path().map(str::to_owned) {
            self.vfs
                .insert(format!("__rsrc__{}", app_path), fork.serialized().to_vec());
        }
        let file = self.allocate_resource_fork(fork, bus);
        self.remember_preloaded_resource_residency(0, &file);
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager
                .resource_file_order
                .insert(0, Self::resource_reference_order(fork));
        });
        self.clear_resource_file_backing_data(0);
        self.remember_resource_fork_backing_data(0, fork);
        // Log resource types summary including nrct check.
        // Behind SYSTEMLESS_TRACE_LOAD so library consumers don't see this
        // ~30-line dump on every game load.
        if crate::runner::trace_load_enabled() {
            let mut type_counts: HashMap<[u8; 4], usize> = HashMap::new();
            for (res_type, _) in file.loaded.keys() {
                *type_counts.entry(*res_type).or_insert(0) += 1;
            }
            let has_nrct = file.loaded.contains_key(&(*b"nrct", 128i16));
            eprintln!("[RESOURCE] nrct 128 present: {}", has_nrct);
            // List all PICT resource IDs
            let mut pict_ids: Vec<i16> = file
                .loaded
                .keys()
                .filter(|(t, _)| t == b"PICT")
                .map(|(_, id)| *id)
                .collect();
            pict_ids.sort();
            eprintln!("[RESOURCE] PICT IDs: {:?}", pict_ids);
            let mut clut_ids: Vec<i16> = file
                .loaded
                .keys()
                .filter(|(t, _)| t == b"clut")
                .map(|(_, id)| *id)
                .collect();
            clut_ids.sort();
            eprintln!("[RESOURCE] clut IDs: {:?}", clut_ids);
            // Dialog Manager IDs are useful when investigating
            // launch-time alerts whose message text we'd otherwise
            // have no visibility into.
            for ttype in &[b"ALRT", b"DITL", b"DLOG", b"MENU"] {
                let mut ids: Vec<i16> = file
                    .loaded
                    .keys()
                    .filter(|(t, _)| t == *ttype)
                    .map(|(_, id)| *id)
                    .collect();
                ids.sort();
                if !ids.is_empty() {
                    eprintln!(
                        "[RESOURCE] {} IDs: {:?}",
                        std::str::from_utf8(ttype.as_slice()).unwrap_or("????"),
                        ids
                    );
                }
            }
            let mut types: Vec<_> = type_counts.iter().collect();
            types.sort_by_key(|(t, _)| **t);
            for (t, count) in &types {
                let ts = String::from_utf8_lossy(t.as_slice());
                eprintln!("[RESOURCE]   '{}' x{}", ts, count);
            }
            eprintln!(
                "[RESOURCE] Loaded {} resources ({} named) from fork",
                file.loaded.len(),
                file.named.len()
            );
        }
        let mut files = HashMap::new();
        files.insert(0, file);
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager.resources = Some(LoadedResources {
                files,
                names: HashMap::from([(0, "Application".to_string())]),
                search_order: vec![0],
                current_file: 0,
            });
        });
        bus.write_word(0x0A5A, 0);
        // Some classic runtimes locate relocation resources by walking the
        // Resource Manager's guest-visible map through TopMapHndl. Keep the
        // HLE indexes above, but also expose the serialized application map
        // and populate its reference-record handles.
        if !fork.map().is_empty() {
            let map_ptr = bus.alloc(fork.map().len() as u32);
            bus.write_bytes(map_ptr, fork.map());
            bus.write_long(map_ptr + 16, 0); // no older map in this minimal chain
            bus.write_word(map_ptr + 20, 0); // application resource-map refnum

            let map_handle = bus.alloc(4);
            bus.write_long(map_handle, map_ptr);
            bus.write_long(0x0A50, map_handle); // TopMapHndl

            let mut resources: Vec<_> = fork.resources().values().collect();
            resources.sort_by_key(|resource| (resource.res_type, resource.id));
            for resource in resources {
                let ptr = self
                    .resources
                    .as_ref()
                    .and_then(|loaded| loaded.files.get(&0))
                    .and_then(|file| file.loaded.get(&(resource.res_type, resource.id)))
                    .copied()
                    .unwrap_or(0);
                let handle = self.get_or_create_resource_handle_in_file(
                    bus,
                    resource.res_type,
                    resource.id,
                    ptr,
                    0,
                );
                bus.write_long(map_ptr + resource.reference_offset as u32 + 8, handle);
            }
        }
    }

    pub(crate) fn register_resource_file(&mut self, refnum: u16, file: ResourceFileMap) {
        self.with_resource_manager_mut(|resource_manager| {
            let resources = resource_manager.resources.get_or_insert_with(|| LoadedResources {
                files: HashMap::new(),
                names: HashMap::new(),
                search_order: vec![0],
                current_file: 0,
            });
            resources.files.insert(refnum, file);
            if !resources.search_order.contains(&refnum) {
                resources.search_order.push(refnum);
            }
        });
    }

    pub(crate) fn register_empty_resource_file(&mut self, refnum: u16) {
        self.register_resource_file(refnum, ResourceFileMap::default());
    }

    /// Load resources from a fork and merge missing entries into an already
    /// registered resource file without replacing its existing map.
    pub(crate) fn merge_resources_into_existing_file(
        &mut self,
        fork: &ResourceFork,
        bus: &mut MacMemoryBus,
        refnum: u16,
    ) -> usize {
        let incoming = self.allocate_resource_fork(fork, bus);
        self.remember_preloaded_resource_residency(refnum, &incoming);
        let incoming_order = Self::resource_reference_order(fork);
        let count = incoming.loaded.len();
        self.with_resource_manager_mut(|resource_manager| {
            let resources = resource_manager.resources.get_or_insert_with(|| LoadedResources {
                files: HashMap::new(),
                names: HashMap::new(),
                search_order: vec![refnum],
                current_file: refnum,
            });
            if !resources.search_order.contains(&refnum) {
                resources.search_order.push(refnum);
            }

            let target = resources.files.entry(refnum).or_default();
            for (key, ptr) in incoming.loaded {
                target.loaded.entry(key).or_insert(ptr);
            }
            for (key, value) in incoming.named {
                target.named.entry(key).or_insert(value);
            }
            for (key, name) in incoming.names_by_id {
                target.names_by_id.entry(key).or_insert(name);
            }
            for (key, attrs) in incoming.attrs {
                target.attrs.entry(key).or_insert(attrs);
            }
            let order = resource_manager.resource_file_order.entry(refnum).or_default();
            for key in incoming_order {
                if !order.contains(&key) {
                    order.push(key);
                }
            }
        });
        self.remember_resource_fork_backing_data(refnum, fork);
        count
    }

    /// Load resources from a resource fork and merge them into the existing resource map.
    /// Used when the app opens additional resource files (e.g. Sounds, Images).
    pub fn merge_resources_from_fork(
        &mut self,
        fork: &ResourceFork,
        bus: &mut MacMemoryBus,
        refnum: u16,
    ) {
        let file = self.allocate_resource_fork(fork, bus);
        self.remember_preloaded_resource_residency(refnum, &file);
        self.with_resource_manager_mut(|resource_manager| {
            resource_manager
                .resource_file_order
                .insert(refnum, Self::resource_reference_order(fork));
        });
        let count = file.loaded.len();
        if trace_sound_enabled() {
            let mut type_counts: HashMap<[u8; 4], usize> = HashMap::new();
            for (res_type, _) in file.loaded.keys() {
                *type_counts.entry(*res_type).or_default() += 1;
            }
            if !type_counts.is_empty() {
                let mut counts: Vec<_> = type_counts.into_iter().collect();
                counts.sort_by_key(|(res_type, _)| *res_type);
                eprintln!("[RESOURCE] Additional fork types:");
                for (res_type, count) in counts {
                    let type_str = String::from_utf8_lossy(&res_type);
                    eprintln!("[RESOURCE]   '{}' x{}", type_str, count);
                }
            }
        }
        self.register_resource_file(refnum, file);
        self.clear_resource_file_backing_data(refnum);
        self.remember_resource_fork_backing_data(refnum, fork);
        if crate::runner::trace_load_enabled() {
            eprintln!("[RESOURCE] Merged {} resources from additional fork", count);
        }
    }

    /// Find a file in vfs_rsrc by name, preserving explicit path components.
    ///
    /// Basename matching is retained for the classic search-path behavior of
    /// basename-only requests, but an explicit nested pathname must not fall
    /// through to an unrelated file with the same leaf name.
    pub(crate) fn find_vfs_rsrc_file(&self, name: &str) -> Option<String> {
        let normalized = Self::normalize_vfs_path(name);
        let hfs_normalized = Self::normalize_hfs_path(name);
        // Sort iteration so the first-match is stable across runs.
        let mut sorted_keys: Vec<&String> = self.vfs_rsrc.keys().collect();
        sorted_keys.sort_unstable();
        if let Some(found) = sorted_keys
            .iter()
            .copied()
            .find(|key| key.eq_ignore_ascii_case(&hfs_normalized))
        {
            return Some(found.clone());
        }
        if let Some(found) = sorted_keys
            .iter()
            .copied()
            .find(|key| Self::normalize_vfs_path(key).eq_ignore_ascii_case(&normalized))
        {
            return Some(found.clone());
        }
        if let Some(found) =
            Self::find_case_insensitive_relative_key(sorted_keys.iter().copied(), &normalized)
        {
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

    /// Dispatch the profile-defined `_Unimplemented` operation separately
    /// from manager adapters. Modern 1,024-entry Toolbox tables identify
    /// `$AA6E` as this routine, and invoking it raises system error 12.
    /// Inside Macintosh: Operating System Utilities (1994), pp. 8-22, 8-32;
    /// Inside Macintosh: Overview (1992), pp. 9-14--9-15.
    fn raise_unimplemented(bus: &mut MacMemoryBus) -> Result<()> {
        bus.write_word(crate::memory::globals::addr::DS_ERR_CODE, 12);
        Err(Error::Halted)
    }

    fn dispatch_unimplemented<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        _cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        Some(match (is_tool, trap_num) {
            (true, 0x26E) => Self::raise_unimplemented(bus),
            _ => return None,
        })
    }

    /// Main trap dispatch entry point. Decodes the trap word and routes to
    /// the appropriate sub-dispatcher module.
    pub fn dispatch<C: CpuOps>(
        &mut self,
        trap: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Result<()> {
        self.dispatch_inner(trap, cpu, bus, None, None)
    }

    pub(crate) fn dispatch_with_process_services<C: CpuOps>(
        &mut self,
        trap: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        cfm: &crate::cfm::CfmState,
        bindings: Option<&mut dyn crate::cfm::CfmSymbolBindings>,
    ) -> Result<()> {
        self.dispatch_inner(trap, cpu, bus, Some(cfm), bindings)
    }

    fn dispatch_inner<C: CpuOps>(
        &mut self,
        trap: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        cfm: Option<&crate::cfm::CfmState>,
        bindings: Option<&mut dyn crate::cfm::CfmSymbolBindings>,
    ) -> Result<()> {
        if crate::execution_m68k::complete_classic_manager_return(&self.guest_calls, cpu, bus)
            && (self.resume_completed_menu_bar_build(cpu, bus)
                || self.resume_menu_tracking(cpu, bus).is_some())
        {
            return Ok(());
        }
        self.initialize_trap_tables(bus)?;
        // Low-memory Ticks is guest-owned writable state. Import it at the
        // ABI boundary before any manager, trace, or diagnostic path observes
        // the process clock so a direct guest store cannot be shadowed by a
        // stale host pacing snapshot.
        self.read_tick_count(bus);
        // Opt-in per-trap wall-clock timing.
        let timing_start = if trap_timing_enabled() {
            Some(std::time::Instant::now())
        } else {
            None
        };

        self.trap_count += 1;
        self.current_trap_word = trap;
        self.current_selector_operation = None;
        let pc = cpu.read_reg(Register::PC);
        // Append (trap-instruction PC, trap word) to the file named by
        // SYSTEMLESS_TRACE_TRAP_PCS, if any. PC is the post-trap PC; subtract
        // 2 for the actual trap-instruction address. No-op when unset.
        if let Some(sink) = trace_trap_pcs_sink() {
            use std::io::Write;
            if let Ok(mut w) = sink.lock() {
                let _ = writeln!(w, "T {:08X} {:04X}", pc.wrapping_sub(2), trap);
            }
        }
        // Read-only watcher for (A5+$BFCC) byte + (A5+$BFBA) word. Logs
        // on every change. Cheap when env unset.
        if let Some(sink) = log_m1_gates_sink() {
            let a5 = cpu.read_reg(Register::A5);
            if a5 >= 0x00010000 {
                let target_bfcc = a5.wrapping_add(0xFFFFBFCCu32);
                let target_bfba = a5.wrapping_add(0xFFFFBFBAu32);
                let cur_bfcc = bus.read_byte(target_bfcc);
                let cur_bfba = bus.read_word(target_bfba);
                let last_bfcc = M1_GATES_LAST_BFCC.load(std::sync::atomic::Ordering::Relaxed);
                let last_bfba = M1_GATES_LAST_BFBA.load(std::sync::atomic::Ordering::Relaxed);
                if cur_bfcc != last_bfcc || cur_bfba != last_bfba {
                    M1_GATES_LAST_BFCC.store(cur_bfcc, std::sync::atomic::Ordering::Relaxed);
                    M1_GATES_LAST_BFBA.store(cur_bfba, std::sync::atomic::Ordering::Relaxed);
                    use std::io::Write;
                    if let Ok(mut w) = sink.lock() {
                        let _ = writeln!(
                            w,
                            "M1-GATE trap=${:04X} pc=${:08X} a5=${:08X} BFCC.B=${:02X} BFBA.W=${:04X}",
                            trap,
                            pc.wrapping_sub(2),
                            a5,
                            cur_bfcc,
                            cur_bfba
                        );
                    }
                }
            }
        }
        if self.fade_trace_remaining > 0 {
            self.fade_trace_remaining -= 1;
            eprintln!(
                "[FADE-TRACE] trap=${:04X} pc=${:08X} tick={} d0=${:08X} a0=${:08X}",
                trap,
                pc.wrapping_sub(2),
                self.current_tick(),
                cpu.read_reg(Register::D0),
                cpu.read_reg(Register::A0),
            );
        }
        // SYSTEMLESS_TRACE_PC=0xADDR logs context whenever a trap fires from
        // a specific PC: registers, stack window, and 16 bytes of M68K
        // opcodes around both the trap PC and the return PC. Per-call cost
        // is one env-var lookup and a hex-parse — only set during investigation.
        if let Some(target_pc) = trace_pc_target() {
            let trap_pc = pc.wrapping_sub(2);
            if trap_pc == target_pc {
                let sp = cpu.read_reg(Register::A7);
                eprintln!(
                    "[TRACE-PC] trap=${:04X} pc=${:08X} tick={} sp=${:08X}",
                    trap,
                    trap_pc,
                    self.current_tick(),
                    sp
                );
                eprintln!(
                    "[TRACE-PC]   d0=${:08X} d1=${:08X} d2=${:08X} d3=${:08X} d4=${:08X} d5=${:08X} d6=${:08X} d7=${:08X}",
                    cpu.read_reg(Register::D0),
                    cpu.read_reg(Register::D1),
                    cpu.read_reg(Register::D2),
                    cpu.read_reg(Register::D3),
                    cpu.read_reg(Register::D4),
                    cpu.read_reg(Register::D5),
                    cpu.read_reg(Register::D6),
                    cpu.read_reg(Register::D7),
                );
                eprintln!(
                    "[TRACE-PC]   a0=${:08X} a1=${:08X} a2=${:08X} a3=${:08X} a4=${:08X} a5=${:08X} a6=${:08X}",
                    cpu.read_reg(Register::A0),
                    cpu.read_reg(Register::A1),
                    cpu.read_reg(Register::A2),
                    cpu.read_reg(Register::A3),
                    cpu.read_reg(Register::A4),
                    cpu.read_reg(Register::A5),
                    cpu.read_reg(Register::A6),
                );
                // Dump 128 bytes of stack memory at SP. Pascal A-traps don't
                // push a JSR return PC — the trap handler arrives with USP
                // holding the Pascal args. The JSR-pushed caller PC lives
                // DEEPER on the stack (after any pushed locals).
                let stack_words: Vec<String> = (0..32)
                    .map(|i| format!("{:08X}", bus.read_long(sp.wrapping_add(i * 4))))
                    .collect();
                for chunk_idx in 0..4 {
                    let start_word = chunk_idx * 8;
                    let chunk = &stack_words[start_word..start_word + 8];
                    eprintln!(
                        "[TRACE-PC]   stack@${:08X}: {}",
                        sp.wrapping_add((start_word as u32) * 4),
                        chunk.join(" ")
                    );
                }
                // Dump opcodes around the trap PC: 512 bytes BEFORE and 16
                // bytes AFTER. The pre-bytes typically include the routine
                // prologue (LINK A6 = 4E 56) which marks the function entry.
                let pre_start = trap_pc.wrapping_sub(512);
                for line_start in 0..32 {
                    let row_addr = pre_start.wrapping_add(line_start * 16);
                    let row_bytes: Vec<String> = (0..16)
                        .map(|i| format!("{:02X}", bus.read_byte(row_addr.wrapping_add(i))))
                        .collect();
                    eprintln!(
                        "[TRACE-PC]   pre @${:08X}: {}",
                        row_addr,
                        row_bytes.join(" ")
                    );
                }
                let trap_bytes: Vec<String> = (0..16)
                    .map(|i| format!("{:02X}", bus.read_byte(trap_pc.wrapping_add(i))))
                    .collect();
                eprintln!(
                    "[TRACE-PC]   trap@${:08X}: {}",
                    trap_pc,
                    trap_bytes.join(" ")
                );
            }
        }
        // Tick-windowed A-trap trace.
        // `SYSTEMLESS_TRACE_ATRAPS_WINDOW=LO-HI` logs trap+pc+tick for every
        // trap whose `tick_count` is in `[LO, HI]`.
        if let Some((lo, hi)) = trace_atraps_window() {
            if self.current_tick() >= lo && self.current_tick() <= hi {
                eprintln!(
                    "[ATRAP-WIN] tick={} trap=${:04X} pc=${:08X}",
                    self.current_tick(),
                    trap,
                    pc.wrapping_sub(2),
                );
            }
        }
        // Preserve the complete A-line classification before any table mask
        // is applied. Every later consumer uses this generated route, so OS
        // flag bits and Toolbox auto-pop cannot be reconstructed differently
        // by separate dispatch paths.
        let input_route = raw_trap_route(trap);
        let input_base_trap = input_route.canonical_word;
        let default_os_gateway_call = !input_route.is_toolbox
            && bus
                .system_trap_gateway(input_base_trap)
                .is_some_and(|addr| pc == addr + 2);
        // A JMP to a saved OS gateway keeps the dispatcher's synthesized
        // return long at the top of the original argument stack. A JSR to the
        // same saved pointer has its own return frame and is an independent
        // old-routine call, not the tail of the active daisy chain.
        let saved_os_daisy_chain_call = default_os_gateway_call
            && self
                .pending_native_trap_calls
                .get(&input_base_trap)
                .and_then(|calls| calls.last())
                .is_some_and(|call| {
                    call.os_dispatch_frame.is_some()
                        && cpu.read_reg(Register::A7) == call.argument_sp.wrapping_sub(4)
                        && bus.read_long(cpu.read_reg(Register::A7)) == call.return_pc
                });
        let effective_trap = if saved_os_daisy_chain_call {
            self.pending_native_trap_calls
                .get(&input_base_trap)
                .and_then(|calls| calls.last())
                .and_then(|call| call.os_dispatch_frame)
                .map_or(trap, |frame| frame.trap_word)
        } else {
            trap
        };
        self.current_trap_word = effective_trap;
        // PowerMgrDispatch ($A09E)
        // Dispatches register-based Power Manager routines selected by D0.W.
        // short PMSelectorCount(void);
        // Inside Macintosh: Devices (1994), p. 6-41.
        let power_operation =
            power_manager_operation_route(effective_trap, cpu.read_reg(Register::D0) as u16);
        self.current_selector_operation = power_operation.map(|route| route.operation_id);
        let route = raw_trap_route(effective_trap);
        let is_tool = route.is_toolbox;
        // Count game traps: from game code (PC < 0x800000), NOT during
        // remaining tracking loops (synthetic HLE re-dispatches), and
        // NOT idle-loop traps (GetNextEvent, WaitNextEvent, EventAvail)
        // which fire at wildly different rates depending on CPU speed.
        let trap_number = route.table_slot;
        let is_idle_trap = match trap_number {
            0x0170 => true,            // GetNextEvent ($A970)
            0x0060 if is_tool => true, // WaitNextEvent ($A860), not HFSDispatch ($A060)
            0x0171 => true,            // EventAvail ($A971)
            0x0175 => true,            // TickCount ($A975) - polled in busy wait loops
            0x006E => true,            // SANE FP68K ($A86E) - ROM package on real Mac
            0x006C => true,            // SANE Elems68K ($A86C) - ROM package on real Mac
            0x0031 => true,            // GetOSEvent ($A031) - event polling
            0x0062 if is_tool => true, // Button ($A862), not FSDispatch selector space
            _ => false,
        };
        if pc < 0x00800000
            && self.menu_tracking.is_none()
            && self.dialog_tracking.is_none()
            && self.standard_file_put_tracking.is_none()
            && self.standard_file_get_tracking.is_none()
            && !is_idle_trap
        {
            self.game_trap_count += 1;
        }
        // Gated per-trap histogram. Opt-in via SYSTEMLESS_TRACE_TRAP_COUNTS=1.
        // Counts ALL dispatches (system + game), not the game_trap_count
        // filtered subset, so the full mix including ROM/system traps is
        // visible.
        if trap_histogram_enabled() || trap_timing_enabled() {
            self.trap_histogram[(trap & 0xFFF) as usize] =
                self.trap_histogram[(trap & 0xFFF) as usize].saturating_add(1);
        }
        let auto_pop = route.toolbox_auto_pop;
        let trap_num = route.table_slot;
        let pc = cpu.read_reg(Register::PC);

        if trace_guest_pc_traps_enabled() && (0x00235000..=0x00238000).contains(&pc) {
            eprintln!(
                "[PC-TRAP] PC=${:08X} trap=${:04X} base=${:04X} tool={} auto_pop={}",
                pc, trap, route.canonical_word, is_tool, auto_pop,
            );
        }
        if trace_all_traps_enabled() {
            eprintln!(
                "[ALL-TRAP] PC=${:08X} trap=${:04X} base=${:04X} tool={} num=0x{:03X}",
                pc, trap, route.canonical_word, is_tool, trap_num,
            );
        }
        if trace_dialog_traps_enabled() && self.dialog_tracking.is_some() {
            eprintln!(
                "[DIALOG-TRAP] PC=${:08X} trap=${:04X} base=${:04X} tool={} auto_pop={}",
                pc, trap, route.canonical_word, is_tool, auto_pop,
            );
        }

        // Handle auto-pop: save return address and adjust SP
        let saved_return_addr = if auto_pop {
            let sp = cpu.read_reg(Register::A7);
            let ret_addr = bus.read_long(sp);
            cpu.write_reg(Register::A7, sp + 4);
            Some(ret_addr)
        } else {
            None
        };
        // Surface the auto-pop caller PC to sub-dispatchers
        // (read by e.g. the SANE-NAN tracer in trap/sane.rs).
        self.current_trap_caller = saved_return_addr;

        // Check for native trap handler installed by SetTrapAddress.
        // The CRT installs handlers for LoadSeg ($A9F0), UnloadSeg ($A9F1),
        // and ExitToShell ($A9F4). These native handlers perform code
        // relocation that our HLE LoadSeg cannot replicate. We simulate
        // a JSR to the native handler: push return address, set PC.
        // The base trap word (without variant/auto-pop bits) is used for lookup.
        let base_trap = route.canonical_word;
        // A pointer returned before a patch was installed remains the saved
        // address of the original system routine. The OS gateway is the
        // canonical trap followed by RTS, so recognize its exact trap PC and
        // bypass the current table head. Toolbox gateways use auto-pop for the
        // same saved-old behavior. Inside Macintosh: Operating System
        // Utilities (1994), pp. 8-23--8-30.
        // Some native patches embed the saved auto-pop A-line in their own
        // successor stub instead of calling the cached gateway. It is a daisy
        // chain handoff only when the removed return PC and post-pop argument
        // SP exactly match the active invocation. A reentrant auto-pop glue
        // call has its own return frame and must enter the current patch head.
        let saved_tool_daisy_chain_call = is_tool
            && auto_pop
            && saved_return_addr.is_some_and(|return_pc| {
                self.pending_native_trap_calls
                    .get(&base_trap)
                    .and_then(|calls| calls.last())
                    .is_some_and(|call| {
                        call.return_pc == return_pc
                            && call.argument_sp == cpu.read_reg(Register::A7)
                    })
            });
        let default_tool_gateway_call = is_tool
            && auto_pop
            && (bus
                .system_trap_gateway(base_trap)
                .is_some_and(|addr| pc == addr + 2)
                || saved_tool_daisy_chain_call);
        let os_dispatch_frame = if is_tool {
            None
        } else if saved_os_daisy_chain_call {
            self.pending_native_trap_calls
                .get(&base_trap)
                .and_then(|calls| calls.last())
                .and_then(|call| call.os_dispatch_frame)
        } else {
            Some(capture_os_trap_dispatch_frame(cpu, effective_trap))
        };
        if !is_tool {
            // The dispatcher writes only D1's low word. The high word remains
            // caller state until D1 is restored after the routine returns.
            deliver_os_trap_word(cpu, effective_trap);
        }
        if !default_os_gateway_call && !default_tool_gateway_call {
            let handler_addr = self
                .trap_table_address(bus, base_trap)
                .ok_or(Error::TrapTableLookup(base_trap))?;
            if self.default_trap_gateway(bus, base_trap) != Some(handler_addr) {
                // Simulate JSR to native handler: push return PC, jump to
                // handler. For an auto-pop trap, the dispatcher's documented
                // return target is the caller address removed from the glue
                // frame, not the instruction after the glue's A-line. Inside
                // Macintosh: Operating System Utilities (1994), p. 8-20.
                let return_pc = saved_return_addr.unwrap_or_else(|| cpu.read_reg(Register::PC));
                let sp = cpu.read_reg(Register::A7);
                self.retain_native_trap_call(
                    base_trap,
                    NativeTrapCallState {
                        return_pc,
                        argument_sp: sp,
                        os_dispatch_frame,
                        preserved_d_regs: [
                            cpu.read_reg(Register::D3),
                            cpu.read_reg(Register::D4),
                            cpu.read_reg(Register::D5),
                            cpu.read_reg(Register::D6),
                            cpu.read_reg(Register::D7),
                        ],
                        preserved_a_regs: [
                            cpu.read_reg(Register::A2),
                            cpu.read_reg(Register::A3),
                            cpu.read_reg(Register::A4),
                            cpu.read_reg(Register::A5),
                            cpu.read_reg(Register::A6),
                        ],
                    },
                );
                let new_sp = sp.wrapping_sub(4);
                bus.write_long(new_sp, return_pc);
                cpu.write_reg(Register::A7, new_sp);
                cpu.write_reg(Register::PC, handler_addr);
                if trace_native_traps_enabled() {
                    eprintln!(
                        "[DISPATCH] -> native handler at ${:08X} for trap ${:04X}",
                        handler_addr, base_trap
                    );
                }
                return Ok(());
            }
        }

        // Track consecutive SANE and TickCount calls. The generated registry
        // names the canonical operation and expected first adapter; recording
        // the actual first match makes a declared nonterminal row distinct
        // from registry drift.
        let sp_before = cpu.read_reg(Register::A7);
        let declared_route = *default_trap_route(effective_trap);
        let mut selected_adapter = TrapAdapterId::Nonterminal;
        let result = self
            .dispatch_unimplemented(is_tool, trap_num, cpu, bus)
            .map(|result| {
                selected_adapter = TrapAdapterId::Unimplemented;
                result
            })
            .or_else(|| {
                self.dispatch_memory(is_tool, trap_num, cpu, bus)
                    .map(|result| {
                        selected_adapter = TrapAdapterId::Memory;
                        result
                    })
            })
            .or_else(|| {
                self.dispatch_event(is_tool, trap_num, cpu, bus)
                    .map(|result| {
                        selected_adapter = TrapAdapterId::Event;
                        result
                    })
            })
            .or_else(|| {
                self.dispatch_resource(is_tool, trap_num, cpu, bus)
                    .map(|result| {
                        selected_adapter = TrapAdapterId::Resource;
                        result
                    })
            })
            .or_else(|| {
                self.dispatch_quickdraw(is_tool, trap_num, cpu, bus)
                    .map(|result| {
                        selected_adapter = TrapAdapterId::QuickDraw;
                        result
                    })
            })
            .or_else(|| {
                self.dispatch_menu(is_tool, trap_num, cpu, bus)
                    .map(|result| {
                        selected_adapter = TrapAdapterId::Menu;
                        result
                    })
            })
            .or_else(|| {
                self.dispatch_window(is_tool, trap_num, cpu, bus)
                    .map(|result| {
                        selected_adapter = TrapAdapterId::Window;
                        result
                    })
            })
            .or_else(|| {
                self.dispatch_control(is_tool, trap_num, cpu, bus)
                    .map(|result| {
                        selected_adapter = TrapAdapterId::Control;
                        result
                    })
            })
            .or_else(|| {
                self.dispatch_dialog(is_tool, trap_num, cpu, bus)
                    .map(|result| {
                        selected_adapter = TrapAdapterId::Dialog;
                        result
                    })
            })
            .or_else(|| {
                self.dispatch_collection(is_tool, trap_num, cpu, bus)
                    .map(|result| {
                        selected_adapter = TrapAdapterId::Collection;
                        result
                    })
            })
            .or_else(|| {
                self.dispatch_sound(is_tool, trap_num, cpu, bus)
                    .map(|result| {
                        selected_adapter = TrapAdapterId::Sound;
                        result
                    })
            })
            .or_else(|| {
                self.dispatch_toolbox_with_process_services(
                    is_tool, trap_num, cpu, bus, cfm, bindings,
                )
                .map(|result| {
                    selected_adapter = TrapAdapterId::Toolbox;
                    result
                })
            })
            .or_else(|| {
                self.dispatch_sane(is_tool, trap_num, cpu, bus)
                    .map(|result| {
                        selected_adapter = TrapAdapterId::Sane;
                        result
                    })
            })
            .unwrap_or_else(|| {
                eprintln!(
                    "[TRAP] UNIMPLEMENTED ${:04X} (is_tool={}, num=0x{:03X})",
                    effective_trap, is_tool, trap_num
                );
                Err(Error::UnimplementedTrap(effective_trap))
            });
        self.current_trap_operation = declared_route.operation_id;
        self.current_trap_adapter = selected_adapter;
        debug_assert!(
            declared_route.allows(selected_adapter),
            "generated adapter registry drift for ${effective_trap:04X}: {selected_adapter:?}"
        );

        if result.is_ok() && !is_tool {
            restore_os_trap_dispatch_frame(
                cpu,
                os_dispatch_frame.expect("OS dispatch must retain its register frame"),
            );
            cpu.write_reg(Register::A7, sp_before);
            apply_os_trap_dispatcher_ccr(cpu);
        }

        if result.is_ok() && trace_trap_sp_enabled() {
            let sp_after = cpu.read_reg(Register::A7);
            let delta = sp_after.wrapping_sub(sp_before) as i32;
            eprintln!(
                "[SP-DELTA] trap=${:04X} sp_before=${:08X} sp_after=${:08X} delta={}",
                trap, sp_before, sp_after, delta
            );
        }
        // Handle auto-pop return.
        // Only push ret_addr back when the CURRENT trap is one of the
        // remaining refire traps (matches the runner's is_tracking_refire
        // logic). is_tracking_refire is shared so dispatch.rs and runner.rs
        // can never diverge on the match logic.
        if let Some(ret_addr) = saved_return_addr {
            if result.is_ok() && !self.is_tracking_refire(trap) {
                if self.preserve_auto_pop_pc_once {
                    self.preserve_auto_pop_pc_once = false;
                } else {
                    cpu.write_reg(Register::PC, ret_addr);
                }
            } else {
                self.preserve_auto_pop_pc_once = false;
                // Push the return address back onto the stack.
                // This covers two cases:
                // 1. Tracking refire: the trap must re-fire next frame,
                //    so undo the auto-pop so the stack stays as the
                //    game set it.
                // 2. Unimplemented/halt trap: prevent stack corruption
                //    from the lost return address.
                let sp = cpu.read_reg(Register::A7);
                bus.write_long(sp.wrapping_sub(4), ret_addr);
                cpu.write_reg(Register::A7, sp.wrapping_sub(4));
            }
        }
        if saved_os_daisy_chain_call || default_tool_gateway_call {
            // Reaching a saved system gateway hands this invocation to the
            // next routine in the daisy chain. LoadSeg consumes its retained
            // state earlier because it needs the original jump-table frame;
            // ordinary traps simply retire it here.
            self.take_latest_native_trap_call(base_trap);
        }
        if self.current_trap_caller.is_none() && matches!(&result, Err(Error::Halted)) {
            // Direct halt traps have no auto-pop caller to surface, so
            // fall back to the trap site for the runner's halt log.
            self.current_trap_caller = Some(pc.wrapping_sub(2));
        }
        // Clear the auto-pop caller PC after the trap returns — but ONLY on
        // success. On halt/error, leave it set so the runner's halt log can
        // surface it to the operator.
        if result.is_ok() {
            self.current_trap_caller = None;
        }

        // Accumulate per-trap timing if enabled. End-to-end wall-clock per
        // trap word (dispatch-entry bookkeeping + sub-dispatcher chain +
        // handler body + auto-pop handling).
        if let Some(start) = timing_start {
            let ns = start.elapsed().as_nanos() as u64;
            self.trap_time_ns[(trap & 0xFFF) as usize] =
                self.trap_time_ns[(trap & 0xFFF) as usize].saturating_add(ns);
        }

        result
    }
}

impl Default for TrapDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
