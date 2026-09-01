//! Trap Dispatcher - routes Mac OS traps to per-manager handler modules.
//!
//! The TrapDispatcher struct holds all emulator state. Each sub-module adds
//! `impl TrapDispatcher` blocks with `dispatch_*` methods that return
//! `Option<Result<()>>` — `Some` if the trap was handled, `None` to pass through.

use super::types::UnderlineInfo;
use crate::cpu::{CpuOps, Register};
use crate::display::CursorImage;
use crate::guest_call::SharedGuestCallStack;
use crate::list_manager::ProcessListRecord;
use crate::machine_profile::reference_machine_profile;
use crate::managers::resource::ResourceFork;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::menu_manager::{ProcessMenuTrackingState, SharedNativeMenuSelection};
use crate::process_context::{
    ProcessClassicVfsDirectory, ProcessContext, ProcessForkMap, ProcessKeyRepeatState,
    ProcessLoadedResources, ProcessResourceFileMap, ProcessResourceManagerState,
    ProcessVfsMetadata, ProcessVfsVolumeRecord, SharedProcessAppleEventHandlers,
    SharedProcessControlManager, SharedProcessCursorState, SharedProcessDialogText,
    SharedProcessEventQueue,
    SharedProcessFileSystem, SharedProcessInputState, SharedProcessMemoryManager,
    SharedProcessListManager, SharedProcessMenuTracking, SharedProcessResourceManager, SharedProcessScrapState,
    SharedProcessSoundManager, SharedProcessTextEditManager, SharedProcessValue,
};
use crate::trace::{TraceEvent, TraceSink, TraceSource};
use crate::ui_theme::{UiTheme, UiThemeId};
use crate::{Error, Result};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::PathBuf;

pub use crate::event_queue::QueuedEvent;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PendingFileCompletion {
    pub(crate) parameter_block: u32,
    pub(crate) completion_addr: u32,
    pub(crate) result: i16,
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

fn set_key_map_key(key_map: &mut [u8; 16], key_code: u8, down: bool) {
    let Some((byte_idx, mask)) = key_map_byte_mask(key_code) else {
        return;
    };
    if down {
        key_map[byte_idx] |= mask;
    } else {
        key_map[byte_idx] &= !mask;
    }
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
    pub saved_pixels: Vec<u8>,
    /// Saved stack pointer from ModalDialog's first call
    pub stack_ptr: u32,
    /// Pointer to the itemHit variable (where to write the result)
    pub item_hit_ptr: u32,
    /// Snapshot of the fully-rendered dialog pixels (including pictures).
    /// Used by redraw_chrome to restore the dialog without re-parsing PICTs.
    pub rendered_pixels: Vec<u8>,
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
    pub vref: i16,
    pub old_wd_ref: i16,
    pub dir_id: u32,
    pub prompt: String,
    pub name: String,
    pub sel_start: i16,
    pub sel_end: i16,
    pub bounds: (i16, i16, i16, i16),
    pub saved_pixels: Vec<u8>,
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
    pub saved_pixels: Vec<u8>,
}

/// Popup-menu control state owned by an active ModalDialog loop.
pub struct DialogPopupTrackingState {
    pub item_no: i16,
    pub ctrl_handle: u32,
    pub ctrl_ptr: u32,
    pub active_menu: usize,
    pub highlighted_item: i16,
    pub saved_pixels: Vec<u8>,
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
    pub pixels: Vec<u8>,
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
    pub saved_pixels: Vec<u8>,
    pub dropdown_rect: (i16, i16, i16, i16),
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
    pub outline_saved_pixels: Vec<(i16, i16, i16, i16, Vec<u8>)>,
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
    pub size_rect: (i16, i16, i16, i16),
    pub outline_rect: (i16, i16, i16, i16),
    pub outline_saved_pixels: Vec<(i16, i16, i16, i16, Vec<u8>)>,
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
    pub outline_saved_pixels: Vec<(i16, i16, i16, i16, Vec<u8>)>,
    pub outline_pattern: [u8; 8],
}

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Debug)]
pub(crate) struct PortRegionSnapshot {
    pub handle: u32,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(crate) struct PortStateSnapshot {
    pub port: u32,
    pub gdevice: u32,
    pub draw_state: PortDrawState,
    pub port_state_bytes: [u8; 56],
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

/// Stack size handed to a cooperative thread when `NewThread` is passed 0
/// and the size reported by `GetDefaultThreadStackSize`. The 68K Thread
/// Manager's own default is a small multiple of a page; 32K comfortably
/// covers the Toolbox call depth Systemless threads reach.
pub(crate) const DEFAULT_COOPERATIVE_THREAD_STACK_SIZE: u32 = 32 * 1024;

/// Saved 68K state for one cooperative Thread Manager thread.
///
/// Cooperative switches occur only inside `_ThreadDispatch`, so the HLE can
/// preserve the complete caller-visible register file without involving a
/// host thread. New threads inherit the creator's register world (notably A5)
/// and receive a private guest stack.
#[derive(Clone, Debug)]
pub(crate) struct CooperativeThread {
    pub(crate) d_regs: [u32; 8],
    pub(crate) a_regs: [u32; 8],
    pub(crate) pc: u32,
    pub(crate) ccr: u8,
    /// `ThreadState` from Threads.h: 0 ready, 1 stopped, 2 running.
    pub(crate) state: u16,
    /// `void **threadResult` the entry proc's return value is stored to.
    pub(crate) result_destination: u32,
    /// Lowest address of the private guest stack, or 0 for the
    /// application thread, which keeps the process stack.
    pub(crate) stack_base: u32,
    /// Address one past the top of the private guest stack.
    pub(crate) stack_limit: u32,
    /// `SetThreadSwitcher` switch-in proc and its `switchProcParam`.
    pub(crate) switch_in: (u32, u32),
    /// `SetThreadSwitcher` switch-out proc and its `switchProcParam`.
    pub(crate) switch_out: (u32, u32),
    /// `SetThreadTerminator` proc and its `terminationProcParam`.
    pub(crate) terminator: (u32, u32),
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

/// Minimal AppleEvent descriptor value tracked by Pack8. The real Apple Event
/// Manager serializes descriptor records into handles; Systemless only needs
/// enough structured state for caller-observable get/put routines.
#[derive(Clone, Debug)]
pub(crate) struct AeDescriptor {
    pub desc_type: u32,
    pub data: Vec<u8>,
    pub fields: HashMap<u32, AeDescriptor>,
    pub items: Vec<(u32, AeDescriptor)>,
}

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

/// Minimal AppleEvent descriptor state synthesized by Pack8. This records
/// attributes that AEGetAttribute* must expose and parameters that
/// AEGetParam* must return while dispatching AppleEvents.
#[derive(Clone, Debug)]
pub(crate) struct SyntheticAppleEvent {
    pub event_class: u32,
    pub event_id: u32,
    pub params: HashMap<u32, AeDescriptor>,
    pub items: Vec<(u32, AeDescriptor)>,
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
pub(crate) type VfsDirectory = ProcessClassicVfsDirectory;
pub(crate) type VfsVolume = ProcessVfsVolumeRecord;

#[derive(Clone, Copy, Debug)]
pub(crate) struct WorkingDirectory {
    pub ref_num: i16,
    pub volume_ref_num: i16,
    pub dir_id: u32,
    pub proc_id: u32,
}

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

/// Pre-initialization mirror of native trap installations.
///
/// Focused dispatcher tests and early startup consult this before the raw
/// guest tables exist. Once materialized, the guest longs are authoritative.
/// The default `HashMap` paid
/// SipHash plus a SwissTable probe per lookup, and a linear scan is
/// unbounded: `SetTrapAddress`/`NSetTrapAddress` can populate arbitrarily
/// many slots, so an application that patches many traps would make the
/// hottest dispatch path O(n).
///
/// Every key producer normalizes into two bands — the Operating System
/// table `0xA000..=0xA0FF` (`trap_address_table_key`, and dispatch's
/// `0xA000 | (trap & 0x00FF)`) and the Toolbox table `0xA800..=0xABFF`
/// (`0xA800 | (trap & 0x03FF)`) — 1,280 possible keys in total, so the
/// table direct-indexes them: O(1) for any occupancy, no hashing, no
/// probe, ~10 KB once. Keys outside the two bands (which no production
/// caller generates) go to a spill list so the `HashMap`-shaped contract
/// stays total; the dispatch path never touches it because its keys are
/// in-band by construction.
#[derive(Debug, Clone)]
pub(crate) struct TrapWordMap {
    bands: Box<[Option<u32>; Self::SLOTS]>,
    spill: Vec<(u16, u32)>,
}

impl Default for TrapWordMap {
    fn default() -> Self {
        Self {
            bands: Box::new([None; Self::SLOTS]),
            spill: Vec::new(),
        }
    }
}

impl TrapWordMap {
    const OS_SLOTS: usize = 0x100;
    const TOOL_SLOTS: usize = 0x400;
    const SLOTS: usize = Self::OS_SLOTS + Self::TOOL_SLOTS;

    fn slot(trap: u16) -> Option<usize> {
        match trap {
            0xA000..=0xA0FF => Some(usize::from(trap - 0xA000)),
            0xA800..=0xABFF => Some(Self::OS_SLOTS + usize::from(trap - 0xA800)),
            _ => None,
        }
    }

    pub(crate) fn get(&self, trap: &u16) -> Option<&u32> {
        match Self::slot(*trap) {
            Some(index) => self.bands[index].as_ref(),
            None => self
                .spill
                .iter()
                .find(|(word, _)| word == trap)
                .map(|(_, handler)| handler),
        }
    }

    pub(crate) fn insert(&mut self, trap: u16, handler: u32) -> Option<u32> {
        match Self::slot(trap) {
            Some(index) => self.bands[index].replace(handler),
            None => match self.spill.iter_mut().find(|(word, _)| *word == trap) {
                Some(slot) => Some(std::mem::replace(&mut slot.1, handler)),
                None => {
                    self.spill.push((trap, handler));
                    None
                }
            },
        }
    }

    pub(crate) fn remove(&mut self, trap: &u16) -> Option<u32> {
        match Self::slot(*trap) {
            Some(index) => self.bands[index].take(),
            None => {
                let index = self.spill.iter().position(|(word, _)| word == trap)?;
                Some(self.spill.swap_remove(index).1)
            }
        }
    }
}

/// Mac OS 8.1 trap-table layout selected by the reference machine profile.
/// The Operating System table contains 256 routine addresses and the Toolbox
/// table contains 1,024. Inside Macintosh: Operating System Utilities (1994),
/// pp. 8-4--8-6 describes the table shapes; Inside Macintosh Volume VI (1991),
/// Gestalt Manager constants `gestaltOSTable` and `gestaltToolboxTable`, expose
/// their bases to applications.
pub(crate) const OS_TRAP_TABLE_BASE: u32 = 0x0400;
pub(crate) const TOOLBOX_TRAP_TABLE_BASE: u32 = 0x0E00;
pub(crate) const OS_TRAP_TABLE_SLOTS: u16 = 0x0100;
pub(crate) const TOOLBOX_TRAP_TABLE_SLOTS: u16 = 0x0400;
pub(crate) const COME_FROM_PATCH_SIGNATURE: u32 = 0x6006_4EF9;

/// Source-backed meanings for OS-trap routine bits 10 and 9.
///
/// These bits are private to each OS routine; they must not be interpreted as
/// global dispatcher flags. The Memory Manager meanings below come from
/// *Inside Macintosh: Memory* (1992), pp. 2-31, 2-33, 2-35, 2-53--2-55,
/// 2-65--2-68, and 2-71--2-74. The text transformations come from *Inside
/// Macintosh*, Volume VI (1991), pp. 14-62--14-63 and Appendix C, table C-2.
/// The string-comparison permutations come from *Inside Macintosh: Text*
/// (1993), pp. 5-51--5-52 and 5-60--5-61. The later table is authoritative
/// over the contradictory MARKS annotation in Volume II (1985), p. II-377.
/// UpperString's MARKS form comes from the same book, pp. 5-64--5-65.
/// Parameter-block synchronous, immediate, and asynchronous forms come from
/// *Inside Macintosh: Devices* (1994), p. 1-16.
/// Original and extended Time Manager task installation comes from *Inside
/// Macintosh: Processes* (1994), pp. 3-18--3-20.
/// Trap Manager legacy, new-OS, and new-Tool forms come from *Inside Macintosh:
/// Operating System Utilities* (1994), pp. 8-27--8-31 and 8-32--8-33.
/// Gestalt, NewGestalt, and ReplaceGestalt come from the same book,
/// pp. 1-31--1-36.
/// SleepQInstall and SleepQRemove come from *Inside Macintosh: Devices*
/// (1994), pp. 6-18, 6-26, and 6-33.
/// IdleUpdate, IdleState, and SerialPower come from the same book,
/// pp. 6-29--6-30 and 6-33--6-35.
/// File Manager synchronous, asynchronous, and HFS forms come from *Inside
/// Macintosh: Files* (1992), pp. 2-6, 2-238--2-239, and its assembly-language
/// summary. Universal Interfaces 3.4 independently declares reviewed exact
/// words in `MacMemory.h` (lines 436--1010, 1331--1362), `TextUtils.h` (lines
/// 404--455), `Devices.h` (lines 905--1044, 1282--1415), and `Files.h` (lines
/// 1315--3343); `Timer.h` lines 74--100 declares InsTime and InsXTime;
/// `Patches.h` lines 80--231 declares the Trap Manager forms;
/// `Gestalt.h` lines 55--105 declares the three Gestalt Manager forms;
/// `Power.h` lines 447--461 and 705--731 declares the sleep-queue record and
/// register entry points; lines 650--701 and 733--791 declares the idle-state
/// and serial-power entry points;
/// `StringCompare.h` lines 567--596 retains the comparison APIs;
/// `Devices.h` lines 1109--1141 declares DriverInstall and its bit-10
/// DriverInstallReserveMem form; `MacMemory.h` lines 1184--1202 declares
/// ReallocateHandle and ReallocateHandleSys.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OsRoutineVariant {
    Unclassified,
    CurrentHeap,
    SystemHeap,
    CurrentHeapClear,
    SystemHeapClear,
    LowerText,
    StripText,
    UpperText,
    StripUpperText,
    TextCompareFoldCaseAndMarks,
    TextCompareFoldCase,
    TextCompareStripMarks,
    TextCompareExact,
    UpperStringPreserveMarks,
    UpperStringStripMarks,
    ParameterBlockSynchronous,
    ParameterBlockImmediate,
    ParameterBlockAsynchronous,
    TimeTaskOriginal,
    TimeTaskExtended,
    TrapAddressLegacy,
    TrapAddressNewOs,
    TrapAddressNewTool,
    GestaltQuery,
    GestaltRegister,
    GestaltReplace,
    SleepQueueInstall,
    SleepQueueRemove,
    PowerIdleUpdate,
    PowerIdleState,
    PowerSerial,
    DriverInstall,
    DriverInstallReserveMemory,
    FileSynchronous,
    FileAsynchronous,
    FileHfsSynchronous,
    FileHfsAsynchronous,
}

impl OsRoutineVariant {
    /// Returns `(case_sensitive, diacritic_sensitive)` for CmpString/RelString.
    pub(crate) const fn text_comparison_sensitivity(self) -> Option<(bool, bool)> {
        match self {
            Self::TextCompareFoldCaseAndMarks => Some((false, false)),
            Self::TextCompareFoldCase => Some((false, true)),
            Self::TextCompareStripMarks => Some((true, false)),
            Self::TextCompareExact => Some((true, true)),
            _ => None,
        }
    }
}

const fn classify_os_routine_variant(raw_word: u16) -> OsRoutineVariant {
    if (raw_word & 0x0800) != 0 {
        return OsRoutineVariant::Unclassified;
    }

    let slot = raw_word & 0x00FF;
    let routine_bits = raw_word & 0x0600;
    match (slot, routine_bits) {
        // NewPtr and NewHandle use bit 10 for SYS and bit 9 for CLEAR.
        (0x1E | 0x22, 0x0000) => OsRoutineVariant::CurrentHeap,
        (0x1E | 0x22, 0x0200) => OsRoutineVariant::CurrentHeapClear,
        (0x1E | 0x22, 0x0400) => OsRoutineVariant::SystemHeap,
        (0x1E | 0x22, 0x0600) => OsRoutineVariant::SystemHeapClear,

        // These Memory Manager routines document only bit 10 as SYS. Leave
        // their bit-9 forms unclassified rather than inventing a meaning.
        (0x1C | 0x1D | 0x27 | 0x28 | 0x40 | 0x4C | 0x4D | 0x61 | 0x62 | 0x66, 0x0000) => {
            OsRoutineVariant::CurrentHeap
        }
        (0x1C | 0x1D | 0x27 | 0x28 | 0x40 | 0x4C | 0x4D | 0x61 | 0x62 | 0x66, 0x0400) => {
            OsRoutineVariant::SystemHeap
        }

        // LowerText, StripText, UpperText, and StripUpperText share slot $56.
        (0x56, 0x0000) => OsRoutineVariant::LowerText,
        (0x56, 0x0200) => OsRoutineVariant::StripText,
        (0x56, 0x0400) => OsRoutineVariant::UpperText,
        (0x56, 0x0600) => OsRoutineVariant::StripUpperText,

        // Text 1993, pp. 5-51--5-52 and 5-60--5-61: MARKS (bit 9)
        // makes comparison diacritic-sensitive and CASE (bit 10) makes it
        // case-sensitive. Both CmpString and RelString use this table.
        (0x3C | 0x50, 0x0000) => OsRoutineVariant::TextCompareFoldCaseAndMarks,
        (0x3C | 0x50, 0x0200) => OsRoutineVariant::TextCompareFoldCase,
        (0x3C | 0x50, 0x0400) => OsRoutineVariant::TextCompareStripMarks,
        (0x3C | 0x50, 0x0600) => OsRoutineVariant::TextCompareExact,

        // Text 1993, pp. 5-64--5-65: bare UprString preserves marks while
        // MARKS (bit 9) strips them. Bit 10 has no documented meaning here.
        (0x54, 0x0000) => OsRoutineVariant::UpperStringPreserveMarks,
        (0x54, 0x0200) => OsRoutineVariant::UpperStringStripMarks,

        // Devices 1994, p. 1-16 and UI 3.4 Devices.h lines 905--1044 and
        // 1282--1415: these PB slots have exact Sync/Immed/Async declarations.
        // Slot $00 is excluded because $A200 also means PBHOpenSync/OpenSlotSync.
        (0x01..=0x06, 0x0000) => OsRoutineVariant::ParameterBlockSynchronous,
        (0x01..=0x06, 0x0200) => OsRoutineVariant::ParameterBlockImmediate,
        (0x01..=0x06, 0x0400) => OsRoutineVariant::ParameterBlockAsynchronous,

        // Processes 1994, pp. 3-18--3-20 and UI 3.4 Timer.h lines 74--100:
        // bit 10 selects InsXTime's extended, drift-free TMTask record.
        // Bit 9 has no documented meaning for this slot.
        (0x58, 0x0000) => OsRoutineVariant::TimeTaskOriginal,
        (0x58, 0x0400) => OsRoutineVariant::TimeTaskExtended,

        // Operating System Utilities 1994, pp. 8-27--8-31: bit 9 selects
        // the new typed form and bit 10 then selects the Toolbox table.
        // UI 3.4 Patches.h lines 80--231 declares the exact legacy, new-OS,
        // and new-Tool getter/setter words. Bit 10 alone is not declared.
        (0x46 | 0x47, 0x0000) => OsRoutineVariant::TrapAddressLegacy,
        (0x46 | 0x47, 0x0200) => OsRoutineVariant::TrapAddressNewOs,
        (0x46 | 0x47, 0x0600) => OsRoutineVariant::TrapAddressNewTool,

        // Operating System Utilities 1994, pp. 1-31--1-36 and UI 3.4
        // Gestalt.h lines 55--105: the three operations share slot $AD.
        // Bit 9 selects NewGestalt and bit 10 selects ReplaceGestalt; the
        // combined form remains unclassified. UI 3.4 Traps.h line 804 names
        // $A7AD as _GetGestaltProcPtr, but the reviewed sources do not define
        // its ABI or semantics.
        (0xAD, 0x0000) => OsRoutineVariant::GestaltQuery,
        (0xAD, 0x0200) => OsRoutineVariant::GestaltRegister,
        (0xAD, 0x0400) => OsRoutineVariant::GestaltReplace,

        // Devices 1994, pp. 6-18, 6-26, and 6-33; UI 3.4 Power.h lines
        // 447--461 and 705--731: bit 9 installs an A0-supplied SleepQRec and
        // bit 10 removes it. The combined form has no reviewed semantics.
        (0x8A, 0x0200) => OsRoutineVariant::SleepQueueInstall,
        (0x8A, 0x0400) => OsRoutineVariant::SleepQueueRemove,

        // Devices 1994, pp. 6-29--6-30 and 6-33--6-35; UI 3.4 Power.h
        // lines 650--701 and 733--791: bits 9 and 10 distinguish the three
        // Power Manager entry points sharing slot $85. The bare form has no
        // reviewed routine identity.
        (0x85, 0x0200) => OsRoutineVariant::PowerIdleUpdate,
        (0x85, 0x0400) => OsRoutineVariant::PowerIdleState,
        (0x85, 0x0600) => OsRoutineVariant::PowerSerial,

        // Devices 1994, pp. 1-83--1-85 and UI 3.4 Devices.h lines
        // 1109--1141: bit 10 selects DriverInstallReserveMem, which calls
        // ReserveMem before the shared DCE installation. Bit 9 and the
        // combined form have no reviewed meanings.
        (0x3D, 0x0000) => OsRoutineVariant::DriverInstall,
        (0x3D, 0x0400) => OsRoutineVariant::DriverInstallReserveMemory,

        // Files 1992 identifies bit 10 as ASYNC and bit 9 as newHFS. These
        // reviewed slots have exact basic Sync/Async declarations in UI 3.4
        // Files.h; do not extend the meanings to other OS-table slots.
        (
            0x07 | 0x08 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x10 | 0x11 | 0x12 | 0x13 | 0x14
            | 0x15 | 0x18 | 0x41 | 0x42 | 0x43 | 0x44 | 0x45,
            0x0000,
        ) => OsRoutineVariant::FileSynchronous,
        (
            0x07 | 0x08 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x10 | 0x11 | 0x12 | 0x13 | 0x14
            | 0x15 | 0x18 | 0x41 | 0x42 | 0x43 | 0x44 | 0x45,
            0x0400,
        ) => OsRoutineVariant::FileAsynchronous,

        // These slots additionally have exact PBH...Sync/PBH...Async words.
        // A200/PBHOpen is deliberately excluded because UI 3.4 also declares
        // that word as PBOpenImmed, so one label would overstate the evidence.
        (
            0x07 | 0x08 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x10 | 0x14 | 0x15 | 0x41 | 0x42,
            0x0200,
        ) => OsRoutineVariant::FileHfsSynchronous,
        (
            0x07 | 0x08 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x10 | 0x14 | 0x15 | 0x41 | 0x42,
            0x0600,
        ) => OsRoutineVariant::FileHfsAsynchronous,
        _ => OsRoutineVariant::Unclassified,
    }
}

/// One raw A-line word's table selection and preserved variant metadata.
///
/// The Trap Dispatcher must classify the complete word before masking it to
/// a table slot. Operating System words use bits 10--8 as routine/A0 flags
/// and bits 7--0 as the slot; Toolbox words use bit 10 as auto-pop and bits
/// 9--0 as the slot. Inside Macintosh: Operating System Utilities (1994),
/// pp. 8-10--8-15 and 8-20--8-21.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RawTrapRoute {
    pub(crate) raw_word: u16,
    pub(crate) canonical_word: u16,
    pub(crate) table_slot: u16,
    pub(crate) table_index: u16,
    pub(crate) table_address: u32,
    pub(crate) is_toolbox: bool,
    pub(crate) os_flags: u16,
    pub(crate) os_routine_variant: OsRoutineVariant,
    pub(crate) os_returns_a0: bool,
    pub(crate) toolbox_auto_pop: bool,
}

const EMPTY_RAW_TRAP_ROUTE: RawTrapRoute = RawTrapRoute {
    raw_word: 0,
    canonical_word: 0,
    table_slot: 0,
    table_index: 0,
    table_address: 0,
    is_toolbox: false,
    os_flags: 0,
    os_routine_variant: OsRoutineVariant::Unclassified,
    os_returns_a0: false,
    toolbox_auto_pop: false,
};

const fn generate_raw_trap_routes() -> [RawTrapRoute; 4096] {
    let mut routes = [EMPTY_RAW_TRAP_ROUTE; 4096];
    let mut low_word = 0u16;
    while low_word < 4096 {
        let raw_word = 0xA000 | low_word;
        let is_toolbox = (raw_word & 0x0800) != 0;
        let table_slot = if is_toolbox {
            raw_word & 0x03FF
        } else {
            raw_word & 0x00FF
        };
        routes[low_word as usize] = RawTrapRoute {
            raw_word,
            canonical_word: if is_toolbox {
                0xA800 | table_slot
            } else {
                0xA000 | table_slot
            },
            table_slot,
            table_index: if is_toolbox {
                OS_TRAP_TABLE_SLOTS + table_slot
            } else {
                table_slot
            },
            table_address: if is_toolbox {
                TOOLBOX_TRAP_TABLE_BASE + table_slot as u32 * 4
            } else {
                OS_TRAP_TABLE_BASE + table_slot as u32 * 4
            },
            is_toolbox,
            os_flags: if is_toolbox { 0 } else { raw_word & 0x0700 },
            os_routine_variant: classify_os_routine_variant(raw_word),
            os_returns_a0: !is_toolbox && (raw_word & 0x0100) != 0,
            toolbox_auto_pop: is_toolbox && (raw_word & 0x0400) != 0,
        };
        low_word += 1;
    }
    routes
}

/// Complete generated routing denominator for `$A000..=$AFFF`.
const RAW_TRAP_ROUTES: [RawTrapRoute; 4096] = generate_raw_trap_routes();

pub(crate) fn raw_trap_route(trap_word: u16) -> &'static RawTrapRoute {
    &RAW_TRAP_ROUTES[usize::from(trap_word & 0x0FFF)]
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

/// The observable target behind a raw trap-table long.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrapTableTarget {
    Direct(u32),
    Protected {
        last_head: u32,
        logical_successor: u32,
    },
}

/// Follow every protected come-from head and return the last mutable edge.
/// Trap Manager getters hide all permanent heads, while setters replace the
/// exit JMP target in the last head. Inside Macintosh: Operating System
/// Utilities (1994), pp. 8-8--8-9 and 8-27--8-31.
pub(crate) fn resolve_trap_table_target(
    raw_target: u32,
    mut read_long: impl FnMut(u32) -> Option<u32>,
) -> Option<TrapTableTarget> {
    let mut target = raw_target;
    let mut last_head = None;
    let mut visited = HashSet::new();
    loop {
        if read_long(target) != Some(COME_FROM_PATCH_SIGNATURE) {
            return Some(match last_head {
                Some(last_head) => TrapTableTarget::Protected {
                    last_head,
                    logical_successor: target,
                },
                None => TrapTableTarget::Direct(target),
            });
        }
        if !visited.insert(target) {
            return None;
        }
        last_head = Some(target);
        target = read_long(target.checked_add(4)?)?;
    }
}

/// Trap-table topology selected for the emulated Mac OS 8.1 machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrapTableProfile {
    M68k68040,
    PowerPc604,
}

/// Profile-specific classification layered over the generated raw-word map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProfileTrapRoute {
    pub(crate) raw: RawTrapRoute,
    pub(crate) default_is_unimplemented: bool,
    pub(crate) has_permanent_come_from: bool,
    pub(crate) default_gateway_word: u16,
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

// Permanent come-from heads observed in the selected Mac OS 8.1 profiles.
// Trap Manager APIs expose and replace the successor behind each head, while
// the raw low-memory table continues to contain the head itself. IM:OSUtils
// (1994), pp. 8-8--8-9 and 8-25--8-31.
const M68K_68040_COME_FROM_TRAPS: &[u16] = &[
    0xA002, 0xA003, 0xA008, 0xA00A, 0xA012, 0xA023, 0xA024, 0xA030, 0xA031, 0xA054, 0xA078, 0xA823,
    0xA869, 0xA873, 0xA879, 0xA88B, 0xA893, 0xA89C, 0xA8A1, 0xA8B5, 0xA8CF, 0xA8E4, 0xA8E5, 0xA8EA,
    0xA8EC, 0xA905, 0xA908, 0xA909, 0xA90A, 0xA90C, 0xA90D, 0xA91E, 0xA924, 0xA956, 0xA972, 0xA999,
    0xA9A0, 0xA9A2, 0xA9C9, 0xA9DC, 0xA9E1, 0xA9ED, 0xA9EF, 0xAA00, 0xAA1F, 0xAA27, 0xAA43, 0xAA4E,
    0xAAFB,
];

const POWERPC_604_COME_FROM_TRAPS: &[u16] = &[0xA823, 0xA851, 0xA996, 0xA999, 0xAAFB];

// Reviewed default-address aliases observed repeatedly in the selected 68040
// profile. Universal Interfaces 3.4 assigns CloseCPort to $AA02 while Inside
// Macintosh Volume V, V-72/V-291 records its earlier $A87D entry. The same
// profile also shares the compound-handle disposal routine used by
// DisposePixPat and DisposeCCursor; IM:V V-55/V-63 shows their common leading
// map/data/expanded-data/expanded-map layout. The selected 604 profile exposes
// distinct addresses for all four slots, so these aliases are profile-local.
const M68K_68040_DEFAULT_GATEWAY_ALIASES: &[(u16, u16)] = &[(0xAA02, 0xA87D), (0xAA26, 0xAA08)];

// Both selected Mac OS 8.1 profiles identify the modern 1,024-entry Toolbox
// table's `$AA6E` slot as the `Unimplemented` logical identity. All other
// captured slots have callable defaults. IM:OSUtils (1994), pp. 8-22 and
// 8-32; IM:Overview (1992), pp. 9-14--9-15.
const MAC_OS_81_UNIMPLEMENTED_TRAPS: &[u16] = &[0xAA6E];

impl TrapTableProfile {
    fn come_from_traps(self) -> &'static [u16] {
        match self {
            Self::M68k68040 => M68K_68040_COME_FROM_TRAPS,
            Self::PowerPc604 => POWERPC_604_COME_FROM_TRAPS,
        }
    }

    fn unimplemented_traps(self) -> &'static [u16] {
        match self {
            Self::M68k68040 | Self::PowerPc604 => MAC_OS_81_UNIMPLEMENTED_TRAPS,
        }
    }

    fn default_gateway_word(self, canonical_word: u16) -> u16 {
        match self {
            Self::M68k68040 => M68K_68040_DEFAULT_GATEWAY_ALIASES
                .iter()
                .find_map(|&(alias, target)| (alias == canonical_word).then_some(target))
                .unwrap_or(canonical_word),
            Self::PowerPc604 => canonical_word,
        }
    }

    pub(crate) fn route(self, trap_word: u16) -> ProfileTrapRoute {
        let raw = *raw_trap_route(trap_word);
        ProfileTrapRoute {
            raw,
            default_is_unimplemented: self.unimplemented_traps().contains(&raw.canonical_word),
            has_permanent_come_from: self.come_from_traps().contains(&raw.canonical_word),
            default_gateway_word: self.default_gateway_word(raw.canonical_word),
        }
    }
}

/// Trap dispatcher with resource fork access and emulator state.
pub struct TrapDispatcher {
    /// Synthetic keyboard and mouse entries exposed by the ADB Manager.
    pub(crate) adb: crate::adb::AdbManager,
    /// Process-owned Resource Manager state shared by attached CPU adapters.
    process_resource_manager: SharedProcessResourceManager,
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
    /// Synthetic AppleEvent descriptors currently visible to guest
    /// handlers. Key is the guest address of the AEDesc record.
    pub(crate) ae_events: HashMap<u32, SyntheticAppleEvent>,
    /// Non-event AEDesc records currently visible to guest AppleEvent code.
    /// Key is the guest address of the AEDesc record.
    pub(crate) ae_descriptors: HashMap<u32, AeDescriptor>,
    /// Shared descriptor-list/record backing keyed by AEDesc data pointer.
    /// AE records are often copied by value; the copied descriptor record
    /// keeps the same data handle, so keyed fields must follow that backing
    /// rather than the stack address of a single AEDesc variable.
    pub(crate) ae_descriptor_backing: HashMap<u32, AeDescriptor>,
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
    /// Address of the lazily-allocated trampoline used by the Menu Manager to
    /// call an application MDEF procedure.
    pub(crate) menu_def_trampoline: u32,
    /// Reusable trampoline cells for multi-control CDEF callback chains.
    pub(crate) control_def_trampoline_chain: Vec<u32>,
    /// Address of the lazily-allocated trampoline used by DeferUserFn
    /// to call a callable userFunction immediately. Holds
    /// `48E7 F0F0 207C xxxx xxxx 4EB9 xxxx xxxx 4CDF 0F0F 7000 4E75`.
    pub(crate) defer_user_fn_trampoline: u32,
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
    /// Nesting depth for the guest's critical sections. Systemless runs the
    /// HLE on one host thread, so Thread Manager critical sections collapse
    /// to a single dispatcher-wide counter.
    pub(crate) thread_critical_nesting: u32,
    /// Cooperative Thread Manager contexts keyed by their opaque ThreadID.
    pub(crate) cooperative_threads: HashMap<u32, CooperativeThread>,
    /// Round-robin queue of ready cooperative threads.
    pub(crate) cooperative_thread_ready: VecDeque<u32>,
    /// ThreadID whose register context is currently installed in the CPU.
    pub(crate) current_cooperative_thread: u32,
    /// Next guest-visible ThreadID. IDs 1 and 2 are reserved by Threads.h.
    pub(crate) next_cooperative_thread_id: u32,
    /// Guest trampoline entered when a ThreadEntryProc returns.
    pub(crate) thread_return_trampoline: u32,
    /// Custom `ThreadSchedulerProcPtr` installed by `SetThreadScheduler`.
    pub(crate) cooperative_thread_scheduler: u32,
    /// Default cooperative stack size reported by
    /// `GetDefaultThreadStackSize` and used when `NewThread` is passed 0.
    pub(crate) cooperative_thread_stack_size: u32,
    /// Cooperative stacks banked by `CreateThreadPool` and recycled by
    /// `DisposeThread`, reused by `NewThread` before allocating.
    pub(crate) cooperative_thread_pool: Vec<(u32, u32)>,
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
    /// `sent_open_app_event` (which tracks the synthetic OAPP queued
    /// for `WaitNextEvent` delivery): an app may call
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
    /// Cache of allocated OS-trap trampolines for GetTrapAddress.
    /// Each entry is a 4-byte allocation containing the canonical OS trap
    /// word followed by RTS. Direct calls preserve the JSR return address on
    /// the stack while the register-based trap runs. Calls originating at a
    /// cached trampoline bypass a later patch so saved-old pointers continue
    /// to reach the original system routine. Inside Macintosh: Operating
    /// System Utilities (1994), pp. 8-23--8-30.
    pub(crate) os_trap_trampolines: HashMap<u16, u32>,
    /// Cache of allocated tool-trap trampolines for GetTrapAddress.
    /// Each entry is a 2-byte allocation containing the auto-pop
    /// variant of the canonical tool-trap word. When the guest does
    /// `JSR (trampoline)` the dispatcher pops the saved return PC,
    /// runs the trap, and resumes at the JSR caller — see
    /// [`Self::get_or_create_tool_trap_trampoline`]. Inside Macintosh Volume
    /// II, II-384 (NGetTrapAddress); IM:V V-577 (auto-pop bit).
    pub(crate) tool_trap_trampolines: HashMap<u16, u32>,
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
    /// Directory catalog metadata keyed by normalized path.
    pub(crate) vfs_directories: SharedProcessValue<HashMap<String, VfsDirectory>>,
    /// Reverse directory lookup by catalog ID.
    pub(crate) vfs_directory_paths: SharedProcessValue<HashMap<u32, String>>,
    /// Read-only disk-image volumes mounted alongside the synthetic boot volume.
    pub(crate) vfs_volumes: SharedProcessValue<HashMap<i16, VfsVolume>>,
    /// Mounted volume lookup by normalized, case-insensitive name.
    pub(crate) vfs_volume_names: SharedProcessValue<HashMap<String, i16>>,
    /// Open working directories keyed by working directory reference number.
    pub(crate) working_directories: HashMap<i16, WorkingDirectory>,
    /// Open file table: refnum -> filename
    pub(crate) open_files: HashMap<u16, String>,
    /// Synthetic Device Manager drivers opened by name via PBOpen/OpenDriver.
    pub(crate) synthetic_drivers: HashMap<u16, String>,
    /// Guest SndChannel storage used by writes to the ROM Sound Driver
    /// reference number (-4). Allocated lazily on the first StartSound write.
    pub(crate) legacy_sound_driver_channel: Option<u32>,
    /// Refnums opened with write permission (fsRdWrPerm=3 or fsWrPerm=2).
    /// Used to enforce opWrErr (-49) per IM:Files 9578.
    pub(crate) write_refnums: std::collections::HashSet<u16>,
    /// File position table: refnum -> current byte offset
    pub(crate) file_positions: HashMap<u16, usize>,
    /// Most recent successful PBRead/FSRead from a data fork.
    pub(crate) recent_file_read: Option<RecentFileRead>,
    /// Completed asynchronous File Manager requests awaiting `ioResult`
    /// publication and optional completion-procedure delivery.
    pub(crate) pending_file_completions: VecDeque<PendingFileCompletion>,
    /// Set of VFS keys whose `ioFlAttrib` lock bit is set.
    /// Maintained by SetFilLock/HSetFLock ($A041/$A241) and
    /// RstFilLock/HRstFLock ($A042/$A242); read by
    /// `fill_file_catalog_info` to set bit 0 of `ioFlAttrib`.
    /// Files 1992, 2-205 (`ioFlAttrib` field), 9302..9352 (HSetFLock/HRstFLock).
    /// Public to mirror `vfs`/`vfs_rsrc` so frontends and tests can
    /// inspect or seed lock state directly.
    pub locked_files: SharedProcessValue<std::collections::HashSet<String>>,
    /// Next available file reference number
    pub(crate) next_refnum: u16,
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
    pub(crate) next_working_dir_refnum: i16,
    /// Normalized VFS path of the launched application, if known.
    pub(crate) launched_app_path: Option<String>,
    /// Foreground application launch queued by LaunchApplication. When
    /// `after_event_yield` is set, the runner starts it after the current
    /// app next yields through WaitNextEvent/EventAvail/GetNextEvent.
    pub(crate) pending_launch_app: Option<PendingLaunchApplication>,
    /// Current default directory.
    pub(crate) default_dir_id: SharedProcessValue<u32>,
    /// Working directory reference number for the application's folder.
    pub(crate) app_wd_refnum: i16,
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
    /// Current hilite color (RGBColor: R, G, B). Set by HiliteColor
    /// ($AA22) and conceptually stored in the cGrafPort's grafVars
    /// handle per IM:V V-149. Systemless uses a single dispatcher field
    /// since most apps only have one cGrafPort active at a time.
    pub hilite_color: (u16, u16, u16),
    /// Operation color (RGBColor: R, G, B) for arithmetic transfer
    /// modes (addPin, subPin, blend). Set by OpColor ($AA21) and
    /// stored in the grafVars handle's rgbOpColor field per IM:V V-77.
    /// Initialized to black per IM:V V-63.
    pub op_color: (u16, u16, u16),
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
    /// Simulated tick count
    pub(crate) tick_count: u32,
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
    pub(crate) window_saved_under_pixels: HashMap<u32, (i16, i16, i16, i16, Vec<u8>)>,
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
    pub(crate) window_list: Vec<u32>,
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
    /// Custom popup MDEF state before its returned rectangle creates a pane.
    pub(crate) menu_definition_tracking: Option<crate::menu_manager::MenuDefinitionTracking>,
    /// GetNewMBar result and remaining menus while custom mSizeMsg callbacks run.
    pub(crate) pending_menu_bar_build: Option<super::menu::PendingMenuBarBuild>,
    /// Caller QuickDraw state restored after a retained custom MDEF finishes.
    pub(crate) menu_definition_port_state: Option<PortStateSnapshot>,
    /// 68k call frame parked while the shared Menu Manager state yields.
    pub(crate) menu_tracking_stack_ptr: u32,
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
    /// Active DragWindow tracking state.
    pub(crate) window_tracking: Option<WindowTrackingState>,
    /// Active TrackGoAway close-box tracking state.
    pub(crate) go_away_tracking: Option<GoAwayTrackingState>,
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
    /// Whether the synthetic kAEOpenApplication event has been delivered.
    /// On a real Mac, the Finder sends this Apple Event at launch.
    /// Macintosh Toolbox Essentials 1992, p. 5-90
    pub(crate) sent_open_app_event: bool,
    /// Whether the application's `'SIZE'` resource declares
    /// `isHighLevelEventAware`. The Process Manager defaults this capability
    /// to false when the resource or flag is absent.
    /// Macintosh Toolbox Essentials 1992, pp. 2-30 to 2-32.
    pub(crate) application_high_level_event_aware: bool,
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
    pub(crate) current_gdevice: SharedProcessValue<u32>,
    /// Current GrafPort/GWorld pointer
    pub(crate) current_port: SharedProcessValue<u32>,
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
    /// Pixel-state flags keyed by offscreen PixMapHandle. Tracks the
    /// `keepLocal`, `pixelsPurgeable`, and `pixelsLocked` subset surfaced by
    /// GetPixelsState / SetPixelsState and the direct LockPixels /
    /// UnlockPixels aliases. Imaging With QuickDraw 1994, 6-36..6-38.
    pub(crate) gworld_pixel_states: HashMap<u32, u32>,
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
    /// Runtime device CLUT for 8bpp mode. 256 entries of [R, G, B] in 16-bit Mac values.
    /// Initialized to the standard Mac 8-bit system palette. Updated by SetEntries trap
    /// and low-level video driver cscSetEntries. Used for DISPLAY rendering only.
    pub device_clut: SharedProcessValue<[[u16; 3]; 256]>,
    /// Per-channel transfer tables installed by the video driver's
    /// `cscSetGamma` control call. These affect presentation only; the device
    /// and Color Manager CLUTs retain the guest's uncorrected 16-bit values.
    pub device_gamma: SharedProcessValue<crate::display::DisplayGamma>,
    pub device_gamma_explicit: SharedProcessValue<bool>,
    /// Color Manager CLUT for 8bpp mode. Updated only by high-level SetEntries ($AA3F)
    /// and ActivatePalette — NOT by low-level video driver palette fades.
    /// Used by QuickDraw shape drawing (PaintRect, etc.) for RGB→index mapping,
    /// mirroring the real Mac OS ITable which is derived from the Color Manager palette.
    /// Imaging With QuickDraw 1994, p. 4-82
    pub color_manager_clut: SharedProcessValue<[[u16; 3]; 256]>,
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
    /// Pre-initialization mirror mapping a canonical trap word to a native 68K
    /// handler. SetTrapAddress keeps it coherent for focused unit tests. It is
    /// cleared when a process table is activated; initialized runners dispatch
    /// exclusively from writable guest cells. Native handlers execute as
    /// simulated JSR targets, allowing CRT-installed LoadSeg, UnloadSeg, and
    /// ExitToShell patches to relocate code normally.
    pub(crate) native_trap_table: TrapWordMap,
    /// Whether the selected profile's two raw trap tables have been written
    /// into guest low memory. Before application initialization, focused trap
    /// unit tests retain the host-map fallback; afterwards the guest longs are
    /// authoritative, including writes performed directly by native code.
    pub(crate) trap_tables_materialized: bool,
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
    /// Active Standard File Package save dialog tracking state.
    pub(crate) standard_file_put_tracking: Option<StandardFilePutTrackingState>,
    /// Active Standard File Package open dialog tracking state.
    pub(crate) standard_file_get_tracking: Option<StandardFileGetTrackingState>,
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
    pub(crate) dialog_saved_pixels: HashMap<u32, Vec<u8>>,
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
    /// Clean GrafPort baseline established before a dialog's first userItem
    /// callback. Dialog Manager drawing must not contaminate later callbacks.
    pub(crate) dialog_user_item_port_states: HashMap<u32, PortStateSnapshot>,
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
        &self.process_resource_manager
    }
}

impl std::ops::DerefMut for TrapDispatcher {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.process_resource_manager
    }
}

impl TrapDispatcher {
    /// Access the process-owned Sound Manager state.
    pub fn sound_manager(&self) -> &crate::sound::SoundManager {
        &self.sound_manager
    }

    /// Mutably access the process-owned Sound Manager state.
    pub fn sound_manager_mut(&mut self) -> &mut crate::sound::SoundManager {
        &mut self.sound_manager
    }

    /// Attach shared process resources to this dispatcher.
    pub(crate) fn attach_process_context(&mut self, context: &mut ProcessContext) {
        context.attach_file_system(&mut self.process_file_system);
        context.attach_resource_manager(&mut self.process_resource_manager);
        context.attach_sound_manager(&mut self.sound_manager);
        context.attach_callback_tasks(
            &mut self.timer_tasks,
            &mut self.vbl_tasks,
            &mut self.callback_scheduling,
        );
        context.attach_scrap_state(&mut self.scrap);
        context.attach_control_manager(&mut self.control_manager);
        context.attach_list_manager(&mut self.list_states);
        context.attach_text_edit_manager(&mut self.textedit_states);
        context.attach_dialog_text(&mut self.param_text);
        context.attach_cursor_state(&mut self.cursor_state);
        context.attach_quickdraw_selection(&mut self.current_port, &mut self.current_gdevice);
        context.attach_display_color_state(
            &mut self.device_clut,
            &mut self.color_manager_clut,
            &mut self.device_gamma,
            &mut self.device_gamma_explicit,
        );
        context.attach_event_queue(&mut self.event_queue);
        context.attach_input_state(&mut self.input_state);
        context.attach_menu_tracking(&mut self.menu_tracking);
        context.attach_classic_file_system(&mut self.vfs, &mut self.vfs_rsrc);
        context.attach_classic_vfs_catalogue(
            &mut self.vfs_metadata,
            &mut self.vfs_directories,
            &mut self.vfs_directory_paths,
            &mut self.vfs_volumes,
            &mut self.vfs_volume_names,
            &mut self.locked_files,
            &mut self.next_vfs_dir_id,
            &mut self.next_vfs_volume_ref_num,
            &mut self.next_vfs_file_id,
            &mut self.next_vfs_timestamp,
            &mut self.default_dir_id,
        );
        self.process_file_system.publish_classic_vfs_catalogue();
        let mut memory_manager = None;
        context.attach_memory_manager(&mut memory_manager);
        self.attach_memory_manager_handle(
            memory_manager.expect("process context supplies a Memory Manager"),
        );
        context.attach_native_menu_selection(&mut self.pending_native_menu_selection);
        context.attach_guest_calls(&mut self.guest_calls);
        context.attach_apple_event_handlers(&mut self.ae_handlers);
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
            target.adopt_handle_metadata(&mut standalone);
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
        let result = memory_manager
            .borrow_mut()
            .replace_native_handle_bytes(bus, handle, expected_ptr, bytes);
        match result {
            Ok((old_ptr, new_ptr)) => {
                self.untrack_handle_ptr(old_ptr);
                self.track_handle_ptr(new_ptr, handle);
                bus.write_word(crate::memory::globals::addr::MEM_ERR, 0);
                true
            }
            Err(error) => {
                bus.write_word(
                    crate::memory::globals::addr::MEM_ERR,
                    error as u16,
                );
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
            frame, self.tick_count, self.trap_count, safe_label
        );
        let path = dir.join(&filename);
        let mut rgba = crate::display::render_screen_with_gamma(
            bus,
            self.screen_mode,
            &self.device_clut,
            &self.device_gamma,
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
                self.tick_count,
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
        *self.current_port = port;
    }

    /// Test-only: invoke save_dialog_pixels for the byte-isomorphism gate.
    /// Used by tests asserting the bulk path returns the same bytes the
    /// per-pixel reference would have produced.
    pub fn save_dialog_pixels_for_test(
        &self,
        bus: &MacMemoryBus,
        rect: (i16, i16, i16, i16),
    ) -> Vec<u8> {
        self.save_dialog_pixels(bus, rect)
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
        self.restore_dialog_pixels(bus, rect, saved);
    }

    /// Test-only: set the tick count.
    /// Production code uses advance_guest_tick() to update tick_count.
    pub fn set_tick_count_for_test(&mut self, tick: u32) {
        self.tick_count = tick;
    }

    /// Test-only: mark the synthetic kAEOpenApplication event as
    /// already delivered so the next GetNextEvent/WaitNextEvent
    /// returns a real null event instead of the boot-time oapp stub.
    pub fn set_sent_open_app_event_for_test(&mut self, sent: bool) {
        self.sent_open_app_event = sent;
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

        {
            let resources = self.resources.get_or_insert_with(|| LoadedResources {
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
        }
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
        if let Some(resources) = self.resources.as_mut() {
            let file = resources.files.entry(refnum).or_default();
            file.named
                .insert((res_type, name.to_string()), (id, data_ptr));
            file.names_by_id.insert((res_type, id), name.to_string());
        }
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
            self.trace_sink
                .as_mut()
                .expect("trace_sink checked above")
                .record_snapshot(
                    bus,
                    self.screen_mode,
                    &self.device_clut,
                    self.screen_event_count,
                    self.tick_count,
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
            tick: self.tick_count,
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
        key_map_key_is_down(&self.input_state.key_map, key_code)
    }

    pub(crate) fn key_map_bytes(&self) -> &[u8; 16] {
        &self.input_state.key_map
    }

    pub(crate) fn current_event_modifiers(&self) -> u16 {
        const BTN_STATE: u16 = 128;
        const CMD_KEY: u16 = 256;
        const SHIFT_KEY: u16 = 512;
        const ALPHA_LOCK: u16 = 1024;
        const OPTION_KEY: u16 = 2048;
        const CONTROL_KEY: u16 = 4096;

        let mut modifiers = 0u16;
        if !self.input_state.mouse_button {
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
        let key_map = if self.input_state.key_map.iter().any(|&byte| byte != 0) {
            self.input_state.key_map
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join("")
        } else {
            "none".to_string()
        };
        format!(
            "state=mouse=({},{}) button={} live_modifiers=${:04X} key_map={} tracking=menu:{} dialog:{} control:{}",
            self.input_state.mouse_pos.0,
            self.input_state.mouse_pos.1,
            if self.input_state.mouse_button { "down" } else { "up" },
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
        let mut vfs_directories = HashMap::new();
        let mut vfs_directory_paths = HashMap::new();
        vfs_directories.insert(
            String::new(),
            VfsDirectory {
                dir_id: 2,
                parent_dir_id: 1,
                // The root directory's catalog name is the volume name.
                // Files 1992, 2-27 and 2-85.
                name: BOOT_VOLUME_NAME.to_string(),
            },
        );
        vfs_directory_paths.insert(2, String::new());

        let mut dispatcher = Self {
            adb: crate::adb::AdbManager::new(),
            process_resource_manager: SharedProcessResourceManager::default(),
            process_file_system: SharedProcessFileSystem::default(),
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
            ae_events: HashMap::new(),
            ae_descriptors: HashMap::new(),
            ae_descriptor_backing: HashMap::new(),
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
            menu_def_trampoline: 0,
            control_def_trampoline_chain: Vec::new(),
            defer_user_fn_trampoline: 0,
            qddone_seen_ports: HashSet::new(),
            pict_info_ids: HashSet::new(),
            ppc_initialized: false,
            thread_critical_nesting: 0,
            cooperative_threads: HashMap::new(),
            cooperative_thread_ready: VecDeque::new(),
            current_cooperative_thread: 2,
            next_cooperative_thread_id: 3,
            thread_return_trampoline: 0,
            cooperative_thread_scheduler: 0,
            cooperative_thread_stack_size: DEFAULT_COOPERATIVE_THREAD_STACK_SIZE,
            cooperative_thread_pool: Vec::new(),
            synthetic_component_instances: HashSet::new(),
            next_synthetic_component_instance: 0x00C1_0001,
            saved_draw_old_regions: HashMap::new(),
            fired_oapp_handler: false,
            system_str_cache: HashMap::new(),
            system_intl_cache: HashMap::new(),
            system_cursor_cache: HashMap::new(),
            system_clut_cache: HashMap::new(),
            system_wctb_cache: HashMap::new(),
            system_kchr_cache: HashMap::new(),
            system_kmap_cache: HashMap::new(),
            system_wdef_cache: HashMap::new(),
            system_mdef_cache: HashMap::new(),
            os_trap_trampolines: HashMap::new(),
            tool_trap_trampolines: HashMap::new(),
            std_pix_gateway: 0,
            param_text: SharedProcessDialogText::default(),
            ui_theme_id: UiThemeId::ClassicSystem7,
            vfs: SharedProcessValue::default(),
            vfs_rsrc: SharedProcessValue::default(),
            vfs_metadata: SharedProcessValue::default(),
            vfs_directories: SharedProcessValue::from_value(vfs_directories),
            vfs_directory_paths: SharedProcessValue::from_value(vfs_directory_paths),
            vfs_volumes: SharedProcessValue::default(),
            vfs_volume_names: SharedProcessValue::default(),
            working_directories: HashMap::new(),
            open_files: HashMap::new(),
            synthetic_drivers: HashMap::new(),
            legacy_sound_driver_channel: None,
            write_refnums: HashSet::new(),
            file_positions: HashMap::new(),
            recent_file_read: None,
            pending_file_completions: VecDeque::new(),
            locked_files: SharedProcessValue::default(),
            next_refnum: 100,
            mmu_mode: 1,                      // true32b — 32-bit addressing by default
            default_video_rec: 0x0000,        // no default video device selected
            default_os_rec: 0x0001,           // Macintosh Operating System
            default_startup_rec: 0x0000_0000, // zero-filled first-device startup default
            next_vfs_dir_id: SharedProcessValue::from_value(16),
            next_vfs_volume_ref_num: SharedProcessValue::from_value(-2),
            next_vfs_file_id: SharedProcessValue::from_value(32),
            next_vfs_timestamp: SharedProcessValue::from_value(1),
            next_working_dir_refnum: 32,
            launched_app_path: None,
            pending_launch_app: None,
            default_dir_id: SharedProcessValue::from_value(2),
            app_wd_refnum: BOOT_VOLUME_REF_NUM,
            output_dir: None,
            fg_color: (0, 0, 0),
            bg_color: (0xFFFF, 0xFFFF, 0xFFFF),
            pm_fg_color: None,
            pm_bg_color: None,
            makergbpat_colors: HashMap::new(),
            // Default HiliteRGB used before an application calls HiliteColor.
            // The System 7.5.3 BasiliskII reference resolves this to EV's
            // darker selected-list green rather than a saturated primary.
            hilite_color: (0x0000, 0x8000, 0x0000),
            op_color: (0x0000, 0x0000, 0x0000),
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
            tick_count: 0,
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
            window_list: Vec::new(),
            fullscreen_locked: false,
            menu_bar_policy: crate::runner::MenuBarPolicy::GuestControlled,
            initial_kiosk_guest_hide_observed: false,
            menu_bar_hidden: false,
            sound_manager: SharedProcessSoundManager::default(),
            menus: Vec::new(),
            menu_tracking: SharedProcessMenuTracking::default(),
            guest_calls: SharedGuestCallStack::default(),
            menu_definition_tracking: None,
            pending_menu_bar_build: None,
            menu_definition_port_state: None,
            menu_tracking_stack_ptr: 0,
            pending_native_menu_selection: SharedNativeMenuSelection::default(),
            pending_native_menu_event: None,
            pending_native_menu_event_tick: None,
            control_tracking: None,
            window_tracking: None,
            go_away_tracking: None,
            grow_window_tracking: None,
            region_tracking: None,
            underline_info: None,
            input_state: SharedProcessInputState::default(),
            debug_getkeys_nonzero_count: 0,
            debug_last_getkeys_nonzero_key_map: [0; 16],
            debug_key_event_delivery_count: 0,
            debug_last_key_event_message: 0,
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
            sent_open_app_event: false,
            application_high_level_event_aware: false,
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
            current_gdevice: SharedProcessValue::from_value(0),
            current_port: SharedProcessValue::from_value(0),
            port_draw_states: HashMap::new(),
            resolved_port_color_fields: HashMap::new(),
            gworld_devices: HashMap::new(),
            disposed_gworld_portbits: HashMap::new(),
            gworld_pixel_states: HashMap::new(),
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
            device_clut: SharedProcessValue::from_value(Self::standard_mac_8bpp_clut()),
            device_gamma: SharedProcessValue::from_value(crate::display::default_display_gamma()),
            device_gamma_explicit: SharedProcessValue::from_value(false),
            color_manager_clut: SharedProcessValue::from_value(Self::standard_mac_8bpp_clut()),
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
            explicit_palette_ctabs: HashSet::new(),
            icon_transform_override: 0,
            printing_error: 0,
            next_ct_seed: 1,
            fill_black_override: None,
            recording_picture: None,
            recording_picture_bitmap: None,
            native_trap_table: TrapWordMap::default(),
            trap_tables_materialized: false,
            trap_table_profile: None,
            trap_exception_vector_defaults: None,
            pending_native_trap_calls: HashMap::new(),
            bits_proc_reentry: None,
            timer_tasks: Default::default(),
            callback_scheduling: Default::default(),
            sleep_queue: Vec::new(),
            vbl_tasks: Default::default(),
            dialog_tracking: None,
            standard_file_put_tracking: None,
            standard_file_get_tracking: None,
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
            dialog_user_item_port_states: HashMap::new(),
            retained_modal_dialog_click: None,
            pending_modal_button_dispose_dialog: None,
            window_stack: Vec::new(),
            saved_vis_regions: HashMap::new(),
            list_states: SharedProcessListManager::default(),
            textedit_states: SharedProcessTextEditManager::default(),
            control_manager: SharedProcessControlManager::default(),
            last_inserted_menu_id: None,
            pending_dialog_popup_menu: None,
            dialog_item_popup_menus: HashMap::new(),
            dialog_popup_original_rects: HashMap::new(),
            dialog_popup_candidate_items: HashSet::new(),
            scrap: SharedProcessScrapState::default(),
            last_init_pack_id: None,
        };
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
        self.is_menu_definition_callback_pending_with_tracking(self.menu_tracking.as_ref())
    }

    pub(crate) fn is_menu_definition_callback_pending_with_tracking(
        &self,
        menu_tracking: Option<&ProcessMenuTrackingState>,
    ) -> bool {
        self.pending_menu_bar_build.is_some()
            || self
                .menu_tracking
                .as_ref()
                .or(menu_tracking)
                .and_then(crate::menu_manager::MenuTrackingState::active_definition)
                .or(self.menu_definition_tracking.as_ref())
                .is_some_and(|tracking| tracking.pending_invocation().is_some())
    }

    /// Shared check used by both dispatch.rs (auto-pop push-back) and
    /// runner.rs (PC rewind for refire). Returns true when the given trap
    /// word should refire next frame because one of the synchronous Toolbox
    /// tracking loops is active and the trap is the matching routine. Strips
    /// the auto-pop bit (0x0400) so auto-pop variants match too.
    pub fn is_tracking_refire(&self, opcode: u16) -> bool {
        self.is_tracking_refire_with_menu_tracking(opcode, self.is_menu_tracking())
    }

    pub(crate) fn is_tracking_refire_with_menu_tracking(
        &self,
        opcode: u16,
        is_menu_tracking: bool,
    ) -> bool {
        let trap_no_autopop = opcode & !0x0400;
        let is_menu_refire = trap_no_autopop == 0xA93D || trap_no_autopop == 0xA80B;
        let is_menu_bar_build_refire = trap_no_autopop == 0xA9C0;
        let is_dialog_refire =
            matches!(trap_no_autopop, 0xA991 | 0xA985 | 0xA986 | 0xA987 | 0xA988);
        let is_standard_file_refire = trap_no_autopop == 0xA9EA;
        let is_control_refire = trap_no_autopop == 0xA968;
        let is_window_refire = trap_no_autopop == 0xA925;
        let is_go_away_refire = trap_no_autopop == 0xA91E;
        let is_grow_window_refire = trap_no_autopop == 0xA92B;
        let is_region_refire = matches!(trap_no_autopop, 0xA905 | 0xA926);
        (is_menu_refire && is_menu_tracking)
            || (is_menu_bar_build_refire && self.pending_menu_bar_build.is_some())
            || (is_dialog_refire && self.is_dialog_tracking())
            || (is_standard_file_refire
                && (self.is_standard_file_put_tracking() || self.is_standard_file_get_tracking()))
            || (is_control_refire && self.is_control_tracking())
            || (is_window_refire && self.is_window_tracking())
            || (is_go_away_refire && self.is_go_away_tracking())
            || (is_grow_window_refire && self.is_grow_window_tracking())
            || (is_region_refire && self.is_region_tracking())
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

    /// 4-bit-per-channel inverse table (16x16x16 = 4096 cells) precomputed
    /// from `standard_mac_8bpp_clut`. Each cell holds the CLUT index
    /// whose entry is closest (by Euclidean distance in 16-bit RGB) to
    /// the centre of that cube cell.
    ///
    /// This is the cached form the Mac ROM uses via MakeITable: built
    /// once per GDevice from the GDevice's CTab, then consulted by
    /// CopyBits/DrawPicture for every pixel mapping. CRITICALLY, the
    /// ITable is NOT rebuilt when SetEntries modifies the active CLUT —
    /// the active CLUT controls how stored framebuffer indices DISPLAY
    /// (palette lookup at scan-out), but the ITable controls which
    /// dst index gets WRITTEN to the framebuffer (during DrawPicture).
    ///
    /// Systemless's prior `closest_clut_index` re-runs full-precision
    /// closest-match against `device_clut` per pixel, which produces
    /// different dst indices than the ROM whenever device_clut has
    /// drifted from the System palette via SetEntries fades.
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

    /// Resolve an RGB color through the main 8-bit screen GDevice's cached
    /// inverse table. Exact endpoints retain QuickDraw's conventional white
    /// and black pixels; all other colors use the four-bit device lookup.
    pub(crate) fn standard_screen_itable_index(rgb: [u16; 3]) -> u8 {
        if rgb == [0xFFFF; 3] {
            0
        } else if rgb == [0; 3] {
            255
        } else {
            Self::standard_itable_lookup(rgb[0], rgb[1], rgb[2])
        }
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

    fn encode_hfs_component_for_vfs(component: &str) -> String {
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
        let key = normalized.to_ascii_lowercase();
        if let Some(ref_num) = self.vfs_volume_names.get(&key).copied() {
            return ref_num;
        }

        let root_dir_id = self.ensure_vfs_directory(&normalized);
        let mut ref_num = *self.next_vfs_volume_ref_num;
        while ref_num == 0
            || ref_num == Self::boot_volume_ref_num()
            || self.vfs_volumes.contains_key(&ref_num)
        {
            ref_num = ref_num.saturating_sub(1);
        }
        *self.next_vfs_volume_ref_num = ref_num.saturating_sub(1);
        self.vfs_volumes.insert(
            ref_num,
            VfsVolume {
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
            },
        );
        self.vfs_volume_names.insert(key, ref_num);
        self.process_file_system.publish_classic_vfs_volume(ref_num);
        ref_num
    }

    pub(crate) fn vfs_volume_by_name(&self, name: &str) -> Option<&VfsVolume> {
        let key = Self::normalize_vfs_path(name).to_ascii_lowercase();
        self.vfs_volume_names
            .get(&key)
            .and_then(|ref_num| self.vfs_volumes.get(ref_num))
    }

    pub(crate) fn vfs_volume_for_ref_num(&self, ref_num: i16) -> Option<&VfsVolume> {
        self.vfs_volumes.get(&ref_num)
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

    fn allocate_vfs_timestamp(&mut self) -> u32 {
        let timestamp = *self.next_vfs_timestamp;
        *self.next_vfs_timestamp = self.next_vfs_timestamp.saturating_add(1);
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
        if let Some(dir_id) = self
            .vfs_directories
            .get(&normalized)
            .map(|directory| directory.dir_id)
        {
            self.process_file_system
                .publish_classic_vfs_directory(&normalized);
            return dir_id;
        }

        let parent_path = Self::vfs_parent_path(&normalized).to_string();
        let parent_dir_id = self.ensure_vfs_directory(&parent_path);
        let dir_id = *self.next_vfs_dir_id;
        *self.next_vfs_dir_id = self.next_vfs_dir_id.saturating_add(1);

        self.vfs_directories.insert(
            normalized.clone(),
            VfsDirectory {
                dir_id,
                parent_dir_id,
                name: Self::vfs_basename(&normalized).to_string(),
            },
        );
        self.vfs_directory_paths.insert(dir_id, normalized.clone());
        self.process_file_system
            .publish_classic_vfs_directory(&normalized);
        dir_id
    }

    pub(crate) fn ensure_vfs_file_metadata(&mut self, path: &str) {
        let normalized = Self::normalize_vfs_path(path);
        if normalized.is_empty() {
            return;
        }
        if self.vfs_metadata.contains_key(&normalized) {
            self.process_file_system
                .publish_classic_vfs_metadata(&normalized);
            return;
        }

        let parent_path = Self::vfs_parent_path(&normalized).to_string();
        let parent_dir_id = self.ensure_vfs_directory(&parent_path);
        let timestamp = self.allocate_vfs_timestamp();
        self.vfs_metadata.insert(
            normalized.clone(),
            VfsMetadata {
                file_id: *self.next_vfs_file_id,
                parent_dir_id,
                file_type: u32::from_be_bytes(*b"????"),
                creator: u32::from_be_bytes(*b"????"),
                finder_flags: 0,
                created_date: timestamp,
                modified_date: timestamp,
            },
        );
        *self.next_vfs_file_id = self.next_vfs_file_id.saturating_add(1);
        self.process_file_system
            .publish_classic_vfs_metadata(&normalized);
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
        if let Some(metadata) = self.vfs_metadata.get_mut(&normalized) {
            metadata.file_type = u32::from_be_bytes(file_type);
            metadata.creator = u32::from_be_bytes(creator);
            metadata.finder_flags = finder_flags;
        }
        self.process_file_system
            .publish_classic_vfs_metadata(&normalized);
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
        if let Some(metadata) = self.vfs_metadata.get_mut(&normalized) {
            metadata.file_type = file_type;
            metadata.creator = creator;
            metadata.finder_flags = finder_flags;
        }
        self.process_file_system
            .publish_classic_vfs_metadata(&normalized);
    }

    pub(crate) fn set_launched_app_path(&mut self, name: &str) {
        let normalized = Self::normalize_vfs_path(name);
        self.ensure_vfs_file_metadata(&normalized);
        if let Some(metadata) = self.vfs_metadata.get(&normalized).copied() {
            *self.default_dir_id = metadata.parent_dir_id;
            self.process_file_system.default_dir_id = metadata.parent_dir_id;
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
                self.app_wd_refnum = wd_ref;
            }
        }
        self.launched_app_path = Some(normalized);
    }

    pub fn launched_app_path(&self) -> Option<&str> {
        self.launched_app_path.as_deref()
    }

    pub fn materialize_quilt_resources(&mut self) -> usize {
        let (materialized_count, synthesized_files) =
            crate::managers::resource::quilt::materialize_quilt_resources_for_vfs(
                &self.vfs,
                &mut self.vfs_rsrc,
            );
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
        if let Some(metadata) = self.vfs_metadata.get_mut(&normalized) {
            metadata.modified_date = timestamp;
            if metadata.created_date == 0 {
                metadata.created_date = timestamp;
            }
        }
        self.process_file_system
            .publish_classic_vfs_metadata(&normalized);
    }

    pub(crate) fn remove_vfs_entry_metadata(&mut self, name: &str) {
        let normalized = Self::normalize_vfs_path(name);
        self.vfs_metadata.remove(&normalized);
    }

    pub(crate) fn publish_vfs_entry_to_process(&mut self, name: &str) {
        let normalized = Self::normalize_vfs_path(name);
        self.process_file_system
            .publish_classic_vfs_metadata(&normalized);
    }

    pub(crate) fn remove_vfs_entry_from_process(&mut self, name: &str) {
        let normalized = Self::normalize_vfs_path(name);
        self.process_file_system.remove_classic_vfs_path(&normalized);
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

        let directory_keys: Vec<String> = self
            .vfs_directories
            .keys()
            .filter(|key| *key == &normalized || key.starts_with(&prefix))
            .cloned()
            .collect();
        for key in directory_keys {
            if let Some(directory) = self.vfs_directories.remove(&key) {
                self.vfs_directory_paths.remove(&directory.dir_id);
                removed = true;
            }
        }

        self.process_file_system.remove_classic_vfs_path(&normalized);

        removed
    }

    pub fn remove_vfs_path_relative_to_launched_app(&mut self, name: &str) -> bool {
        if self.remove_vfs_path(name) {
            return true;
        }

        let Some(app_path) = self.launched_app_path.clone() else {
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
        self.vfs_directory_paths.get(&dir_id).map(String::as_str)
    }

    pub(crate) fn directory_entry_for_id(&self, dir_id: u32) -> Option<&VfsDirectory> {
        self.vfs_directory_paths
            .get(&dir_id)
            .and_then(|path| self.vfs_directories.get(path))
    }

    pub(crate) fn resolve_volume_ref_num(&self, vref: i16) -> i16 {
        if vref == 0 {
            return Self::boot_volume_ref_num();
        }
        if vref == Self::boot_volume_ref_num() {
            return vref;
        }
        if self.vfs_volumes.contains_key(&vref) {
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
            if let Some(volume) = self.vfs_volumes.get(&vref) {
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

        let mut ref_num = self.next_working_dir_refnum;
        while self.working_directories.contains_key(&ref_num) {
            ref_num = ref_num.saturating_add(1);
        }
        self.next_working_dir_refnum = ref_num.saturating_add(1);
        self.working_directories.insert(
            ref_num,
            WorkingDirectory {
                ref_num,
                volume_ref_num,
                dir_id: effective_dir_id,
                proc_id,
            },
        );
        Some(ref_num)
    }

    pub(crate) fn close_working_directory(&mut self, wd_ref_num: i16) -> bool {
        self.working_directories.remove(&wd_ref_num).is_some()
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
        let normalized = Self::normalize_hfs_path(name);
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
        let normalized = Self::normalize_hfs_path(name);
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
        let normalized = Self::normalize_hfs_path(name);
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
        // Sort vfs_directories keys before first-match .find() to avoid
        // leaking HashMap hash-randomisation into directory resolution
        // (and from there into resource load order, allocation order, and
        // tick advancement under realtime cadence).
        let mut sorted_keys: Vec<&String> = self.vfs_directories.keys().collect();
        sorted_keys.sort_unstable();
        if let Some(dir_path) = self.directory_path_for_id(dir_id) {
            let candidate = if dir_path.is_empty() {
                normalized.clone()
            } else {
                format!("{dir_path}/{normalized}")
            };
            if let Some(found) = sorted_keys
                .iter()
                .copied()
                .find(|path| path.eq_ignore_ascii_case(&candidate))
            {
                return Some(found.clone());
            }
        }
        if normalized.contains('/') {
            return sorted_keys
                .iter()
                .copied()
                .find(|path| path.eq_ignore_ascii_case(&normalized))
                .cloned();
        }
        None
    }

    pub(crate) fn list_vfs_catalog_entries(&mut self, dir_id: u32) -> Vec<VfsCatalogEntry> {
        self.ensure_vfs_catalog();
        let mut entries = Vec::new();
        let effective_dir_id = if self.vfs_directory_paths.contains_key(&dir_id) {
            dir_id
        } else {
            2
        };

        // Iterate vfs_directories in path-sorted order so the entries Vec is
        // built deterministically. The final entries.sort_by_key(name) below
        // only sorts by name (case-insensitive), so two directories with the
        // same name but different paths would otherwise land in HashMap-random
        // order.
        let mut dir_paths: Vec<&String> = self.vfs_directories.keys().collect();
        dir_paths.sort_unstable();
        for path in dir_paths {
            let Some(directory) = self.vfs_directories.get(path) else {
                continue;
            };
            if path.is_empty() || directory.parent_dir_id != effective_dir_id {
                continue;
            }
            entries.push(VfsCatalogEntry {
                path: path.clone(),
                name: directory.name.clone(),
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
        self.input_state.mouse_pos = (v, h);
        self.adb.note_mouse_state(self.input_state.mouse_pos, self.input_state.mouse_button);
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
        self.input_state.mouse_button = true;
        self.input_state.mouse_pos = (v, h);
        self.adb.note_mouse_state(self.input_state.mouse_pos, self.input_state.mouse_button);
        let modifiers = self.current_event_modifiers();
        self.event_queue.push_back(QueuedEvent {
            what: 1, // mouseDown
            message: 0,
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
        self.input_state.mouse_pos = (v, h);
        self.input_state.mouse_button = false;
        self.adb.note_mouse_state(self.input_state.mouse_pos, self.input_state.mouse_button);
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
        self.event_queue.push_back(QueuedEvent {
            what: 2, // mouseUp
            message: 0,
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
            if self.input_state.caps_lock_physically_pressed {
                return;
            }
            self.input_state.caps_lock_physically_pressed = true;
            // Caps Lock latches on one physical press and releases on the
            // next. Inside Macintosh Volume I (1985), p. I-34.
            let latched = !self.key_is_down(key_code);
            set_key_map_key(&mut self.input_state.key_map, key_code, latched);
        } else {
            if self.key_is_down(key_code) {
                return;
            }
            set_key_map_key(&mut self.input_state.key_map, key_code, true);
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
        self.event_queue.push_back(QueuedEvent {
            what: 3, // keyDown
            message,
            where_v: self.input_state.mouse_pos.0,
            where_h: self.input_state.mouse_pos.1,
            modifiers,
        });

        if Self::key_generates_auto_key(key_code) {
            // Auto-key timing defaults are 16 ticks for the first repeat and
            // 4 ticks thereafter. Inside Macintosh Volume I, I-246.
            self.input_state.key_repeat = Some(ProcessKeyRepeatState {
                key_code,
                char_code,
                next_tick: self.tick_count.wrapping_add(Self::AUTO_KEY_THRESHOLD_TICKS),
            });
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
            self.input_state.caps_lock_physically_pressed = false;
        } else {
            set_key_map_key(&mut self.input_state.key_map, key_code, false);
        }
        if self
            .input_state
            .key_repeat
            .is_some_and(|repeat| repeat.key_code == key_code)
        {
            self.input_state.key_repeat = None;
        }
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
            self.event_queue.push_back(QueuedEvent {
                what: 4, // keyUp
                message,
                where_v: self.input_state.mouse_pos.0,
                where_h: self.input_state.mouse_pos.1,
                modifiers,
            });
        }
    }

    /// Get the current cursor data for rendering overlay.
    pub fn cursor(&self) -> Option<&CursorImage> {
        if self.cursor_state.visible() {
            self.cursor_state.image.as_ref()
        } else {
            None
        }
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
        self.cursor_state.level
    }

    /// Whether a cursor image is installed, independent of visibility.
    pub fn cursor_data_present(&self) -> bool {
        self.cursor_state.image.is_some()
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
        self.cursor_state.image.as_ref().map(CursorImage::mono_parts)
    }

    /// Get the current mouse position.
    pub fn mouse_position(&self) -> (i16, i16) {
        self.input_state.mouse_pos
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
            let ptr = if self.policy.res_load && res.attrs & RES_PRELOAD_ATTR != 0 {
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
        self.resident_resources.extend(
            file.loaded
                .iter()
                .filter(|(_, ptr)| **ptr != 0)
                .map(|(&(res_type, res_id), _)| (refnum, res_type, res_id)),
        );
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
        self.resource_backing_data
            .insert((refnum, res_type, res_id), data);
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

    pub(crate) fn resource_handle_memory_size(
        &self,
        bus: &MacMemoryBus,
        handle: u32,
        ptr: u32,
    ) -> Option<u32> {
        let size = bus.get_alloc_size(ptr)?;
        if let Some((refnum, res_type, res_id)) = self.resource_record_for_handle(handle) {
            if let Some(data) = self.resource_backing_data.get(&(refnum, res_type, res_id)) {
                // A resource handle's logical size is the resource's own data
                // length. The block behind it is padded out to a 4-byte
                // boundary, but that padding is physical size and GetHandleSize
                // reports the logical one.
                // GetHandleSize ($A025)
                // FUNCTION GetHandleSize (h: Handle): Size;
                // Inside Macintosh Volume II, II-31; Memory 1992, 2-32
                // ("the logical size ... not the physical size").
                //
                // Rounding up here hands callers that walk a resource as an
                // array a phantom trailing element. SimCity 2000's far-model
                // runtime derives its CREL relocation count as
                // GetHandleSize/2, so a rounded-up size added a zero-offset
                // entry and relocated the CODE segment's own header, leaving
                // its jump-table entries permanently unpatched.
                return Some(data.len() as u32);
            }
        }
        Some(size)
    }

    pub(crate) fn forget_resource_backing_data(
        &mut self,
        refnum: u16,
        res_type: [u8; 4],
        res_id: i16,
    ) {
        self.resource_backing_data
            .remove(&(refnum, res_type, res_id));
    }

    pub(crate) fn remember_resource_fork_backing_data(&mut self, refnum: u16, fork: &ResourceFork) {
        for ((res_type, res_id), resource) in fork.resources() {
            self.resource_backing_data
                .entry((refnum, *res_type, *res_id))
                .or_insert_with(|| resource.data.clone());
        }
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
        self.resource_backing_data
            .retain(|(entry_refnum, _, _), _| *entry_refnum != refnum);
    }

    pub(crate) fn remember_resource_handle_index(
        &mut self,
        handle: u32,
        refnum: u16,
        res_type: [u8; 4],
        res_id: i16,
    ) {
        self.resource_handles_by_key
            .insert((refnum, res_type, res_id), handle);
    }

    pub(crate) fn forget_resource_handle_index_for_handle(&mut self, handle: u32) {
        let Some((_, res_type, res_id)) = self.loaded_handles.get(&handle).copied() else {
            return;
        };
        let Some(refnum) = self.resource_handle_files.get(&handle).copied() else {
            return;
        };
        self.resource_handles_by_key
            .remove(&(refnum, res_type, res_id));
    }

    pub(crate) fn unload_resource_live_map_entry_for_handle(&mut self, handle: u32) {
        let Some((ptr, res_type, res_id)) = self.loaded_handles.get(&handle).copied() else {
            return;
        };
        let Some(refnum) = self.resource_handle_files.get(&handle).copied() else {
            return;
        };
        let Some(file) = self
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
    }

    pub(crate) fn clear_resource_file_handle_index(&mut self, refnum: u16) {
        self.resource_handles_by_key
            .retain(|(entry_refnum, _, _), _| *entry_refnum != refnum);
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
        let mut selected = 0;
        if let Some(resources) = self.resources.as_mut() {
            resources.current_file = if resources.files.contains_key(&refnum) {
                refnum
            } else {
                0
            };
            selected = resources.current_file;
        }
        *self.current_resource_file = selected as i16;
        bus.write_word(0x0A5A, self.current_resource_refnum());
    }

    pub(crate) fn set_resource_file_name(&mut self, refnum: u16, name: impl Into<String>) {
        if let Some(resources) = self.resources.as_mut() {
            resources.names.insert(refnum, name.into());
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
        let refnum = self.next_refnum;
        self.next_refnum += 1;
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

        let mut file_ptrs: HashSet<u32> = HashSet::new();
        let mut externally_referenced_ptrs: HashSet<u32> = HashSet::new();
        let mut closed_name: Option<String> = None;
        let mut closed = false;
        let closing_current = self.current_resource_refnum() == refnum;

        let mut surviving_classic_current = 0;
        if let Some(resources) = self.resources.as_mut() {
            if !resources.files.contains_key(&refnum) {
                return false;
            }

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
            closed_name = resources.names.remove(&refnum);
            surviving_classic_current = resources.current_file;
            closed = true;
        }
        if closing_current {
            *self.current_resource_file = surviving_classic_current as i16;
        }
        self.clear_resource_file_backing_data(refnum);
        self.resource_file_order.remove(&refnum);
        self.clear_resource_file_handle_index(refnum);
        self.resident_resources
            .retain(|(entry_refnum, _, _)| *entry_refnum != refnum);

        if !closed {
            return false;
        }

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
            self.loaded_handles.remove(handle);
            self.resource_handle_files.remove(handle);
            self.detached_handle_files.remove(handle);
            self.detached_handles.remove(handle);
            self.remove_handle_state_bits(*handle);
        }

        let detached_handles: Vec<u32> = self
            .detached_handle_files
            .iter()
            .filter_map(|(&handle, &handle_refnum)| (handle_refnum == refnum).then_some(handle))
            .collect();
        for handle in detached_handles {
            self.detached_handle_files.remove(&handle);
        }

        self.write_refnums.remove(&refnum);
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
            -16096 => Some(b"\x0EMacintosh User"),
            // Macintosh Name (Sharing Setup, AppleTalk identity)
            -16413 => Some(b"\x09Macintosh"),
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
        let canonical_trap_word = raw_trap_route(0xA800 | (trap_word & 0x03FF)).canonical_word;
        if let Some(&addr) = self.tool_trap_trampolines.get(&canonical_trap_word) {
            // The returned address represents a ROM trap entry. Re-seed the
            // synthetic instruction on every lookup so guest code that
            // temporarily writes through the saved address cannot corrupt a
            // later JSR through the same trap pointer.
            bus.write_readonly_code_word(addr, canonical_trap_word | 0x0400);
            return addr;
        }
        let addr = bus.alloc_synthetic(2);
        bus.write_readonly_code_word(addr, canonical_trap_word | 0x0400);
        bus.protect_readonly_code(addr, 2);
        self.tool_trap_trampolines.insert(canonical_trap_word, addr);
        addr
    }

    /// Allocate a stable callable gateway for one Operating System trap-table
    /// slot. OS traps use register conventions, so the JSR return address stays
    /// at the top of the stack while the canonical A-line executes and the
    /// following RTS returns to the caller.
    pub(crate) fn get_or_create_os_trap_trampoline(
        &mut self,
        bus: &mut MacMemoryBus,
        trap_word: u16,
    ) -> u32 {
        let canonical_trap_word = raw_trap_route(0xA000 | (trap_word & 0x00FF)).canonical_word;
        if let Some(&addr) = self.os_trap_trampolines.get(&canonical_trap_word) {
            bus.write_readonly_code_word(addr, canonical_trap_word);
            bus.write_readonly_code_word(addr + 2, 0x4E75);
            return addr;
        }
        let addr = bus.alloc_synthetic(4);
        bus.write_readonly_code_word(addr, canonical_trap_word);
        bus.write_readonly_code_word(addr + 2, 0x4E75);
        bus.protect_readonly_code(addr, 4);
        self.os_trap_trampolines.insert(canonical_trap_word, addr);
        addr
    }

    fn canonical_trap_word(trap_word: u16) -> u16 {
        raw_trap_route(trap_word).canonical_word
    }

    fn raw_trap_table_entry(trap_word: u16) -> u32 {
        raw_trap_route(trap_word).table_address
    }

    fn default_trap_gateway(&self, trap_word: u16) -> Option<u32> {
        let canonical = Self::canonical_trap_word(trap_word);
        let gateway_word = self.trap_table_profile.map_or(canonical, |profile| {
            profile.route(canonical).default_gateway_word
        });
        if (canonical & 0x0800) != 0 {
            self.tool_trap_trampolines.get(&gateway_word).copied()
        } else {
            self.os_trap_trampolines.get(&gateway_word).copied()
        }
    }

    fn write_readonly_code_long(bus: &mut MacMemoryBus, address: u32, value: u32) {
        bus.write_readonly_code_word(address, (value >> 16) as u16);
        bus.write_readonly_code_word(address + 2, value as u16);
    }

    /// Allocate the stable system-owned head used by a permanent come-from
    /// patch. Its first four bytes are the Trap Manager signature
    /// `$60064EF9`: normal entry branches around the exit JMP to a minimal
    /// patch body, which then branches back to that same logical successor.
    fn create_trap_come_from_head(&mut self, bus: &mut MacMemoryBus, successor: u32) -> u32 {
        let head = bus.alloc_synthetic(10);
        bus.write_readonly_code_word(head, 0x6006); // BRA.S patch body
        bus.write_readonly_code_word(head + 2, 0x4EF9); // exit JMP absolute
        Self::write_readonly_code_long(bus, head + 4, successor);
        bus.write_readonly_code_word(head + 8, 0x60F8); // patch body: BRA.S exit JMP
        bus.protect_readonly_code(head, 10);
        head
    }

    /// Allocate the protected restart gateway named by a generated exception
    /// vector. Line-A and line-F faults stack the faulting instruction's PC;
    /// RTE therefore retries it after the writable vector cell has been
    /// restored. Ordinary generated-default dispatch is recognized before
    /// entering this gateway.
    fn create_exception_vector_gateway(bus: &mut MacMemoryBus) -> u32 {
        let gateway = bus.alloc_synthetic(2);
        bus.write_readonly_code_word(gateway, 0x4E73); // RTE
        bus.protect_readonly_code(gateway, 2);
        gateway
    }

    /// Create an inactive, profile-complete table for a new process.
    /// Permanent come-from heads belong to this context, while callable
    /// gateways remain system-owned and may be shared by every process.
    pub(crate) fn create_trap_table_process_context(
        &mut self,
        bus: &mut MacMemoryBus,
        profile: TrapTableProfile,
    ) -> TrapTableProcessContext {
        let mut raw_entries =
            Vec::with_capacity(usize::from(OS_TRAP_TABLE_SLOTS + TOOLBOX_TRAP_TABLE_SLOTS));
        let unimplemented_gateway = self.get_or_create_tool_trap_trampoline(bus, 0xAA6E);
        for slot in 0..OS_TRAP_TABLE_SLOTS {
            let trap_word = 0xA000 | slot;
            let route = profile.route(trap_word);
            let default = if route.default_is_unimplemented {
                self.os_trap_trampolines
                    .insert(trap_word, unimplemented_gateway);
                unimplemented_gateway
            } else {
                self.get_or_create_os_trap_trampoline(bus, route.default_gateway_word)
            };
            let default = self
                .native_trap_table
                .get(&trap_word)
                .copied()
                .unwrap_or(default);
            raw_entries.push(if route.has_permanent_come_from {
                self.create_trap_come_from_head(bus, default)
            } else {
                default
            });
        }
        for slot in 0..TOOLBOX_TRAP_TABLE_SLOTS {
            let trap_word = 0xA800 | slot;
            let route = profile.route(trap_word);
            let default = if route.default_is_unimplemented {
                self.tool_trap_trampolines
                    .insert(trap_word, unimplemented_gateway);
                unimplemented_gateway
            } else {
                self.get_or_create_tool_trap_trampoline(bus, route.default_gateway_word)
            };
            let default = self
                .native_trap_table
                .get(&trap_word)
                .copied()
                .unwrap_or(default);
            raw_entries.push(if route.has_permanent_come_from {
                self.create_trap_come_from_head(bus, default)
            } else {
                default
            });
        }
        let default_exception_vectors = [
            Self::create_exception_vector_gateway(bus),
            Self::create_exception_vector_gateway(bus),
        ];
        TrapTableProcessContext {
            profile,
            raw_entries,
            raw_exception_vectors: default_exception_vectors,
            default_exception_vectors,
            pending_native_trap_calls: HashMap::new(),
            current_trap_caller: None,
        }
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
        self.trap_tables_materialized = true;
        self.native_trap_table = TrapWordMap::default();
        outgoing
    }

    /// Discard the active application's trap context during process teardown.
    pub(crate) fn teardown_trap_table_process_context(&mut self) {
        self.native_trap_table = TrapWordMap::default();
        self.pending_native_trap_calls.clear();
        self.current_trap_caller = None;
        self.trap_table_profile = None;
        self.trap_exception_vector_defaults = None;
        self.trap_tables_materialized = false;
    }

    /// Materialize the selected machine profile's complete raw Trap Manager
    /// tables. Every default entry is a stable, callable gateway into its HLE
    /// implementation. The table storage itself remains writable; generated
    /// gateways and permanent come-from heads are protected.
    pub(crate) fn materialize_trap_tables(
        &mut self,
        bus: &mut MacMemoryBus,
        profile: TrapTableProfile,
    ) {
        let context = self.create_trap_table_process_context(bus, profile);
        let _ = self.switch_trap_table_process_context(bus, context);
    }

    /// Whether low-memory exception vector 10 still names this process's
    /// generated A-line dispatcher identity. Before a process topology is
    /// materialized, retain the historical direct-HLE behavior used by
    /// focused manager tests.
    pub(crate) fn aline_vector_is_default(&self, bus: &MacMemoryBus) -> bool {
        self.trap_exception_vector_defaults
            .is_none_or(|defaults| bus.read_long(0x28) == defaults[0])
    }

    /// Whether low-memory exception vector 11 still names this process's
    /// generated line-F handler identity.
    pub(crate) fn fline_vector_is_default(&self, bus: &MacMemoryBus) -> bool {
        self.trap_exception_vector_defaults
            .is_none_or(|defaults| bus.read_long(0x2C) == defaults[1])
    }

    /// Return the logical address currently selected by a materialized raw
    /// table entry. Trap Manager getters use this view even when the address
    /// is the default gateway; dispatch uses [`Self::native_trap_handler`] to
    /// distinguish that default from an installed patch.
    pub(crate) fn trap_table_address(&self, bus: &MacMemoryBus, trap_word: u16) -> Option<u32> {
        if !self.trap_tables_materialized {
            return None;
        }
        let canonical = Self::canonical_trap_word(trap_word);
        let installed = bus.read_long(Self::raw_trap_table_entry(canonical));
        Some(
            match resolve_trap_table_target(installed, |address| Some(bus.read_long(address))) {
                Some(TrapTableTarget::Direct(target)) => target,
                Some(TrapTableTarget::Protected {
                    logical_successor, ..
                }) => logical_successor,
                None => installed,
            },
        )
    }

    /// Return the current non-default handler for a canonical trap slot.
    /// Once low-memory tables exist, their bytes are the source of truth so a
    /// guest can patch a trap with an ordinary longword store.
    pub(crate) fn native_trap_handler(&self, bus: &MacMemoryBus, trap_word: u16) -> Option<u32> {
        let canonical = Self::canonical_trap_word(trap_word);
        if self.trap_tables_materialized {
            let logical = self.trap_table_address(bus, canonical)?;
            if self.default_trap_gateway(canonical) == Some(logical) {
                None
            } else {
                Some(logical)
            }
        } else {
            self.native_trap_table.get(&canonical).copied()
        }
    }

    pub(crate) fn install_trap_address(
        &mut self,
        bus: &mut MacMemoryBus,
        trap_word: u16,
        handler: u32,
    ) {
        let canonical = Self::canonical_trap_word(trap_word);
        let protected_tail = if self.trap_tables_materialized {
            let raw = bus.read_long(Self::raw_trap_table_entry(canonical));
            match resolve_trap_table_target(raw, |address| Some(bus.read_long(address))) {
                Some(TrapTableTarget::Direct(_)) => None,
                Some(TrapTableTarget::Protected { last_head, .. }) => Some(last_head),
                None => return,
            }
        } else {
            None
        };
        if self.trap_tables_materialized {
            let entry = if self.default_trap_gateway(canonical) == Some(handler) {
                self.default_trap_gateway(canonical).unwrap_or(handler)
            } else {
                handler
            };
            let raw_entry = Self::raw_trap_table_entry(canonical);
            if let Some(last_head) = protected_tail {
                Self::write_readonly_code_long(bus, last_head + 4, entry);
            } else {
                bus.write_long(raw_entry, entry);
            }
        } else if self.default_trap_gateway(canonical) == Some(handler) {
            self.native_trap_table.remove(&canonical);
        } else {
            self.native_trap_table.insert(canonical, handler);
        }
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

    /// Whether guest execution is currently inside any routine installed by
    /// the Trap Manager. The runner keeps these uncommon calls on exact
    /// instruction boundaries so an ordinary RTS can retire its continuation.
    pub(crate) fn has_pending_native_trap_call(&self) -> bool {
        !self.pending_native_trap_calls.is_empty()
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
        resources
            .files
            .get(&refnum)
            .and_then(|file| file.named.get(&(res_type, name.to_string())).copied())
            .map(|(id, ptr)| (refnum, id, ptr))
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
        if let Some(app_path) = self.launched_app_path.as_ref() {
            self.vfs
                .insert(format!("__rsrc__{}", app_path), fork.serialized().to_vec());
        }
        let file = self.allocate_resource_fork(fork, bus);
        self.remember_preloaded_resource_residency(0, &file);
        self.resource_file_order
            .insert(0, Self::resource_reference_order(fork));
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
        self.resources = Some(LoadedResources {
            files,
            names: HashMap::from([(0, "Application".to_string())]),
            search_order: vec![0],
            current_file: 0,
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
        let resources = self.resources.get_or_insert_with(|| LoadedResources {
            files: HashMap::new(),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        resources.files.insert(refnum, file);
        if !resources.search_order.contains(&refnum) {
            resources.search_order.push(refnum);
        }
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
        let resources = self.resources.get_or_insert_with(|| LoadedResources {
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
        let order = self.resource_file_order.entry(refnum).or_default();
        for key in incoming_order {
            if !order.contains(&key) {
                order.push(key);
            }
        }
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
        self.resource_file_order
            .insert(refnum, Self::resource_reference_order(fork));
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

    /// Find a file in vfs_rsrc by name (exact match, then basename match).
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
                self.tick_count,
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
                    trap, trap_pc, self.tick_count, sp
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
            if self.tick_count >= lo && self.tick_count <= hi {
                eprintln!(
                    "[ATRAP-WIN] tick={} trap=${:04X} pc=${:08X}",
                    self.tick_count,
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
            && self
                .os_trap_trampolines
                .get(&input_base_trap)
                .is_some_and(|&addr| pc == addr + 2);
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
        // menu/dialog tracking loops (synthetic HLE re-dispatches), and
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
            && (self
                .tool_trap_trampolines
                .get(&base_trap)
                .is_some_and(|&addr| pc == addr + 2)
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
            if let Some(handler_addr) = self.native_trap_handler(bus, base_trap) {
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
                self.dispatch_sound(is_tool, trap_num, cpu, bus)
                    .map(|result| {
                        selected_adapter = TrapAdapterId::Sound;
                        result
                    })
            })
            .or_else(|| {
                self.dispatch_toolbox(is_tool, trap_num, cpu, bus)
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
        // menu/dialog refire traps (matches the runner's is_tracking_refire
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
mod tests {
    use super::*;
    use crate::cpu::{CpuOps, Register};
    use crate::trap::menu::test_tracked_menu_state;
    use crate::trap::test_helpers::setup;
    use std::collections::VecDeque;

    #[test]
    fn generated_raw_trap_routes_cover_every_a_line_word_exactly() {
        for low_word in 0u16..0x1000 {
            let word = 0xA000 | low_word;
            let route = raw_trap_route(word);
            assert_eq!(route.raw_word, word);
            if (word & 0x0800) != 0 {
                let slot = word & 0x03FF;
                assert!(route.is_toolbox);
                assert_eq!(route.table_slot, slot);
                assert_eq!(route.table_index, OS_TRAP_TABLE_SLOTS + slot);
                assert_eq!(
                    route.table_address,
                    TOOLBOX_TRAP_TABLE_BASE + u32::from(slot) * 4
                );
                assert_eq!(route.canonical_word, 0xA800 | slot);
                assert_eq!(route.os_flags, 0);
                assert!(!route.os_returns_a0);
                assert_eq!(route.toolbox_auto_pop, (word & 0x0400) != 0);
            } else {
                let slot = word & 0x00FF;
                assert!(!route.is_toolbox);
                assert_eq!(route.table_slot, slot);
                assert_eq!(route.table_index, slot);
                assert_eq!(
                    route.table_address,
                    OS_TRAP_TABLE_BASE + u32::from(slot) * 4
                );
                assert_eq!(route.canonical_word, 0xA000 | slot);
                assert_eq!(route.os_flags, word & 0x0700);
                assert_eq!(route.os_returns_a0, (word & 0x0100) != 0);
                assert!(!route.toolbox_auto_pop);
            }
        }
    }

    #[test]
    fn raw_routes_classify_only_source_backed_os_routine_variants() {
        use OsRoutineVariant::{
            CurrentHeap, CurrentHeapClear, DriverInstall, DriverInstallReserveMemory,
            FileAsynchronous, FileHfsAsynchronous, FileHfsSynchronous, FileSynchronous,
            GestaltQuery, GestaltRegister, GestaltReplace, LowerText, ParameterBlockAsynchronous,
            ParameterBlockImmediate, ParameterBlockSynchronous, PowerIdleState, PowerIdleUpdate,
            PowerSerial, SleepQueueInstall, SleepQueueRemove, StripText, StripUpperText,
            SystemHeap, SystemHeapClear, TextCompareExact, TextCompareFoldCase,
            TextCompareFoldCaseAndMarks, TextCompareStripMarks, TimeTaskExtended, TimeTaskOriginal,
            TrapAddressLegacy, TrapAddressNewOs, TrapAddressNewTool, Unclassified,
            UpperStringPreserveMarks, UpperStringStripMarks, UpperText,
        };

        // Inside Macintosh: Memory (1992), pp. 2-31 and 2-35; Universal
        // Interfaces 3.4 MacMemory.h lines 436--485 and 550--599.
        for slot in [0x1Eu16, 0x22] {
            for return_a0 in [0x0000u16, 0x0100] {
                for (routine_bits, expected) in [
                    (0x0000, CurrentHeap),
                    (0x0200, CurrentHeapClear),
                    (0x0400, SystemHeap),
                    (0x0600, SystemHeapClear),
                ] {
                    assert_eq!(
                        raw_trap_route(0xA000 | slot | return_a0 | routine_bits).os_routine_variant,
                        expected
                    );
                }
            }
        }

        // Inside Macintosh: Operating System Utilities (1994), pp. 8-27--8-31
        // and 8-32--8-33; UI 3.4 Patches.h lines 80--231.
        for slot in [0x46u16, 0x47] {
            for return_a0 in [0x0000u16, 0x0100] {
                assert_eq!(
                    raw_trap_route(0xA000 | slot | return_a0).os_routine_variant,
                    TrapAddressLegacy
                );
                assert_eq!(
                    raw_trap_route(0xA200 | slot | return_a0).os_routine_variant,
                    TrapAddressNewOs
                );
                assert_eq!(
                    raw_trap_route(0xA600 | slot | return_a0).os_routine_variant,
                    TrapAddressNewTool
                );
                assert_eq!(
                    raw_trap_route(0xA400 | slot | return_a0).os_routine_variant,
                    Unclassified,
                    "bit 10 without the new-system bit is undeclared"
                );
            }
        }

        // Inside Macintosh: Operating System Utilities (1994),
        // pp. 1-31--1-36; UI 3.4 Gestalt.h lines 55--105.
        for return_a0 in [0x0000u16, 0x0100] {
            assert_eq!(
                raw_trap_route(0xA0AD | return_a0).os_routine_variant,
                GestaltQuery
            );
            assert_eq!(
                raw_trap_route(0xA2AD | return_a0).os_routine_variant,
                GestaltRegister
            );
            assert_eq!(
                raw_trap_route(0xA4AD | return_a0).os_routine_variant,
                GestaltReplace
            );
            assert_eq!(
                raw_trap_route(0xA6AD | return_a0).os_routine_variant,
                Unclassified,
                "combined Gestalt Manager modifier bits are undeclared"
            );
        }

        // Inside Macintosh: Devices (1994), pp. 1-83--1-85; UI 3.4
        // Devices.h lines 1109--1141 declares DriverInstall $A03D and
        // DriverInstallReserveMem $A43D.
        for return_a0 in [0x0000u16, 0x0100] {
            assert_eq!(
                raw_trap_route(0xA03D | return_a0).os_routine_variant,
                DriverInstall
            );
            assert_eq!(
                raw_trap_route(0xA43D | return_a0).os_routine_variant,
                DriverInstallReserveMemory
            );
            assert_eq!(
                raw_trap_route(0xA23D | return_a0).os_routine_variant,
                Unclassified
            );
            assert_eq!(
                raw_trap_route(0xA63D | return_a0).os_routine_variant,
                Unclassified
            );
        }

        // Inside Macintosh: Devices (1994), pp. 6-18, 6-26, and 6-33;
        // UI 3.4 Power.h lines 447--461 and 705--731.
        for return_a0 in [0x0000u16, 0x0100] {
            assert_eq!(
                raw_trap_route(0xA28A | return_a0).os_routine_variant,
                SleepQueueInstall
            );
            assert_eq!(
                raw_trap_route(0xA48A | return_a0).os_routine_variant,
                SleepQueueRemove
            );
            assert_eq!(
                raw_trap_route(0xA68A | return_a0).os_routine_variant,
                Unclassified,
                "combined sleep-queue modifier bits have no reviewed semantics"
            );
        }

        // Inside Macintosh: Devices (1994), pp. 6-29--6-30 and 6-33--6-35;
        // UI 3.4 Power.h lines 650--701 and 733--791.
        for return_a0 in [0x0000u16, 0x0100] {
            assert_eq!(
                raw_trap_route(0xA285 | return_a0).os_routine_variant,
                PowerIdleUpdate
            );
            assert_eq!(
                raw_trap_route(0xA485 | return_a0).os_routine_variant,
                PowerIdleState
            );
            assert_eq!(
                raw_trap_route(0xA685 | return_a0).os_routine_variant,
                PowerSerial
            );
            assert_eq!(
                raw_trap_route(0xA085 | return_a0).os_routine_variant,
                Unclassified,
                "the bare slot has no reviewed Power Manager routine identity"
            );
        }

        // Inside Macintosh: Processes (1994), pp. 3-18--3-20; UI 3.4
        // Timer.h lines 74--100 declare InsTime $A058 and InsXTime $A458.
        for return_a0 in [0x0000u16, 0x0100] {
            assert_eq!(
                raw_trap_route(0xA058 | return_a0).os_routine_variant,
                TimeTaskOriginal
            );
            assert_eq!(
                raw_trap_route(0xA458 | return_a0).os_routine_variant,
                TimeTaskExtended
            );
            assert_eq!(
                raw_trap_route(0xA258 | return_a0).os_routine_variant,
                Unclassified
            );
            assert_eq!(
                raw_trap_route(0xA658 | return_a0).os_routine_variant,
                Unclassified
            );
        }

        // Inside Macintosh: Text (1993), pp. 5-64--5-65.
        for return_a0 in [0x0000u16, 0x0100] {
            assert_eq!(
                raw_trap_route(0xA054 | return_a0).os_routine_variant,
                UpperStringPreserveMarks
            );
            assert_eq!(
                raw_trap_route(0xA254 | return_a0).os_routine_variant,
                UpperStringStripMarks
            );
            assert_eq!(
                raw_trap_route(0xA454 | return_a0).os_routine_variant,
                Unclassified
            );
            assert_eq!(
                raw_trap_route(0xA654 | return_a0).os_routine_variant,
                Unclassified
            );
        }

        // Devices 1994, p. 1-16; UI 3.4 Devices.h lines 905--1044 and
        // 1282--1415 declare exact Sync, Immed, and Async words for $01--$06.
        for slot in 0x01u16..=0x06 {
            for return_a0 in [0x0000u16, 0x0100] {
                for (routine_bits, expected) in [
                    (0x0000, ParameterBlockSynchronous),
                    (0x0200, ParameterBlockImmediate),
                    (0x0400, ParameterBlockAsynchronous),
                ] {
                    assert_eq!(
                        raw_trap_route(0xA000 | slot | return_a0 | routine_bits).os_routine_variant,
                        expected
                    );
                }
                assert_eq!(
                    raw_trap_route(0xA600 | slot | return_a0).os_routine_variant,
                    Unclassified,
                    "combined ASYNC+IMMED slot ${slot:02X} is undeclared"
                );
            }
        }

        // Inside Macintosh: Text (1993), pp. 5-51--5-52 and 5-60--5-61.
        for slot in [0x3Cu16, 0x50] {
            for return_a0 in [0x0000u16, 0x0100] {
                for (routine_bits, expected, sensitivity) in [
                    (0x0000, TextCompareFoldCaseAndMarks, (false, false)),
                    (0x0200, TextCompareFoldCase, (false, true)),
                    (0x0400, TextCompareStripMarks, (true, false)),
                    (0x0600, TextCompareExact, (true, true)),
                ] {
                    let variant =
                        raw_trap_route(0xA000 | slot | return_a0 | routine_bits).os_routine_variant;
                    assert_eq!(variant, expected);
                    assert_eq!(variant.text_comparison_sensitivity(), Some(sensitivity));
                }
            }
        }

        // IM:Memory documents SYS, but not bit 9, for these routines. UI 3.4
        // MacMemory.h declares the current/system pairs at lines 517--533,
        // 631--695, 862--1010, 1184--1202, and 1331--1362.
        for slot in [
            0x1Cu16, 0x1D, 0x27, 0x28, 0x40, 0x4C, 0x4D, 0x61, 0x62, 0x66,
        ] {
            for return_a0 in [0x0000u16, 0x0100] {
                assert_eq!(
                    raw_trap_route(0xA000 | slot | return_a0).os_routine_variant,
                    CurrentHeap
                );
                assert_eq!(
                    raw_trap_route(0xA400 | slot | return_a0).os_routine_variant,
                    SystemHeap
                );
                assert_eq!(
                    raw_trap_route(0xA200 | slot | return_a0).os_routine_variant,
                    Unclassified
                );
                assert_eq!(
                    raw_trap_route(0xA600 | slot | return_a0).os_routine_variant,
                    Unclassified
                );
            }
        }

        // Inside Macintosh VI, pp. 14-62--14-63 and Appendix C table C-2;
        // Universal Interfaces 3.4 TextUtils.h lines 404--455.
        for return_a0 in [0x0000u16, 0x0100] {
            for (routine_bits, expected) in [
                (0x0000, LowerText),
                (0x0200, StripText),
                (0x0400, UpperText),
                (0x0600, StripUpperText),
            ] {
                assert_eq!(
                    raw_trap_route(0xA056 | return_a0 | routine_bits).os_routine_variant,
                    expected
                );
            }
        }

        // Files 1992, pp. 2-6 and 2-238--2-239 plus its assembly summary;
        // UI 3.4 Files.h lines 1315--3343 declare these exact words.
        let basic_file_slots = [
            0x07u16, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x18,
            0x41, 0x42, 0x43, 0x44, 0x45,
        ];
        let hfs_file_slots = [
            0x07u16, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x10, 0x14, 0x15, 0x41, 0x42,
        ];
        for return_a0 in [0x0000u16, 0x0100] {
            for slot in basic_file_slots {
                assert_eq!(
                    raw_trap_route(0xA000 | slot | return_a0).os_routine_variant,
                    FileSynchronous
                );
                assert_eq!(
                    raw_trap_route(0xA400 | slot | return_a0).os_routine_variant,
                    FileAsynchronous
                );
            }
            for slot in hfs_file_slots {
                assert_eq!(
                    raw_trap_route(0xA200 | slot | return_a0).os_routine_variant,
                    FileHfsSynchronous
                );
                assert_eq!(
                    raw_trap_route(0xA600 | slot | return_a0).os_routine_variant,
                    FileHfsAsynchronous
                );
            }
        }

        let classified = (0xA000u16..=0xAFFF)
            .filter(|&word| raw_trap_route(word).os_routine_variant != Unclassified)
            .count();
        assert_eq!(classified, 280);
        assert_eq!(
            raw_trap_route(0xA271).os_routine_variant,
            Unclassified,
            "an unrelated OS bit-9 form must not acquire invented semantics"
        );
        assert_eq!(raw_trap_route(0xAE56).os_routine_variant, Unclassified);
        assert_eq!(
            raw_trap_route(0xA200).os_routine_variant,
            Unclassified,
            "PBHOpen/PBOpenImmed is an intentionally unresolved declaration collision"
        );
        assert_eq!(raw_trap_route(0xA613).os_routine_variant, Unclassified);
    }

    #[test]
    fn generated_profile_routes_cover_all_raw_words_and_live_table_cells() {
        for profile in [TrapTableProfile::M68k68040, TrapTableProfile::PowerPc604] {
            let (mut dispatcher, _cpu, mut bus) = setup();
            dispatcher.materialize_trap_tables(&mut bus, profile);
            let unimplemented = dispatcher.default_trap_gateway(0xAA6E).unwrap();

            for low_word in 0u16..0x1000 {
                let word = 0xA000 | low_word;
                let route = profile.route(word);
                let table_entry = route.raw.table_address;
                assert_eq!(
                    table_entry,
                    TrapDispatcher::raw_trap_table_entry(word),
                    "raw word ${word:04X}"
                );
                let raw_target = bus.read_long(table_entry);
                assert_eq!(
                    route.has_permanent_come_from,
                    bus.read_long(raw_target) == COME_FROM_PATCH_SIGNATURE,
                    "raw word ${word:04X}"
                );
                let logical = dispatcher.trap_table_address(&bus, word).unwrap();
                assert_eq!(
                    route.default_is_unimplemented,
                    logical == unimplemented,
                    "raw word ${word:04X}"
                );
                assert_eq!(
                    logical,
                    dispatcher
                        .trap_table_address(&bus, route.raw.canonical_word)
                        .unwrap(),
                    "variant ${word:04X} must select its canonical slot"
                );
            }
        }
    }

    #[test]
    fn generated_default_routes_cover_every_canonical_operation_once() {
        let mut seen = HashSet::new();
        for table_index in 0..(OS_TRAP_TABLE_SLOTS + TOOLBOX_TRAP_TABLE_SLOTS) {
            let canonical_word = if table_index < OS_TRAP_TABLE_SLOTS {
                0xA000 | table_index
            } else {
                0xA800 | (table_index - OS_TRAP_TABLE_SLOTS)
            };
            let route = default_trap_route(canonical_word);
            assert_eq!(route.operation_id, canonical_word);
            assert!(seen.insert(route.operation_id));
            assert_eq!(
                route,
                default_trap_route(
                    canonical_word | if table_index < 0x100 { 0x0700 } else { 0x0400 }
                )
            );
        }
        assert_eq!(seen.len(), 1280);
    }

    #[test]
    fn power_manager_generated_routes_preserve_exact_low_word_values() {
        assert_eq!(POWER_MANAGER_OPERATION_ROUTES.len(), 34);
        assert!(POWER_MANAGER_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0000, "PMSelectorCount"),
            (0x0001, "PMFeatures"),
            (0x0002, "GetSleepTimeout"),
            (0x0003, "SetSleepTimeout"),
            (0x0004, "GetHardDiskTimeout"),
            (0x0005, "SetHardDiskTimeout"),
            (0x0006, "HardDiskPowered"),
            (0x0007, "SpinDownHardDisk"),
            (0x0008, "IsSpindownDisabled"),
            (0x0009, "SetSpindownDisable"),
            (0x000A, "HardDiskQInstall"),
            (0x000B, "HardDiskQRemove"),
            (0x000C, "GetScaledBatteryInfo"),
            (0x000D, "AutoSleepControl"),
            (0x000E, "GetIntModemInfo"),
            (0x000F, "SetIntModemState"),
            (0x0010, "MaximumProcessorSpeed"),
            (0x0011, "CurrentProcessorSpeed"),
            (0x0012, "FullProcessorSpeed"),
            (0x0013, "SetProcessorSpeed"),
            (0x0014, "GetSCSIDiskModeAddress"),
            (0x0015, "SetSCSIDiskModeAddress"),
            (0x0016, "GetWakeupTimer"),
            (0x0017, "SetWakeupTimer"),
            (0x0018, "IsProcessorCyclingEnabled"),
            (0x0019, "EnableProcessorCycling"),
            (0x001A, "BatteryCount"),
            (0x001B, "GetBatteryVoltage"),
            (0x001C, "GetBatteryTimes"),
            (0x001D, "GetDimmingTimeout"),
            (0x001E, "SetDimmingTimeout"),
            (0x001F, "DimmingControl"),
            (0x0020, "IsDimmingControlDisabled"),
            (0x0021, "IsAutoSlpControlDisabled"),
        ] {
            let route =
                power_manager_operation_route(0xA09E, selector).expect("PowerMgrDispatch route");
            assert_eq!(route.routine_name, routine_name);
        }

        for (trap_word, selector) in [
            (0xA19E, 0x0003),
            (0xA09E, 0x0022),
            (0xA09E, 0x0036),
            (0xA09E, 0x7000),
            (0xA09E, 0x303C),
        ] {
            assert!(power_manager_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn power_manager_records_identity_while_remaining_fail_closed() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0x1234_0003);

        let result = dispatcher.dispatch(0xA09E, &mut cpu, &mut bus);
        assert!(matches!(result, Err(Error::UnimplementedTrap(0xA09E))));
        assert_eq!(dispatcher.current_trap_adapter, TrapAdapterId::Nonterminal);
        assert_eq!(
            dispatcher.current_selector_operation,
            Some("selector-operation:_PowerMgrDispatch:0x0003:d0-low-word-immediate:16")
        );

        cpu.write_reg(Register::D0, 0x0004);
        let result = dispatcher.dispatch(0xA09E, &mut cpu, &mut bus);
        assert!(matches!(result, Err(Error::UnimplementedTrap(0xA09E))));
        assert_eq!(
            dispatcher.current_selector_operation,
            Some("selector-operation:_PowerMgrDispatch:0x0004:d0-moveq-immediate:8")
        );

        cpu.write_reg(Register::D0, 0x0022);
        let result = dispatcher.dispatch(0xA09E, &mut cpu, &mut bus);
        assert!(matches!(result, Err(Error::UnimplementedTrap(0xA09E))));
        assert_eq!(dispatcher.current_selector_operation, None);

        cpu.write_reg(Register::D0, 0x0003);
        let result = dispatcher.dispatch(0xA19E, &mut cpu, &mut bus);
        assert!(matches!(result, Err(Error::UnimplementedTrap(0xA19E))));
        assert_eq!(dispatcher.current_selector_operation, None);
    }

    #[test]
    fn every_profile_saved_default_reaches_its_declared_adapter() {
        // A saved Trap Manager pointer remains callable after replacement and
        // reaches the original system routine. Inside Macintosh: Operating
        // System Utilities (1994), pp. 8-23--8-30. Isolate every invocation
        // because arbitrary default operations may legitimately mutate global
        // manager state even when their poison arguments produce an error.
        const CALLER_SP: u32 = 0x001F_FF00;
        const RETURN_PC: u32 = 0x001F_0002;
        const PATCH: u32 = 0x0028_0000;

        for profile in [TrapTableProfile::M68k68040, TrapTableProfile::PowerPc604] {
            for table_index in 0..(OS_TRAP_TABLE_SLOTS + TOOLBOX_TRAP_TABLE_SLOTS) {
                let is_toolbox = table_index >= OS_TRAP_TABLE_SLOTS;
                let slot = if is_toolbox {
                    table_index - OS_TRAP_TABLE_SLOTS
                } else {
                    table_index
                };
                let canonical_word = if is_toolbox {
                    0xA800 | slot
                } else {
                    0xA000 | slot
                };
                let (mut dispatcher, mut cpu, mut bus) = setup();
                dispatcher.materialize_trap_tables(&mut bus, profile);
                let saved_default = dispatcher.trap_table_address(&bus, canonical_word).unwrap();
                let profile_route = profile.route(canonical_word);
                let invoked_word = bus.read_word(saved_default);
                let invoked_operation = profile_route.default_gateway_word;
                let declared = *default_trap_route(invoked_operation);
                dispatcher.install_trap_address(&mut bus, canonical_word, PATCH);
                assert_eq!(
                    dispatcher.native_trap_handler(&bus, canonical_word),
                    Some(PATCH)
                );

                let entry_sp = CALLER_SP - 4;
                bus.write_long(entry_sp, RETURN_PC);
                cpu.write_reg(Register::D0, 0xD0D0_0000);
                cpu.write_reg(Register::D1, 0xD1D1_0000);
                cpu.write_reg(Register::D2, 0xD2D2_0000);
                cpu.write_reg(Register::A0, 0);
                cpu.write_reg(Register::A1, 0);
                cpu.write_reg(Register::A2, 0);
                cpu.write_reg(Register::A7, entry_sp);
                cpu.write_reg(Register::PC, saved_default + 2);

                let result = dispatcher.dispatch(invoked_word, &mut cpu, &mut bus);

                assert_eq!(
                    dispatcher.current_trap_operation, invoked_operation,
                    "{profile:?} operation ${canonical_word:04X}"
                );
                assert!(
                    declared.allows(dispatcher.current_trap_adapter),
                    "{profile:?} adapter ${canonical_word:04X}: {:?}",
                    dispatcher.current_trap_adapter
                );
                if dispatcher.current_trap_adapter == TrapAdapterId::Nonterminal {
                    assert!(
                        matches!(result, Err(Error::UnimplementedTrap(word)) if word == invoked_word),
                        "{profile:?} declared nonterminal ${canonical_word:04X}: {result:?}"
                    );
                } else {
                    assert!(
                        !matches!(result, Err(Error::UnimplementedTrap(_))),
                        "{profile:?} declared adapter fell through ${canonical_word:04X}"
                    );
                }
                assert!(
                    dispatcher.pending_native_trap_calls.is_empty(),
                    "saved default must bypass current patch ${canonical_word:04X}"
                );
            }
        }
    }

    fn call_trap_manager_getter<C: CpuOps>(
        dispatcher: &mut TrapDispatcher,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        getter: u16,
        trap_word: u16,
    ) -> u32 {
        cpu.write_reg(Register::D0, 0xFFFF_0000 | u32::from(trap_word));
        let saved_gateway = dispatcher
            .default_trap_gateway(getter)
            .expect("materialized Trap Manager getter gateway");
        cpu.write_reg(Register::PC, saved_gateway + 2);
        dispatcher
            .dispatch(getter, cpu, bus)
            .unwrap_or_else(|error| panic!("getter ${getter:04X} for ${trap_word:04X}: {error:?}"));
        cpu.read_reg(Register::A0)
    }

    fn call_trap_manager_setter<C: CpuOps>(
        dispatcher: &mut TrapDispatcher,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        setter: u16,
        trap_word: u16,
        handler: u32,
    ) {
        cpu.write_reg(Register::D0, 0xFFFF_0000 | u32::from(trap_word));
        cpu.write_reg(Register::A0, handler);
        let saved_gateway = dispatcher
            .default_trap_gateway(setter)
            .expect("materialized Trap Manager setter gateway");
        cpu.write_reg(Register::PC, saved_gateway + 2);
        dispatcher
            .dispatch(setter, cpu, bus)
            .unwrap_or_else(|error| panic!("setter ${setter:04X} for ${trap_word:04X}: {error:?}"));
    }

    #[test]
    fn generated_profile_slots_exhaustively_roundtrip_classic_patch_lifecycle() {
        // The typed and legacy Trap Manager operations must observe the same
        // process table used by A-line dispatch. Saved logical pointers remain
        // callable after a patch, nested replacement restores in LIFO order,
        // and a raw table write bypasses any permanent come-from head. Inside
        // Macintosh: Operating System Utilities (1994), pp. 8-23--8-33.
        const RETURN_PC: u32 = 0x001F_0002;
        const SP: u32 = 0x001F_FF00;
        const FIRST_PATCH_BASE: u32 = 0x0028_0000;
        const SECOND_PATCH_BASE: u32 = 0x0029_0000;
        const RAW_PATCH_BASE: u32 = 0x002A_0000;

        for profile in [TrapTableProfile::M68k68040, TrapTableProfile::PowerPc604] {
            let (mut dispatcher, mut cpu, mut bus) = setup();
            dispatcher.materialize_trap_tables(&mut bus, profile);

            for table_index in 0..(OS_TRAP_TABLE_SLOTS + TOOLBOX_TRAP_TABLE_SLOTS) {
                let is_toolbox = table_index >= OS_TRAP_TABLE_SLOTS;
                let slot = if is_toolbox {
                    table_index - OS_TRAP_TABLE_SLOTS
                } else {
                    table_index
                };
                let trap_word = if is_toolbox {
                    0xA800 | slot
                } else {
                    0xA000 | slot
                };
                let getter = if is_toolbox { 0xA746 } else { 0xA346 };
                let setter = if is_toolbox { 0xA647 } else { 0xA247 };
                let route = profile.route(trap_word);
                let raw_entry = route.raw.table_address;
                let initial_raw = bus.read_long(raw_entry);
                let default = dispatcher.trap_table_address(&bus, trap_word).unwrap();
                let first_patch = FIRST_PATCH_BASE + u32::from(table_index) * 4;
                let second_patch = SECOND_PATCH_BASE + u32::from(table_index) * 4;
                let raw_patch = RAW_PATCH_BASE + u32::from(table_index) * 4;

                assert_eq!(
                    call_trap_manager_getter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        getter,
                        trap_word,
                    ),
                    default,
                    "{profile:?} typed default getter ${trap_word:04X}"
                );

                let legacy_uses_os = matches!(slot, 0x000..=0x04F | 0x054 | 0x057);
                if legacy_uses_os != is_toolbox {
                    assert_eq!(
                        call_trap_manager_getter(
                            &mut dispatcher,
                            &mut cpu,
                            &mut bus,
                            0xA146,
                            trap_word,
                        ),
                        default,
                        "{profile:?} legacy default getter ${trap_word:04X}"
                    );
                }

                call_trap_manager_setter(
                    &mut dispatcher,
                    &mut cpu,
                    &mut bus,
                    setter,
                    trap_word,
                    first_patch,
                );
                assert_eq!(
                    call_trap_manager_getter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        getter,
                        trap_word,
                    ),
                    first_patch,
                    "{profile:?} first patch ${trap_word:04X}"
                );
                if route.has_permanent_come_from {
                    assert_eq!(
                        bus.read_long(raw_entry),
                        initial_raw,
                        "{profile:?} protected raw head ${trap_word:04X}"
                    );
                } else {
                    assert_eq!(
                        bus.read_long(raw_entry),
                        first_patch,
                        "{profile:?} direct raw patch ${trap_word:04X}"
                    );
                }

                let saved_first = call_trap_manager_getter(
                    &mut dispatcher,
                    &mut cpu,
                    &mut bus,
                    getter,
                    trap_word,
                );
                call_trap_manager_setter(
                    &mut dispatcher,
                    &mut cpu,
                    &mut bus,
                    setter,
                    trap_word,
                    second_patch,
                );
                assert_eq!(
                    call_trap_manager_getter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        getter,
                        trap_word,
                    ),
                    second_patch,
                    "{profile:?} nested patch ${trap_word:04X}"
                );
                call_trap_manager_setter(
                    &mut dispatcher,
                    &mut cpu,
                    &mut bus,
                    setter,
                    trap_word,
                    saved_first,
                );

                let variants = if is_toolbox { 2 } else { 8 };
                for variant in 0..variants {
                    let raw_word = if is_toolbox {
                        trap_word | (variant << 10)
                    } else {
                        trap_word | (variant << 8)
                    };
                    cpu.write_reg(Register::PC, RETURN_PC);
                    cpu.write_reg(Register::A7, SP);
                    cpu.write_reg(Register::D1, 0xD1D1_BEEF);
                    if is_toolbox && variant != 0 {
                        bus.write_long(SP, RETURN_PC);
                    }

                    dispatcher.dispatch(raw_word, &mut cpu, &mut bus).unwrap();
                    let argument_sp = if is_toolbox && variant != 0 {
                        SP + 4
                    } else {
                        SP
                    };
                    let handler_sp = argument_sp - 4;

                    assert_eq!(
                        cpu.read_reg(Register::PC),
                        first_patch,
                        "{profile:?} patched variant ${raw_word:04X}"
                    );
                    assert_eq!(
                        cpu.read_reg(Register::A7),
                        handler_sp,
                        "{profile:?} handler SP ${raw_word:04X}"
                    );
                    assert_eq!(bus.read_long(handler_sp), RETURN_PC);
                    if !is_toolbox {
                        assert_eq!(
                            cpu.read_reg(Register::D1),
                            0xD1D1_0000 | u32::from(raw_word),
                            "{profile:?} full OS word ${raw_word:04X}"
                        );
                    }

                    cpu.write_reg(Register::PC, RETURN_PC);
                    cpu.write_reg(Register::A7, argument_sp);
                    dispatcher.retire_returned_native_trap_call(&mut cpu);
                    assert!(
                        dispatcher.pending_native_trap_calls.is_empty(),
                        "{profile:?} retired ${raw_word:04X}"
                    );
                }

                // The saved default remains an executable OS trap-plus-RTS or
                // Toolbox auto-pop A-line while its table slot is patched.
                // A profile may intentionally give multiple cells the same
                // procedure address, so inspect its declared gateway identity
                // rather than assuming every cell embeds its own slot word.
                if is_toolbox {
                    assert_eq!(bus.read_word(default), route.default_gateway_word | 0x0400);
                } else {
                    assert_eq!(bus.read_word(default), route.default_gateway_word);
                    assert_eq!(bus.read_word(default + 2), 0x4E75);
                }

                call_trap_manager_setter(
                    &mut dispatcher,
                    &mut cpu,
                    &mut bus,
                    setter,
                    trap_word,
                    default,
                );
                assert_eq!(bus.read_long(raw_entry), initial_raw);
                assert_eq!(
                    call_trap_manager_getter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        getter,
                        trap_word,
                    ),
                    default,
                    "{profile:?} restored default ${trap_word:04X}"
                );

                if legacy_uses_os != is_toolbox {
                    call_trap_manager_setter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        0xA047,
                        trap_word,
                        first_patch,
                    );
                    assert_eq!(
                        call_trap_manager_getter(
                            &mut dispatcher,
                            &mut cpu,
                            &mut bus,
                            getter,
                            trap_word,
                        ),
                        first_patch,
                        "{profile:?} legacy setter ${trap_word:04X}"
                    );
                    call_trap_manager_setter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        0xA047,
                        trap_word,
                        default,
                    );
                    assert_eq!(bus.read_long(raw_entry), initial_raw);
                }

                bus.write_long(raw_entry, raw_patch);
                assert_eq!(
                    call_trap_manager_getter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        getter,
                        trap_word,
                    ),
                    raw_patch,
                    "{profile:?} raw table patch ${trap_word:04X}"
                );
                cpu.write_reg(Register::PC, RETURN_PC);
                cpu.write_reg(Register::A7, SP);
                dispatcher.dispatch(trap_word, &mut cpu, &mut bus).unwrap();
                assert_eq!(cpu.read_reg(Register::PC), raw_patch);
                cpu.write_reg(Register::PC, RETURN_PC);
                cpu.write_reg(Register::A7, SP);
                dispatcher.retire_returned_native_trap_call(&mut cpu);
                assert!(dispatcher.pending_native_trap_calls.is_empty());

                bus.write_long(raw_entry, initial_raw);
                assert_eq!(
                    call_trap_manager_getter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        getter,
                        trap_word,
                    ),
                    default,
                    "{profile:?} raw restore ${trap_word:04X}"
                );
            }
        }
    }

    #[test]
    fn trap_word_map_preserves_the_hashmap_contract() {
        let mut map = super::TrapWordMap::default();
        // First insert returns None; lookup sees it.
        assert_eq!(map.insert(0xA9F0, 0x1000), None);
        assert_eq!(map.get(&0xA9F0), Some(&0x1000));
        assert_eq!(map.get(&0xA9F1), None, "absent key misses");
        // Replacement returns the previous handler and keeps one entry.
        assert_eq!(map.insert(0xA9F0, 0x2000), Some(0x1000));
        assert_eq!(map.get(&0xA9F0), Some(&0x2000));
        // A second key coexists.
        assert_eq!(map.insert(0xA146, 0x3000), None);
        assert_eq!(map.get(&0xA9F0), Some(&0x2000));
        assert_eq!(map.get(&0xA146), Some(&0x3000));
        // Removal returns the value exactly once and clears lookup.
        assert_eq!(map.remove(&0xA9F0), Some(0x2000));
        assert_eq!(map.remove(&0xA9F0), None, "second remove is a miss");
        assert_eq!(map.get(&0xA9F0), None);
        // The untouched key survives its neighbor's removal.
        assert_eq!(map.get(&0xA146), Some(&0x3000));
        // Replace-then-remove on the survivor behaves like HashMap too.
        assert_eq!(map.insert(0xA146, 0x4000), Some(0x3000));
        assert_eq!(map.remove(&0xA146), Some(0x4000));
        assert_eq!(map.get(&0xA146), None);
    }

    #[test]
    fn trap_word_map_stays_exact_at_full_occupancy_and_off_band() {
        // SetTrapAddress can populate every slot of both trap tables, so
        // fill them completely: 256 OS words and 1,024 Toolbox words.
        let mut map = super::TrapWordMap::default();
        for os in 0xA000u16..=0xA0FF {
            assert_eq!(map.insert(os, u32::from(os) | 0x10_0000), None);
        }
        for tool in 0xA800u16..=0xABFF {
            assert_eq!(map.insert(tool, u32::from(tool) | 0x20_0000), None);
        }
        // Every lookup is exact at full occupancy — the boundary the
        // linear-scan design regressed on.
        for os in 0xA000u16..=0xA0FF {
            assert_eq!(map.get(&os), Some(&(u32::from(os) | 0x10_0000)));
        }
        for tool in 0xA800u16..=0xABFF {
            assert_eq!(map.get(&tool), Some(&(u32::from(tool) | 0x20_0000)));
        }
        // Replacement and removal stay exact with every slot occupied.
        assert_eq!(map.insert(0xA9F0, 0xDEAD), Some(0xA9F0 | 0x20_0000));
        assert_eq!(map.remove(&0xA9F0), Some(0xDEAD));
        assert_eq!(map.get(&0xA9F0), None);
        assert_eq!(
            map.get(&0xA9F1),
            Some(&(0xA9F1 | 0x20_0000)),
            "neighbors survive"
        );
        // The band boundaries themselves: words between the OS and
        // Toolbox tables, and past the Toolbox table, are storable and
        // retrievable (spill), and absent ones stay absent.
        for off_band in [0x01F4u16, 0xA100, 0xA7FF, 0xAC00, 0xFFFF] {
            assert_eq!(map.get(&off_band), None);
            assert_eq!(map.insert(off_band, 0xBEEF), None);
            assert_eq!(map.get(&off_band), Some(&0xBEEF));
            assert_eq!(map.remove(&off_band), Some(0xBEEF));
            assert_eq!(map.get(&off_band), None);
        }
    }

    #[test]
    fn come_from_chain_resolution_reaches_the_last_exit_and_rejects_cycles() {
        let first = 0x0010_0000;
        let second = 0x0010_0100;
        let target = 0x0020_0000;
        let read_chain = |address| match address {
            address if address == first || address == second => Some(COME_FROM_PATCH_SIGNATURE),
            address if address == first + 4 => Some(second),
            address if address == second + 4 => Some(target),
            _ => None,
        };

        assert_eq!(
            resolve_trap_table_target(first, read_chain),
            Some(TrapTableTarget::Protected {
                last_head: second,
                logical_successor: target,
            })
        );
        assert_eq!(
            resolve_trap_table_target(target, read_chain),
            Some(TrapTableTarget::Direct(target))
        );
        assert_eq!(
            resolve_trap_table_target(first, |address| match address {
                address if address == first || address == second => {
                    Some(COME_FROM_PATCH_SIGNATURE)
                }
                address if address == first + 4 => Some(second),
                address if address == second + 4 => Some(first),
                _ => None,
            }),
            None
        );
    }

    #[test]
    fn materialized_trap_tables_contain_all_callable_profile_entries() {
        let (mut dispatcher, _cpu, mut bus) = setup();
        dispatcher.materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040);

        for slot in 0..OS_TRAP_TABLE_SLOTS {
            let trap_word = 0xA000 | slot;
            let gateway_word = TrapTableProfile::M68k68040
                .route(trap_word)
                .default_gateway_word;
            let entry = bus.read_long(OS_TRAP_TABLE_BASE + u32::from(slot) * 4);
            assert_ne!(entry, 0, "OS trap slot ${slot:02X}");
            if M68K_68040_COME_FROM_TRAPS.contains(&trap_word) {
                assert_eq!(bus.read_long(entry), 0x6006_4EF9);
                assert_eq!(bus.read_word(entry + 8), 0x60F8);
                assert_eq!(bus.read_word(bus.read_long(entry + 4)), gateway_word);
            } else {
                assert_eq!(bus.read_word(entry), gateway_word);
                assert_eq!(bus.read_word(entry + 2), 0x4E75);
            }
        }
        for slot in 0..TOOLBOX_TRAP_TABLE_SLOTS {
            let trap_word = 0xA800 | slot;
            let gateway_word = TrapTableProfile::M68k68040
                .route(trap_word)
                .default_gateway_word;
            let entry = bus.read_long(TOOLBOX_TRAP_TABLE_BASE + u32::from(slot) * 4);
            assert_ne!(entry, 0, "Toolbox trap slot ${slot:03X}");
            if M68K_68040_COME_FROM_TRAPS.contains(&trap_word) {
                assert_eq!(bus.read_long(entry), 0x6006_4EF9);
                assert_eq!(bus.read_word(entry + 8), 0x60F8);
                assert_eq!(
                    bus.read_word(bus.read_long(entry + 4)),
                    gateway_word | 0x0400
                );
            } else {
                assert_eq!(bus.read_word(entry), gateway_word | 0x0400);
            }
        }
    }

    #[test]
    fn machine_profiles_generate_distinct_protected_exception_vector_defaults() {
        for profile in [TrapTableProfile::M68k68040, TrapTableProfile::PowerPc604] {
            let (mut dispatcher, _cpu, mut bus) = setup();
            dispatcher.materialize_trap_tables(&mut bus, profile);
            let defaults = dispatcher.trap_exception_vector_defaults.unwrap();

            assert_ne!(defaults[0], defaults[1]);
            assert_eq!([bus.read_long(0x28), bus.read_long(0x2C)], defaults);
            for &gateway in &defaults {
                assert_ne!(gateway, 0);
                assert_eq!(bus.read_word(gateway), 0x4E73); // RTE
                bus.write_word(gateway, 0x4E71);
                assert_eq!(bus.read_word(gateway), 0x4E73, "gateway is protected");

                for slot in 0..OS_TRAP_TABLE_SLOTS {
                    assert_ne!(
                        gateway,
                        bus.read_long(OS_TRAP_TABLE_BASE + u32::from(slot) * 4)
                    );
                }
                for slot in 0..TOOLBOX_TRAP_TABLE_SLOTS {
                    assert_ne!(
                        gateway,
                        bus.read_long(TOOLBOX_TRAP_TABLE_BASE + u32::from(slot) * 4)
                    );
                }
            }
        }
    }

    #[test]
    fn machine_profiles_materialize_their_observed_come_from_sets() {
        for (profile, expected) in [
            (TrapTableProfile::M68k68040, M68K_68040_COME_FROM_TRAPS),
            (TrapTableProfile::PowerPc604, POWERPC_604_COME_FROM_TRAPS),
        ] {
            let (mut dispatcher, _cpu, mut bus) = setup();
            dispatcher.materialize_trap_tables(&mut bus, profile);
            let mut observed = Vec::new();
            for slot in 0..OS_TRAP_TABLE_SLOTS {
                let word = 0xA000 | slot;
                let raw = bus.read_long(TrapDispatcher::raw_trap_table_entry(word));
                if bus.read_long(raw) == 0x6006_4EF9 {
                    observed.push(word);
                }
            }
            for slot in 0..TOOLBOX_TRAP_TABLE_SLOTS {
                let word = 0xA800 | slot;
                let raw = bus.read_long(TrapDispatcher::raw_trap_table_entry(word));
                if bus.read_long(raw) == 0x6006_4EF9 {
                    observed.push(word);
                }
            }
            assert_eq!(observed, expected);
        }
    }

    #[test]
    fn process_switch_restores_raw_trap_topology_and_native_call_frames() {
        let (mut dispatcher, _cpu, mut bus) = setup();
        let protected_word = 0xA823;
        let direct_word = 0xA975;
        let protected_entry = TrapDispatcher::raw_trap_table_entry(protected_word);
        let direct_entry = TrapDispatcher::raw_trap_table_entry(direct_word);
        let first_protected_patch = 0x0021_0000;
        let first_direct_patch = 0x0021_1000;
        let second_protected_patch = 0x0022_0000;
        let second_direct_patch = 0x0022_1000;

        dispatcher.materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040);
        let first_head = bus.read_long(protected_entry);
        let first_defaults = dispatcher.trap_exception_vector_defaults.unwrap();
        assert_eq!([bus.read_long(0x28), bus.read_long(0x2C)], first_defaults);
        let first_aline_patch = 0x0020_F000;
        bus.write_long(0x28, first_aline_patch);
        dispatcher.install_trap_address(&mut bus, protected_word, first_protected_patch);
        bus.write_long(direct_entry, first_direct_patch);
        dispatcher.pending_native_trap_calls.insert(
            0xA039,
            vec![NativeTrapCallState {
                return_pc: 0x0023_0000,
                argument_sp: 0x003F_FF00,
                os_dispatch_frame: None,
                preserved_d_regs: [1; 5],
                preserved_a_regs: [2; 5],
            }],
        );
        dispatcher.current_trap_caller = Some(0x0023_1000);

        let fresh_second =
            dispatcher.create_trap_table_process_context(&mut bus, TrapTableProfile::PowerPc604);
        let first_context = dispatcher
            .switch_trap_table_process_context(&mut bus, fresh_second)
            .expect("first process context must be saved");
        let second_head = bus.read_long(protected_entry);
        let second_defaults = dispatcher.trap_exception_vector_defaults.unwrap();
        assert_ne!(second_head, first_head);
        assert_ne!(second_defaults, first_defaults);
        assert_eq!([bus.read_long(0x28), bus.read_long(0x2C)], second_defaults);
        assert_eq!(
            dispatcher.trap_table_profile,
            Some(TrapTableProfile::PowerPc604)
        );
        assert!(!dispatcher.has_native_trap_patch(&bus, protected_word));
        assert!(!dispatcher.has_native_trap_patch(&bus, direct_word));
        assert!(dispatcher.pending_native_trap_calls.is_empty());
        assert_eq!(dispatcher.current_trap_caller, None);

        dispatcher.install_trap_address(&mut bus, protected_word, second_protected_patch);
        bus.write_long(direct_entry, second_direct_patch);
        dispatcher.pending_native_trap_calls.insert(
            0xA975,
            vec![NativeTrapCallState {
                return_pc: 0x0024_0000,
                argument_sp: 0x003F_FE00,
                os_dispatch_frame: None,
                preserved_d_regs: [3; 5],
                preserved_a_regs: [4; 5],
            }],
        );
        dispatcher.current_trap_caller = Some(0x0024_1000);

        let second_context = dispatcher
            .switch_trap_table_process_context(&mut bus, first_context)
            .expect("second process context must be saved");
        assert_eq!(
            dispatcher.trap_table_profile,
            Some(TrapTableProfile::M68k68040)
        );
        assert_eq!(bus.read_long(protected_entry), first_head);
        assert_eq!(
            dispatcher.trap_table_address(&bus, protected_word),
            Some(first_protected_patch)
        );
        assert_eq!(bus.read_long(direct_entry), first_direct_patch);
        assert_eq!(bus.read_long(0x28), first_aline_patch);
        assert_eq!(bus.read_long(0x2C), first_defaults[1]);
        assert_eq!(
            dispatcher.trap_exception_vector_defaults,
            Some(first_defaults)
        );
        assert!(dispatcher.pending_native_trap_calls.contains_key(&0xA039));
        assert!(!dispatcher.pending_native_trap_calls.contains_key(&0xA975));
        assert_eq!(dispatcher.current_trap_caller, Some(0x0023_1000));

        let _first_context = dispatcher
            .switch_trap_table_process_context(&mut bus, second_context)
            .expect("restored first context must be saved again");
        assert_eq!(bus.read_long(protected_entry), second_head);
        assert_eq!(
            dispatcher.trap_table_address(&bus, protected_word),
            Some(second_protected_patch)
        );
        assert_eq!(bus.read_long(direct_entry), second_direct_patch);
        assert_eq!([bus.read_long(0x28), bus.read_long(0x2C)], second_defaults);
        assert_eq!(
            dispatcher.trap_exception_vector_defaults,
            Some(second_defaults)
        );
        assert!(dispatcher.pending_native_trap_calls.contains_key(&0xA975));
        assert!(!dispatcher.pending_native_trap_calls.contains_key(&0xA039));
        assert_eq!(dispatcher.current_trap_caller, Some(0x0024_1000));

        dispatcher.teardown_trap_table_process_context();
        assert!(!dispatcher.trap_tables_materialized);
        assert_eq!(dispatcher.trap_table_profile, None);
        assert_eq!(dispatcher.trap_exception_vector_defaults, None);
        assert!(dispatcher.pending_native_trap_calls.is_empty());
        assert_eq!(dispatcher.current_trap_caller, None);
    }

    #[test]
    fn machine_profiles_classify_only_aa6e_as_unimplemented() {
        let declared = default_trap_route(0xAA6E);
        assert!(declared.allows(TrapAdapterId::Unimplemented));
        assert!(!declared.allows(TrapAdapterId::Toolbox));

        for profile in [TrapTableProfile::M68k68040, TrapTableProfile::PowerPc604] {
            let (mut dispatcher, _cpu, mut bus) = setup();
            dispatcher.materialize_trap_tables(&mut bus, profile);
            let unimplemented = dispatcher.trap_table_address(&bus, 0xAA6E).unwrap();
            let mut matching_slots = Vec::new();

            for slot in 0..OS_TRAP_TABLE_SLOTS {
                let word = 0xA000 | slot;
                if dispatcher.trap_table_address(&bus, word) == Some(unimplemented) {
                    matching_slots.push(word);
                }
            }
            for slot in 0..TOOLBOX_TRAP_TABLE_SLOTS {
                let word = 0xA800 | slot;
                if dispatcher.trap_table_address(&bus, word) == Some(unimplemented) {
                    matching_slots.push(word);
                }
            }

            assert_eq!(matching_slots, [0xAA6E]);
            assert_eq!(bus.read_word(unimplemented), 0xAE6E);
            assert_ne!(
                dispatcher.trap_table_address(&bus, 0xAA57),
                Some(unimplemented)
            );
        }
    }

    #[test]
    fn aa6e_uses_the_terminal_unimplemented_adapter() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::DS_ERR_CODE, 0xBEEF);

        let result = dispatcher.dispatch(0xAA6E, &mut cpu, &mut bus);

        assert!(matches!(result, Err(crate::Error::Halted)));
        assert_eq!(dispatcher.current_trap_operation, 0xAA6E);
        assert_eq!(
            dispatcher.current_trap_adapter,
            TrapAdapterId::Unimplemented
        );
        assert_eq!(bus.read_word(crate::memory::globals::addr::DS_ERR_CODE), 12);
    }

    #[test]
    fn machine_profiles_materialize_only_their_reviewed_default_pointer_aliases() {
        let aliases = [(0xA87D, 0xAA02), (0xAA08, 0xAA26)];

        let (mut dispatcher, _cpu, mut bus) = setup();
        dispatcher.materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040);
        for (target, alias) in aliases {
            assert_eq!(
                dispatcher.trap_table_address(&bus, target),
                dispatcher.trap_table_address(&bus, alias),
                "68040 defaults ${target:04X}/${alias:04X}"
            );
            assert_eq!(
                TrapTableProfile::M68k68040
                    .route(alias)
                    .default_gateway_word,
                target
            );
        }

        let second =
            dispatcher.create_trap_table_process_context(&mut bus, TrapTableProfile::PowerPc604);
        let _first = dispatcher
            .switch_trap_table_process_context(&mut bus, second)
            .expect("68040 context must be saved");
        for (target, alias) in aliases {
            assert_ne!(
                dispatcher.trap_table_address(&bus, target),
                dispatcher.trap_table_address(&bus, alias),
                "604 defaults ${target:04X}/${alias:04X}"
            );
            assert_eq!(
                TrapTableProfile::PowerPc604
                    .route(alias)
                    .default_gateway_word,
                alias
            );
        }
    }

    #[test]
    fn a_profile_default_alias_keeps_independent_patch_and_restore_state() {
        let (mut dispatcher, _cpu, mut bus) = setup();
        dispatcher.materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040);

        for (target, alias, patch) in [(0xA87D, 0xAA02, 0x0021_0000), (0xAA08, 0xAA26, 0x0021_1000)]
        {
            let shared_default = dispatcher.trap_table_address(&bus, alias).unwrap();
            assert_eq!(
                dispatcher.trap_table_address(&bus, target),
                Some(shared_default)
            );

            dispatcher.install_trap_address(&mut bus, alias, patch);
            assert_eq!(dispatcher.native_trap_handler(&bus, alias), Some(patch));
            assert_eq!(
                dispatcher.trap_table_address(&bus, target),
                Some(shared_default),
                "patching alias ${alias:04X} must not patch ${target:04X}"
            );

            dispatcher.install_trap_address(&mut bus, alias, shared_default);
            assert_eq!(dispatcher.native_trap_handler(&bus, alias), None);
            assert_eq!(
                dispatcher.trap_table_address(&bus, alias),
                Some(shared_default)
            );
        }
    }

    #[test]
    fn saved_closecport_alias_gateway_executes_the_shared_default_procedure() {
        // Universal Interfaces 3.4 names $AA02 as CloseCPort, while Inside
        // Macintosh Volume V, V-72/V-291 records $A87D. The selected 68040
        // profile exposes one default address for those slots. Calling the
        // address saved from $AA02 must therefore execute the shared $A87D
        // procedure and retain its one-CGrafPtr Pascal stack contract.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040);
        let gateway = dispatcher.trap_table_address(&bus, 0xAA02).unwrap();
        let return_pc = 0x0020_0000;
        let sp = 0x003F_FF00;

        assert_eq!(dispatcher.trap_table_address(&bus, 0xA87D), Some(gateway));
        assert_eq!(bus.read_word(gateway), 0xAC7D);
        bus.write_long(sp, return_pc);
        bus.write_long(sp + 4, 0); // NIL CGrafPtr
        cpu.write_reg(Register::PC, gateway + 2);
        cpu.write_reg(Register::A7, sp);

        dispatcher
            .dispatch(bus.read_word(gateway), &mut cpu, &mut bus)
            .unwrap();

        assert_eq!(cpu.read_reg(Register::PC), return_pc);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert!(dispatcher.pending_native_trap_calls.is_empty());
    }

    #[test]
    fn trap_manager_mutates_hidden_successor_without_replacing_raw_head() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040);
        let trap_word = 0xA078;
        let raw_entry = TrapDispatcher::raw_trap_table_entry(trap_word);
        let head = bus.read_long(raw_entry);
        let original = bus.read_long(head + 4);
        let first = 0x0021_0000;
        let nested = 0x0021_1000;

        bus.write_long(head + 4, 0xDEAD_BEEF);
        assert_eq!(bus.read_long(head + 4), original);

        for handler in [first, nested, first, original] {
            cpu.write_reg(Register::D0, u32::from(trap_word));
            cpu.write_reg(Register::A0, handler);
            dispatcher.dispatch(0xA247, &mut cpu, &mut bus).unwrap();
            assert_eq!(bus.read_long(raw_entry), head);
            assert_eq!(bus.read_long(head + 4), handler);

            cpu.write_reg(Register::D0, u32::from(trap_word));
            dispatcher.dispatch(0xA346, &mut cpu, &mut bus).unwrap();
            assert_eq!(cpu.read_reg(Register::A0), handler);
        }
        assert!(!dispatcher.has_native_trap_patch(&bus, trap_word));
    }

    #[test]
    fn trap_manager_mutates_the_last_exit_in_a_multi_head_chain() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040);
        let trap_word = 0xA078;
        let raw_entry = TrapDispatcher::raw_trap_table_entry(trap_word);
        let first = bus.read_long(raw_entry);
        let original = bus.read_long(first + 4);
        let second = bus.alloc_synthetic(10);
        bus.write_readonly_code_word(second, 0x6006);
        bus.write_readonly_code_word(second + 2, 0x4EF9);
        TrapDispatcher::write_readonly_code_long(&mut bus, second + 4, original);
        bus.write_readonly_code_word(second + 8, 0x60F8);
        bus.protect_readonly_code(second, 10);
        TrapDispatcher::write_readonly_code_long(&mut bus, first + 4, second);

        cpu.write_reg(Register::D0, u32::from(trap_word));
        dispatcher.dispatch(0xA346, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), original);

        let replacement = 0x0021_0000;
        cpu.write_reg(Register::D0, u32::from(trap_word));
        cpu.write_reg(Register::A0, replacement);
        dispatcher.dispatch(0xA247, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_long(raw_entry), first);
        assert_eq!(bus.read_long(first + 4), second);
        assert_eq!(bus.read_long(second + 4), replacement);
        assert_eq!(
            dispatcher.native_trap_handler(&bus, trap_word),
            Some(replacement)
        );
    }

    #[test]
    fn direct_raw_write_can_bypass_a_permanent_come_from_head() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040);
        let trap_word = 0xAAFB;
        let raw_entry = TrapDispatcher::raw_trap_table_entry(trap_word);
        let old_head = bus.read_long(raw_entry);
        let direct = 0x0021_0000;
        let replacement = 0x0021_1000;

        bus.write_long(raw_entry, direct);
        assert_eq!(
            dispatcher.native_trap_handler(&bus, trap_word),
            Some(direct)
        );

        cpu.write_reg(Register::D0, u32::from(trap_word));
        cpu.write_reg(Register::A0, replacement);
        dispatcher.dispatch(0xA647, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_long(raw_entry), replacement);
        assert_eq!(
            bus.read_long(old_head + 4),
            dispatcher.default_trap_gateway(trap_word).unwrap()
        );
    }

    #[test]
    fn trap_setter_preserves_an_arbitrary_00f0_pointer_exactly() {
        // Inside Macintosh: Operating System Utilities (1994), pp. 8-29--8-31:
        // Set/NSet installs the supplied address; no guest address range is a
        // host-only restoration token.
        let (mut dispatcher, _cpu, mut bus) = setup();
        dispatcher.materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040);
        let trap_word = 0xA004;
        let handler = 0x00F0_A004;

        dispatcher.install_trap_address(&mut bus, trap_word, handler);

        assert_eq!(
            dispatcher.trap_table_address(&bus, trap_word),
            Some(handler)
        );
        assert_eq!(
            dispatcher.native_trap_handler(&bus, trap_word),
            Some(handler)
        );
    }

    #[test]
    fn nset_rejects_a_come_from_head_as_the_new_handler() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040);
        let trap_word = 0xA078;
        let raw_entry = TrapDispatcher::raw_trap_table_entry(trap_word);
        let head = bus.read_long(raw_entry);

        cpu.write_reg(Register::D0, u32::from(trap_word));
        cpu.write_reg(Register::A0, head);
        let result = dispatcher.dispatch(0xA247, &mut cpu, &mut bus);

        assert!(matches!(result, Err(crate::Error::Halted)));
        assert_eq!(bus.read_word(crate::memory::globals::addr::DS_ERR_CODE), 12);
        assert_eq!(bus.read_long(raw_entry), head);
        assert!(!dispatcher.has_native_trap_patch(&bus, trap_word));
    }

    #[test]
    fn direct_raw_table_write_is_authoritative_for_dispatch() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040);
        let handler = 0x0021_0000;
        let return_pc = 0x0020_0002;
        let sp = 0x003F_FF00;
        bus.write_long(TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4, handler);
        cpu.write_reg(Register::PC, return_pc);
        cpu.write_reg(Register::A7, sp);

        dispatcher.dispatch(0xA975, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::PC), handler);
        assert_eq!(cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(bus.read_long(sp - 4), return_pc);
        assert!(dispatcher.has_native_trap_patch(&bus, 0xA975));
    }

    #[test]
    fn trap_manager_apis_and_raw_table_long_stay_coherent() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040);
        let entry_address = TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4;
        let default = bus.read_long(entry_address);
        let handler = 0x0021_0000;

        cpu.write_reg(Register::D0, 0xA975);
        cpu.write_reg(Register::A0, handler);
        dispatcher.dispatch(0xA047, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_long(entry_address), handler);

        cpu.write_reg(Register::D0, 0xA975);
        dispatcher.dispatch(0xA146, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), handler);

        cpu.write_reg(Register::D0, 0xA975);
        cpu.write_reg(Register::A0, default);
        dispatcher.dispatch(0xA047, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_long(entry_address), default);
        assert!(!dispatcher.has_native_trap_patch(&bus, 0xA975));
    }

    fn make_single_resource_fork_bytes(res_type: [u8; 4], res_id: i16, data: &[u8]) -> Vec<u8> {
        make_single_resource_fork_bytes_with_attrs(res_type, res_id, data, 0)
    }

    fn make_single_resource_fork_bytes_with_attrs(
        res_type: [u8; 4],
        res_id: i16,
        data: &[u8],
        attrs: u8,
    ) -> Vec<u8> {
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
        bytes[map_start + 24..map_start + 26].copy_from_slice(&type_list_offset.to_be_bytes());
        bytes[map_start + 26..map_start + 28].copy_from_slice(&name_list_offset.to_be_bytes());

        let type_list_start = map_start + type_list_offset as usize;
        bytes[type_list_start..type_list_start + 2].copy_from_slice(&0u16.to_be_bytes());
        bytes[type_list_start + 2..type_list_start + 6].copy_from_slice(&res_type);
        bytes[type_list_start + 6..type_list_start + 8].copy_from_slice(&0u16.to_be_bytes());
        bytes[type_list_start + 8..type_list_start + 10]
            .copy_from_slice(&ref_list_offset.to_be_bytes());

        let ref_list_start = map_start + type_list_offset as usize + ref_list_offset as usize;
        bytes[ref_list_start..ref_list_start + 2].copy_from_slice(&(res_id as u16).to_be_bytes());
        bytes[ref_list_start + 2..ref_list_start + 4].copy_from_slice(&0xFFFFu16.to_be_bytes());
        bytes[ref_list_start + 4] = attrs;
        bytes[ref_list_start + 5..ref_list_start + 8].copy_from_slice(&0u32.to_be_bytes()[1..4]);

        bytes
    }

    fn minimal_test_nfnt() -> Vec<u8> {
        let mut bytes = vec![0u8; 38];
        bytes[2..4].copy_from_slice(&32u16.to_be_bytes());
        bytes[4..6].copy_from_slice(&32u16.to_be_bytes());
        bytes[6..8].copy_from_slice(&1u16.to_be_bytes());
        bytes[14..16].copy_from_slice(&1u16.to_be_bytes());
        bytes[16..18].copy_from_slice(&9u16.to_be_bytes());
        bytes[18..20].copy_from_slice(&1u16.to_be_bytes());
        bytes[24..26].copy_from_slice(&1u16.to_be_bytes());
        bytes[26] = 0xC0;
        bytes[28..30].copy_from_slice(&0u16.to_be_bytes());
        bytes[30..32].copy_from_slice(&1u16.to_be_bytes());
        bytes[32..34].copy_from_slice(&2u16.to_be_bytes());
        bytes[34..36].copy_from_slice(&1u16.to_be_bytes());
        bytes[36..38].copy_from_slice(&1u16.to_be_bytes());
        bytes
    }

    fn test_fond(font_resource_id: i16, size: i16) -> Vec<u8> {
        let mut bytes = vec![0u8; 60];
        bytes[52..54].copy_from_slice(&0u16.to_be_bytes()); // one association minus one
        bytes[54..56].copy_from_slice(&(size as u16).to_be_bytes());
        bytes[56..58].copy_from_slice(&0u16.to_be_bytes()); // plain style
        bytes[58..60].copy_from_slice(&(font_resource_id as u16).to_be_bytes());
        bytes
    }

    #[test]
    fn native_trap_dispatch_returns_past_the_a_line_opcode() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let trap_pc = 0x0020_0000u32;
        let handler_addr = 0x0021_0000u32;
        let sp = 0x003F_FF00u32;
        dispatcher.native_trap_table.insert(0xA9F0, handler_addr);
        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(Register::A7, sp);

        dispatcher.dispatch(0xA9F0, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::PC), handler_addr);
        assert_eq!(cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(bus.read_long(sp - 4), trap_pc + 2);
    }

    #[test]
    fn os_hle_dispatch_enforces_every_structural_variant_frame() {
        // The OS Trap Dispatcher places the actual word in D1, restores
        // D1/D2/A1/A2 and conditionally A0, leaves the stack unchanged, and
        // performs TST.W D0. Inside Macintosh: Operating System Utilities
        // (1994), pp. 8-11--8-13.
        for variant in 0u16..8 {
            let (mut dispatcher, mut cpu, mut bus) = setup();
            let trap_word = 0xA01E | (variant << 8); // NewPtr flag/A0 forms
            let original_d1 = 0xD1D1_BEEF;
            let original_d2 = 0xD2D2_BEEF;
            let original_a0 = 0xA0A0_BEEF;
            let original_a1 = 0xA1A1_BEEF;
            let original_a2 = 0xA2A2_BEEF;
            let original_sp = cpu.read_reg(Register::A7);
            cpu.write_reg(Register::D0, 4);
            cpu.write_reg(Register::D1, original_d1);
            cpu.write_reg(Register::D2, original_d2);
            cpu.write_reg(Register::A0, original_a0);
            cpu.write_reg(Register::A1, original_a1);
            cpu.write_reg(Register::A2, original_a2);
            cpu.set_ccr(0x1F);

            dispatcher.dispatch(trap_word, &mut cpu, &mut bus).unwrap();

            assert_eq!(dispatcher.current_trap_word, trap_word);
            assert_eq!(cpu.read_reg(Register::D1), original_d1);
            assert_eq!(cpu.read_reg(Register::D2), original_d2);
            assert_eq!(cpu.read_reg(Register::A1), original_a1);
            assert_eq!(cpu.read_reg(Register::A2), original_a2);
            assert_eq!(cpu.read_reg(Register::A7), original_sp);
            if (trap_word & 0x0100) == 0 {
                assert_eq!(cpu.read_reg(Register::A0), original_a0);
            } else {
                assert_ne!(cpu.read_reg(Register::A0), original_a0);
                assert_ne!(cpu.read_reg(Register::A0), 0);
            }
            assert_eq!(cpu.read_reg(Register::D0), 0);
            assert_eq!(cpu.get_ccr(), 0x14, "variant ${trap_word:04X}");
        }
    }

    #[test]
    fn native_os_patch_receives_and_retires_every_structural_variant_frame() {
        let handler_addr = 0x0021_0000u32;
        let return_pc = 0x0020_0002u32;
        let sp = 0x003F_FF00u32;
        for variant in 0u16..8 {
            let (mut dispatcher, mut cpu, mut bus) = setup();
            let trap_word = 0xA039 | (variant << 8); // ReadDateTime variants
            let original_d1 = 0xD1D1_BEEF;
            let original_d2 = 0xD2D2_BEEF;
            let original_a0 = 0xA0A0_BEEF;
            let original_a1 = 0xA1A1_BEEF;
            let original_a2 = 0xA2A2_BEEF;
            dispatcher.native_trap_table.insert(0xA039, handler_addr);
            cpu.write_reg(Register::PC, return_pc);
            cpu.write_reg(Register::A7, sp);
            cpu.write_reg(Register::D1, original_d1);
            cpu.write_reg(Register::D2, original_d2);
            cpu.write_reg(Register::A0, original_a0);
            cpu.write_reg(Register::A1, original_a1);
            cpu.write_reg(Register::A2, original_a2);

            dispatcher.dispatch(trap_word, &mut cpu, &mut bus).unwrap();

            assert_eq!(cpu.read_reg(Register::PC), handler_addr);
            assert_eq!(cpu.read_reg(Register::A7), sp - 4);
            assert_eq!(bus.read_long(sp - 4), return_pc);
            assert_eq!(
                cpu.read_reg(Register::D1),
                0xD1D1_0000 | u32::from(trap_word)
            );
            assert_eq!(
                dispatcher
                    .pending_native_trap_calls
                    .get(&0xA039)
                    .and_then(|calls| calls.last())
                    .and_then(|call| call.os_dispatch_frame)
                    .map(|frame| frame.trap_word),
                Some(trap_word)
            );

            cpu.write_reg(Register::D0, 0xCAFE_8000);
            cpu.write_reg(Register::D1, 0x1111_1111);
            cpu.write_reg(Register::D2, 0x2222_2222);
            cpu.write_reg(Register::A0, 0xAAAA_AAAA);
            cpu.write_reg(Register::A1, 0x1111_AAAA);
            cpu.write_reg(Register::A2, 0x2222_AAAA);
            cpu.write_reg(Register::PC, return_pc);
            cpu.write_reg(Register::A7, sp);
            cpu.set_ccr(0x1F);
            dispatcher.retire_returned_native_trap_call(&mut cpu);

            assert!(dispatcher.pending_native_trap_calls.is_empty());
            assert_eq!(cpu.read_reg(Register::D0), 0xCAFE_8000);
            assert_eq!(cpu.read_reg(Register::D1), original_d1);
            assert_eq!(cpu.read_reg(Register::D2), original_d2);
            assert_eq!(cpu.read_reg(Register::A1), original_a1);
            assert_eq!(cpu.read_reg(Register::A2), original_a2);
            assert_eq!(
                cpu.read_reg(Register::A0),
                if (trap_word & 0x0100) == 0 {
                    original_a0
                } else {
                    0xAAAA_AAAA
                }
            );
            assert_eq!(cpu.get_ccr(), 0x18, "variant ${trap_word:04X}");
        }
    }

    #[test]
    fn saved_os_gateway_tail_uses_the_original_variant_dispatch_frame() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let gateway = dispatcher.get_or_create_os_trap_trampoline(&mut bus, 0xA01E);
        let handler = 0x0021_0000u32;
        let return_pc = 0x0020_0002u32;
        let sp = 0x003F_FF00u32;
        let trap_word = 0xA71E; // NewPtrSysClear, returning A0
        let original_d1 = 0xD1D1_BEEF;
        let original_d2 = 0xD2D2_BEEF;
        let original_a1 = 0xA1A1_BEEF;
        let original_a2 = 0xA2A2_BEEF;
        dispatcher.native_trap_table.insert(0xA01E, handler);
        cpu.write_reg(Register::PC, return_pc);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 4);
        cpu.write_reg(Register::D1, original_d1);
        cpu.write_reg(Register::D2, original_d2);
        cpu.write_reg(Register::A0, 0xA0A0_BEEF);
        cpu.write_reg(Register::A1, original_a1);
        cpu.write_reg(Register::A2, original_a2);

        dispatcher.dispatch(trap_word, &mut cpu, &mut bus).unwrap();
        cpu.write_reg(Register::D1, 0x1111_1111);
        cpu.write_reg(Register::D2, 0x2222_2222);
        cpu.write_reg(Register::A0, 0xAAAA_AAAA);
        cpu.write_reg(Register::A1, 0x1111_AAAA);
        cpu.write_reg(Register::A2, 0x2222_AAAA);
        cpu.write_reg(Register::PC, gateway + 2);

        dispatcher.dispatch(0xA01E, &mut cpu, &mut bus).unwrap();

        let result_ptr = cpu.read_reg(Register::A0);
        assert_eq!(dispatcher.current_trap_word, trap_word);
        assert_ne!(result_ptr, 0);
        assert_ne!(result_ptr, 0xAAAA_AAAA);
        assert_eq!(bus.read_long(result_ptr), 0);
        assert_eq!(cpu.read_reg(Register::D1), original_d1);
        assert_eq!(cpu.read_reg(Register::D2), original_d2);
        assert_eq!(cpu.read_reg(Register::A1), original_a1);
        assert_eq!(cpu.read_reg(Register::A2), original_a2);
        assert_eq!(cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(cpu.get_ccr(), 0x04);
        assert!(dispatcher.pending_native_trap_calls.is_empty());
    }

    #[test]
    fn saved_os_gateway_subroutine_keeps_the_outer_dispatch_frame() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let gateway = dispatcher.get_or_create_os_trap_trampoline(&mut bus, 0xA039);
        let handler = 0x0021_0000u32;
        let patch_continuation = 0x0021_0100u32;
        let return_pc = 0x0020_0002u32;
        let output = 0x0022_0000u32;
        let sp = 0x003F_FF00u32;
        let original_d1 = 0xD1D1_BEEF;
        let original_d2 = 0xD2D2_BEEF;
        let original_a0 = 0xA0A0_BEEF;
        let original_a1 = 0xA1A1_BEEF;
        let original_a2 = 0xA2A2_BEEF;
        dispatcher.native_trap_table.insert(0xA039, handler);
        bus.write_long(crate::memory::globals::addr::TIME, 0x1234_5678);
        cpu.write_reg(Register::PC, return_pc);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D1, original_d1);
        cpu.write_reg(Register::D2, original_d2);
        cpu.write_reg(Register::A0, original_a0);
        cpu.write_reg(Register::A1, original_a1);
        cpu.write_reg(Register::A2, original_a2);

        dispatcher.dispatch(0xA039, &mut cpu, &mut bus).unwrap();
        let nested_sp = sp - 8;
        bus.write_long(nested_sp, patch_continuation);
        cpu.write_reg(Register::D1, 0x1111_1111);
        cpu.write_reg(Register::D2, 0x2222_2222);
        cpu.write_reg(Register::A0, output);
        cpu.write_reg(Register::A1, 0x1111_AAAA);
        cpu.write_reg(Register::A2, 0x2222_AAAA);
        cpu.write_reg(Register::A7, nested_sp);
        cpu.write_reg(Register::PC, gateway + 2);

        dispatcher.dispatch(0xA039, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_long(output), 0x1234_5678);
        assert_eq!(cpu.read_reg(Register::D1), 0x1111_1111);
        assert_eq!(cpu.read_reg(Register::D2), 0x2222_2222);
        assert_eq!(cpu.read_reg(Register::A0), output);
        assert_eq!(cpu.read_reg(Register::A1), 0x1111_AAAA);
        assert_eq!(cpu.read_reg(Register::A2), 0x2222_AAAA);
        assert_eq!(cpu.read_reg(Register::A7), nested_sp);
        assert_eq!(
            dispatcher
                .pending_native_trap_calls
                .get(&0xA039)
                .map(Vec::len),
            Some(1)
        );

        // The saved routine's RTS would return to the patch, whose final RTS
        // then closes the original dispatcher frame.
        cpu.write_reg(Register::D0, 0xCAFE_8000);
        cpu.write_reg(Register::D1, 0x3333_3333);
        cpu.write_reg(Register::D2, 0x4444_4444);
        cpu.write_reg(Register::A0, 0xBBBB_BBBB);
        cpu.write_reg(Register::A1, 0x3333_AAAA);
        cpu.write_reg(Register::A2, 0x4444_AAAA);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        cpu.set_ccr(0x1F);
        dispatcher.retire_returned_native_trap_call(&mut cpu);

        assert!(dispatcher.pending_native_trap_calls.is_empty());
        assert_eq!(cpu.read_reg(Register::D0), 0xCAFE_8000);
        assert_eq!(cpu.read_reg(Register::D1), original_d1);
        assert_eq!(cpu.read_reg(Register::D2), original_d2);
        assert_eq!(cpu.read_reg(Register::A0), original_a0);
        assert_eq!(cpu.read_reg(Register::A1), original_a1);
        assert_eq!(cpu.read_reg(Register::A2), original_a2);
        assert_eq!(cpu.get_ccr(), 0x18);
    }

    #[test]
    fn native_auto_pop_trap_enters_patch_with_the_glue_callers_return_frame() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let trap_pc = 0x0020_0000u32;
        let handler_addr = 0x0021_0000u32;
        let caller_pc = 0x0022_0000u32;
        let sp = 0x003F_FF00u32;
        dispatcher.native_trap_table.insert(0xA975, handler_addr);
        bus.write_long(sp, caller_pc);
        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(Register::A7, sp);

        dispatcher.dispatch(0xAD75, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::PC), handler_addr);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(bus.read_long(sp), caller_pc);
        let calls = dispatcher.pending_native_trap_calls.get(&0xA975).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].return_pc, caller_pc);
        assert_eq!(calls[0].argument_sp, sp + 4);
    }

    #[test]
    fn nested_same_native_trap_calls_retain_lifo_call_state() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let handler_addr = 0x0021_0000u32;
        let outer_return = 0x0020_0002u32;
        let inner_return = 0x0021_0102u32;
        let outer_sp = 0x003F_FF00u32;
        let inner_sp = 0x003F_FE00u32;
        dispatcher.native_trap_table.insert(0xA9F0, handler_addr);

        cpu.write_reg(Register::PC, outer_return);
        cpu.write_reg(Register::A7, outer_sp);
        dispatcher.dispatch(0xA9F0, &mut cpu, &mut bus).unwrap();

        cpu.write_reg(Register::PC, inner_return);
        cpu.write_reg(Register::A7, inner_sp);
        bus.write_long(inner_sp, inner_return);
        dispatcher.dispatch(0xADF0, &mut cpu, &mut bus).unwrap();

        let calls = dispatcher.pending_native_trap_calls.get(&0xA9F0).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].return_pc, outer_return);
        assert_eq!(calls[1].return_pc, inner_return);
        assert_eq!(calls[1].argument_sp, inner_sp + 4);
        assert_eq!(
            dispatcher
                .take_latest_native_trap_call(0xA9F0)
                .unwrap()
                .return_pc,
            inner_return
        );
        assert_eq!(
            dispatcher
                .take_latest_native_trap_call(0xA9F0)
                .unwrap()
                .return_pc,
            outer_return
        );
        assert!(!dispatcher.pending_native_trap_calls.contains_key(&0xA9F0));
    }

    #[test]
    fn saved_tool_trap_gateway_bypasses_a_later_patch() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let gateway = dispatcher.get_or_create_tool_trap_trampoline(&mut bus, 0xA975);
        let caller_pc = 0x0021_0000u32;
        let sp = 0x003F_FF00u32;
        dispatcher.tick_count = 0x1234_5678;
        dispatcher.native_trap_table.insert(0xA975, 0x0030_0000);
        bus.write_long(sp, caller_pc);
        cpu.write_reg(Register::PC, 0x0020_0002);
        cpu.write_reg(Register::A7, sp);

        dispatcher.dispatch(0xAD75, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::PC), 0x0030_0000);
        assert_eq!(
            dispatcher
                .pending_native_trap_calls
                .get(&0xA975)
                .unwrap()
                .len(),
            1
        );

        cpu.write_reg(Register::PC, gateway + 2);
        dispatcher.dispatch(0xAD75, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::PC), caller_pc);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_long(sp + 4), 0x1234_5678);
        assert!(dispatcher.pending_native_trap_calls.is_empty());
    }

    #[test]
    fn saved_os_trap_gateway_bypasses_a_later_patch() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let gateway = dispatcher.get_or_create_os_trap_trampoline(&mut bus, 0xA039);
        let sp = 0x003F_FF00u32;
        let output = 0x0020_0000u32;
        bus.write_long(crate::memory::globals::addr::TIME, 0x1234_5678);
        bus.write_long(sp, 0x0021_0000);
        dispatcher.native_trap_table.insert(0xA039, 0x0030_0000);
        cpu.write_reg(Register::PC, gateway + 2);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::A0, output);

        dispatcher.dispatch(0xA039, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::PC), gateway + 2);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(output), 0x1234_5678);
        assert!(dispatcher.pending_native_trap_calls.is_empty());
    }

    #[test]
    fn fond_associations_register_nfnt_independent_of_resource_load_order() {
        let nfnt = minimal_test_nfnt();

        let mut fond_first = TrapDispatcher::new();
        fond_first.remember_resource_backing_data(7, *b"FOND", 30010, test_fond(42, 15));
        fond_first.remember_resource_backing_data(7, *b"NFNT", 42, nfnt.clone());
        let face = crate::quickdraw::fonts::get_font_face(30010, 15)
            .expect("FOND loaded before NFNT should register the associated face");
        assert_eq!((face.font_id, face.size), (30010, 15));

        let mut nfnt_first = TrapDispatcher::new();
        nfnt_first.remember_resource_backing_data(8, *b"NFNT", 43, nfnt);
        nfnt_first.remember_resource_backing_data(8, *b"FOND", 30011, test_fond(43, 16));
        let face = crate::quickdraw::fonts::get_font_face(30011, 16)
            .expect("FOND loaded after NFNT should register the associated face");
        assert_eq!((face.font_id, face.size), (30011, 16));
    }

    #[test]
    fn hle_tick_cost_accumulates_and_resets() {
        let mut disp = TrapDispatcher::new();

        disp.add_hle_tick_cost(123);
        disp.add_hle_tick_cost(456);

        assert_eq!(disp.take_hle_tick_cost(), 579);
        assert_eq!(disp.take_hle_tick_cost(), 0);
    }

    #[test]
    fn hle_work_cost_helpers_scale_with_resource_and_pixel_work() {
        let small_resource = TrapDispatcher::resource_load_tick_cost(128);
        let large_resource = TrapDispatcher::resource_load_tick_cost(4096);
        let small_blit = TrapDispatcher::quickdraw_blit_tick_cost(16, 16, 8, 8, false);
        let large_blit = TrapDispatcher::quickdraw_blit_tick_cost(320, 200, 8, 8, false);
        let transformed_blit = TrapDispatcher::quickdraw_blit_tick_cost(320, 200, 8, 8, true);
        let picture = TrapDispatcher::draw_picture_tick_cost(320, 200, 32_768);

        assert!(large_resource > small_resource);
        assert!(large_blit > small_blit);
        assert!(transformed_blit > large_blit);
        assert!(picture > large_blit);
    }

    fn install_menu_tracking(disp: &mut TrapDispatcher) {
        *disp.menu_tracking = Some(test_tracked_menu_state(0, (0, 0, 0, 0), 0));
    }

    fn install_dialog_tracking(disp: &mut TrapDispatcher) {
        disp.dialog_tracking = Some(DialogTrackingState {
            dialog_ptr: 0,
            bounds: (0, 0, 0, 0),
            title: String::new(),
            proc_id: 0,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Vec::new(),
            stack_ptr: 0,
            item_hit_ptr: 0,
            rendered_pixels: Vec::new(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: true,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });
    }

    fn install_control_tracking(disp: &mut TrapDispatcher) {
        disp.control_tracking = Some(ControlTrackingState {
            ctrl_handle: 0,
            ctrl_ptr: 0,
            popup_tracking: true,
            active_menu: 0,
            highlighted_item: 0,
            saved_pixels: Vec::new(),
            dropdown_rect: (0, 0, 0, 0),
            simple_part: 0,
            simple_screen_rect: (0, 0, 0, 0),
            simple_highlighted: false,
            saved_hilite: 0,
            stack_ptr: 0,
            scrollbar_action_proc: 0,
            scrollbar_part: 0,
            scrollbar_last_action_tick: 0,
            scrollbar_idle_refires: 0,
            scrollbar_callback_pending: false,
        });
    }

    fn install_window_tracking(disp: &mut TrapDispatcher) {
        disp.window_tracking = Some(WindowTrackingState {
            window_ptr: 0,
            stack_ptr: 0,
            start_mouse: (0, 0),
            original_port_origin: (0, 0),
            bounds_rect: (0, 0, 0, 0),
            original_outline_rect: (0, 0, 0, 0),
            outline_rect: (0, 0, 0, 0),
            outline_saved_pixels: Vec::new(),
            command_down: false,
        });
    }

    fn install_go_away_tracking(disp: &mut TrapDispatcher) {
        disp.go_away_tracking = Some(GoAwayTrackingState {
            window_ptr: 0,
            stack_ptr: 0,
            hit_rect: (0, 0, 18, 18),
            highlight_rect: (3, 8, 14, 19),
            highlighted: true,
        });
    }

    fn install_region_tracking(disp: &mut TrapDispatcher) {
        disp.region_tracking = Some(RegionTrackingState {
            stack_ptr: 0,
            start_mouse: (0, 0),
            port_bounds_origin: (0, 0),
            limit_rect: None,
            slop_rect: (0, 0, 1, 1),
            axis: 0,
            original_outline_rect: (0, 0, 1, 1),
            outline_rect: None,
            outline_saved_pixels: Vec::new(),
            outline_pattern: [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
        });
    }

    fn centered_playfield_rect() -> ScreenCopyBitsRect {
        ScreenCopyBitsRect {
            src_top: 0,
            src_left: 0,
            src_bottom: 400,
            src_right: 640,
            dst_top: 100,
            dst_left: 80,
            dst_bottom: 500,
            dst_right: 720,
        }
    }

    #[test]
    fn fullscreen_input_transform_requires_fullscreen_and_hidden_cursor() {
        let mut disp = TrapDispatcher::new();
        disp.screen_mode = (0, 1000, 800, 600, 8);
        disp.last_screen_copybits_rect = Some(centered_playfield_rect());

        disp.fullscreen_locked = false;
        disp.cursor_state.level = -1;
        assert_eq!(disp.fullscreen_input_transform(), None);

        disp.fullscreen_locked = true;
        disp.cursor_state.level = 0;
        assert_eq!(disp.fullscreen_input_transform(), None);

        disp.cursor_state.level = -1;
        assert_eq!(
            disp.fullscreen_input_transform(),
            Some(centered_playfield_rect())
        );
    }

    #[test]
    fn fullscreen_input_transform_rejects_identity_fullscreen_blit() {
        let mut disp = TrapDispatcher::new();
        disp.screen_mode = (0, 1000, 800, 600, 8);
        disp.fullscreen_locked = true;
        disp.cursor_state.level = -1;
        disp.last_screen_copybits_rect = Some(ScreenCopyBitsRect {
            src_top: 0,
            src_left: 0,
            src_bottom: 600,
            src_right: 800,
            dst_top: 0,
            dst_left: 0,
            dst_bottom: 600,
            dst_right: 800,
        });

        assert_eq!(disp.fullscreen_input_transform(), None);
    }

    #[test]
    fn fullscreen_input_transform_rejects_invalid_copybits_rect() {
        let mut disp = TrapDispatcher::new();
        disp.screen_mode = (0, 1000, 800, 600, 8);
        disp.fullscreen_locked = true;
        disp.cursor_state.level = -1;
        disp.last_screen_copybits_rect = Some(ScreenCopyBitsRect {
            src_top: 0,
            src_left: 0,
            src_bottom: 0,
            src_right: 640,
            dst_top: 100,
            dst_left: 80,
            dst_bottom: 500,
            dst_right: 720,
        });

        assert_eq!(disp.fullscreen_input_transform(), None);
    }

    #[test]
    fn find_vfs_file_in_directory_falls_back_from_colon_path_to_basename() {
        let mut disp = TrapDispatcher::new();
        disp.vfs
            .insert("Disk/App Folder/Settings".to_string(), vec![1, 2, 3]);
        let dir_id = disp.ensure_vfs_directory("Disk/App Folder");

        assert_eq!(
            disp.find_vfs_file_in_directory(dir_id, ":Resources:Settings"),
            Some("Disk/App Folder/Settings".to_string())
        );
    }

    #[test]
    fn hfs_path_encoding_preserves_literal_slashes_and_percent_sequences() {
        let encoded = TrapDispatcher::normalize_hfs_path("Folder:100%/Done");

        assert_eq!(encoded, format!("Folder/100%{VFS_HFS_LITERAL_SLASH}Done"));
        assert_eq!(
            TrapDispatcher::hfs_name_from_vfs_component(&format!(
                "100%{VFS_HFS_LITERAL_SLASH}Done"
            )),
            "100%/Done"
        );
    }

    #[test]
    fn hfs_path_encoding_strips_the_synthetic_unix_volume() {
        assert_eq!(
            TrapDispatcher::normalize_hfs_path("Unix:assertions.txt"),
            "assertions.txt"
        );
        assert_eq!(
            TrapDispatcher::normalize_hfs_path("Unix:Folder:100%/Done"),
            format!("Folder/100%{VFS_HFS_LITERAL_SLASH}Done")
        );
    }

    #[test]
    fn find_vfs_file_in_directory_does_not_escape_explicit_parent_for_basename() {
        // Files 1992, 2-29: the poor man's search path is used only when
        // dirID is 0; an explicit parent dirID must not fall through to an
        // unrelated file with the same basename elsewhere on the volume.
        let mut disp = TrapDispatcher::new();
        disp.vfs
            .insert("App/Shared Preferences".to_string(), vec![1, 2, 3]);
        let pref_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");

        assert_eq!(
            disp.find_vfs_file_in_directory(pref_dir_id, "Shared Preferences"),
            None
        );
    }

    #[test]
    fn find_vfs_directory_in_directory_does_not_escape_explicit_parent() {
        // Files 1992, 2-29: a nonzero parent directory ID suppresses the
        // poor man's search path. A same-named directory elsewhere on the
        // volume must not satisfy the lookup.
        let mut disp = TrapDispatcher::new();
        let simfarm_dir_id = disp.ensure_vfs_directory("Maxis/SimFarm");

        assert_eq!(
            disp.find_vfs_directory_in_directory(simfarm_dir_id, "SimFarm"),
            None
        );
    }

    #[test]
    fn find_vfs_rsrc_file_in_directory_falls_back_from_colon_path_to_basename() {
        let mut disp = TrapDispatcher::new();
        disp.vfs_rsrc
            .insert("Disk/App Folder/Companion.rsrc".to_string(), vec![1, 2, 3]);
        let dir_id = disp.ensure_vfs_directory("Disk/App Folder");

        assert_eq!(
            disp.find_vfs_rsrc_file_in_directory(dir_id, ":Resources:Companion.rsrc"),
            Some("Disk/App Folder/Companion.rsrc".to_string())
        );
    }

    #[test]
    fn find_vfs_rsrc_file_in_directory_does_not_escape_explicit_parent_for_basename() {
        // Same explicit-parent rule as data forks: a concrete dirID bounds
        // the lookup, so a resource fork with the same basename elsewhere
        // must not satisfy the request.
        let mut disp = TrapDispatcher::new();
        disp.vfs_rsrc
            .insert("App/Settings.rsrc".to_string(), vec![1, 2, 3]);
        let pref_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");

        assert_eq!(
            disp.find_vfs_rsrc_file_in_directory(pref_dir_id, "Settings.rsrc"),
            None
        );
    }

    #[test]
    fn remove_vfs_path_removes_data_resource_and_metadata_entries() {
        let mut disp = TrapDispatcher::new();
        disp.vfs.insert("Game/Plug-In".to_string(), vec![1, 2, 3]);
        disp.vfs_rsrc
            .insert("Game/Plug-In".to_string(), vec![4, 5, 6]);
        disp.set_vfs_entry_metadata("Game/Plug-In", *b"DATA", *b"TEST", 0x4000);

        assert!(disp.remove_vfs_path("Game/Plug-In"));
        assert!(!disp.vfs.contains_key("Game/Plug-In"));
        assert!(!disp.vfs_rsrc.contains_key("Game/Plug-In"));
        assert!(!disp.vfs_metadata.contains_key("Game/Plug-In"));
    }

    #[test]
    fn remove_vfs_path_removes_directory_subtree_without_touching_siblings() {
        let mut disp = TrapDispatcher::new();
        disp.ensure_vfs_directory("Game/Plug-Ins/MAGMA");
        disp.ensure_vfs_directory("Game/Plug-Ins/Keep");
        disp.vfs
            .insert("Game/Plug-Ins/MAGMA/Data".to_string(), vec![1]);
        disp.vfs_rsrc
            .insert("Game/Plug-Ins/MAGMA/Data".to_string(), vec![2]);
        disp.vfs
            .insert("Game/Plug-Ins/Keep/Data".to_string(), vec![3]);
        disp.set_vfs_entry_metadata("Game/Plug-Ins/MAGMA/Data", *b"DATA", *b"MAGM", 0);
        disp.set_vfs_entry_metadata("Game/Plug-Ins/Keep/Data", *b"DATA", *b"KEEP", 0);

        assert!(disp.remove_vfs_path("Game/Plug-Ins/MAGMA"));
        assert!(!disp.vfs.contains_key("Game/Plug-Ins/MAGMA/Data"));
        assert!(!disp.vfs_rsrc.contains_key("Game/Plug-Ins/MAGMA/Data"));
        assert!(!disp.vfs_metadata.contains_key("Game/Plug-Ins/MAGMA/Data"));
        assert!(!disp.vfs_directories.contains_key("Game/Plug-Ins/MAGMA"));
        assert!(disp.vfs.contains_key("Game/Plug-Ins/Keep/Data"));
        assert!(disp.vfs_metadata.contains_key("Game/Plug-Ins/Keep/Data"));
        assert!(disp.vfs_directories.contains_key("Game/Plug-Ins/Keep"));
    }

    #[test]
    fn remove_vfs_path_relative_to_launched_app_uses_app_parent() {
        let mut disp = TrapDispatcher::new();
        disp.vfs
            .insert("Game Folder/Plug-Ins/MAGMA".to_string(), vec![1]);
        disp.vfs_rsrc
            .insert("Game Folder/Plug-Ins/MAGMA".to_string(), vec![2]);
        disp.set_launched_app_path("Game Folder/Game App");

        assert!(disp.remove_vfs_path_relative_to_launched_app("Plug-Ins/MAGMA"));
        assert!(!disp.vfs.contains_key("Game Folder/Plug-Ins/MAGMA"));
        assert!(!disp.vfs_rsrc.contains_key("Game Folder/Plug-Ins/MAGMA"));
    }

    #[test]
    fn load_resources_exposes_application_map_and_fork_to_native_runtime_code() {
        let serialized = make_single_resource_fork_bytes_with_attrs(*b"TEST", 7, b"payload", 0x04);
        let fork = ResourceFork::parse(&serialized).unwrap();
        let reference_offset = fork.get(*b"TEST", 7).unwrap().reference_offset as u32;
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let mut disp = TrapDispatcher::new();
        disp.set_launched_app_path("Game Folder/Game App");

        disp.load_resources(&fork, &mut bus);

        let map_handle = bus.read_long(0x0A50);
        let map_ptr = bus.read_long(map_handle);
        let resource_handle = bus.read_long(map_ptr + reference_offset + 8);
        let resource_ptr = bus.read_long(resource_handle);

        assert_ne!(map_handle, 0, "TopMapHndl must address the application map");
        assert_eq!(
            bus.read_long(map_ptr + 16),
            0,
            "map chain terminates at NIL"
        );
        assert_eq!(
            bus.read_word(map_ptr + 20),
            0,
            "map belongs to the app file"
        );
        assert_eq!(bus.read_bytes(resource_ptr, 7), b"payload");
        assert_eq!(
            disp.vfs.get("__rsrc__Game Folder/Game App"),
            Some(&serialized),
            "native PBRead must see the open application resource fork"
        );
    }

    #[test]
    fn merge_resources_into_existing_file_adds_missing_entries_without_replacing() {
        let app_rsrc = make_single_resource_fork_bytes(*b"TEST", 1, b"app");
        let companion_rsrc = make_single_resource_fork_bytes(*b"TEST", 2, b"side");
        let duplicate_rsrc = make_single_resource_fork_bytes(*b"TEST", 1, b"other");
        let app_fork = ResourceFork::parse(&app_rsrc).unwrap();
        let companion_fork = ResourceFork::parse(&companion_rsrc).unwrap();
        let duplicate_fork = ResourceFork::parse(&duplicate_rsrc).unwrap();
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let mut disp = TrapDispatcher::new();

        disp.load_resources(&app_fork, &mut bus);
        assert_eq!(
            disp.merge_resources_into_existing_file(&companion_fork, &mut bus, 0),
            1
        );
        assert_eq!(
            disp.merge_resources_into_existing_file(&duplicate_fork, &mut bus, 0),
            1
        );

        let (_, app_ptr) = disp
            .find_or_load_resource_any(&mut bus, *b"TEST", 1)
            .unwrap();
        let (_, companion_ptr) = disp
            .find_or_load_resource_any(&mut bus, *b"TEST", 2)
            .unwrap();
        assert_eq!(bus.read_bytes(app_ptr, 3), b"app");
        assert_eq!(bus.read_bytes(companion_ptr, 4), b"side");
        assert_eq!(disp.count_resources(*b"TEST", true), 2);
    }

    #[test]
    fn opening_resource_fork_defers_non_preload_data_until_requested() {
        let payload = vec![0xA5; 1024 * 1024];
        let serialized = make_single_resource_fork_bytes(*b"TEST", 7, &payload);
        let fork = ResourceFork::parse(&serialized).unwrap();
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let mut disp = TrapDispatcher::new();
        let heap_before = bus.heap_bump_ptr();

        disp.load_resources(&fork, &mut bus);

        let heap_after_open = bus.heap_bump_ptr();
        assert!(
            heap_after_open - heap_before < payload.len() as u32,
            "opening the resource map must not copy ordinary resource data into the guest heap"
        );
        assert_eq!(
            disp.resources.as_ref().unwrap().files[&0].loaded[&(*b"TEST", 7)],
            0
        );

        let (_, ptr) = disp
            .find_or_load_resource_any(&mut bus, *b"TEST", 7)
            .expect("GetResource-style lookup should materialize deferred data");
        assert_eq!(bus.read_bytes(ptr, payload.len()), payload);
        assert!(bus.heap_bump_ptr() - heap_after_open >= payload.len() as u32);
    }

    #[test]
    fn opening_resource_fork_materializes_respreload_data() {
        let serialized =
            make_single_resource_fork_bytes_with_attrs(*b"TEST", 7, b"preloaded", 0x04);
        let fork = ResourceFork::parse(&serialized).unwrap();
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let mut disp = TrapDispatcher::new();

        disp.load_resources(&fork, &mut bus);

        let ptr = disp.resources.as_ref().unwrap().files[&0].loaded[&(*b"TEST", 7)];
        assert_ne!(ptr, 0);
        assert_eq!(bus.read_bytes(ptr, 9), b"preloaded");
        assert!(disp.resident_resources.contains(&(0, *b"TEST", 7)));
    }

    // Lock the `is_tracking_refire` contract — returns true exactly when
    // (a) tracking is active AND (b) the trap word is one of the
    // refire-relevant traps (auto-pop variants included). The method is
    // the canonical predicate; both dispatch.rs and runner.rs call it.

    #[test]
    fn is_tracking_refire_false_when_no_tracking_active() {
        let disp = TrapDispatcher::new();
        // Refire-relevant traps with no tracking → false.
        assert!(!disp.is_tracking_refire(0xA93D)); // MenuSelect
        assert!(!disp.is_tracking_refire(0xA80B)); // MenuKey
        assert!(!disp.is_tracking_refire(0xA991)); // ModalDialog
        assert!(!disp.is_tracking_refire(0xA985)); // Alert
        assert!(!disp.is_tracking_refire(0xA986)); // StopAlert
        assert!(!disp.is_tracking_refire(0xA987)); // NoteAlert
        assert!(!disp.is_tracking_refire(0xA988)); // CautionAlert
        assert!(!disp.is_tracking_refire(0xA968)); // TrackControl
        assert!(!disp.is_tracking_refire(0xA91E)); // TrackGoAway
        assert!(!disp.is_tracking_refire(0xA925)); // DragWindow
        assert!(!disp.is_tracking_refire(0xA92B)); // GrowWindow
        assert!(!disp.is_tracking_refire(0xA905)); // DragGrayRgn
        assert!(!disp.is_tracking_refire(0xA926)); // DragTheRgn

        // Auto-pop variants too.
        assert!(!disp.is_tracking_refire(0xAD3D));
        assert!(!disp.is_tracking_refire(0xAC0B));
        assert!(!disp.is_tracking_refire(0xAD91));
        assert!(!disp.is_tracking_refire(0xAD68));
        assert!(!disp.is_tracking_refire(0xAD2B));
    }

    #[test]
    fn is_tracking_refire_true_for_menu_traps_when_menu_tracking() {
        let mut disp = TrapDispatcher::new();
        install_menu_tracking(&mut disp);
        assert!(disp.is_tracking_refire(0xA93D));
        assert!(disp.is_tracking_refire(0xA80B));
        // Auto-pop variants share the same predicate.
        assert!(disp.is_tracking_refire(0xAD3D));
        assert!(disp.is_tracking_refire(0xAC0B));
    }

    #[test]
    fn is_tracking_refire_true_for_dialog_trap_when_dialog_tracking() {
        let mut disp = TrapDispatcher::new();
        install_dialog_tracking(&mut disp);
        assert!(disp.is_tracking_refire(0xA991));
        assert!(disp.is_tracking_refire(0xAD91));
        assert!(disp.is_tracking_refire(0xA985));
        assert!(disp.is_tracking_refire(0xA986));
        assert!(disp.is_tracking_refire(0xA987));
        assert!(disp.is_tracking_refire(0xA988));
    }

    #[test]
    fn is_tracking_refire_true_for_trackcontrol_when_control_tracking() {
        let mut disp = TrapDispatcher::new();
        install_control_tracking(&mut disp);
        assert!(disp.is_tracking_refire(0xA968));
        assert!(disp.is_tracking_refire(0xAD68));
    }

    #[test]
    fn is_tracking_refire_true_for_dragwindow_when_window_tracking() {
        let mut disp = TrapDispatcher::new();
        install_window_tracking(&mut disp);
        assert!(disp.is_tracking_refire(0xA925));
        assert!(disp.is_tracking_refire(0xAD25));
    }

    #[test]
    fn is_tracking_refire_true_for_trackgoaway_when_close_box_tracking() {
        let mut disp = TrapDispatcher::new();
        install_go_away_tracking(&mut disp);
        assert!(disp.is_tracking_refire(0xA91E));
        assert!(disp.is_tracking_refire(0xAD1E));
    }

    #[test]
    fn is_tracking_refire_true_for_drag_region_family_when_region_tracking() {
        let mut disp = TrapDispatcher::new();
        install_region_tracking(&mut disp);
        assert!(disp.is_tracking_refire(0xA905));
        assert!(disp.is_tracking_refire(0xAD05));
        assert!(disp.is_tracking_refire(0xA926));
        assert!(disp.is_tracking_refire(0xAD26));
    }

    // Lock the `current_trap_caller` contract — preserved when an auto-pop
    // trap halts (so the runner's halt log can surface the JSR caller PC),
    // cleared on success.

    #[test]
    fn current_trap_caller_preserved_on_halt() {
        use crate::memory::MemoryBus;
        use crate::trap::test_helpers::{setup, TEST_SP};

        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let caller_pc = 0xCAFE_BABEu32;
        // Auto-pop pops the JSR return address from the top of stack.
        bus.write_long(sp, caller_pc);
        // SysError reads errorCode (INTEGER, 16-bit) from new SP after
        // auto-pop has advanced past the return address.
        bus.write_word(sp + 4, 0x002A);

        // SysError ($A9C9) with auto-pop bit set ($A9C9 | 0x0400 = $ADC9).
        let result = disp.dispatch(0xADC9, &mut cpu, &mut bus);

        assert!(
            matches!(result, Err(crate::Error::Halted)),
            "SysError must halt the runner, got {:?}",
            result
        );
        assert_eq!(
            disp.current_trap_caller,
            Some(caller_pc),
            "current_trap_caller must be retained across a halt so \
             the runner halt log can surface caller=$XXXXXXXX"
        );
    }

    #[test]
    fn current_trap_caller_falls_back_to_direct_halt_site() {
        use crate::trap::test_helpers::setup;

        let (mut disp, mut cpu, mut bus) = setup();
        let trap_pc = 0x1234_5678u32;
        cpu.write_reg(Register::PC, trap_pc);

        let result = disp.dispatch(0xA05B, &mut cpu, &mut bus);

        assert!(
            matches!(result, Err(crate::Error::Halted)),
            "PowerOff must halt the runner, got {:?}",
            result
        );
        assert_eq!(
            disp.current_trap_caller,
            Some(trap_pc.wrapping_sub(2)),
            "direct halt traps must surface the trap site when no auto-pop \
             caller is available"
        );
    }

    #[test]
    fn current_trap_caller_cleared_on_success() {
        use crate::memory::MemoryBus;
        use crate::trap::test_helpers::{setup, TEST_SP};

        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let caller_pc = 0xDEAD_BEEFu32;
        bus.write_long(sp, caller_pc);

        // TickCount ($A975) auto-pop variant ($AD75). No-arg trap that
        // writes a 32-bit tick count to the (post-auto-pop) top of stack
        // and returns Ok.
        let result = disp.dispatch(0xAD75, &mut cpu, &mut bus);

        assert!(result.is_ok(), "TickCount must succeed: {:?}", result);
        assert_eq!(
            disp.current_trap_caller, None,
            "current_trap_caller must be cleared after a successful \
             auto-pop dispatch so the next trap doesn't inherit a stale value"
        );
    }

    #[test]
    fn tool_trap_trampoline_canonicalizes_bare_and_canonical_getmasktable_words() {
        use crate::memory::MemoryBus;
        use crate::trap::test_helpers::setup;

        let (mut disp, _cpu, mut bus) = setup();

        let addr_bare = disp.get_or_create_tool_trap_trampoline(&mut bus, 0x836);
        let addr_canonical = disp.get_or_create_tool_trap_trampoline(&mut bus, 0xA836);

        assert_eq!(
            addr_bare, addr_canonical,
            "canonicalized tool-trap words should share one trampoline"
        );
        assert_eq!(
            bus.read_word(addr_bare),
            0xAC36,
            "GetMaskTable trampoline must store the canonical auto-pop trap word"
        );
    }

    #[test]
    fn is_tracking_refire_false_for_unrelated_traps_during_tracking() {
        // Even with tracking active, only the specific refire traps must
        // trigger push-back. Any other trap dispatched during tracking
        // (TickCount, GetNewWindow, SysError, the game's own jump-table
        // A-line stubs, …) MUST return false.
        let mut disp = TrapDispatcher::new();
        install_menu_tracking(&mut disp);
        install_dialog_tracking(&mut disp);
        assert!(!disp.is_tracking_refire(0xA975)); // TickCount
        assert!(!disp.is_tracking_refire(0xA9BD)); // GetNewWindow
        assert!(!disp.is_tracking_refire(0xA9C9)); // SysError
        assert!(!disp.is_tracking_refire(0xA89F)); // Random unrelated trap
                                                   // Cross-trap negative cases: dialog refire word with only menu
                                                   // tracking, and vice versa.
        let mut menu_only = TrapDispatcher::new();
        install_menu_tracking(&mut menu_only);
        assert!(!menu_only.is_tracking_refire(0xA991));
        assert!(!menu_only.is_tracking_refire(0xA985));
        let mut dialog_only = TrapDispatcher::new();
        install_dialog_tracking(&mut dialog_only);
        assert!(!dialog_only.is_tracking_refire(0xA93D));
        assert!(!dialog_only.is_tracking_refire(0xA80B));
    }

    /// Pin the system-STR synthesizer table. Adding or removing a known
    /// ID is a deliberate change that must update this test — the
    /// table is the source of truth for which `'STR '` resources
    /// systemless synthesizes when no loaded fork provides them, and
    /// silently dropping a row would regress games that depend on it
    /// (see Meteor Storm's owner-name probe in commit 62da1616 and the
    /// meteor_storm_launch_chain memory note).
    #[test]
    fn system_str_default_body_pins_known_ids() {
        // Owner Name (Sharing Setup) — Networking 1994, 2-799.
        assert_eq!(
            TrapDispatcher::system_str_default_body(-16096),
            Some(&b"\x0EMacintosh User"[..])
        );
        // Macintosh Name (Sharing Setup, AppleTalk identity).
        assert_eq!(
            TrapDispatcher::system_str_default_body(-16413),
            Some(&b"\x09Macintosh"[..])
        );
        // Owner Password (encrypted blob — empty Pascal string).
        assert_eq!(
            TrapDispatcher::system_str_default_body(-16097),
            Some(&b"\x00"[..])
        );

        // Pascal-string contract: every body must start with a valid
        // length byte that matches the tail length.
        for &id in &[-16096i16, -16097, -16413] {
            let body = TrapDispatcher::system_str_default_body(id).expect("known id");
            assert!(!body.is_empty(), "id={} body must be non-empty", id);
            let len = body[0] as usize;
            assert_eq!(
                len + 1,
                body.len(),
                "id={} length byte ({}) must match tail length ({})",
                id,
                len,
                body.len() - 1
            );
        }

        // Negative space: anything outside the table returns None so
        // unrelated GetResource('STR ', N) probes still observe the
        // documented resNotFound behaviour.
        for &id in &[
            0i16, 1, 100, -1, -100, -16095, -16098, -16412, -16414, 16096,
        ] {
            assert!(
                TrapDispatcher::system_str_default_body(id).is_none(),
                "id={} must NOT be in the synthesizer table",
                id
            );
        }
    }

    #[test]
    fn active_modal_dialog_is_visible_to_frontends_before_snapshot_retention() {
        let mut disp = TrapDispatcher::new();
        let bus = MacMemoryBus::new(4 * 1024 * 1024);
        let bounds = (93, 236, 225, 564);
        disp.dialog_tracking = Some(DialogTrackingState {
            dialog_ptr: 0x0010_0000,
            bounds,
            proc_id: 1,
            ..DialogTrackingState::default()
        });

        assert_eq!(disp.visible_dialog_bounds(), Some(bounds));
        assert_eq!(disp.visible_dialog_structure_bounds(&bus), Some(bounds));
    }

    #[test]
    fn app_managed_front_dialog_is_visible_without_modal_tracking_or_snapshot() {
        let mut disp = TrapDispatcher::new();
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let dialog_ptr = 0x0010_0000;
        let bounds = (93, 236, 225, 564);
        disp.front_window = dialog_ptr;
        disp.window_bounds = bounds;
        disp.dialog_items.insert(dialog_ptr, Vec::new());
        disp.window_proc_ids.insert(dialog_ptr, 1);
        bus.write_byte(dialog_ptr + 110, 1);
        bus.write_long(dialog_ptr + 2, 0x0010_1000);
        bus.write_word(dialog_ptr + 6, 0);
        bus.write_word(dialog_ptr + 8, (-bounds.0) as u16);
        bus.write_word(dialog_ptr + 10, (-bounds.1) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, (bounds.2 - bounds.0) as u16);
        bus.write_word(dialog_ptr + 22, (bounds.3 - bounds.1) as u16);

        assert_eq!(disp.visible_dialog_bounds(), Some(bounds));
        assert_eq!(
            disp.visible_dialog_structure_bounds(&bus),
            Some(bounds),
            "synthetic records without Window Manager regions fall back to content bounds"
        );
    }

    #[test]
    fn attached_event_queue_remains_shared_through_panic() {
        let mut dispatcher = TrapDispatcher::new();
        let mut context = ProcessContext::default();
        dispatcher.attach_process_context(&mut context);
        context.event_queue_mut().push_back(QueuedEvent {
            what: 1,
            message: 0x1111,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        });

        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dispatcher.event_queue.push_back(QueuedEvent {
                what: 2,
                message: 0x2222,
                where_v: 30,
                where_h: 40,
                modifiers: 0,
            });
            dispatcher.event_queue.invalidate_menu_bar();
            panic!("simulated panic inside guest execution");
        }));

        assert!(panic_result.is_err());
        assert_eq!(context.event_queue().len(), 2);
        assert_eq!(context.event_queue()[0].message, 0x1111);
        assert_eq!(context.event_queue()[1].message, 0x2222);
        assert!(context.event_queue().menu_bar_is_invalid());
        assert_eq!(dispatcher.event_queue.len(), 2);
        assert!(dispatcher.event_queue.menu_bar_is_invalid());
    }

    #[test]
    fn attached_menu_tracking_mutates_process_context_immediately() {
        let mut dispatcher = TrapDispatcher::new();
        let mut context = ProcessContext::default();
        context.set_menu_tracking(Some(crate::menu_manager::test_process_menu_tracking(0x1234)));
        dispatcher.attach_process_context(&mut context);

        assert_eq!(
            dispatcher.menu_tracking.as_ref().map(|t| t.menu_handle),
            Some(0x1234)
        );
        dispatcher.menu_tracking.as_mut().unwrap().highlighted_item = 5;

        assert_eq!(
            context
                .menu_tracking()
                .map(|t| (t.menu_handle, t.highlighted_item)),
            Some((0x1234, 5))
        );
        assert_eq!(dispatcher.menu_tracking.as_ref().unwrap().highlighted_item, 5);
    }

    #[test]
    fn attached_menu_tracking_remains_shared_through_panic() {
        let mut dispatcher = TrapDispatcher::new();
        let mut context = ProcessContext::default();
        context.set_menu_tracking(Some(crate::menu_manager::test_process_menu_tracking(0x5678)));
        dispatcher.attach_process_context(&mut context);

        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dispatcher.menu_tracking.as_mut().unwrap().highlighted_item = 9;
            panic!("simulated panic inside menu trap execution");
        }));

        assert!(panic_result.is_err());
        assert_eq!(
            context
                .menu_tracking()
                .map(|t| (t.menu_handle, t.highlighted_item)),
            Some((0x5678, 9))
        );
        assert_eq!(dispatcher.menu_tracking.as_ref().unwrap().highlighted_item, 9);
    }

    #[test]
    fn attached_process_state_remains_canonical_through_panic() {
        let mut dispatcher = TrapDispatcher::new();
        let mut context = ProcessContext::default();
        context.set_menu_tracking(Some(crate::menu_manager::test_process_menu_tracking(0x9abc)));
        dispatcher.attach_process_context(&mut context);
        let memory_manager = context.memory_manager_handle().clone();

        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dispatcher.with_process_state(|disp| {
                disp.event_queue.push_back(QueuedEvent {
                    what: 1,
                    message: 0x3333,
                    where_v: 1,
                    where_h: 2,
                    modifiers: 0,
                });
                disp.menu_tracking.as_mut().unwrap().highlighted_item = 7;
                disp.track_handle_ptr(0x4444, 0x5555);
                disp.set_handle_state_bits(0x5555, 0xc0);
                panic!("simulated panic inside a complete guest execution slice");
            });
        }));

        assert!(panic_result.is_err());
        assert_eq!(
            context.event_queue().front().map(|event| event.message),
            Some(0x3333)
        );
        assert_eq!(
            memory_manager.borrow().handle_for_ptr(0x4444),
            Some(0x5555)
        );
        assert_eq!(memory_manager.borrow().handle_state(0x5555), 0xc0);
        assert_eq!(dispatcher.handle_for_ptr(0x4444), Some(0x5555));
        assert_eq!(dispatcher.handle_state_bits(0x5555), Some(0xc0));
        memory_manager
            .borrow_mut()
            .track_handle_ptr(0x6666, 0x7777);
        assert_eq!(dispatcher.handle_for_ptr(0x6666), Some(0x7777));
        assert_eq!(
            context
                .menu_tracking()
                .map(|tracking| (tracking.menu_handle, tracking.highlighted_item)),
            Some((0x9abc, 7))
        );
        assert_eq!(dispatcher.event_queue.len(), 1);
        assert_eq!(dispatcher.menu_tracking.as_ref().unwrap().highlighted_item, 7);
        assert!(dispatcher
            .process_memory_manager
            .as_ref()
            .is_some_and(|attached| attached.ptr_eq(&memory_manager)));
    }

    #[test]
    fn detached_dispatchers_keep_independent_memory_manager_metadata() {
        let mut first = TrapDispatcher::new();
        let mut second = TrapDispatcher::new();
        let mut first_context = ProcessContext::default();
        let mut second_context = ProcessContext::default();
        first.attach_process_context(&mut first_context);
        second.attach_process_context(&mut second_context);

        first.track_handle_ptr(0x2200, 0x1100);
        first.set_handle_state_bits(0x1100, 0x80);

        assert_eq!(second.handle_for_ptr(0x2200), None);
        assert_eq!(second.handle_state_bits(0x1100), None);
        assert!(!first
            .process_memory_manager
            .as_ref()
            .unwrap()
            .ptr_eq(second.process_memory_manager.as_ref().unwrap()));
    }

    #[test]
    fn attaching_dispatcher_moves_standalone_metadata_into_process_owner() {
        let mut dispatcher = TrapDispatcher::new();
        let standalone = dispatcher.process_memory_manager();
        dispatcher.track_handle_ptr(0x2200, 0x1100);
        dispatcher.set_handle_state_bits(0x1100, 0x80);

        let mut context = ProcessContext::default();
        dispatcher.attach_process_context(&mut context);
        let attached = dispatcher.process_memory_manager();

        assert!(!attached.ptr_eq(&standalone));
        assert_eq!(attached.borrow().handle_for_ptr(0x2200), Some(0x1100));
        assert_eq!(attached.borrow().state_for_handle(0x1100), Some(0x80));
        assert_eq!(standalone.borrow().handle_for_ptr(0x2200), None);
        assert_eq!(standalone.borrow().state_for_handle(0x1100), None);
    }

    #[test]
    fn attached_dispatcher_relocates_native_handle_without_slice_pointer() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let handle = HEAP_BASE;
        let old_ptr = HEAP_BASE + 0x10;
        let heap_cursor = HEAP_BASE + 0x40;
        let mut native = crate::memory::GuestAddressSpace::new();
        native.add_region(HEAP_BASE, vec![0; 0x1000]);
        ppc::PpcMemory::write_u32_be(&mut native, handle, old_ptr).unwrap();

        let mut context = ProcessContext::default();
        {
            let mut manager = context.memory_manager_mut();
            manager.publish_native_allocator(
                crate::process_context::ProcessNativeHeapState {
                    heap_base: HEAP_BASE,
                    heap_cursor,
                    heap_limit: HEAP_BASE + 0x1000,
                    last_mem_error: 0,
                    heap_maximized: false,
                    master_pointer_blocks_requested: 0,
                },
                &[],
                &[],
                &[],
            );
            manager.register_native_handle_records([(
                crate::process_context::ProcessHandleRecord {
                    handle,
                    ptr: old_ptr,
                    size: 8,
                    capacity: 16,
                },
                0,
            )]);
        }

        let mut bus = MacMemoryBus::new(0x2000);
        let shared = native.shared_view();
        bus.attach_guest_address_space(shared);
        let mut dispatcher = TrapDispatcher::new();
        dispatcher.attach_process_context(&mut context);
        let replacement = vec![0x5a; 48];

        assert!(dispatcher.replace_process_native_handle_bytes(
            &mut bus,
            handle,
            old_ptr,
            &replacement,
        ));
        assert_eq!(bus.read_long(handle), heap_cursor);
        assert_eq!(bus.read_bytes(heap_cursor, replacement.len()), replacement);
        assert_eq!(dispatcher.handle_for_ptr(heap_cursor), Some(handle));
        assert_eq!(dispatcher.handle_for_ptr(old_ptr), None);
        assert_eq!(
            context.memory_manager_mut().native_allocation(handle),
            Some(crate::process_context::ProcessHandleRecord {
                handle,
                ptr: heap_cursor,
                size: 48,
                capacity: 48,
            })
        );
    }
}
