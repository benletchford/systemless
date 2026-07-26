//! Trap Dispatcher - routes Mac OS traps to per-manager handler modules.
//!
//! The TrapDispatcher struct holds all emulator state. Each sub-module adds
//! `impl TrapDispatcher` blocks with `dispatch_*` methods that return
//! `Option<Result<()>>` — `Some` if the trap was handled, `None` to pass through.

use super::types::UnderlineInfo;
use crate::cpu::{CpuOps, Register};
use crate::display::CursorImage;
use crate::machine_profile::reference_machine_profile;
use crate::managers::resource::ResourceFork;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::trace::{TraceEvent, TraceSink, TraceSource};
use crate::ui_theme::{UiTheme, UiThemeId};
use crate::{Error, Result};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::PathBuf;

pub(crate) const BOOT_VOLUME_NAME: &str = "MacintoshHD";
pub(crate) const BOOT_VOLUME_REF_NUM: i16 = -1;

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
// `typedef long KeyMap[4]`). Inside Macintosh Volume I, I-260 and Macintosh
// Toolbox Essentials 2-110 document the virtual-key-indexed logical array;
// classic code commonly tests the returned bytes directly:
// `((uint8_t *)keyMap)[key >> 3] & (1 << (key & 7))`.
fn key_map_byte_mask(key_code: u8) -> Option<(usize, u8)> {
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

fn key_map_key_is_down(key_map: &[u8; 16], key_code: u8) -> bool {
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
    // Macintosh Revealed Vol. 1, p. 136; Inside Macintosh Vol. II, II-14
    let mut ccr = cpu.get_ccr() & 0x10;
    let low_word = cpu.read_reg(Register::D0) as u16;
    if low_word == 0 {
        ccr |= 0x04;
    } else if (low_word & 0x8000) != 0 {
        ccr |= 0x08;
    }
    cpu.set_ccr(ccr);
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
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PortDrawState {
    pub fg_color: (u16, u16, u16),
    pub bg_color: (u16, u16, u16),
    pub bk_pat: [u8; 8],
    pub pn_loc: (i16, i16),
    pub pn_size: (i16, i16),
    pub pn_mode: i16,
    pub pn_pat: [u8; 8],
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
            bk_pat: [0x00; 8],
            pn_loc: (0, 0),
            pn_size: (1, 1),
            pn_mode: 8,
            pn_pat: [0xFF; 8],
            tx_font: 0,
            tx_face: 0,
            tx_mode: 1,
            tx_size: 0,
        }
    }
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

/// An installed Time Manager task.
/// Processes 1994, 3-14
#[derive(Clone, Debug)]
pub struct TimerTask {
    /// Guest address of the TMTask record
    pub task_ptr: u32,
    /// Address of the callback procedure (from tmAddr at task_ptr+6)
    pub tm_addr: u32,
    /// Whether the task is primed (waiting to fire)
    pub active: bool,
    /// Tick count at which this task should fire.
    /// Computed from current ticks + delay when PrimeTime is called.
    pub fire_at_tick: u32,
    /// Revised Time Manager deadline in millionths of a 60 Hz guest tick.
    /// This preserves negative PrimeTime microsecond delays below one VBL.
    pub fire_at_subtick: u64,
    /// VBL tick in which this task was most recently dispatched.
    ///
    /// BasiliskII's Time Manager services an individual queue element at
    /// most once per VBL even when its revised/extended delay is shorter.
    /// Retaining the sub-tick deadline still orders concurrent tasks without
    /// allowing one self-repriming task to monopolize the interrupt queue.
    pub last_fired_tick: Option<u32>,
}

/// An installed Vertical Retrace Manager task.
/// Processes 1994, 4-6 to 4-7
#[derive(Clone, Debug)]
pub struct VblTask {
    /// Guest address of the VBLTask record.
    pub task_ptr: u32,
    /// Optional slot number for slot-based VBL tasks.
    pub slot: Option<i16>,
}

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
    pub(crate) state: u16,
    pub(crate) result_destination: u32,
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

/// A queued Mac event (mouseDown, mouseUp, keyDown, etc.)
#[derive(Clone, Debug)]
pub struct QueuedEvent {
    /// Event type (1=mouseDown, 2=mouseUp, 3=keyDown, etc.)
    pub what: u16,
    /// Event message (key code for key events, window ptr for activate, etc.)
    pub message: u32,
    /// Mouse location at time of event (v, h)
    pub where_v: i16,
    pub where_h: i16,
    /// Modifier flags
    pub modifiers: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct KeyRepeatState {
    pub key_code: u8,
    pub char_code: u8,
    pub next_tick: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct ListState {
    /// List view rectangle in local coordinates.
    pub view_rect: (i16, i16, i16, i16),
    /// Allocated cell bounds as (top, left, bottom, right) in cell coordinates.
    pub data_bounds: (i16, i16, i16, i16),
    /// Cell size as (v, h) in pixels.
    pub cell_size: (i16, i16),
    /// Visible cell rectangle in cell coordinates.
    pub visible: (i16, i16, i16, i16),
    /// Owning window/dialog port.
    pub port: u32,
    /// Whether drawing is enabled.
    pub draw_enabled: bool,
    /// Raw cell bytes keyed by (row, column).
    pub cells: HashMap<(i16, i16), Vec<u8>>,
    /// Selected cells keyed by (row, column).
    pub selected: BTreeSet<(i16, i16)>,
    /// Most recently clicked cell in (row, column), or (-1, -1) if none.
    pub last_click: (i16, i16),
    /// Tick count of the previous click for double-click detection.
    pub last_click_tick: u32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TextEditState {
    /// Feature bits toggled through TEFeatureFlag / TEAutoView.
    pub feature_bits: u16,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ControlAuxRecordState {
    /// Guest AuxCtlHandle returned by GetAuxCtl.
    pub handle: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VfsMetadata {
    pub file_id: u32,
    pub parent_dir_id: u32,
    pub file_type: u32,
    pub creator: u32,
    pub finder_flags: u16,
    pub created_date: u32,
    pub modified_date: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct VfsDirectory {
    pub dir_id: u32,
    pub parent_dir_id: u32,
    pub name: String,
}

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

/// Trap dispatcher with resource fork access and emulator state.
pub struct TrapDispatcher {
    /// Loaded resources by handle -> (ptr, type, id)
    pub(crate) loaded_handles: HashMap<u32, (u32, [u8; 4], i16)>,
    /// Fast index from (resource-file refnum, type, id) to its live handle.
    pub(crate) resource_handles_by_key: HashMap<(u16, [u8; 4], i16), u32>,
    /// Mutable Memory Manager state bits by handle.
    /// Resource ownership is derived from `loaded_handles`; this map stores
    /// only flags guest code may change (lock, purgeable, etc.).
    pub(crate) handle_state_bits: HashMap<u32, u8>,
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
    /// Map from a relocatable block's data pointer to the handle that
    /// owns it. Populated by NewHandle / SetHandleSize / ReallocateHandle
    /// and drained by DisposeHandle. Used by RecoverHandle to look up the
    /// handle for a master-pointer dereferenced address.
    /// Inside Macintosh Volume V, V-579
    pub(crate) ptr_to_handle: HashMap<u32, u32>,
    /// Detached resource handles that should no longer be treated as resource-backed.
    pub(crate) detached_handles: HashMap<u32, ([u8; 4], i16)>,
    /// Resource-file refnum for each resource-backed handle.
    pub(crate) resource_handle_files: HashMap<u32, u16>,
    /// Resource-file refnum for detached resource handles.
    pub(crate) detached_handle_files: HashMap<u32, u16>,
    /// Resource fork reference (loaded into memory)
    pub(crate) resources: Option<LoadedResources>,
    /// Canonical resource bytes keyed by (resource-file refnum, type, id).
    /// Used to reload unloaded resources without reparsing the whole fork.
    pub(crate) resource_backing_data: HashMap<(u16, [u8; 4], i16), Vec<u8>>,
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
    /// AppleEvent handlers registered by the guest via Pack8 routine 31
    /// (AEInstallEventHandler). Key is `(eventClass, eventID)` packed
    /// as 4-char-codes; value is `(handler_proc_ptr, handler_refcon)`.
    /// Inside Macintosh Volume VI, 6-43.
    pub ae_handlers: HashMap<(u32, u32), (u32, u32)>,
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
    /// Cache of synthetic System-file `'KCHR'` resource pointers. The
    /// U.S. Roman keyboard-layout resource ID 0 is present in every
    /// System file and is used directly by apps that call KeyTranslate.
    pub(crate) system_kchr_cache: HashMap<i16, u32>,
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
    /// Cache of allocated tool-trap trampolines for GetTrapAddress.
    /// Each entry is a 2-byte allocation containing the auto-pop
    /// variant of the canonical tool-trap word. When the guest does
    /// `JSR (trampoline)` the dispatcher pops the saved return PC,
    /// runs the trap, and resumes at the JSR caller — see
    /// [`Self::get_or_create_tool_trap_trampoline`]. OS traps stay on
    /// the simpler `$00F0xxxx` fake-ptr scheme because they have no
    /// auto-pop semantics. Inside Macintosh Volume II, II-384
    /// (NGetTrapAddress); IM:V V-577 (auto-pop bit).
    pub(crate) tool_trap_trampolines: HashMap<u16, u32>,
    /// Substitution strings most recently set via `ParamText`. Indices
    /// 0..3 correspond to `^0`..`^3` placeholders in any subsequently
    /// drawn dialog/alert static-text item. Inside Macintosh Volume I,
    /// I-422 (ParamText).
    pub(crate) param_text: [Vec<u8>; 4],
    /// Selected UI rendering provider. The default preserves the legacy
    /// System 7 renderer; explicit non-classic providers are allowed to change
    /// chrome pixels without changing guest-visible Toolbox behavior.
    pub(crate) ui_theme_id: UiThemeId,
    /// Virtual filesystem: filename -> data fork contents
    pub vfs: HashMap<String, Vec<u8>>,
    /// Virtual filesystem: filename -> resource fork contents
    pub vfs_rsrc: HashMap<String, Vec<u8>>,
    /// Finder metadata and catalog IDs for VFS file entries.
    pub(crate) vfs_metadata: HashMap<String, VfsMetadata>,
    /// Directory catalog metadata keyed by normalized path.
    pub(crate) vfs_directories: HashMap<String, VfsDirectory>,
    /// Reverse directory lookup by catalog ID.
    pub(crate) vfs_directory_paths: HashMap<u32, String>,
    /// Open working directories keyed by working directory reference number.
    pub(crate) working_directories: HashMap<i16, WorkingDirectory>,
    /// Open file table: refnum -> filename
    pub(crate) open_files: HashMap<u16, String>,
    /// Synthetic Device Manager drivers opened by name via PBOpen/OpenDriver.
    pub(crate) synthetic_drivers: HashMap<u16, String>,
    /// Refnums opened with write permission (fsRdWrPerm=3 or fsWrPerm=2).
    /// Used to enforce opWrErr (-49) per IM:Files 9578.
    pub(crate) write_refnums: std::collections::HashSet<u16>,
    /// File position table: refnum -> current byte offset
    pub(crate) file_positions: HashMap<u16, usize>,
    /// Most recent successful PBRead/FSRead from a data fork.
    pub(crate) recent_file_read: Option<RecentFileRead>,
    /// Set of VFS keys whose `ioFlAttrib` lock bit is set.
    /// Maintained by SetFilLock/HSetFLock ($A041/$A241) and
    /// RstFilLock/HRstFLock ($A042/$A242); read by
    /// `fill_file_catalog_info` to set bit 0 of `ioFlAttrib`.
    /// Files 1992, 2-205 (`ioFlAttrib` field), 9302..9352 (HSetFLock/HRstFLock).
    /// Public to mirror `vfs`/`vfs_rsrc` so frontends and tests can
    /// inspect or seed lock state directly.
    pub locked_files: std::collections::HashSet<String>,
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
    pub(crate) next_vfs_dir_id: u32,
    /// Next synthetic file ID for VFS files.
    pub(crate) next_vfs_file_id: u32,
    /// Monotonic source for VFS creation and modification timestamps.
    pub(crate) next_vfs_timestamp: u32,
    /// Next working directory reference number.
    pub(crate) next_working_dir_refnum: i16,
    /// Normalized VFS path of the launched application, if known.
    pub(crate) launched_app_path: Option<String>,
    /// Foreground application launch queued by LaunchApplication. When
    /// `after_event_yield` is set, the runner starts it after the current
    /// app next yields through WaitNextEvent/EventAvail/GetNextEvent.
    pub(crate) pending_launch_app: Option<PendingLaunchApplication>,
    /// Current default directory.
    pub(crate) default_dir_id: u32,
    /// Working directory reference number for the application's folder.
    pub(crate) app_wd_refnum: i16,
    /// Host directory to write output files to (if set)
    pub output_dir: Option<std::path::PathBuf>,
    /// Current foreground color (RGBColor: R, G, B)
    pub(crate) fg_color: (u16, u16, u16),
    /// Current background color (RGBColor: R, G, B)
    pub(crate) bg_color: (u16, u16, u16),
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
    pub(crate) window_list: Vec<u32>,
    /// Set once the game has entered fullscreen (window covers entire screen
    /// and MBarHeight was 0). While set, the menu bar is suppressed even if
    /// the game temporarily restores MBarHeight (e.g. on cursor-at-top).
    pub fullscreen_locked: bool,
    /// Host-controlled override for menu bar visibility. When true, the menu
    /// bar is suppressed regardless of game state — unlike `fullscreen_locked`
    /// which the emulator auto-clears when the game writes MBarHeight > 0.
    /// Defaults to `true` so the HLE renders like a kiosk by default; the
    /// menu bar is a Mac OS chrome surface that has no analogue in a game-
    /// only runtime, and showing it diverges screenshots from the
    /// original-machine reference whenever the cursor hovers `y < 20`.
    /// Set `SYSTEMLESS_SHOW_MENU_BAR=1` (or assign `menu_bar_hidden = false`
    /// after construction) to opt back in for environments where the menu
    /// bar IS the user-facing surface (e.g. running a Mac app, not a game).
    pub menu_bar_hidden: bool,
    /// Sound Manager state (channels, playback buffers).
    pub sound_manager: crate::sound::SoundManager,
    /// Menus loaded from MENU resources, in order of insertion
    pub(crate) menus: Vec<super::menu::Menu>,
    /// Snapshots of `menus` taken by GetMenuBar ($A93B), keyed by the
    /// guest-side master pointer the trap returned. SetMenuBar ($A93C)
    /// restores from this map when the caller passes a handle that was
    /// previously vended by GetMenuBar — the typical save/restore pattern
    /// real Mac apps use around modal dialogs that disable command keys.
    /// Inside Macintosh Volume I, I-354
    pub(crate) saved_menu_bars: HashMap<u32, Vec<super::menu::Menu>>,
    /// Active menu tracking state (non-None while MenuSelect is tracking the mouse)
    pub(crate) menu_tracking: Option<super::menu::MenuTrackingState>,
    /// A host-native menu selection waiting for the guest's normal
    /// FindWindow -> MenuSelect event path.  It is consumed only by
    /// MenuSelect and revalidated against the live menu list there.
    pub(crate) pending_native_menu_selection: Option<(i16, i16)>,
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
    /// Underline info for continuous underline across a string (set by draw_string)
    pub(crate) underline_info: Option<UnderlineInfo>,
    /// Current mouse position in Mac screen coordinates (v, h)
    pub(crate) mouse_pos: (i16, i16),
    /// Current mouse button state: true = button is pressed
    pub(crate) mouse_button: bool,
    /// Current keyboard state as a classic 16-byte KeyMap (128 keys).
    /// Bits are packed for direct byte/bit readers:
    /// key >> 3 selects the byte, key & 7 selects the bit.
    pub(crate) key_map: [u8; 16],
    /// Auto-key repeat state for the currently repeating character key.
    pub(crate) key_repeat: Option<KeyRepeatState>,
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
    pub(crate) event_queue: VecDeque<QueuedEvent>,
    /// One-shot update events recovered after FlushEvents drops queue entries
    /// while the Window Manager update region remains dirty.
    pub(crate) flushed_update_events: VecDeque<QueuedEvent>,
    /// System event mask used by PostEvent/PPostEvent filtering.
    /// Inside Macintosh Volume II, II-70.
    pub(crate) system_event_mask: u16,
    /// Whether the synthetic kAEOpenApplication event has been delivered.
    /// On a real Mac, the Finder sends this Apple Event at launch.
    /// Macintosh Toolbox Essentials 1992, p. 5-90
    pub(crate) sent_open_app_event: bool,
    /// Full trap word currently being dispatched. Some OS traps share the
    /// low 8-bit trap number and require bit 8 to distinguish variants.
    pub(crate) current_trap_word: u16,
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
    /// Custom cursor image installed by SetCursor / SetCCursor.
    pub(crate) cursor_data: Option<CursorImage>,
    /// Cursor level per IM:I I-167..I-168. `0` means visible; negative
    /// values mean hidden by one or more HideCursor/ShieldCursor calls.
    pub(crate) cursor_level: i16,
    /// Cached cursor visibility for host rendering fast-paths.
    /// Kept in sync with `cursor_level` by cursor traps.
    pub(crate) cursor_visible: bool,
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
    /// Per-trap count of inline-skipped dispatches. Incremented by the
    /// runner's pre-dispatch fast paths for each *virtual* trap entry that
    /// bypassed the real `dispatch()` body. Combined with
    /// `trap_histogram\[idx\]` (total entries) and `trap_time_ns\[idx\]`
    /// (only counts non-inline dispatches), gives:
    ///   `actual_dispatches = trap_histogram\[idx\] - inline_skipped\[idx\]`
    ///   `per-actual-dispatch ns = trap_time_ns\[idx\] / actual_dispatches`
    pub inline_skipped: Box<[u64; 4096]>,
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
    pub(crate) current_gdevice: u32,
    /// Current GrafPort/GWorld pointer
    pub(crate) current_port: u32,
    /// Per-port pen/color/text state restored by SetPort and SetGWorld.
    pub(crate) port_draw_states: HashMap<u32, PortDrawState>,
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
    pub device_clut: [[u16; 3]; 256],
    /// Color Manager CLUT for 8bpp mode. Updated only by high-level SetEntries ($AA3F)
    /// and ActivatePalette — NOT by low-level video driver palette fades.
    /// Used by QuickDraw shape drawing (PaintRect, etc.) for RGB→index mapping,
    /// mirroring the real Mac OS ITable which is derived from the Color Manager palette.
    /// Imaging With QuickDraw 1994, p. 4-82
    pub color_manager_clut: [[u16; 3]; 256],
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
    /// Most recent non-system GetCTable resource fetch. Some games fetch a
    /// CLUT immediately before drawing a screen-backed PICT and expect that
    /// table to drive the initial palette seed for the picture.
    pub(crate) recent_resource_ctable_fetch: Option<RecentColorTableFetch>,
    /// Window palette associations keyed by WindowPtr. A key of `0xFFFF_FFFF`
    /// acts as the application/default palette sentinel.
    pub(crate) window_palettes: HashMap<u32, (u32, i16)>,
    /// Palette update flags keyed by PaletteHandle.
    pub(crate) palette_updates: HashMap<u32, i16>,
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
    /// Active picture recording state: (pic_handle, frame top, left, bottom, right).
    /// Set by OpenPicture, cleared by ClosePicture.
    pub(crate) recording_picture: Option<(u32, i16, i16, i16, i16)>,
    /// Native trap dispatch table: maps raw trap word -> native 68K handler address.
    /// Populated by SetTrapAddress ($A047/$A647). When an A-line instruction fires
    /// and a native handler exists, the dispatcher simulates a JSR to the handler
    /// instead of running HLE code. This allows CRT-installed handlers (LoadSeg,
    /// UnloadSeg, ExitToShell) to run natively with proper code relocation.
    pub(crate) native_trap_table: HashMap<u16, u32>,
    /// Re-entrancy guard for the CopyBits `grafProcs.bitsProc` bottleneck:
    /// `(bitsProc address, stack pointer at the tail call)`. A custom bitsProc
    /// normally reaches the real transfer by calling CopyBits again; without
    /// this guard that second call would be handed back to the same proc
    /// forever. While the stack pointer is still at or below the recorded value
    /// we are nested inside the proc, so CopyBits performs the blit itself.
    pub(crate) bits_proc_reentry: Option<(u32, u32)>,
    /// Installed Time Manager tasks.
    /// Processes 1994, 3-14
    pub(crate) timer_tasks: Vec<TimerTask>,
    /// Exact Time Manager time while a callback is being delivered.
    pub(crate) timer_current_subtick: u64,
    /// Installed Vertical Retrace Manager tasks.
    /// Processes 1994, 4-6 to 4-7
    pub(crate) vbl_tasks: Vec<VblTask>,
    /// Dormant system-owned queue element kept ahead of application VBL tasks.
    pub(crate) system_vbl_queue_anchor: u32,
    /// Slot number of the primary video monitor for AttachVBL / VBL cursor routing.
    pub(crate) primary_vbl_slot: i16,
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
    /// Host-side List Manager state keyed by guest ListHandle.
    pub(crate) list_states: HashMap<u32, ListState>,
    /// Host-side TextEdit feature state keyed by guest TEHandle.
    pub(crate) textedit_states: HashMap<u32, TextEditState>,
    /// Maps guest ControlRecord pointer → procID, set by NewControl/GetNewControl.
    /// Used by DrawControls to dispatch to the correct rendering routine.
    /// Inside Macintosh Volume I, I-331
    pub(crate) control_proc_ids: HashMap<u32, i16>,
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
    /// Desk scrap contents: list of (type_code, data) entries.
    /// Each entry stores a 4-byte ResType and the raw data bytes.
    /// Inside Macintosh Volume I, I-453
    pub scrap_entries: Vec<([u8; 4], Vec<u8>)>,
    /// Scrap change counter, incremented by ZeroScrap.
    /// Inside Macintosh Volume I, I-458
    pub scrap_count: i16,
    /// True when the desk scrap is resident in memory.
    /// False means InfoScrap reports the scrap on disk and hides the handle
    /// until LoadScrap/ZeroScrap brings it back into memory.
    pub scrap_in_memory: bool,
    /// Whether the desk scrap can be persisted to a writable clipboard file.
    /// False keeps UnloadScrap on the observable error path when the scrap is
    /// resident and a disk write would be required.
    pub(crate) scrap_clipboard_writable: bool,
    /// Most recent pack ID passed to InitPack.
    /// Kept as lightweight bookkeeping for future pack-specific heuristics.
    pub last_init_pack_id: Option<i16>,
    /// Guest address of the master pointer for the in-memory desk scrap.
    /// Lazily allocated on first InfoScrap call.
    /// Inside Macintosh Volume I, I-457
    pub scrap_handle: Option<u32>,
    /// True when the serialized desk scrap handle needs rebuilding from
    /// `scrap_entries` before the next InfoScrap observation.
    pub scrap_handle_dirty: bool,
    /// Guest address of the ScrapStuff record returned by InfoScrap.
    /// Lazily allocated on first InfoScrap call.
    /// Inside Macintosh Volume I, I-457
    pub scrap_stuff_ptr: Option<u32>,
    /// Whether SetResLoad(TRUE) is active (default: true).
    /// When false, resource-retrieval functions return empty handles.
    /// Inside Macintosh Volume I, I-118
    pub res_load: bool,
    /// Whether SetResPurge(TRUE) is active (default: false).
    /// When true, resources are written to disk before purging if modified.
    /// Inside Macintosh Volume I, I-126
    pub res_purge: bool,
}

/// Resources loaded into guest memory.
#[derive(Clone, Default)]
pub(crate) struct ResourceFileMap {
    /// Map of (type, id) -> memory address of resource data.
    pub loaded: HashMap<([u8; 4], i16), u32>,
    /// Map of (type, name) -> (id, memory address) for named resources.
    /// Name lookups on classic Mac OS are not a uniqueness constraint:
    /// several resources may share a display name. This map intentionally
    /// keeps the historical lookup path, while `names_by_id` below preserves
    /// the exact name returned by GetResInfo for each resource ID.
    pub named: HashMap<([u8; 4], String), (i16, u32)>,
    /// Map of (type, id) -> resource name for GetResInfo.
    pub names_by_id: HashMap<([u8; 4], i16), String>,
    /// Map of (type, id) -> resource attribute bits from the resource map.
    pub attrs: HashMap<([u8; 4], i16), u8>,
    /// Resource-map-level attribute bits (mapReadOnly = 0x80,
    /// mapCompact = 0x40, mapChanged = 0x20) per IM:I-126. Read/written
    /// by GetResFileAttrs ($A9F6) and SetResFileAttrs ($A9F7). Stored
    /// as u16 since both traps marshal the full INTEGER even though
    /// only the low byte has documented bits.
    pub map_attrs: u16,
}

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

pub(crate) struct LoadedResources {
    /// Resources grouped by resource-file reference number.
    pub files: HashMap<u16, ResourceFileMap>,
    /// Debug names keyed by resource-file reference number.
    pub names: HashMap<u16, String>,
    /// Open/search order for resource files. The app's own resources use refnum 0.
    pub search_order: Vec<u16>,
    /// Current resource file selected by UseResFile / CurResFile.
    pub current_file: u16,
}

impl TrapDispatcher {
    pub(crate) const AUTO_KEY_THRESHOLD_TICKS: u32 = 16;
    pub(crate) const AUTO_KEY_RATE_TICKS: u32 = 4;

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
        let mut rgba = crate::display::render_screen(bus, self.screen_mode, &self.device_clut);
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
        self.current_port = port;
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
        key_map_key_is_down(&self.key_map, key_code)
    }

    pub(crate) fn key_map_bytes(&self) -> &[u8; 16] {
        &self.key_map
    }

    pub(crate) fn current_event_modifiers(&self) -> u16 {
        const BTN_STATE: u16 = 128;
        const CMD_KEY: u16 = 256;
        const SHIFT_KEY: u16 = 512;
        const OPTION_KEY: u16 = 2048;
        const CONTROL_KEY: u16 = 4096;

        let mut modifiers = 0u16;
        if !self.mouse_button {
            modifiers |= BTN_STATE;
        }
        if self.key_is_down(0x37) {
            modifiers |= CMD_KEY;
        }
        if self.key_is_down(0x38) || self.key_is_down(0x3C) {
            modifiers |= SHIFT_KEY;
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
        let key_map = if self.key_map.iter().any(|&byte| byte != 0) {
            self.key_map
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join("")
        } else {
            "none".to_string()
        };
        format!(
            "state=mouse=({},{}) button={} live_modifiers=${:04X} key_map={} tracking=menu:{} dialog:{} control:{}",
            self.mouse_pos.0,
            self.mouse_pos.1,
            if self.mouse_button { "down" } else { "up" },
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
            // Distinguish inline-skipped (counted but bypassed real dispatch)
            // from real dispatches (counted AND timed). The per-real-dispatch
            // ns figure is the actionable number — the inline-padded average
            // dilutes it toward zero.
            let inline = self.inline_skipped[*idx as usize];
            let real_dispatches = count.saturating_sub(inline);
            let real_avg_ns = ns.checked_div(real_dispatches).unwrap_or(0);
            eprintln!(
                "[TRAP-TIME]   {:>11} ns total  {:>7} ns/real-call  ({:>10} real / {:>10} inline)  idx=${:03X} (tool ${:04X} / os ${:04X})",
                ns, real_avg_ns, real_dispatches, inline, idx, as_tool, as_os
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
            loaded_handles: HashMap::new(),
            resource_handles_by_key: HashMap::new(),
            handle_state_bits: HashMap::new(),
            vm_held_page_counts: HashMap::new(),
            vm_held_page_history: HashSet::new(),
            vm_locked_page_counts: HashMap::new(),
            instruction_cache_enabled: true,
            data_cache_enabled: true,
            ptr_to_handle: HashMap::new(),
            detached_handles: HashMap::new(),
            resource_handle_files: HashMap::new(),
            detached_handle_files: HashMap::new(),
            resources: None,
            resource_backing_data: HashMap::new(),
            movie_states: HashMap::new(),
            movie_by_controller: HashMap::new(),
            movie_error: 0,
            movie_sticky_error: 0,
            dialogs_drawn_by_app: std::collections::HashSet::new(),
            segment_map: HashMap::new(),
            ae_handlers: HashMap::new(),
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
            synthetic_component_instances: HashSet::new(),
            next_synthetic_component_instance: 0x00C1_0001,
            saved_draw_old_regions: HashMap::new(),
            fired_oapp_handler: false,
            system_str_cache: HashMap::new(),
            system_cursor_cache: HashMap::new(),
            system_clut_cache: HashMap::new(),
            system_kchr_cache: HashMap::new(),
            system_wdef_cache: HashMap::new(),
            system_mdef_cache: HashMap::new(),
            tool_trap_trampolines: HashMap::new(),
            param_text: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            ui_theme_id: UiThemeId::ClassicSystem7,
            vfs: HashMap::new(),
            vfs_rsrc: HashMap::new(),
            vfs_metadata: HashMap::new(),
            vfs_directories,
            vfs_directory_paths,
            working_directories: HashMap::new(),
            open_files: HashMap::new(),
            synthetic_drivers: HashMap::new(),
            write_refnums: HashSet::new(),
            file_positions: HashMap::new(),
            recent_file_read: None,
            locked_files: HashSet::new(),
            next_refnum: 100,
            mmu_mode: 1,                      // true32b — 32-bit addressing by default
            default_video_rec: 0x0000,        // no default video device selected
            default_os_rec: 0x0001,           // Macintosh Operating System
            default_startup_rec: 0x0000_0000, // zero-filled first-device startup default
            next_vfs_dir_id: 16,
            next_vfs_file_id: 32,
            next_vfs_timestamp: 1,
            next_working_dir_refnum: 32,
            launched_app_path: None,
            pending_launch_app: None,
            default_dir_id: 2,
            app_wd_refnum: BOOT_VOLUME_REF_NUM,
            output_dir: None,
            fg_color: (0, 0, 0),
            bg_color: (0xFFFF, 0xFFFF, 0xFFFF),
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
            // Default to hiding the menu bar — the HLE is a game runtime,
            // not a Finder, and a leaking menu bar at `y < 20` is the
            // single biggest source of visual glitches where the classic
            // menu bar bleeds into the game's top rows. Frontends that
            // host a Mac app (rather than a game) can opt back in via
            // `SYSTEMLESS_SHOW_MENU_BAR=1`.
            menu_bar_hidden: std::env::var_os("SYSTEMLESS_SHOW_MENU_BAR").is_none(),
            sound_manager: crate::sound::SoundManager::new(),
            menus: Vec::new(),
            saved_menu_bars: HashMap::new(),
            menu_tracking: None,
            pending_native_menu_selection: None,
            pending_native_menu_event: None,
            pending_native_menu_event_tick: None,
            control_tracking: None,
            underline_info: None,
            mouse_pos: (0, 0),
            mouse_button: false,
            key_map: [0; 16],
            key_repeat: None,
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
            event_queue: VecDeque::new(),
            flushed_update_events: VecDeque::new(),
            system_event_mask: 0xFFEF, // everyEvent - keyUpMask
            sent_open_app_event: false,
            current_trap_word: 0,
            current_trap_caller: None,
            pending_wait_sleep_ticks: 0,
            pending_wait_next_event_return: None,
            pending_hle_tick_cost: 0,
            yield_for_ui: false,
            pending_delay_ticks: 0,
            cursor_data: Some(Self::default_arrow_cursor_image()),
            cursor_level: 0,
            cursor_visible: true,
            trap_count: 0,
            game_trap_count: 0,
            trap_histogram: Box::new([0u64; 4096]),
            trap_time_ns: Box::new([0u64; 4096]),
            inline_skipped: Box::new([0u64; 4096]),
            copybits_screen_count: 0,
            last_screen_copybits_rect: None,
            last_screen_frame_rect: None,
            last_screen_frame_rect_tick: 0,
            screen_event_count: 0,
            copybits_screen_secs: Vec::new(),
            trace_sink: None,
            main_gdevice_handle: 0,
            current_gdevice: 0,
            current_port: 0,
            port_draw_states: HashMap::new(),
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
            device_clut: Self::standard_mac_8bpp_clut(),
            color_manager_clut: Self::standard_mac_8bpp_clut(),
            inverse_table_cache: Vec::new(),
            clut_protected: [false; 256],
            clut_reserved: [false; 256],
            seeded_picture_palette_until_tick: 0,
            seeded_picture_palette: Self::standard_mac_8bpp_clut(),
            recent_resource_ctable_fetch: None,
            window_palettes: HashMap::new(),
            palette_updates: HashMap::new(),
            printing_error: 0,
            next_ct_seed: 1,
            fill_black_override: None,
            recording_picture: None,
            native_trap_table: HashMap::new(),
            bits_proc_reentry: None,
            timer_tasks: Vec::new(),
            timer_current_subtick: 0,
            vbl_tasks: Vec::new(),
            system_vbl_queue_anchor: 0,
            primary_vbl_slot: 0,
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
            dialog_edit_text_modified_items: HashSet::new(),
            dialog_initial_draw_deferred: HashSet::new(),
            modeless_dialog_draw_proc_queue: VecDeque::new(),
            active_modeless_dialog_draw_proc: None,
            retained_modal_dialog_click: None,
            pending_modal_button_dispose_dialog: None,
            window_stack: Vec::new(),
            saved_vis_regions: HashMap::new(),
            list_states: HashMap::new(),
            textedit_states: HashMap::new(),
            control_proc_ids: HashMap::new(),
            last_inserted_menu_id: None,
            pending_dialog_popup_menu: None,
            dialog_item_popup_menus: HashMap::new(),
            dialog_popup_original_rects: HashMap::new(),
            dialog_popup_candidate_items: HashSet::new(),
            scrap_entries: Vec::new(),
            scrap_count: 0,
            scrap_in_memory: true,
            scrap_clipboard_writable: false,
            last_init_pack_id: None,
            scrap_handle: None,
            scrap_handle_dirty: false,
            scrap_stuff_ptr: None,
            res_load: true,
            res_purge: false,
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

    /// Shared check used by both dispatch.rs (auto-pop push-back) and
    /// runner.rs (PC rewind for refire). Returns true when the given trap
    /// word should refire next frame because menu or dialog tracking is
    /// active and the trap is one of the refire-relevant kind. Strips the
    /// auto-pop bit (0x0400) so auto-pop variants ($AD3D, $AC0B, $AD91)
    /// match too.
    pub fn is_tracking_refire(&self, opcode: u16) -> bool {
        let trap_no_autopop = opcode & !0x0400;
        let is_menu_refire = trap_no_autopop == 0xA93D || trap_no_autopop == 0xA80B;
        let is_dialog_refire =
            matches!(trap_no_autopop, 0xA991 | 0xA985 | 0xA986 | 0xA987 | 0xA988);
        let is_standard_file_refire = trap_no_autopop == 0xA9EA;
        let is_control_refire = trap_no_autopop == 0xA968;
        (is_menu_refire && self.is_menu_tracking())
            || (is_dialog_refire && self.is_dialog_tracking())
            || (is_standard_file_refire
                && (self.is_standard_file_put_tracking() || self.is_standard_file_get_tracking()))
            || (is_control_refire && self.is_control_tracking())
    }

    /// Generate the standard Mac 8-bit system palette as 16-bit RGB values.
    pub(crate) fn standard_mac_8bpp_clut() -> [[u16; 3]; 256] {
        let mut clut = [[0u16; 3]; 256];
        // Indices 0-214: 6x6x6 color cube (215 entries)
        // R varies slowest (÷36), G medium (÷6 %6), B fastest (%6)
        // Each component has 6 levels: 5→0xFFFF, 4→0xCCCC, 3→0x9999, 2→0x6666, 1→0x3333, 0→0x0000
        // Index 0 = (5,5,5) = white, index 214 = (0,0,1)
        // Imaging With QuickDraw 1994, Table 4-6, p. 4-93
        // references/executor/src/quickdraw/default_ctab_values.cpp
        for i in 0u32..=214 {
            let r = 5 - (i / 36);
            let g = 5 - ((i / 6) % 6);
            let b = 5 - (i % 6);
            clut[i as usize] = [
                (r as u16) * 0x3333,
                (g as u16) * 0x3333,
                (b as u16) * 0x3333,
            ];
        }
        // Indices 215-254: primary + gray ramps (10 entries each)
        // Brightness levels: n * 0x1111 for n in {14,13,11,10,8,7,5,4,2,1}
        // (integers 1-14 excluding multiples of 3: 3,6,9,12)
        // references/executor/src/quickdraw/default_ctab_values.cpp
        const RAMP: [u16; 10] = [
            0xEEEE, 0xDDDD, 0xBBBB, 0xAAAA, 0x8888, 0x7777, 0x5555, 0x4444, 0x2222, 0x1111,
        ];
        for j in 0..10usize {
            clut[215 + j] = [RAMP[j], 0, 0]; // Red ramp
            clut[225 + j] = [0, RAMP[j], 0]; // Green ramp
            clut[235 + j] = [0, 0, RAMP[j]]; // Blue ramp
            clut[245 + j] = [RAMP[j], RAMP[j], RAMP[j]]; // Gray ramp
        }
        // Index 255: black
        clut[255] = [0, 0, 0];
        clut
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
            .map(|(_, bytes)| bytes.clone())
    }

    pub(crate) fn boot_volume_ref_num() -> i16 {
        BOOT_VOLUME_REF_NUM
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
        let timestamp = self.next_vfs_timestamp;
        self.next_vfs_timestamp = self.next_vfs_timestamp.saturating_add(1);
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

    pub(crate) fn ensure_vfs_directory(&mut self, path: &str) -> u32 {
        let normalized = Self::normalize_vfs_path(path);
        if normalized.is_empty() {
            return 2;
        }
        if let Some(dir) = self.vfs_directories.get(&normalized) {
            return dir.dir_id;
        }

        let parent_path = Self::vfs_parent_path(&normalized).to_string();
        let parent_dir_id = self.ensure_vfs_directory(&parent_path);
        let dir_id = self.next_vfs_dir_id;
        self.next_vfs_dir_id = self.next_vfs_dir_id.saturating_add(1);

        self.vfs_directories.insert(
            normalized.clone(),
            VfsDirectory {
                dir_id,
                parent_dir_id,
                name: Self::vfs_basename(&normalized).to_string(),
            },
        );
        self.vfs_directory_paths.insert(dir_id, normalized);
        dir_id
    }

    pub(crate) fn ensure_vfs_file_metadata(&mut self, path: &str) {
        let normalized = Self::normalize_vfs_path(path);
        if normalized.is_empty() || self.vfs_metadata.contains_key(&normalized) {
            return;
        }

        let parent_path = Self::vfs_parent_path(&normalized).to_string();
        let parent_dir_id = self.ensure_vfs_directory(&parent_path);
        let timestamp = self.allocate_vfs_timestamp();
        self.vfs_metadata.insert(
            normalized,
            VfsMetadata {
                file_id: self.next_vfs_file_id,
                parent_dir_id,
                file_type: u32::from_be_bytes(*b"????"),
                creator: u32::from_be_bytes(*b"????"),
                finder_flags: 0,
                created_date: timestamp,
                modified_date: timestamp,
            },
        );
        self.next_vfs_file_id = self.next_vfs_file_id.saturating_add(1);
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
    }

    pub(crate) fn set_launched_app_path(&mut self, name: &str) {
        let normalized = Self::normalize_vfs_path(name);
        self.ensure_vfs_file_metadata(&normalized);
        if let Some(metadata) = self.vfs_metadata.get(&normalized).copied() {
            self.default_dir_id = metadata.parent_dir_id;
            // Open a working directory for the app's parent folder so that
            // PBGetVol returns a WDRefNum and PBGetWDInfo resolves the correct dirID.
            // Inside Macintosh Volume IV, IV-72
            if let Some(wd_ref) =
                self.open_working_directory(Self::boot_volume_ref_num(), metadata.parent_dir_id, 0)
            {
                self.app_wd_refnum = wd_ref;
            }
        }
        self.launched_app_path = Some(normalized);
    }

    pub fn launched_app_path(&self) -> Option<&str> {
        self.launched_app_path.as_deref()
    }

    pub(crate) fn queue_pending_launch_application(&mut self, name: &str, after_event_yield: bool) {
        let normalized = Self::normalize_vfs_path(name);
        self.pending_launch_app = Some(PendingLaunchApplication {
            path: normalized,
            after_event_yield,
        });
    }

    pub(crate) fn take_pending_launch_application(
        &mut self,
        event_yield_reached: bool,
    ) -> Option<String> {
        let ready = self
            .pending_launch_app
            .as_ref()
            .is_some_and(|pending| !pending.after_event_yield || event_yield_reached);
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
    }

    pub(crate) fn remove_vfs_entry_metadata(&mut self, name: &str) {
        let normalized = Self::normalize_vfs_path(name);
        self.vfs_metadata.remove(&normalized);
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
            if let Some(working_directory) = self.working_directories.get(&vref) {
                return working_directory.dir_id;
            }
            if dir_id == 0 && vref == 0 {
                return self.default_dir_id;
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
            return self.default_dir_id;
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
            Some(self.default_dir_id)
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
        let normalized = Self::normalize_vfs_path(name);
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
        let normalized = Self::normalize_vfs_path(name);
        if let Some(dir_path) = self.directory_path_for_id(dir_id) {
            let candidate = if dir_path.is_empty() {
                normalized.clone()
            } else {
                format!("{dir_path}/{normalized}")
            };
            if let Some(found) = Self::find_case_insensitive_key(self.vfs_rsrc.keys(), &candidate) {
                return Some(found);
            }
        }
        if normalized.contains('/') {
            if let Some(found) = Self::find_case_insensitive_key(self.vfs_rsrc.keys(), &normalized)
            {
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
            }
        }
        None
    }

    pub(crate) fn find_vfs_directory_in_directory(
        &mut self,
        dir_id: u32,
        name: &str,
    ) -> Option<String> {
        self.ensure_vfs_catalog();
        let normalized = Self::normalize_vfs_path(name);
        if normalized.is_empty() {
            return None;
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
        sorted_keys
            .iter()
            .copied()
            .filter(|path| !path.is_empty())
            .find(|path| Self::vfs_basename(path).eq_ignore_ascii_case(&normalized))
            .cloned()
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
        // Arrow cursor data (16x16, 1 bit/pixel, 2 bytes per row = 32 bytes)
        #[rustfmt::skip]
        let data: [u8; 32] = [
            0x00, 0x00, // ................
            0x40, 0x00, // .X..............
            0x60, 0x00, // .XX.............
            0x70, 0x00, // .XXX............
            0x78, 0x00, // .XXXX...........
            0x7C, 0x00, // .XXXXX..........
            0x7E, 0x00, // .XXXXXX.........
            0x7F, 0x00, // .XXXXXXX........
            0x7F, 0x80, // .XXXXXXXX.......
            0x7C, 0x00, // .XXXXX..........
            0x6C, 0x00, // .XX.XX..........
            0x46, 0x00, // .X...XX.........
            0x06, 0x00, // .....XX.........
            0x03, 0x00, // ......XX........
            0x03, 0x00, // ......XX........
            0x00, 0x00, // ................
        ];
        // Arrow cursor mask
        #[rustfmt::skip]
        let mask: [u8; 32] = [
            0xC0, 0x00, // XX..............
            0xE0, 0x00, // XXX.............
            0xF0, 0x00, // XXXX............
            0xF8, 0x00, // XXXXX...........
            0xFC, 0x00, // XXXXXX..........
            0xFE, 0x00, // XXXXXXX.........
            0xFF, 0x00, // XXXXXXXX........
            0xFF, 0x80, // XXXXXXXXX.......
            0xFF, 0xC0, // XXXXXXXXXX......
            0xFF, 0xE0, // XXXXXXXXXXX.....
            0xFE, 0x00, // XXXXXXX.........
            0xEF, 0x00, // XXX.XXXX........
            0xCF, 0x00, // XX..XXXX........
            0x07, 0x80, // .....XXXX.......
            0x07, 0x80, // .....XXXX.......
            0x03, 0x80, // ......XXX.......
        ];
        (data, mask, 1, 1) // hotspot at (1, 1)
    }

    pub(crate) fn default_arrow_cursor_image() -> CursorImage {
        let (data, mask, hot_v, hot_h) = Self::default_arrow_cursor();
        CursorImage::mono(data, mask, hot_v, hot_h)
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
        self.mouse_pos = (v, h);
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
        self.mouse_button = true;
        self.mouse_pos = (v, h);
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
        self.mouse_pos = (v, h);
        self.mouse_button = false;
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
        if self.key_is_down(key_code) {
            return;
        }
        set_key_map_key(&mut self.key_map, key_code, true);
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
            where_v: self.mouse_pos.0,
            where_h: self.mouse_pos.1,
            modifiers,
        });

        if Self::key_generates_auto_key(key_code) {
            // Auto-key timing defaults are 16 ticks for the first repeat and
            // 4 ticks thereafter. Inside Macintosh Volume I, I-246.
            self.key_repeat = Some(KeyRepeatState {
                key_code,
                char_code,
                next_tick: self.tick_count.wrapping_add(Self::AUTO_KEY_THRESHOLD_TICKS),
            });
        }
    }

    /// Push a key-up event into the event queue.
    pub fn push_key_up(&mut self, key_code: u8, char_code: u8) {
        set_key_map_key(&mut self.key_map, key_code, false);
        if self
            .key_repeat
            .is_some_and(|repeat| repeat.key_code == key_code)
        {
            self.key_repeat = None;
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
        if self.posted_event_is_enabled(4) {
            self.event_queue.push_back(QueuedEvent {
                what: 4, // keyUp
                message,
                where_v: self.mouse_pos.0,
                where_h: self.mouse_pos.1,
                modifiers,
            });
        }
    }

    /// Get the current cursor data for rendering overlay.
    pub fn cursor(&self) -> Option<&CursorImage> {
        if self.cursor_visible {
            self.cursor_data.as_ref()
        } else {
            None
        }
    }

    /// Show the cursor (called by GUI on mouse move to undo ObscureCursor).
    pub fn show_cursor(&mut self) {
        // Respect HideCursor/ShowCursor balancing: mouse motion should not
        // force-show a cursor hidden via cursor level semantics.
        self.cursor_visible = self.cursor_level == 0;
    }

    /// Check if cursor is visible (for debug logging).
    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    /// Current cursor hide/show nesting level.
    pub fn cursor_level(&self) -> i16 {
        self.cursor_level
    }

    /// Whether a cursor image is installed, independent of visibility.
    pub fn cursor_data_present(&self) -> bool {
        self.cursor_data.is_some()
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
        if !self.fullscreen_locked || self.cursor_visible {
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
        self.cursor_data.as_ref().map(|cursor| cursor.mono_parts())
    }

    /// Get the current mouse position.
    pub fn mouse_position(&self) -> (i16, i16) {
        self.mouse_pos
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
        let mut loaded = HashMap::new();
        let mut named = HashMap::new();
        let mut names_by_id = HashMap::new();
        let mut attrs = HashMap::new();
        // Sort resources by (type, id) for deterministic heap layout across runs.
        let mut sorted_resources: Vec<_> = fork.resources().iter().collect();
        sorted_resources.sort_by_key(|((res_type, id), _)| (*res_type, *id));
        for ((res_type, id), res) in sorted_resources {
            let ptr = bus.alloc(res.data.len() as u32);
            bus.write_bytes(ptr, &res.data);
            Self::zero_loaded_resource_padding(bus, ptr, res.data.len() as u32);
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

    pub(crate) fn remember_resource_backing_data(
        &mut self,
        refnum: u16,
        res_type: [u8; 4],
        res_id: i16,
        data: Vec<u8>,
    ) {
        if res_type == *b"FONT" || res_type == *b"NFNT" {
            let _ = crate::quickdraw::fonts::register_resource_font_strike(res_id, &data);
        }
        self.resource_backing_data
            .insert((refnum, res_type, res_id), data);
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
            if res_type == b"FONT" || res_type == b"NFNT" {
                let _ =
                    crate::quickdraw::fonts::register_resource_font_strike(*res_id, &resource.data);
            }
            self.resource_backing_data
                .entry((refnum, *res_type, *res_id))
                .or_insert_with(|| resource.data.clone());
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

    pub(crate) fn forget_resource_live_map_entry_for_handle(&mut self, handle: u32) {
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
            file.loaded.remove(&(res_type, res_id));
        }
        file.named.retain(|(named_type, _), (named_id, named_ptr)| {
            !(*named_type == res_type && *named_id == res_id && *named_ptr == ptr)
        });
    }

    pub(crate) fn clear_resource_file_handle_index(&mut self, refnum: u16) {
        self.resource_handles_by_key
            .retain(|(entry_refnum, _, _), _| *entry_refnum != refnum);
    }

    pub(crate) fn resource_search_order(&self) -> Vec<u16> {
        let Some(resources) = self.resources.as_ref() else {
            return Vec::new();
        };

        // The Resource Manager searches the current file and only the files
        // opened before it, in reverse open order.
        // Inside Macintosh Volume I, I-125 to I-126
        let mut order = Vec::new();
        let mut include = false;
        for refnum in resources.search_order.iter().rev().copied() {
            if refnum == resources.current_file {
                include = true;
            }
            if include && resources.files.contains_key(&refnum) {
                order.push(refnum);
            }
        }
        if order.is_empty() && resources.files.contains_key(&resources.current_file) {
            order.push(resources.current_file);
        }
        order
    }

    pub(crate) fn current_resource_refnum(&self) -> u16 {
        self.resources.as_ref().map_or(0, |resources| {
            if resources.files.contains_key(&resources.current_file) {
                resources.current_file
            } else {
                0
            }
        })
    }

    pub(crate) fn set_current_resource_refnum(&mut self, bus: &mut MacMemoryBus, refnum: u16) {
        if let Some(resources) = self.resources.as_mut() {
            resources.current_file = if resources.files.contains_key(&refnum) {
                refnum
            } else {
                0
            };
        }
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
            closed = true;
        }
        self.clear_resource_file_backing_data(refnum);
        self.clear_resource_file_handle_index(refnum);

        if !closed {
            return false;
        }

        let mut freed_ptrs = 0usize;
        for ptr in file_ptrs {
            self.ptr_to_handle.remove(&ptr);
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
            self.handle_state_bits.remove(handle);
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

    pub(crate) fn find_resource_any(&self, res_type: [u8; 4], res_id: i16) -> Option<(u16, u32)> {
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
        if !matches!(res_id, 1 | 2 | 4 | 8) {
            return None;
        }

        let std_clut = Self::standard_mac_8bpp_clut();
        let ptr = bus.alloc(8 + 256 * 8);
        bus.write_long(ptr, res_id as u32); // ctSeed follows the standard depth ID.
        bus.write_word(ptr + 4, 0); // ctFlags
        bus.write_word(ptr + 6, 255); // ctSize
        for index in 0u32..256 {
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

        let ptr = bus.alloc(8);
        bus.write_word(ptr, 0x205F); // MOVEA.L (SP)+,A0 — recover JSR return PC.
        bus.write_word(ptr + 2, 0xDEFC); // ADDA.W #18,SP — discard MDEF parameters.
        bus.write_word(ptr + 4, 18);
        bus.write_word(ptr + 6, 0x4ED0); // JMP (A0).
        self.system_mdef_cache.insert(res_id, ptr);
        Some(ptr)
    }

    /// Allocate (and cache) the standard U.S. Roman keyboard-layout
    /// resource (`'KCHR'` ID 0). Inside Macintosh: Text 1993, C-18..C-19
    /// defines the resource as a version byte, a 256-byte table-selection
    /// index, and 128-byte character-mapping tables keyed by virtual key code.
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
        const TABLE_BASE: usize = 1 + 256;
        const LEN: usize = TABLE_BASE + TABLES * 128;
        let mut body = vec![0u8; LEN];
        for modifier in 0..=255usize {
            body[1 + modifier] = if (modifier & 0x22) != 0 { 1 } else { 0 };
        }

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
    /// no native handler is installed: instead of returning a bare
    /// fake-ptr that crashes when the guest does `JSR (A0)`, we
    /// hand back the address of a 2-byte stub containing the
    /// auto-pop variant of the canonical tool-trap word.
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
    /// The auto-pop bit is only valid for tool traps. OS traps
    /// would land here with the bit treated as a no-op flag, so
    /// JSR-through-fake-ptr to an OS trap still drifts off into
    /// garbage. Apps that JSR through OS-trap fake-ptrs are rarer
    /// than tool-trap variants in practice — `GetTrapAddress`
    /// callers typically only compare the address against
    /// `_Unimplemented` rather than calling through it. IM:II
    /// II-384 (NGetTrapAddress); IM:V V-577 (auto-pop bit).
    pub(crate) fn get_or_create_tool_trap_trampoline(
        &mut self,
        bus: &mut MacMemoryBus,
        trap_word: u16,
    ) -> u32 {
        let canonical_trap_word = 0xA800 | (trap_word & 0x03FF);
        if let Some(&addr) = self.tool_trap_trampolines.get(&canonical_trap_word) {
            return addr;
        }
        let addr = bus.alloc(2);
        bus.write_word(addr, canonical_trap_word | 0x0400);
        self.tool_trap_trampolines.insert(canonical_trap_word, addr);
        addr
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
    /// current resource search order. Returns `(id, name)` pairs sorted
    /// by id ascending — matching the on-disk resource-map order real
    /// Mac AddResMenu / InsertResMenu walks per IM:I I-353.
    /// Resources without a name are NOT returned (those don't surface
    /// in AddResMenu's output). Inside Macintosh Volume I, I-353
    pub(crate) fn named_resources_of_type(&self, res_type: [u8; 4]) -> Vec<(i16, String)> {
        let res_type = Self::normalize_ostype(res_type);
        let Some(resources) = self.resources.as_ref() else {
            return Vec::new();
        };
        let mut seen_ids = std::collections::HashSet::new();
        let mut entries: Vec<(i16, String)> = Vec::new();
        for refnum in self.resource_search_order() {
            let Some(file) = resources.files.get(&refnum) else {
                continue;
            };
            for ((rt, n), (id, _)) in &file.named {
                if *rt != res_type {
                    continue;
                }
                // Per IM:I I-353 / IM:V V-242, AddResMenu should not
                // surface duplicates that overlay a closer file later
                // in the chain. Dedup on id (the resource map's
                // primary key per type).
                if seen_ids.insert(*id) {
                    entries.push((*id, n.clone()));
                }
            }
        }
        // IM:IV IV-56: "AddResMenu and InsertResMenu both sort the items alphabetically"
        entries.sort_by(|(_, a), (_, b)| a.to_lowercase().cmp(&b.to_lowercase()));
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
        let file = self.allocate_resource_fork(fork, bus);
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
        // Sort iteration so the first-match is stable across runs.
        let mut sorted_keys: Vec<&String> = self.vfs_rsrc.keys().collect();
        sorted_keys.sort_unstable();
        if let Some(found) = sorted_keys
            .iter()
            .copied()
            .find(|key| Self::normalize_vfs_path(key).eq_ignore_ascii_case(&normalized))
        {
            return Some(found.clone());
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
        let is_tool = (trap & 0x0800) != 0;
        // Count game traps: from game code (PC < 0x800000), NOT during
        // menu/dialog tracking loops (synthetic HLE re-dispatches), and
        // NOT idle-loop traps (GetNextEvent, WaitNextEvent, EventAvail)
        // which fire at wildly different rates depending on CPU speed.
        // Extract canonical trap number (strip toolbox/auto-pop bits)
        let trap_number = if (trap & 0x0800) != 0 {
            trap & 0x03FF
        } else {
            trap & 0x00FF
        };
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
        if trap_histogram_enabled() {
            self.trap_histogram[(trap & 0xFFF) as usize] =
                self.trap_histogram[(trap & 0xFFF) as usize].saturating_add(1);
        }
        let auto_pop = is_tool && (trap & 0x0400) != 0;
        let trap_num = if is_tool {
            trap & 0x03FF
        } else {
            trap & 0x00FF
        };
        let pc = cpu.read_reg(Register::PC);

        if trace_guest_pc_traps_enabled() && (0x00235000..=0x00238000).contains(&pc) {
            eprintln!(
                "[PC-TRAP] PC=${:08X} trap=${:04X} base=${:04X} tool={} auto_pop={}",
                pc,
                trap,
                if is_tool {
                    0xA800 | (trap & 0x03FF)
                } else {
                    0xA000 | (trap & 0x00FF)
                },
                is_tool,
                auto_pop,
            );
        }
        if trace_all_traps_enabled() {
            eprintln!(
                "[ALL-TRAP] PC=${:08X} trap=${:04X} base=${:04X} tool={} num=0x{:03X}",
                pc,
                trap,
                if is_tool {
                    0xA800 | (trap & 0x03FF)
                } else {
                    0xA000 | (trap & 0x00FF)
                },
                is_tool,
                trap_num,
            );
        }
        if trace_dialog_traps_enabled() && self.dialog_tracking.is_some() {
            eprintln!(
                "[DIALOG-TRAP] PC=${:08X} trap=${:04X} base=${:04X} tool={} auto_pop={}",
                pc,
                trap,
                if is_tool {
                    0xA800 | (trap & 0x03FF)
                } else {
                    0xA000 | (trap & 0x00FF)
                },
                is_tool,
                auto_pop,
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
        let base_trap = if is_tool {
            0xA800 | (trap & 0x03FF)
        } else {
            0xA000 | (trap & 0x00FF)
        };
        if !auto_pop {
            if let Some(&handler_addr) = self.native_trap_table.get(&base_trap) {
                // Simulate JSR to native handler: push return PC, jump to handler
                let return_pc = cpu.read_reg(Register::PC); // past A-line instruction
                let sp = cpu.read_reg(Register::A7);
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

        // Track consecutive SANE and TickCount calls.
        // Chain sub-dispatchers: first match wins
        let sp_before = cpu.read_reg(Register::A7);
        let result = self
            .dispatch_memory(is_tool, trap_num, cpu, bus)
            .or_else(|| self.dispatch_event(is_tool, trap_num, cpu, bus))
            .or_else(|| self.dispatch_resource(is_tool, trap_num, cpu, bus))
            .or_else(|| self.dispatch_quickdraw(is_tool, trap_num, cpu, bus))
            .or_else(|| self.dispatch_menu(is_tool, trap_num, cpu, bus))
            .or_else(|| self.dispatch_window(is_tool, trap_num, cpu, bus))
            .or_else(|| self.dispatch_control(is_tool, trap_num, cpu, bus))
            .or_else(|| self.dispatch_dialog(is_tool, trap_num, cpu, bus))
            .or_else(|| self.dispatch_sound(is_tool, trap_num, cpu, bus))
            .or_else(|| self.dispatch_toolbox(is_tool, trap_num, cpu, bus))
            .or_else(|| self.dispatch_sane(is_tool, trap_num, cpu, bus))
            .unwrap_or_else(|| {
                eprintln!(
                    "[TRAP] UNIMPLEMENTED ${:04X} (is_tool={}, num=0x{:03X})",
                    trap, is_tool, trap_num
                );
                Err(Error::UnimplementedTrap(trap))
            });

        if result.is_ok() && !is_tool {
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
    use crate::trap::menu::MenuTrackingState;
    use std::collections::VecDeque;

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
        bytes[ref_list_start + 5..ref_list_start + 8].copy_from_slice(&0u32.to_be_bytes()[1..4]);

        bytes
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
        disp.menu_tracking = Some(MenuTrackingState {
            active_menu: 0,
            highlighted_item: 0,
            saved_pixels: Vec::new(),
            dropdown_rect: (0, 0, 0, 0),
            submenu: None,
            stack_ptr: 0,
            flash_remaining: 0,
            flash_delay: 0,
            flash_result: 0,
        });
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
        disp.cursor_visible = false;
        assert_eq!(disp.fullscreen_input_transform(), None);

        disp.fullscreen_locked = true;
        disp.cursor_visible = true;
        assert_eq!(disp.fullscreen_input_transform(), None);

        disp.cursor_visible = false;
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
        disp.cursor_visible = false;
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
        disp.cursor_visible = false;
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

        let (_, app_ptr) = disp.find_resource_any(*b"TEST", 1).unwrap();
        let (_, companion_ptr) = disp.find_resource_any(*b"TEST", 2).unwrap();
        assert_eq!(bus.read_bytes(app_ptr, 3), b"app");
        assert_eq!(bus.read_bytes(companion_ptr, 4), b"side");
        assert_eq!(disp.count_resources(*b"TEST", true), 2);
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
                                                   // Auto-pop variants too.
        assert!(!disp.is_tracking_refire(0xAD3D));
        assert!(!disp.is_tracking_refire(0xAC0B));
        assert!(!disp.is_tracking_refire(0xAD91));
        assert!(!disp.is_tracking_refire(0xAD68));
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
}
