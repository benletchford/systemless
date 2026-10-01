//! Architecture-neutral Dialog Manager records, DITL parsing, and geometry.
//!
//! Inside Macintosh Volume I (1985), pp. I-399--I-434, and
//! Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 6-1--6-179.
use std::borrow::Cow;

use crate::mac_roman::encode_mac_roman_lossy;
use crate::trap::types::decode_mac_roman;

/// Canonical guest DialogRecord byte size.
pub const DIALOG_RECORD_SIZE: u32 = 256;

/// Canonical DialogRecord guest field offsets.
pub const DIALOG_WINDOW_KIND_OFFSET: u32 = 108;
pub const DIALOG_GO_AWAY_FLAG_OFFSET: u32 = 112;
pub const DIALOG_ITEMS_OFFSET: u32 = 156;
pub const DIALOG_TEXT_HANDLE_OFFSET: u32 = 160;
pub const DIALOG_EDIT_FIELD_OFFSET: u32 = 164;
pub const DIALOG_EDIT_OPEN_OFFSET: u32 = 166;
pub const DIALOG_DEFAULT_ITEM_OFFSET: u32 = 168;
pub const DIALOG_RESOURCE_ID_OFFSET: u32 = 170;

/// Canonical DialogRecord GrafPort text font offset (`txFont`).
/// Inside Macintosh Volume I, pp. I-148, I-412.
pub const DIALOG_TX_FONT_OFFSET: u32 = 68;

/// Canonical default dialog font family number (0 = system font / Chicago).
/// Inside Macintosh Volume I, p. I-411.
pub const DIALOG_INITIAL_FONT: i16 = 0;

/// Canonical initial value for `editField` in a newly created DialogRecord (-1 = no edit field active).
/// Inside Macintosh Volume I, p. I-411.
pub const DIALOG_INITIAL_EDIT_FIELD: i16 = -1;

/// Canonical initial value for `editOpen` in a newly created DialogRecord (0 = closed).
/// Inside Macintosh Volume I, p. I-411.
pub const DIALOG_INITIAL_EDIT_OPEN: i16 = 0;

/// Canonical initial value for `aDefItem` in a newly created DialogRecord (1 = item 1 is default button).
/// Inside Macintosh Volume I, p. I-411.
pub const DIALOG_INITIAL_DEFAULT_ITEM: i16 = 1;

/// Host-private Dialog Manager state offsets following the documented DialogRecord.
pub const DIALOG_CANCEL_ITEM_OFFSET: u32 = 172;
pub const DIALOG_ALERT_HIT_OFFSET: u32 = 174;
pub const DIALOG_STANDARD_ALERT_OUTPUT_OFFSET: u32 = 176;
pub const DIALOG_STANDARD_ALERT_STACK_OFFSET: u32 = 180;
pub const DIALOG_TIMEOUT_BUTTON_OFFSET: u32 = 184;
pub const DIALOG_TIMEOUT_SECONDS_OFFSET: u32 = 188;
pub const DIALOG_TIMEOUT_START_TICK_OFFSET: u32 = 192;
#[allow(dead_code)]
pub const DIALOG_MODAL_EVENT_MASK_OFFSET: u32 = 196;
#[allow(dead_code)]
pub const DIALOG_STANDARD_SHEET_COMMAND_OFFSET: u32 = 200;
#[allow(dead_code)]
pub const DIALOG_TRACKS_CURSOR_OFFSET: u32 = 204;

/// Default cursor tracking state (false = off per Macintosh Toolbox Essentials, p. 6-166).
#[allow(dead_code)]
pub const DIALOG_DEFAULT_TRACKS_CURSOR: bool = false;

/// Canonical System 7 and Carbon dialog auto-positioning codes.
/// Universal Interfaces <Dialogs.h> and <MacWindows.h>.
#[allow(dead_code)]
pub const DIALOG_POSITION_DEFAULT: u16 = 0x0000;
pub const DIALOG_POSITION_CENTER_MAIN_SCREEN: u16 = 0x280A;
pub const DIALOG_POSITION_ALERT_MAIN_SCREEN: u16 = 0x300A;
pub const DIALOG_POSITION_STAGGER_MAIN_SCREEN: u16 = 0x380A;
pub const DIALOG_POSITION_CENTER_PARENT_WINDOW: u16 = 0xA80A;
pub const DIALOG_POSITION_ALERT_PARENT_WINDOW: u16 = 0xB00A;
pub const DIALOG_POSITION_STAGGER_PARENT_WINDOW: u16 = 0xB80A;
pub const DIALOG_POSITION_CENTER_PARENT_WINDOW_SCREEN: u16 = 0x680A;
pub const DIALOG_POSITION_ALERT_PARENT_WINDOW_SCREEN: u16 = 0x700A;
pub const DIALOG_POSITION_STAGGER_PARENT_WINDOW_SCREEN: u16 = 0x780A;

/// Default event mask for modal dialog event filtering (`everyEvent`).
#[allow(dead_code)]
pub const DIALOG_DEFAULT_MODAL_EVENT_MASK: u16 = 0xFFFF;

/// Size of `AlertStdCFStringAlertParamRec` in guest memory.
#[allow(dead_code)]
pub const ALERT_STD_CFSTRING_ALERT_PARAM_REC_SIZE: u32 = 32;
/// Version one of `AlertStdCFStringAlertParamRec`.
#[allow(dead_code)]
pub const STD_CFSTRING_ALERT_VERSION_ONE: u32 = 1;
/// Default OK button index for standard alerts.
#[allow(dead_code)]
pub const ALERT_STD_ALERT_OK_BUTTON: i16 = 1;

/// Canonical Dialog window kind. Inside Macintosh Volume I, p. I-273.
pub const DIALOG_WINDOW_KIND: u16 = 2;

/// Canonical evaluated initial fields for a newly created `DialogRecord`.
///
/// Inside Macintosh Volume I, pp. I-407--I-411:
/// Newly created dialogs initialize `windowKind` to `dialogKind` (2), store the items list handle,
/// initialize `textH` to NIL (0), `editField` to -1 (no field active), `editOpen` to 0 (closed),
/// and `aDefItem` to 1 (item 1 default).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialogRecordInitEvaluation {
    items_handle: u32,
}

impl DialogRecordInitEvaluation {
    /// Constructs initial DialogRecord evaluation from an items list handle.
    pub const fn new(items_handle: u32) -> Self {
        Self { items_handle }
    }

    /// The window kind word (`dialogKind` = 2).
    pub const fn window_kind(&self) -> u16 {
        DIALOG_WINDOW_KIND
    }

    /// The item list handle.
    pub const fn items_handle(&self) -> u32 {
        self.items_handle
    }

    /// The initial text handle (NIL = 0).
    #[allow(dead_code)]
    pub const fn text_handle(&self) -> u32 {
        0
    }

    /// The initial edit field index (-1 = no edit field active).
    pub const fn edit_field(&self) -> i16 {
        DIALOG_INITIAL_EDIT_FIELD
    }

    /// The initial edit open flag (0 = closed).
    pub const fn edit_open(&self) -> i16 {
        DIALOG_INITIAL_EDIT_OPEN
    }

    /// The initial default item number (1 = item 1 is default).
    pub const fn default_item(&self) -> i16 {
        DIALOG_INITIAL_DEFAULT_ITEM
    }

    /// The initial cancel item number (0 = none set).
    #[allow(dead_code)]
    pub const fn cancel_item(&self) -> i16 {
        0
    }

    /// The initial alert hit index (0 = none).
    #[allow(dead_code)]
    pub const fn alert_hit(&self) -> i16 {
        0
    }

    /// The initial resource ID (0 = none).
    #[allow(dead_code)]
    pub const fn resource_id(&self) -> i16 {
        0
    }
}

/// Evaluates the initial `DialogRecord` fields for a dialog with the given item list handle.
pub const fn evaluate_dialog_record_init(items_handle: u32) -> DialogRecordInitEvaluation {
    DialogRecordInitEvaluation::new(items_handle)
}

/// Canonical evaluated initial fields for an alert's underlying `DialogRecord`.
///
/// Inside Macintosh Volume I, pp. I-424--I-425:
/// Alerts construct an underlying `DialogRecord` with standard window kind (`dialogKind` = 2),
/// items handle, resource ID, default item, and cancel item tracking.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AlertDialogRecordInitEvaluation {
    items_handle: u32,
    alert_id: i16,
    default_item: i16,
    cancel_item: i16,
}

impl AlertDialogRecordInitEvaluation {
    /// Constructs initial alert DialogRecord evaluation.
    #[inline]
    #[must_use]
    pub const fn new(
        items_handle: u32,
        alert_id: i16,
        default_item: i16,
        cancel_item: i16,
    ) -> Self {
        Self {
            items_handle,
            alert_id,
            default_item,
            cancel_item,
        }
    }

    /// The window kind word (`dialogKind` = 2).
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn window_kind(&self) -> u16 {
        DIALOG_WINDOW_KIND
    }

    /// The item list handle.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn items_handle(&self) -> u32 {
        self.items_handle
    }

    /// The initial text handle (NIL = 0).
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn text_handle(&self) -> u32 {
        0
    }

    /// The initial edit field index (-1 = no edit field active).
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn edit_field(&self) -> i16 {
        DIALOG_INITIAL_EDIT_FIELD
    }

    /// The initial edit open flag (0 = closed).
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn edit_open(&self) -> i16 {
        DIALOG_INITIAL_EDIT_OPEN
    }

    /// The alert resource ID.
    #[inline]
    #[must_use]
    pub const fn alert_id(&self) -> i16 {
        self.alert_id
    }

    /// The default button item number.
    #[inline]
    #[must_use]
    pub const fn default_item(&self) -> i16 {
        self.default_item
    }

    /// The initial alert hit index (0 = none yet).
    #[inline]
    #[must_use]
    pub const fn alert_hit(&self) -> i16 {
        0
    }

    /// The cancel button item number.
    #[inline]
    #[must_use]
    pub const fn cancel_item(&self) -> i16 {
        self.cancel_item
    }
}

/// Evaluates initial `DialogRecord` fields for an alert dialog.
#[inline]
#[must_use]
pub const fn evaluate_alert_dialog_record_init(
    items_handle: u32,
    alert_id: i16,
    default_item: i16,
    cancel_item: i16,
) -> AlertDialogRecordInitEvaluation {
    AlertDialogRecordInitEvaluation::new(items_handle, alert_id, default_item, cancel_item)
}

/// Dialog record storage allocation policy.
///
/// Inside Macintosh Volume I, pp. I-412, I-424:
/// If `dStorage` is NIL (0), the Dialog Manager allocates the storage for the `DialogRecord`.
/// If `dStorage` is non-NIL, it is a caller-supplied pointer to storage for the dialog record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogStoragePolicy {
    /// Dialog Manager allocates storage dynamically from the heap.
    AllocateNew,
    /// Storage is caller-supplied at the given guest memory pointer.
    CallerSupplied(u32),
}

#[allow(dead_code)]
impl DialogStoragePolicy {
    /// Returns true if caller supplied storage.
    #[inline]
    pub const fn is_caller_supplied(&self) -> bool {
        matches!(self, Self::CallerSupplied(_))
    }

    /// Returns true if Dialog Manager needs to allocate new storage.
    #[inline]
    pub const fn is_allocate_new(&self) -> bool {
        matches!(self, Self::AllocateNew)
    }

    /// Returns the caller-supplied storage pointer, or None if newly allocated.
    #[inline]
    pub const fn caller_storage(&self) -> Option<u32> {
        match self {
            Self::CallerSupplied(ptr) => Some(*ptr),
            Self::AllocateNew => None,
        }
    }
}

/// Evaluates the storage allocation policy for a dialog record.
///
/// When `storage` is 0 (NIL), returns `DialogStoragePolicy::AllocateNew`.
/// When non-zero, returns `DialogStoragePolicy::CallerSupplied(storage)`.
#[inline]
pub const fn evaluate_dialog_storage_policy(storage: u32) -> DialogStoragePolicy {
    if storage != 0 {
        DialogStoragePolicy::CallerSupplied(storage)
    } else {
        DialogStoragePolicy::AllocateNew
    }
}

/// Canonical validated parameters for a `GetNewDialog` request.
///
/// Inside Macintosh Volume I, p. I-413;
/// Macintosh Toolbox Essentials (1992), pp. 6-117--6-118:
/// `FUNCTION GetNewDialog (dialogID: Integer; dStorage: Ptr; behind: WindowPtr): DialogPtr;`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetNewDialogParameters {
    dialog_id: i16,
    storage: u32,
    behind: u32,
}

impl GetNewDialogParameters {
    /// Constructs a new `GetNewDialogParameters` instance.
    #[inline]
    #[must_use]
    pub const fn new(dialog_id: i16, storage: u32, behind: u32) -> Self {
        Self {
            dialog_id,
            storage,
            behind,
        }
    }

    /// Target resource ID of the `DLOG` template.
    #[inline]
    #[must_use]
    pub const fn dialog_id(&self) -> i16 {
        self.dialog_id
    }

    /// The raw storage pointer passed by the caller (0 = allocate).
    #[inline]
    #[must_use]
    pub const fn storage(&self) -> u32 {
        self.storage
    }

    /// The dialog storage allocation policy.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn storage_policy(&self) -> DialogStoragePolicy {
        evaluate_dialog_storage_policy(self.storage)
    }

    /// The window pointer to place the dialog behind (`0xFFFFFFFF` = frontmost).
    #[inline]
    #[must_use]
    pub const fn behind(&self) -> u32 {
        self.behind
    }
}

/// Evaluates and validates input parameters for a `GetNewDialog` request.
///
/// Inside Macintosh Volume I, p. I-413;
/// Macintosh Toolbox Essentials (1992), pp. 6-117--6-118.
#[inline]
pub const fn evaluate_get_new_dialog_parameters(
    dialog_id: i16,
    storage: u32,
    behind: u32,
) -> GetNewDialogParameters {
    GetNewDialogParameters::new(dialog_id, storage, behind)
}

/// Canonical validated parameters for `NewDialog`, `NewCDialog`, and `NewFeaturesDialog`.
///
/// Inside Macintosh Volume I, p. I-412, Volume V, p. V-243, and
/// Mac Toolbox: Appearance Manager (1997).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NewDialogParameters {
    storage: u32,
    bounds_ptr: u32,
    title_ptr: u32,
    visible: bool,
    proc_id: i16,
    behind: u32,
    go_away: bool,
    ref_con: u32,
    items: u32,
    is_color: bool,
}

#[allow(dead_code)]
impl NewDialogParameters {
    /// Creates a new evaluated dialog parameters structure.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub const fn new(
        storage: u32,
        bounds_ptr: u32,
        title_ptr: u32,
        visible: bool,
        proc_id: i16,
        behind: u32,
        go_away: bool,
        ref_con: u32,
        items: u32,
        is_color: bool,
    ) -> Self {
        Self {
            storage,
            bounds_ptr,
            title_ptr,
            visible,
            proc_id,
            behind,
            go_away,
            ref_con,
            items,
            is_color,
        }
    }

    /// The raw storage pointer passed by the caller (0 = allocate).
    #[inline]
    pub const fn storage(&self) -> u32 {
        self.storage
    }

    /// The dialog storage allocation policy.
    #[inline]
    pub const fn storage_policy(&self) -> DialogStoragePolicy {
        evaluate_dialog_storage_policy(self.storage)
    }

    /// The pointer to the bounds rectangle in guest memory.
    #[inline]
    pub const fn bounds_ptr(&self) -> u32 {
        self.bounds_ptr
    }

    /// The pointer to the Pascal title string in guest memory.
    #[inline]
    pub const fn title_ptr(&self) -> u32 {
        self.title_ptr
    }

    /// Whether the dialog should be initially visible.
    #[inline]
    pub const fn is_visible(&self) -> bool {
        self.visible
    }

    /// The window definition procedure ID (`procID`).
    #[inline]
    pub const fn proc_id(&self) -> i16 {
        self.proc_id
    }

    /// The window placement pointer (`behind`: -1 = in front, 0 = in back, or WindowPtr).
    #[inline]
    pub const fn behind(&self) -> u32 {
        self.behind
    }

    /// Whether the dialog window includes a close/go-away box.
    #[inline]
    pub const fn go_away(&self) -> bool {
        self.go_away
    }

    /// The dialog reference constant (`refCon`).
    #[inline]
    pub const fn ref_con(&self) -> u32 {
        self.ref_con
    }

    /// The handle to the dialog item list (DITL) resource or data in guest memory.
    #[inline]
    pub const fn items(&self) -> u32 {
        self.items
    }

    /// Whether the dialog opts into color GrafPort representation (`NewCDialog` / `NewFeaturesDialog`).
    #[inline]
    pub const fn is_color(&self) -> bool {
        self.is_color
    }

    /// Returns true if caller supplied a non-null title pointer.
    #[inline]
    pub const fn has_title(&self) -> bool {
        self.title_ptr != 0
    }

    /// Returns true if caller supplied a non-null items list handle.
    #[inline]
    pub const fn has_items(&self) -> bool {
        self.items != 0
    }
}

/// Evaluates and validates creation parameters for `NewDialog` / `NewCDialog`.
///
/// Returns `Err(DIALOG_PARAM_ERR)` (-50) if `bounds_ptr == 0`, otherwise returns `Ok(NewDialogParameters)`.
#[allow(clippy::too_many_arguments)]
#[inline]
pub const fn evaluate_new_dialog_parameters(
    storage: u32,
    bounds_ptr: u32,
    title_ptr: u32,
    visible: bool,
    proc_id: i16,
    behind: u32,
    go_away: bool,
    ref_con: u32,
    items: u32,
    is_color: bool,
) -> Result<NewDialogParameters, i16> {
    if bounds_ptr == 0 {
        return Err(DIALOG_PARAM_ERR);
    }
    Ok(NewDialogParameters {
        storage,
        bounds_ptr,
        title_ptr,
        visible,
        proc_id,
        behind,
        go_away,
        ref_con,
        items,
        is_color,
    })
}

/// Canonical DialogDispatch ($AA68) routine selectors.
/// Macintosh Toolbox Essentials (1992), pp. 6-162--6-167.
#[allow(dead_code)]
pub const DIALOG_DISPATCH_NEW_COLOR_DIALOG: u16 = 0x0000;
pub const DIALOG_DISPATCH_GET_STD_FILTER_PROC: u16 = 0x0003;
pub const DIALOG_DISPATCH_SET_DIALOG_DEFAULT_ITEM: u16 = 0x0004;
pub const DIALOG_DISPATCH_SET_DIALOG_CANCEL_ITEM: u16 = 0x0005;
pub const DIALOG_DISPATCH_SET_DIALOG_TRACKS_CURSOR: u16 = 0x0006;
pub const DIALOG_DISPATCH_NEW_FEATURES_DIALOG: u16 = 0x000C;
pub const DIALOG_DISPATCH_AUTO_SIZE_DIALOG: u16 = 0x000D;
pub const DIALOG_DISPATCH_GET_DIALOG_ITEM_AS_CONTROL: u16 = 0x000F;
pub const DIALOG_DISPATCH_MOVE_DIALOG_ITEM: u16 = 0x0010;
pub const DIALOG_DISPATCH_SIZE_DIALOG_ITEM: u16 = 0x0011;
pub const DIALOG_DISPATCH_APPEND_DIALOG_ITEM_LIST: u16 = 0x0012;
#[allow(dead_code)]
pub const DIALOG_DISPATCH_GET_DIALOG_DEFAULT_ITEM: u16 = 0x0012;
pub const DIALOG_DISPATCH_GET_DIALOG_CANCEL_ITEM: u16 = 0x0013;

/// Standard Mac OS result codes used by Dialog Manager extension routines.
pub const DIALOG_NO_ERR: i16 = 0;
pub const DIALOG_PARAM_ERR: i16 = -50;
pub const DIALOG_RES_NOT_FOUND: i16 = -192;

/// Standard default button outline thickness in pixels.
/// Macintosh Toolbox Essentials (1992), Listing 6-17.
pub const DEFAULT_BUTTON_OUTLINE_THICKNESS: i16 = 3;

/// Standard default button outline outset padding in pixels.
/// Macintosh Toolbox Essentials (1992), Listing 6-17.
pub const DEFAULT_BUTTON_OUTLINE_INSET: i16 = 4;

/// Standard margin around dialog content for a modal dialog box (`dBoxProc`).
/// Inside Macintosh Volume I, p. I-273.
pub const DIALOG_DBOX_FRAME_MARGIN: i16 = 8;

/// Standard outset around an active edit-text field for focus/border frame rendering.
/// Inside Macintosh Volume I, p. I-414; Macintosh Human Interface Guidelines (1992), p. 184.
pub const EDIT_TEXT_FRAME_OUTSET: i16 = 3;

/// Standard flush-left text glyph inset from destination rectangle leading edge.
/// Inside Macintosh Volume I, pp. I-373--I-374.
pub const DIALOG_TEXT_LEFT_INSET: i16 = 1;

/// Canonical AppendDITL placement methods.
/// Macintosh Toolbox Essentials (1992), pp. 6-108, 6-153.
pub const APPEND_DITL_OVERLAY: i16 = 0;
pub const APPEND_DITL_RIGHT: i16 = 1;
pub const APPEND_DITL_BOTTOM: i16 = 2;

/// Canonical DITL entry field offsets in guest memory.
/// Inside Macintosh Volume I, p. I-426.
pub const DITL_ITEM_HANDLE_OFFSET: u32 = 0;
pub const DITL_ITEM_RECT_OFFSET: u32 = 4;
pub const DITL_ITEM_TYPE_OFFSET: u32 = 12;
#[allow(dead_code)]
pub const DITL_ITEM_DATA_LEN_OFFSET: u32 = 13;

/// Item disable bit in the DITL item type byte.
pub const DIALOG_ITEM_DISABLED_FLAG: u8 = 0x80;

/// Canonical Dialog item type codes. Inside Macintosh Volume I, p. I-427.
pub const DIALOG_ITEM_USER_ITEM: u8 = 0;
pub const DIALOG_ITEM_BUTTON: u8 = 4;
pub const DIALOG_ITEM_CHECKBOX: u8 = 5;
pub const DIALOG_ITEM_RADIO: u8 = 6;
pub const DIALOG_ITEM_RESOURCE_CONTROL: u8 = 7;
pub const DIALOG_ITEM_STATIC_TEXT: u8 = 8;
pub const DIALOG_ITEM_EDIT_TEXT: u8 = 16;
pub const DIALOG_ITEM_ICON: u8 = 32;
pub const DIALOG_ITEM_PICTURE: u8 = 64;

/// Canonical Alert types. Inside Macintosh Volume I, p. I-417, and Appearance.h.
pub const ALERT_TYPE_STOP: u16 = 0;
pub const ALERT_TYPE_NOTE: u16 = 1;
pub const ALERT_TYPE_CAUTION: u16 = 2;
#[allow(dead_code)]
pub const ALERT_TYPE_PLAIN: u16 = 3;

/// Canonical Standard Alert button IDs. Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 6-110.
#[allow(dead_code)]
pub const ALERT_BUTTON_OK: i16 = 1;
#[allow(dead_code)]
pub const ALERT_BUTTON_CANCEL: i16 = 2;
#[allow(dead_code)]
pub const ALERT_BUTTON_OTHER: i16 = 3;
#[allow(dead_code)]
pub const ALERT_BUTTON_HELP: i16 = 4;

/// Strongly typed Dialog Manager item kind.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogItemKind {
    UserItem,
    Button,
    Checkbox,
    RadioButton,
    ResourceControl,
    StaticText,
    EditText,
    Icon,
    Picture,
    Unknown(u8),
}

#[allow(dead_code)]
impl DialogItemKind {
    /// Decode the base item kind, masking out the disabled flag bit (0x80).
    pub const fn from_raw_type(raw: u8) -> Self {
        match raw & 0x7F {
            DIALOG_ITEM_USER_ITEM => Self::UserItem,
            DIALOG_ITEM_BUTTON => Self::Button,
            DIALOG_ITEM_CHECKBOX => Self::Checkbox,
            DIALOG_ITEM_RADIO => Self::RadioButton,
            DIALOG_ITEM_RESOURCE_CONTROL => Self::ResourceControl,
            DIALOG_ITEM_STATIC_TEXT => Self::StaticText,
            DIALOG_ITEM_EDIT_TEXT => Self::EditText,
            DIALOG_ITEM_ICON => Self::Icon,
            DIALOG_ITEM_PICTURE => Self::Picture,
            other => Self::Unknown(other),
        }
    }

    /// Return the raw integer code for this item kind.
    pub const fn raw_base_type(self) -> u8 {
        match self {
            Self::UserItem => DIALOG_ITEM_USER_ITEM,
            Self::Button => DIALOG_ITEM_BUTTON,
            Self::Checkbox => DIALOG_ITEM_CHECKBOX,
            Self::RadioButton => DIALOG_ITEM_RADIO,
            Self::ResourceControl => DIALOG_ITEM_RESOURCE_CONTROL,
            Self::StaticText => DIALOG_ITEM_STATIC_TEXT,
            Self::EditText => DIALOG_ITEM_EDIT_TEXT,
            Self::Icon => DIALOG_ITEM_ICON,
            Self::Picture => DIALOG_ITEM_PICTURE,
            Self::Unknown(raw) => raw,
        }
    }

    /// Whether this item represents a clickable button or standard control.
    pub const fn is_control(self) -> bool {
        matches!(
            self,
            Self::Button | Self::Checkbox | Self::RadioButton | Self::ResourceControl
        )
    }

    /// Whether this item represents an application user item (`userItem`, 0).
    pub const fn is_user_item(self) -> bool {
        matches!(self, Self::UserItem)
    }

    /// Whether this item represents a standard pushbutton (`ctrlItem + btnCtrl`, 4).
    pub const fn is_button(self) -> bool {
        matches!(self, Self::Button)
    }

    /// Whether this item represents a checkbox control (`ctrlItem + chkCtrl`, 5).
    pub const fn is_checkbox(self) -> bool {
        matches!(self, Self::Checkbox)
    }

    /// Whether this item represents a radio button control (`ctrlItem + radCtrl`, 6).
    pub const fn is_radio(self) -> bool {
        matches!(self, Self::RadioButton)
    }

    /// Whether this item represents a resource-defined control (`ctrlItem + resCtrl`, 7).
    pub const fn is_resource_control(self) -> bool {
        matches!(self, Self::ResourceControl)
    }

    /// Whether this item represents non-editable static text (`statText`, 8).
    pub const fn is_static_text(self) -> bool {
        matches!(self, Self::StaticText)
    }

    /// Whether this item represents editable text (`editText`, 16).
    pub const fn is_edit_text(self) -> bool {
        matches!(self, Self::EditText)
    }

    /// Whether this item represents a standard icon (`iconItem`, 32).
    pub const fn is_icon(self) -> bool {
        matches!(self, Self::Icon)
    }

    /// Whether this item represents a QuickDraw picture (`picItem`, 64).
    pub const fn is_picture(self) -> bool {
        matches!(self, Self::Picture)
    }

    /// Whether this item contains text payload (static or editable).
    pub const fn is_text(self) -> bool {
        matches!(self, Self::StaticText | Self::EditText)
    }

    /// Whether this item represents static or editable text, or a control with a title string.
    pub const fn has_text_or_title(self) -> bool {
        matches!(
            self,
            Self::StaticText
                | Self::EditText
                | Self::Button
                | Self::Checkbox
                | Self::RadioButton
        )
    }

    /// Whether this item references an external resource (resource control, icon, or picture).
    pub const fn is_resource(self) -> bool {
        matches!(self, Self::ResourceControl | Self::Icon | Self::Picture)
    }

    /// The 4-byte Mac OS resource type associated with this dialog item kind (`b"CNTL"`, `b"ICON"`, or `b"PICT"`).
    pub const fn resource_type(self) -> Option<[u8; 4]> {
        match self {
            Self::ResourceControl => Some(*b"CNTL"),
            Self::Icon => Some(*b"ICON"),
            Self::Picture => Some(*b"PICT"),
            _ => None,
        }
    }

    /// The 4-byte Mac OS resource type associated with this dialog item kind as a big-endian `u32`.
    pub const fn resource_type_u32(self) -> Option<u32> {
        match self.resource_type() {
            Some(bytes) => Some(u32::from_be_bytes(bytes)),
            None => None,
        }
    }
}

/// Parsed Dialog Item List (DITL) entry.
///
/// Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 6-120--6-121.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogItemRecord {
    /// Byte offset of the item entry within the source DITL data.
    pub item_offset: usize,
    /// Raw item type byte (including `DIALOG_ITEM_DISABLED_FLAG`).
    pub item_type: u8,
    /// Display rectangle in dialog-local coordinates (top, left, bottom, right).
    pub rect: (i16, i16, i16, i16),
    /// Handle or procedure pointer from the item header (0 in resource data).
    pub handle: u32,
    /// Raw item payload bytes (Pascal string, resource ID, or application data).
    pub payload: Vec<u8>,
}

#[allow(dead_code)]
impl DialogItemRecord {
    /// Whether the item is enabled for user interaction.
    pub fn is_enabled(&self) -> bool {
        is_dialog_item_enabled(self.item_type)
    }

    /// Whether the item is disabled for user interaction.
    pub fn is_disabled(&self) -> bool {
        is_dialog_item_disabled(self.item_type)
    }

    /// Strongly typed item kind.
    pub fn kind(&self) -> DialogItemKind {
        DialogItemKind::from_raw_type(self.item_type)
    }

    /// Whether the item represents a button, checkbox, or radio control.
    pub fn is_control(&self) -> bool {
        is_dialog_item_control(self.item_type)
    }

    /// Whether the item represents an application user item (`userItem`, 0).
    pub fn is_user_item(&self) -> bool {
        is_dialog_item_user_item(self.item_type)
    }

    /// Whether the item represents an application user item with an installed procedure pointer or handle.
    pub fn has_user_proc(&self) -> bool {
        self.is_user_item() && self.handle != 0
    }

    /// Whether the item represents a standard pushbutton (`ctrlItem + btnCtrl`, 4).
    pub fn is_button(&self) -> bool {
        is_dialog_item_button(self.item_type)
    }

    /// Whether the item represents a checkbox control (`ctrlItem + chkCtrl`, 5).
    pub fn is_checkbox(&self) -> bool {
        is_dialog_item_checkbox(self.item_type)
    }

    /// Whether the item represents a radio button control (`ctrlItem + radCtrl`, 6).
    pub fn is_radio(&self) -> bool {
        is_dialog_item_radio(self.item_type)
    }

    /// Whether the item represents a resource-defined control (`ctrlItem + resCtrl`, 7).
    pub fn is_resource_control(&self) -> bool {
        is_dialog_item_resource_control(self.item_type)
    }

    /// Whether the item represents non-editable static text (`statText`, 8).
    pub fn is_static_text(&self) -> bool {
        is_dialog_item_static_text(self.item_type)
    }

    /// Whether the item represents editable text (`editText`, 16).
    pub fn is_edit_text(&self) -> bool {
        is_dialog_item_edit_text(self.item_type)
    }

    /// Whether the item represents a standard icon (`iconItem`, 32).
    pub fn is_icon(&self) -> bool {
        is_dialog_item_icon(self.item_type)
    }

    /// Whether the item represents a QuickDraw picture (`picItem`, 64).
    pub fn is_picture(&self) -> bool {
        is_dialog_item_picture(self.item_type)
    }

    /// Whether the item represents static or editable text.
    pub fn is_text(&self) -> bool {
        is_dialog_item_text(self.item_type)
    }

    /// Whether the item represents text or a control with a title string.
    pub fn has_text_or_title(&self) -> bool {
        is_dialog_item_text_or_title(self.item_type)
    }

    /// Returns the raw payload bytes if the item represents text or a title string.
    pub fn text_payload(&self) -> Option<&[u8]> {
        if self.has_text_or_title() {
            Some(&self.payload)
        } else {
            None
        }
    }

    /// Decoded Mac Roman text if the payload represents a text or title string; otherwise empty.
    pub fn text(&self) -> String {
        self.text_payload()
            .map(decode_mac_roman)
            .unwrap_or_default()
    }

    /// Sets the decoded text on this item if it represents text or a titled control.
    pub fn set_text(&mut self, text: &str) {
        if self.has_text_or_title() {
            self.payload = encode_mac_roman_lossy(text);
        }
    }

    /// Whether the item references an external resource (resource control, icon, or picture).
    pub fn is_resource(&self) -> bool {
        is_dialog_item_resource(self.item_type)
    }

    /// 4-byte Mac OS resource type if the item references a resource (`b"CNTL"`, `b"ICON"`, or `b"PICT"`).
    pub fn resource_type(&self) -> Option<[u8; 4]> {
        dialog_item_resource_type(self.item_type)
    }

    /// 4-byte Mac OS resource type as `u32` if the item references a resource.
    pub fn resource_type_u32(&self) -> Option<u32> {
        dialog_item_resource_type_u32(self.item_type)
    }

    /// 16-bit resource ID if the item references a resource (CNTL, ICON, PICT).
    pub fn resource_id(&self) -> Option<i16> {
        if self.is_resource() && self.payload.len() >= 2 {
            Some(i16::from_be_bytes([self.payload[0], self.payload[1]]))
        } else {
            None
        }
    }

    /// Whether the item rectangle is moved off-screen via `HideDialogItem`.
    pub fn is_hidden(&self) -> bool {
        is_dialog_item_rect_hidden(self.rect)
    }

    /// Offset the item display rectangle by `(v_delta, h_delta)`.
    pub fn offset_rect(&mut self, v_delta: i16, h_delta: i16) {
        self.rect = offset_rect(self.rect, v_delta, h_delta);
    }

    /// Calculate the enclosing rectangle for invalidation and redrawing.
    pub fn enclosing_rect(&self) -> (i16, i16, i16, i16) {
        dialog_item_enclosing_rect(self.item_type, self.rect)
    }

    /// Evaluates hiding this item, returning the visibility change if it is currently visible.
    pub fn evaluate_hide(&self) -> Option<DialogItemVisibilityChange> {
        evaluate_hide_dialog_item(self.item_type, self.rect)
    }

    /// Evaluates restoring this item, returning the visibility change if it is currently hidden.
    pub fn evaluate_show(
        &self,
        original_rect: Option<(i16, i16, i16, i16)>,
    ) -> Option<DialogItemVisibilityChange> {
        evaluate_show_dialog_item(self.item_type, self.rect, original_rect)
    }

    /// Extracts the item's header representation.
    pub fn header(&self) -> DialogItemHeader {
        DialogItemHeader {
            item_type: u16::from(self.item_type),
            handle: self.handle,
            rect: self.rect,
        }
    }

    /// Updates the header fields of this dialog item record in place.
    pub fn update_header(&mut self, item_type: u8, handle: u32, rect: (i16, i16, i16, i16)) {
        self.item_type = item_type;
        self.handle = handle;
        self.rect = rect;
    }

    /// Evaluates or computes the normalized selection range `(start, end)` for this edit text item.
    ///
    /// Returns `None` if this item is not an `editText` item.
    pub fn select_text(&self, start_sel: i16, end_sel: i16) -> Option<(u16, u16)> {
        if !self.is_edit_text() {
            return None;
        }
        let text_len = self.text_payload().map(|p| p.len()).unwrap_or(0);
        Some(normalize_dialog_item_selection(start_sel, end_sel, text_len))
    }

    /// Evaluates or computes the normalized selection range `(start, end)` for this edit text item.
    ///
    /// Returns `None` if this item is not an `editText` item.
    pub fn evaluate_select_text(&self, start_sel: i16, end_sel: i16) -> Option<(u16, u16)> {
        self.select_text(start_sel, end_sel)
    }
}

#[inline]
fn read_be_i16(bytes: &[u8], offset: usize) -> Option<i16> {
    let slice = bytes.get(offset..offset.checked_add(2)?)?;
    Some(i16::from_be_bytes([slice[0], slice[1]]))
}

#[inline]
fn read_be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Calculate the payload length for a DITL item.
///
/// Inside Macintosh Volume I, pp. I-404--I-405, I-427;
/// Macintosh Toolbox Essentials (1992), p. 6-153.
pub fn ditl_item_payload_len(base_type: u8, data_len_byte: u8, remaining: usize) -> Option<usize> {
    if is_dialog_item_resource(base_type) {
        // icon (32), picture (64), resCtrl (7): 2-byte resource ID.
        // IM:I I-427 describes the byte after itmtype as length=2; MTE 1992
        // p. 6-153 documents the same compiled records as a reserved byte plus
        // the two-byte resource ID. Accept both conventions.
        if remaining < 2 {
            return None;
        }
        if data_len_byte >= 2 {
            Some(usize::from(data_len_byte))
        } else {
            Some(2)
        }
    } else {
        Some(usize::from(data_len_byte))
    }
}

/// Calculate the total byte length of a compiled DITL item record at `record_offset`,
/// including its 14-byte header and padded payload.
pub fn ditl_item_record_len_at(bytes: &[u8], record_offset: usize) -> Option<usize> {
    if record_offset.checked_add(14)? > bytes.len() {
        return None;
    }
    let item_type = *bytes.get(record_offset.checked_add(12)?)?;
    let data_len_byte = *bytes.get(record_offset.checked_add(13)?)?;
    let base_type = dialog_item_base_type(item_type);
    let payload_offset = record_offset.checked_add(14)?;
    let remaining = bytes.len().saturating_sub(payload_offset);
    let payload_len = ditl_item_payload_len(base_type, data_len_byte, remaining)?;
    let padded = (payload_len + 1) & !1;
    if padded > remaining {
        return None;
    }
    Some(14 + padded)
}

/// Calculate the total used bytes of compiled DITL data up to the last item.
pub fn ditl_total_used_len(bytes: &[u8]) -> usize {
    if bytes.len() < 2 {
        return bytes.len();
    }
    let Some(count_minus_one) = read_be_i16(bytes, 0) else {
        return bytes.len();
    };
    let count = if count_minus_one < 0 {
        0
    } else {
        match usize::try_from(count_minus_one)
            .ok()
            .and_then(|c| c.checked_add(1))
        {
            Some(c) => c,
            None => return bytes.len(),
        }
    };
    let mut offset = 2usize;
    for _ in 0..count {
        let Some(record_len) = ditl_item_record_len_at(bytes, offset) else {
            break;
        };
        offset = offset.saturating_add(record_len);
    }
    offset
}

/// Parse raw DITL resource bytes into a list of parsed dialog item records.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-120--6-121.
pub fn parse_ditl_items(bytes: &[u8]) -> Option<Vec<DialogItemRecord>> {
    let count_minus_one = read_be_i16(bytes, 0)?;
    let count = if count_minus_one < 0 {
        0
    } else {
        match usize::try_from(count_minus_one)
            .ok()
            .and_then(|c| c.checked_add(1))
        {
            Some(c) => c,
            None => return None,
        }
    };

    let mut offset = 2usize;
    let mut items = Vec::with_capacity(count);

    for _ in 0..count {
        if offset + 14 > bytes.len() {
            break;
        }
        let Some(handle) = read_be_u32(bytes, offset) else {
            break;
        };
        let Some(top) = read_be_i16(bytes, offset + 4) else {
            break;
        };
        let Some(left) = read_be_i16(bytes, offset + 6) else {
            break;
        };
        let Some(bottom) = read_be_i16(bytes, offset + 8) else {
            break;
        };
        let Some(right) = read_be_i16(bytes, offset + 10) else {
            break;
        };
        let item_type = bytes[offset + 12];
        let data_len_byte = bytes[offset + 13];
        let base_type = dialog_item_base_type(item_type);
        let payload_start = offset + 14;
        let remaining = bytes.len().saturating_sub(payload_start);
        let Some(payload_len) = ditl_item_payload_len(base_type, data_len_byte, remaining) else {
            break;
        };
        let padded = (payload_len + 1) & !1;
        if padded > remaining {
            break;
        }
        let payload = bytes[payload_start..payload_start + payload_len].to_vec();

        items.push(DialogItemRecord {
            item_offset: offset,
            item_type,
            rect: (top, left, bottom, right),
            handle,
            payload,
        });

        offset = payload_start + padded;
    }

    Some(items)
}

/// Map a legacy System 7 dialog positioning code to a canonical `WindowPositionMethod`.
pub const fn dialog_position_code_to_method(position: u16) -> Option<u16> {
    match position {
        DIALOG_POSITION_CENTER_MAIN_SCREEN => Some(crate::window_manager::WINDOW_CENTER_ON_MAIN_SCREEN),
        DIALOG_POSITION_ALERT_MAIN_SCREEN => Some(crate::window_manager::WINDOW_ALERT_POSITION_ON_MAIN_SCREEN),
        DIALOG_POSITION_STAGGER_MAIN_SCREEN => Some(crate::window_manager::WINDOW_STAGGER_ON_MAIN_SCREEN),
        DIALOG_POSITION_CENTER_PARENT_WINDOW => Some(crate::window_manager::WINDOW_CENTER_ON_PARENT_WINDOW),
        DIALOG_POSITION_ALERT_PARENT_WINDOW => Some(crate::window_manager::WINDOW_ALERT_POSITION_ON_PARENT_WINDOW),
        DIALOG_POSITION_STAGGER_PARENT_WINDOW => Some(crate::window_manager::WINDOW_STAGGER_ON_PARENT_WINDOW),
        DIALOG_POSITION_CENTER_PARENT_WINDOW_SCREEN => Some(crate::window_manager::WINDOW_CENTER_ON_PARENT_WINDOW_SCREEN),
        DIALOG_POSITION_ALERT_PARENT_WINDOW_SCREEN => Some(crate::window_manager::WINDOW_ALERT_POSITION_ON_PARENT_WINDOW_SCREEN),
        DIALOG_POSITION_STAGGER_PARENT_WINDOW_SCREEN => Some(crate::window_manager::WINDOW_STAGGER_ON_PARENT_WINDOW_SCREEN),
        _ => None,
    }
}

/// Normalize either a System 7 positioning code (`0x280A`...) or Carbon `WindowPositionMethod` (1..=9).
pub const fn normalize_dialog_position_method(position: u16) -> Option<u16> {
    if position >= 1 && position <= 9 {
        Some(position)
    } else {
        dialog_position_code_to_method(position)
    }
}

/// Architecture-neutral dialog positioning bounds evaluation.
///
/// Supports all 9 canonical placement modes:
/// - Center on main screen
/// - Alert position on main screen (1/5th from top)
/// - Stagger on main screen
/// - Center on parent window
/// - Alert position on parent window
/// - Stagger on parent window
/// - Center on parent window's screen
/// - Alert position on parent window's screen
/// - Stagger on parent window's screen
pub fn evaluate_dialog_position_bounds(
    content_bounds: (i16, i16, i16, i16),
    position: u16,
    structure_bounds: Option<(i16, i16, i16, i16)>,
    parent_structure_bounds: Option<(i16, i16, i16, i16)>,
    screen_width: i32,
    screen_height: i32,
    menu_bar_height: i32,
) -> (i16, i16, i16, i16) {
    let Some(method) = normalize_dialog_position_method(position) else {
        return content_bounds;
    };
    let structure = structure_bounds.unwrap_or(content_bounds);
    crate::window_manager::evaluate_reposition_window_bounds(
        content_bounds,
        structure,
        parent_structure_bounds,
        method,
        screen_width,
        screen_height,
        menu_bar_height,
    )
    .unwrap_or(content_bounds)
}

/// Adjust dialog window bounds according to System 7 AutoPositioning flags.
#[allow(dead_code)]
pub fn position_dialog_bounds(
    bounds: (i16, i16, i16, i16),
    position: u16,
    screen_width: i32,
    screen_height: i32,
) -> (i16, i16, i16, i16) {
    evaluate_dialog_position_bounds(
        bounds,
        position,
        None,
        None,
        screen_width,
        screen_height,
        0,
    )
}

/// Standard coordinate offset applied to move a dialog item off-screen when hidden.
///
/// Inside Macintosh Volume IV, p. IV-59;
/// Macintosh Toolbox Essentials (1992), pp. 6-123--6-124.
pub const DIALOG_ITEM_HIDDEN_OFFSET: i16 = 16384;

/// Horizontal threshold coordinate distinguishing on-screen dialog items from hidden ones.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-123--6-124.
pub const DIALOG_ITEM_HIDDEN_THRESHOLD: i16 = 8192;

/// Offset a rectangle by `(v_delta, h_delta)` using saturating arithmetic.
pub fn offset_rect(rect: (i16, i16, i16, i16), v_delta: i16, h_delta: i16) -> (i16, i16, i16, i16) {
    (
        rect.0.saturating_add(v_delta),
        rect.1.saturating_add(h_delta),
        rect.2.saturating_add(v_delta),
        rect.3.saturating_add(h_delta),
    )
}

/// Convert a dialog-local rectangle into global/screen coordinates based on dialog bounds.
pub fn dialog_rect_to_global(
    bounds: (i16, i16, i16, i16),
    rect: (i16, i16, i16, i16),
) -> (i16, i16, i16, i16) {
    (
        bounds.0.saturating_add(rect.0),
        bounds.1.saturating_add(rect.1),
        bounds.0.saturating_add(rect.2),
        bounds.1.saturating_add(rect.3),
    )
}

/// Convert a global/screen rectangle into dialog-local coordinates based on dialog bounds.
#[allow(dead_code)]
pub fn dialog_rect_to_local(
    bounds: (i16, i16, i16, i16),
    rect: (i16, i16, i16, i16),
) -> (i16, i16, i16, i16) {
    (
        rect.0.saturating_sub(bounds.0),
        rect.1.saturating_sub(bounds.1),
        rect.2.saturating_sub(bounds.0),
        rect.3.saturating_sub(bounds.1),
    )
}

/// Convert a global/screen point into dialog-local coordinates based on dialog bounds.
#[inline]
pub const fn global_to_dialog_local_point(
    bounds: (i16, i16, i16, i16),
    screen_v: i16,
    screen_h: i16,
) -> (i16, i16) {
    (
        screen_v.saturating_sub(bounds.0),
        screen_h.saturating_sub(bounds.1),
    )
}

/// Convert a dialog-local point into global/screen coordinates based on dialog bounds.
#[allow(dead_code)]
#[inline]
pub const fn dialog_local_to_global_point(
    bounds: (i16, i16, i16, i16),
    local_v: i16,
    local_h: i16,
) -> (i16, i16) {
    (
        bounds.0.saturating_add(local_v),
        bounds.1.saturating_add(local_h),
    )
}

/// Whether a rectangle contains a point `(v, h)` (with half-open interval `top <= v < bottom` and `left <= h < right`).
pub fn rect_contains_point(rect: (i16, i16, i16, i16), v: i16, h: i16) -> bool {
    v >= rect.0 && v < rect.2 && h >= rect.1 && h < rect.3
}

/// Inset a rectangle by `(v_delta, h_delta)` using saturating arithmetic.
#[allow(dead_code)]
pub fn inset_rect(rect: (i16, i16, i16, i16), v_delta: i16, h_delta: i16) -> (i16, i16, i16, i16) {
    (
        rect.0.saturating_add(v_delta),
        rect.1.saturating_add(h_delta),
        rect.2.saturating_sub(v_delta),
        rect.3.saturating_sub(h_delta),
    )
}

/// Outset a rectangle by `(v_delta, h_delta)` using saturating arithmetic.
pub fn outset_rect(rect: (i16, i16, i16, i16), v_delta: i16, h_delta: i16) -> (i16, i16, i16, i16) {
    (
        rect.0.saturating_sub(v_delta),
        rect.1.saturating_sub(h_delta),
        rect.2.saturating_add(v_delta),
        rect.3.saturating_add(h_delta),
    )
}

/// Calculate the outer boundary rectangle for a modal dialog box (`dBoxProc`)
/// structure frame including its standard 8-pixel margin.
///
/// Inside Macintosh Volume I, p. I-273.
pub fn dialog_dbox_frame_rect(content_bounds: (i16, i16, i16, i16)) -> (i16, i16, i16, i16) {
    outset_rect(
        content_bounds,
        DIALOG_DBOX_FRAME_MARGIN,
        DIALOG_DBOX_FRAME_MARGIN,
    )
}

/// Calculate the outer border frame rectangle for an edit-text dialog item.
///
/// Inside Macintosh Volume I, p. I-414;
/// Macintosh Human Interface Guidelines (1992), p. 184.
pub fn edit_text_frame_rect(item_rect: (i16, i16, i16, i16)) -> (i16, i16, i16, i16) {
    outset_rect(item_rect, EDIT_TEXT_FRAME_OUTSET, EDIT_TEXT_FRAME_OUTSET)
}

/// Calculate the text rendering rectangle for a static or editable text dialog item,
/// applying the standard 1-pixel flush-left glyph inset.
///
/// Inside Macintosh Volume I, pp. I-373--I-374.
pub fn dialog_text_rect(item_rect: (i16, i16, i16, i16)) -> (i16, i16, i16, i16) {
    (
        item_rect.0,
        item_rect.1.saturating_add(DIALOG_TEXT_LEFT_INSET),
        item_rect.2,
        item_rect.3,
    )
}

/// Extract the base dialog item type code, stripping the disabled bit flag (`0x80`).
pub const fn dialog_item_base_type(raw_type: u8) -> u8 {
    raw_type & !DIALOG_ITEM_DISABLED_FLAG
}

/// Returns whether the dialog item has the disabled bit flag (`0x80`) set.
///
/// Inside Macintosh Volume I, p. I-427;
/// Macintosh Toolbox Essentials (1992), p. 6-152.
#[inline]
pub const fn is_dialog_item_disabled(raw_type: u8) -> bool {
    (raw_type & DIALOG_ITEM_DISABLED_FLAG) != 0
}

/// Returns whether the dialog item is enabled (disabled bit flag `0x80` not set).
///
/// Inside Macintosh Volume I, p. I-427;
/// Macintosh Toolbox Essentials (1992), p. 6-152.
#[inline]
pub const fn is_dialog_item_enabled(raw_type: u8) -> bool {
    (raw_type & DIALOG_ITEM_DISABLED_FLAG) == 0
}

/// Resolve the target dialog pointer for a Toolbox event.
///
/// Inside Macintosh Volume I, pp. I-416--I-417:
/// Update and activate events name their target window in `message`; if that
/// window is a dialog box, the event routes to it (and if not, no dialog is targeted).
/// All other events are routed to the frontmost active dialog.
pub fn dialog_target_for_event<F>(
    what: u16,
    message: u32,
    is_dialog: F,
    front_dialog: Option<u32>,
) -> Option<u32>
where
    F: FnOnce(u32) -> bool,
{
    match what {
        EVENT_UPDATE | EVENT_ACTIVATE => {
            if is_dialog(message) {
                Some(message)
            } else {
                None
            }
        }
        _ => front_dialog,
    }
}

/// Determine whether a dialog item rectangle is hidden off-screen.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-123--6-124.
pub fn is_dialog_item_rect_hidden(rect: (i16, i16, i16, i16)) -> bool {
    rect.1 > DIALOG_ITEM_HIDDEN_THRESHOLD
}

/// Calculate the hidden offscreen rectangle for a dialog item (+16384 on left and right).
pub fn hide_dialog_item_rect(rect: (i16, i16, i16, i16)) -> (i16, i16, i16, i16) {
    (
        rect.0,
        rect.1.wrapping_add(DIALOG_ITEM_HIDDEN_OFFSET),
        rect.2,
        rect.3.wrapping_add(DIALOG_ITEM_HIDDEN_OFFSET),
    )
}

/// Calculate the restored onscreen rectangle for a hidden dialog item (-16384 on left and right).
pub fn show_dialog_item_rect(rect: (i16, i16, i16, i16)) -> (i16, i16, i16, i16) {
    (
        rect.0,
        rect.1.wrapping_sub(DIALOG_ITEM_HIDDEN_OFFSET),
        rect.2,
        rect.3.wrapping_sub(DIALOG_ITEM_HIDDEN_OFFSET),
    )
}

/// Calculate the enclosing rectangle for a dialog item for erasure and invalidation.
///
/// Inside Macintosh Volume IV, p. IV-59 notes that Dialog Manager drawing can extend
/// outside an editText item's display rectangle by 3 pixels.
pub fn dialog_item_enclosing_rect(
    item_type: u8,
    rect: (i16, i16, i16, i16),
) -> (i16, i16, i16, i16) {
    if is_dialog_item_edit_text(item_type) {
        (
            rect.0.saturating_sub(3),
            rect.1.saturating_sub(3),
            rect.2.saturating_add(3),
            rect.3.saturating_add(3),
        )
    } else {
        rect
    }
}

/// The resulting geometry changes when a dialog item's visibility state transitions
/// via `HideDialogItem` or `ShowDialogItem`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialogItemVisibilityChange {
    /// The updated display rectangle for the item (`item.rect`).
    pub new_rect: (i16, i16, i16, i16),
    /// The local rectangle requiring invalidation and/or background erasure.
    pub enclosing_rect: (i16, i16, i16, i16),
}

#[allow(dead_code)]
impl DialogItemVisibilityChange {
    /// The updated display rectangle for the item (`item.rect`).
    #[inline]
    pub const fn new_rect(&self) -> (i16, i16, i16, i16) {
        self.new_rect
    }

    /// The local rectangle requiring invalidation and/or background erasure.
    #[inline]
    pub const fn enclosing_rect(&self) -> (i16, i16, i16, i16) {
        self.enclosing_rect
    }
}

/// Canonical evaluated parameters for `HideDialogItem` and `ShowDialogItem`.
///
/// Inside Macintosh Volume IV, p. IV-59;
/// Macintosh Toolbox Essentials (1992), pp. 6-123--6-124.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialogItemVisibilityParameters {
    dialog_ptr: u32,
    item_number: usize,
}

#[allow(dead_code)]
impl DialogItemVisibilityParameters {
    /// Constructs a new `DialogItemVisibilityParameters`.
    #[inline]
    pub const fn new(dialog_ptr: u32, item_number: usize) -> Self {
        Self {
            dialog_ptr,
            item_number,
        }
    }

    /// The target dialog pointer.
    #[inline]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// The 1-based dialog item index.
    #[inline]
    pub const fn item_number(&self) -> usize {
        self.item_number
    }

    /// The 1-based dialog item index as signed 16-bit integer.
    #[inline]
    pub const fn item_no(&self) -> i16 {
        self.item_number as i16
    }

    /// The 0-based dialog item index, or `None` if item number is 0.
    #[inline]
    pub const fn item_index(&self) -> Option<usize> {
        self.item_number.checked_sub(1)
    }
}

/// Evaluates and validates input parameters for `HideDialogItem` and `ShowDialogItem`.
///
/// Returns `None` if `dialog_ptr == 0` or `item_number == 0`.
#[inline]
pub const fn evaluate_dialog_item_visibility_parameters(
    dialog_ptr: u32,
    item_number: usize,
) -> Option<DialogItemVisibilityParameters> {
    if dialog_ptr == 0 || item_number == 0 {
        return None;
    }
    Some(DialogItemVisibilityParameters {
        dialog_ptr,
        item_number,
    })
}

/// Evaluates and validates input parameters for `HideDialogItem` and `ShowDialogItem`
/// from signed 16-bit item number.
///
/// Returns `None` if `dialog_ptr == 0` or `item_no <= 0`.
#[inline]
pub const fn evaluate_dialog_item_visibility_parameters_signed(
    dialog_ptr: u32,
    item_no: i16,
) -> Option<DialogItemVisibilityParameters> {
    if item_no <= 0 {
        return None;
    }
    evaluate_dialog_item_visibility_parameters(dialog_ptr, item_no as usize)
}

/// Evaluates whether a dialog item should be hidden, and calculates its new offscreen
/// display rectangle and local invalidation/erasure rectangle.
///
/// Macintosh Toolbox Essentials (1992), p. 6-123:
/// If the item's display rectangle is already offscreen (`left > 8192`), returns `None`.
/// Otherwise, offsets the left and right coordinates by `+16384` and returns the
/// new display rectangle and the enclosing rectangle of the original visible item.
pub fn evaluate_hide_dialog_item(
    item_type: u8,
    current_rect: (i16, i16, i16, i16),
) -> Option<DialogItemVisibilityChange> {
    if is_dialog_item_rect_hidden(current_rect) {
        None
    } else {
        Some(DialogItemVisibilityChange {
            new_rect: hide_dialog_item_rect(current_rect),
            enclosing_rect: dialog_item_enclosing_rect(item_type, current_rect),
        })
    }
}

/// Evaluates whether a hidden dialog item should be made visible again, and calculates
/// its restored onscreen display rectangle and local invalidation rectangle.
///
/// Macintosh Toolbox Essentials (1992), p. 6-124:
/// If the item's display rectangle is already visible (`left <= 8192`), returns `None`.
/// Otherwise, restores the original rectangle (using `original_rect` if provided and visible,
/// or subtracting 16384 from left and right) and returns the restored display rectangle
/// and its enclosing rectangle.
pub fn evaluate_show_dialog_item(
    item_type: u8,
    current_rect: (i16, i16, i16, i16),
    original_rect: Option<(i16, i16, i16, i16)>,
) -> Option<DialogItemVisibilityChange> {
    if !is_dialog_item_rect_hidden(current_rect) {
        None
    } else {
        let restored_rect = match original_rect {
            Some(orig) if !is_dialog_item_rect_hidden(orig) => orig,
            _ => show_dialog_item_rect(current_rect),
        };
        Some(DialogItemVisibilityChange {
            new_rect: restored_rect,
            enclosing_rect: dialog_item_enclosing_rect(item_type, restored_rect),
        })
    }
}

/// Find the 0-based index of the first item whose rectangle contains the dialog-local point.
///
/// Inside Macintosh Volume IV, p. IV-60;
/// Macintosh Toolbox Essentials (1992), p. 6-125.
#[allow(dead_code)]
pub fn find_dialog_item_at_local_point<'a>(
    rects: impl IntoIterator<Item = &'a (i16, i16, i16, i16)>,
    pt_v: i16,
    pt_h: i16,
) -> Option<usize> {
    for (idx, &rect) in rects.into_iter().enumerate() {
        if rect_contains_point(rect, pt_v, pt_h) {
            return Some(idx);
        }
    }
    None
}

/// Evaluates a `FindDItem` / `FindDialogItem` query against dialog items, returning the 0-indexed item number
/// of the first matching item, or -1 if no item contains the point.
///
/// Inside Macintosh Volume IV, p. IV-60 and Macintosh Toolbox Essentials (1992), p. 6-125:
/// Returns the 0-indexed item number of the first item (whether enabled or disabled) containing
/// the point (in dialog-local coordinates), or -1 if none match. Hidden items (left > 8192)
/// naturally fail the hit test.
/// For control items, `control_part_hit` is called to allow transparent group box bodies
/// or inactive controls to fall through to enclosed items.
pub fn evaluate_find_dialog_item<I, F>(
    items: I,
    local_v: i16,
    local_h: i16,
    control_part_hit: F,
) -> i16
where
    I: IntoIterator<Item = ((i16, i16, i16, i16), u8)>,
    F: FnMut(usize) -> bool,
{
    find_dialog_item_hit(items, local_v, local_h, false, control_part_hit)
        .map_or(-1, |idx| idx as i16)
}

/// Evaluates a `FindDItem` / `FindDialogItem` query against simple bounding rectangles,
/// returning the 0-indexed item number or -1 if no item contains the point.
#[allow(dead_code)]
pub fn evaluate_find_dialog_item_rects<'a, I>(rects: I, local_v: i16, local_h: i16) -> i16
where
    I: IntoIterator<Item = &'a (i16, i16, i16, i16)>,
{
    find_dialog_item_at_local_point(rects, local_v, local_h).map_or(-1, |idx| idx as i16)
}

/// The evaluated input parameters for a `FindDItem` ($A984) / CFM `FindDialogItem` call.
///
/// Inside Macintosh Volume IV, p. IV-60 and Macintosh Toolbox Essentials (1992), p. 6-125:
/// `FUNCTION FindDItem(theDialog: DialogPtr; thePt: Point): INTEGER;`
/// Takes a dialog pointer and dialog-local coordinates (`thePt`). If `theDialog` is NIL (0),
/// the parameters are invalid and evaluate to `None`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FindDialogItemParameters {
    dialog_ptr: u32,
    pt_v: i16,
    pt_h: i16,
}

impl FindDialogItemParameters {
    /// Constructs parameters from dialog pointer and vertical/horizontal local coordinates.
    #[inline]
    pub const fn new(dialog_ptr: u32, pt_v: i16, pt_h: i16) -> Self {
        Self {
            dialog_ptr,
            pt_v,
            pt_h,
        }
    }

    /// The target dialog pointer.
    #[inline]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// The vertical dialog-local coordinate.
    #[inline]
    pub const fn pt_v(&self) -> i16 {
        self.pt_v
    }

    /// The horizontal dialog-local coordinate.
    #[inline]
    pub const fn pt_h(&self) -> i16 {
        self.pt_h
    }

    /// The dialog-local point as `(v, h)`.
    #[allow(dead_code)]
    #[inline]
    pub const fn point(&self) -> (i16, i16) {
        (self.pt_v, self.pt_h)
    }

    /// The dialog-local point packed as `(v << 16) | (h & 0xffff)`.
    #[allow(dead_code)]
    #[inline]
    pub const fn packed_point(&self) -> u32 {
        ((self.pt_v as u16 as u32) << 16) | (self.pt_h as u16 as u32)
    }
}

/// Evaluates `FindDItem` / `FindDialogItem` parameters from dialog pointer and (v, h) coordinates.
/// Returns `None` if `dialog_ptr == 0`.
#[inline]
pub const fn evaluate_find_dialog_item_parameters(
    dialog_ptr: u32,
    pt_v: i16,
    pt_h: i16,
) -> Option<FindDialogItemParameters> {
    if dialog_ptr == 0 {
        None
    } else {
        Some(FindDialogItemParameters::new(dialog_ptr, pt_v, pt_h))
    }
}

/// Evaluates `FindDialogItem` parameters from dialog pointer and a 32-bit packed `Point` (`(v << 16) | h`).
/// Returns `None` if `dialog_ptr == 0`.
#[inline]
pub const fn evaluate_find_dialog_item_parameters_packed(
    dialog_ptr: u32,
    point: u32,
) -> Option<FindDialogItemParameters> {
    let v = (point >> 16) as u16 as i16;
    let h = point as u16 as i16;
    evaluate_find_dialog_item_parameters(dialog_ptr, v, h)
}

/// Header representation for a dialog item containing its type, handle/ProcPtr, and display rectangle.
///
/// Inside Macintosh Volume I, pp. I-421--I-423;
/// Macintosh Toolbox Essentials (1992), pp. 6-120--6-123.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DialogItemHeader {
    /// Item type word (including `DIALOG_ITEM_DISABLED_FLAG`).
    pub item_type: u16,
    /// Handle to control, text, icon, picture, or ProcPtr for user item.
    pub handle: u32,
    /// Bounding rectangle in dialog-local coordinates (top, left, bottom, right).
    pub rect: (i16, i16, i16, i16),
}

impl DialogItemHeader {
    /// Canonical zeroed item header returned when a requested item does not exist.
    pub const ZERO: Self = Self {
        item_type: 0,
        handle: 0,
        rect: (0, 0, 0, 0),
    };

    /// Construct a new `DialogItemHeader`.
    pub fn new(item_type: u16, handle: u32, rect: (i16, i16, i16, i16)) -> Self {
        Self {
            item_type,
            handle,
            rect,
        }
    }
}

/// Query the 1-indexed dialog item header from a slice of items.
///
/// Inside Macintosh Volume I, p. I-421;
/// Macintosh Toolbox Essentials (1992), pp. 6-120--6-121.
/// Returns the item's header, or `DialogItemHeader::ZERO` if the item number is 0 or out of bounds.
pub fn evaluate_get_dialog_item<T>(
    items: &[T],
    item_number: usize,
    header_extractor: impl FnOnce(&T) -> DialogItemHeader,
) -> DialogItemHeader {
    get_item_at_1_indexed(items, item_number).map_or(DialogItemHeader::ZERO, header_extractor)
}


/// Evaluated parameters for a `GetDialogItem` / `GetDItem` invocation.
///
/// Inside Macintosh Volume I, p. I-421; Macintosh Toolbox Essentials (1992), p. 6-120.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetDialogItemParameters {
    dialog_ptr: u32,
    item_number: usize,
    item_type_ptr: u32,
    item_handle_ptr: u32,
    item_rect_ptr: u32,
}

impl GetDialogItemParameters {
    /// Constructs a new `GetDialogItemParameters`.
    #[inline]
    #[must_use]
    pub const fn new(
        dialog_ptr: u32,
        item_number: usize,
        item_type_ptr: u32,
        item_handle_ptr: u32,
        item_rect_ptr: u32,
    ) -> Self {
        Self {
            dialog_ptr,
            item_number,
            item_type_ptr,
            item_handle_ptr,
            item_rect_ptr,
        }
    }

    /// The target dialog pointer.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// The 1-based dialog item index.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn item_number(&self) -> usize {
        self.item_number
    }

    /// The 1-based dialog item index as signed 16-bit integer.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn item_no(&self) -> i16 {
        self.item_number as i16
    }

    /// The 0-based dialog item index.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn item_index(&self) -> usize {
        self.item_number.saturating_sub(1)
    }

    /// Output pointer receiving the item type word.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn item_type_ptr(&self) -> u32 {
        self.item_type_ptr
    }

    /// Output pointer receiving the item handle or procedure pointer.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn item_handle_ptr(&self) -> u32 {
        self.item_handle_ptr
    }

    /// Output pointer receiving the item display rectangle.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn item_rect_ptr(&self) -> u32 {
        self.item_rect_ptr
    }

    /// Output pointer receiving the item type word (alias for `item_type_ptr`).
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn type_ptr(&self) -> u32 {
        self.item_type_ptr
    }

    /// Output pointer receiving the item handle or procedure pointer (alias for `item_handle_ptr`).
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn handle_ptr(&self) -> u32 {
        self.item_handle_ptr
    }

    /// Output pointer receiving the item display rectangle (alias for `item_rect_ptr`).
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn rect_ptr(&self) -> u32 {
        self.item_rect_ptr
    }

    /// Output pointer receiving the item display bounding box (alias for `item_rect_ptr`).
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn box_ptr(&self) -> u32 {
        self.item_rect_ptr
    }
}

/// Evaluates and validates input parameters for `GetDialogItem` / `GetDItem`.
///
/// Returns `None` if `dialog_ptr == 0` or `item_number == 0`, or if any non-null output pointer
/// fails its respective writability check (`can_write_type`, `can_write_handle`, `can_write_rect`).
///
/// Inside Macintosh Volume I, p. I-421; Macintosh Toolbox Essentials (1992), p. 6-120.
#[inline]
pub const fn evaluate_get_dialog_item_parameters(
    dialog_ptr: u32,
    item_number: usize,
    item_type_ptr: u32,
    item_handle_ptr: u32,
    item_rect_ptr: u32,
    can_write_type: bool,
    can_write_handle: bool,
    can_write_rect: bool,
) -> Option<GetDialogItemParameters> {
    if dialog_ptr == 0 || item_number == 0 {
        return None;
    }
    if (item_type_ptr != 0 && !can_write_type)
        || (item_handle_ptr != 0 && !can_write_handle)
        || (item_rect_ptr != 0 && !can_write_rect)
    {
        return None;
    }
    Some(GetDialogItemParameters::new(
        dialog_ptr,
        item_number,
        item_type_ptr,
        item_handle_ptr,
        item_rect_ptr,
    ))
}

/// Canonical evaluated parameters for `SetDialogItem` / `SetDItem`.
///
/// Inside Macintosh Volume I, p. I-421;
/// Macintosh Toolbox Essentials (1992), pp. 6-120--6-123.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetDialogItemParameters {
    dialog_ptr: u32,
    item_number: usize,
    item_type: u8,
    item_handle: u32,
    rect: (i16, i16, i16, i16),
}

#[allow(dead_code)]
impl SetDialogItemParameters {
    /// Constructs a new `SetDialogItemParameters` instance.
    #[inline]
    pub const fn new(
        dialog_ptr: u32,
        item_number: usize,
        item_type: u8,
        item_handle: u32,
        rect: (i16, i16, i16, i16),
    ) -> Self {
        Self {
            dialog_ptr,
            item_number,
            item_type,
            item_handle,
            rect,
        }
    }

    /// The target dialog pointer.
    #[inline]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// The 1-based dialog item index.
    #[inline]
    pub const fn item_number(&self) -> usize {
        self.item_number
    }

    /// The 1-based dialog item index as signed 16-bit integer.
    #[inline]
    pub const fn item_no(&self) -> i16 {
        self.item_number as i16
    }

    /// The item type byte.
    #[inline]
    pub const fn item_type(&self) -> u8 {
        self.item_type
    }

    /// The base item type (stripped of enabled/disabled flag).
    #[inline]
    pub const fn base_type(&self) -> u8 {
        dialog_item_base_type(self.item_type)
    }

    /// Whether the item is enabled.
    #[inline]
    pub const fn is_enabled(&self) -> bool {
        is_dialog_item_enabled(self.item_type)
    }

    /// The item handle or procedure pointer.
    #[inline]
    pub const fn item_handle(&self) -> u32 {
        self.item_handle
    }

    /// The item bounding rectangle `(top, left, bottom, right)`.
    #[inline]
    pub const fn rect(&self) -> (i16, i16, i16, i16) {
        self.rect
    }
}

/// Evaluates and validates input parameters for `SetDialogItem` / `SetDItem`.
///
/// Returns `None` if `dialog_ptr == 0` or `item_number == 0`.
#[inline]
pub const fn evaluate_set_dialog_item_parameters(
    dialog_ptr: u32,
    item_number: usize,
    item_type: u16,
    item_handle: u32,
    rect: (i16, i16, i16, i16),
) -> Option<SetDialogItemParameters> {
    if dialog_ptr == 0 || item_number == 0 {
        return None;
    }
    Some(SetDialogItemParameters {
        dialog_ptr,
        item_number,
        item_type: item_type as u8,
        item_handle,
        rect,
    })
}

/// Evaluates `GetDialogItemAsControl` for a dialog item.
///
/// Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 6-121.
/// If the item is a control (`is_dialog_item_control`) and has a non-zero control handle,
/// returns `Ok(handle)`. Otherwise, returns `Err(-50)` (`paramErr`).
pub fn evaluate_get_dialog_item_as_control(
    item_type: u8,
    handle: u32,
) -> Result<u32, i16> {
    if is_dialog_item_control(item_type) && handle != 0 {
        Ok(handle)
    } else {
        Err(DIALOG_PARAM_ERR)
    }
}

/// Evaluated input parameters for a `GetDialogItemAsControl` request.
///
/// Inside Macintosh: Appearance Manager (1997);
/// Universal Interfaces `Dialogs.h`.
/// `FUNCTION GetDialogItemAsControl(theDialog: DialogPtr; itemNo: SInt16; VAR outControl: ControlHandle): OSStatus;`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetDialogItemAsControlParameters {
    dialog_ptr: u32,
    item_number: usize,
    control_out: u32,
}

impl GetDialogItemAsControlParameters {
    /// Constructs a new `GetDialogItemAsControlParameters` instance.
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, item_number: usize, control_out: u32) -> Self {
        Self {
            dialog_ptr,
            item_number,
            control_out,
        }
    }

    /// Pointer to the dialog record.
    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// 1-based item number within the dialog item list.
    #[inline]
    #[must_use]
    pub const fn item_number(&self) -> usize {
        self.item_number
    }

    /// 1-based item number as an `i16`.
    #[inline]
    #[must_use]
    pub const fn item_no(&self) -> i16 {
        if self.item_number > i16::MAX as usize {
            i16::MAX
        } else {
            self.item_number as i16
        }
    }

    /// 0-based item index within the dialog item slice.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn item_index(&self) -> usize {
        self.item_number.saturating_sub(1)
    }

    /// Guest pointer to the output `ControlHandle` variable.
    #[inline]
    #[must_use]
    pub const fn control_out(&self) -> u32 {
        self.control_out
    }
}

/// Evaluates and validates input parameters for a `GetDialogItemAsControl` request.
///
/// Returns `Err(paramErr)` (-50) if `theDialog == 0`, `itemNo == 0`, `outControl == 0`,
/// or the output pointer cannot be written.
#[inline]
pub fn evaluate_get_dialog_item_as_control_parameters(
    dialog_ptr: u32,
    item_number: usize,
    control_out: u32,
    can_write: bool,
) -> Result<GetDialogItemAsControlParameters, i16> {
    if dialog_ptr == 0 || item_number == 0 || control_out == 0 || !can_write {
        Err(DIALOG_PARAM_ERR)
    } else {
        Ok(GetDialogItemAsControlParameters::new(
            dialog_ptr,
            item_number,
            control_out,
        ))
    }
}

/// Evaluated parameters for a `SetDialogDefaultItem` request.
///
/// Macintosh Toolbox Essentials (1992), p. 6-164:
/// `FUNCTION SetDialogDefaultItem(theDialog: DialogPtr; newItem: INTEGER): OSErr;`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetDialogDefaultItemParameters {
    dialog_ptr: u32,
    item_no: i16,
}

impl SetDialogDefaultItemParameters {
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, item_no: i16) -> Self {
        Self {
            dialog_ptr,
            item_no,
        }
    }

    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    #[inline]
    #[must_use]
    pub const fn item_no(&self) -> i16 {
        self.item_no
    }
}

/// Evaluated parameters for a `SetDialogCancelItem` request.
///
/// Macintosh Toolbox Essentials (1992), p. 6-165:
/// `FUNCTION SetDialogCancelItem(theDialog: DialogPtr; newItem: INTEGER): OSErr;`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetDialogCancelItemParameters {
    dialog_ptr: u32,
    item_no: i16,
}

impl SetDialogCancelItemParameters {
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, item_no: i16) -> Self {
        Self {
            dialog_ptr,
            item_no,
        }
    }

    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    #[inline]
    #[must_use]
    pub const fn item_no(&self) -> i16 {
        self.item_no
    }
}

/// Evaluates `SetDialogDefaultItem` parameters.
///
/// Macintosh Toolbox Essentials (1992), p. 6-164.
/// Returns `Ok(SetDialogDefaultItemParameters)` if `dialog_ptr != 0`, or `Err(DIALOG_PARAM_ERR)` otherwise.
pub fn evaluate_set_dialog_default_item_parameters(
    dialog_ptr: u32,
    new_item: i16,
) -> Result<SetDialogDefaultItemParameters, i16> {
    if dialog_ptr == 0 {
        Err(DIALOG_PARAM_ERR)
    } else {
        Ok(SetDialogDefaultItemParameters::new(dialog_ptr, new_item))
    }
}

/// Evaluates `SetDialogCancelItem` parameters.
///
/// Macintosh Toolbox Essentials (1992), p. 6-165.
/// Returns `Ok(SetDialogCancelItemParameters)` if `dialog_ptr != 0`, or `Err(DIALOG_PARAM_ERR)` otherwise.
pub fn evaluate_set_dialog_cancel_item_parameters(
    dialog_ptr: u32,
    new_item: i16,
) -> Result<SetDialogCancelItemParameters, i16> {
    if dialog_ptr == 0 {
        Err(DIALOG_PARAM_ERR)
    } else {
        Ok(SetDialogCancelItemParameters::new(dialog_ptr, new_item))
    }
}

/// Evaluated parameters for a `GetDialogDefaultItem` request.
///
/// Inside Macintosh: Appearance Manager (1997).
/// `pascal OSStatus GetDialogDefaultItem(DialogRef theDialog, DialogItemIndex *outDefaultItem);`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetDialogDefaultItemParameters {
    dialog_ptr: u32,
    out_default_item_ptr: u32,
}

impl GetDialogDefaultItemParameters {
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, out_default_item_ptr: u32) -> Self {
        Self {
            dialog_ptr,
            out_default_item_ptr,
        }
    }

    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    #[inline]
    #[must_use]
    pub const fn out_default_item_ptr(&self) -> u32 {
        self.out_default_item_ptr
    }
}

/// Evaluated parameters for a `GetDialogCancelItem` request.
///
/// Inside Macintosh: Appearance Manager (1997).
/// `pascal OSStatus GetDialogCancelItem(DialogRef theDialog, DialogItemIndex *outCancelItem);`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetDialogCancelItemParameters {
    dialog_ptr: u32,
    out_cancel_item_ptr: u32,
}

impl GetDialogCancelItemParameters {
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, out_cancel_item_ptr: u32) -> Self {
        Self {
            dialog_ptr,
            out_cancel_item_ptr,
        }
    }

    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    #[inline]
    #[must_use]
    pub const fn out_cancel_item_ptr(&self) -> u32 {
        self.out_cancel_item_ptr
    }
}

/// Evaluates `GetDialogDefaultItem` parameters.
///
/// Inside Macintosh: Appearance Manager (1997).
/// Returns `Ok(GetDialogDefaultItemParameters)` if `dialog_ptr != 0` and `can_write` is true (valid output pointer),
/// or `Err(DIALOG_PARAM_ERR)` otherwise.
pub fn evaluate_get_dialog_default_item_parameters(
    dialog_ptr: u32,
    out_default_item_ptr: u32,
    can_write: bool,
) -> Result<GetDialogDefaultItemParameters, i16> {
    if dialog_ptr == 0 || out_default_item_ptr == 0 || !can_write {
        Err(DIALOG_PARAM_ERR)
    } else {
        Ok(GetDialogDefaultItemParameters::new(
            dialog_ptr,
            out_default_item_ptr,
        ))
    }
}

/// Evaluates `GetDialogCancelItem` parameters.
///
/// Inside Macintosh: Appearance Manager (1997).
/// Returns `Ok(GetDialogCancelItemParameters)` if `dialog_ptr != 0` and `can_write` is true (valid output pointer),
/// or `Err(DIALOG_PARAM_ERR)` otherwise.
pub fn evaluate_get_dialog_cancel_item_parameters(
    dialog_ptr: u32,
    out_cancel_item_ptr: u32,
    can_write: bool,
) -> Result<GetDialogCancelItemParameters, i16> {
    if dialog_ptr == 0 || out_cancel_item_ptr == 0 || !can_write {
        Err(DIALOG_PARAM_ERR)
    } else {
        Ok(GetDialogCancelItemParameters::new(
            dialog_ptr,
            out_cancel_item_ptr,
        ))
    }
}

/// Canonical default item for a dialog if none is explicitly configured or if configured is 0.
pub const DEFAULT_DIALOG_ITEM: i16 = ALERT_BUTTON_OK;

/// Resolves the effective default item number for a dialog.
///
/// Returns `configured` if `configured > 0`, otherwise returns `ALERT_BUTTON_OK` (1).
#[inline]
pub const fn evaluate_dialog_default_item(configured: Option<i16>) -> i16 {
    match configured {
        Some(item) if item > 0 => item,
        _ => DEFAULT_DIALOG_ITEM,
    }
}

/// Canonical evaluated parameters for `MoveDialogItem`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`:
/// `EXTERN_API( OSErr ) MoveDialogItem(DialogRef inDialog, SInt16 inItemNo, SInt16 inHoriz, SInt16 inVert);`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MoveDialogItemParameters {
    dialog_ptr: u32,
    item_no: i16,
    in_horiz: i16,
    in_vert: i16,
}

impl MoveDialogItemParameters {
    /// Constructs a new `MoveDialogItemParameters`.
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, item_no: i16, in_horiz: i16, in_vert: i16) -> Self {
        Self {
            dialog_ptr,
            item_no,
            in_horiz,
            in_vert,
        }
    }

    /// The target dialog pointer.
    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// The 1-based dialog item number.
    #[inline]
    #[must_use]
    pub const fn item_no(&self) -> i16 {
        self.item_no
    }

    /// The 1-based dialog item number as a `usize`.
    #[inline]
    #[must_use]
    pub const fn item_number(&self) -> usize {
        self.item_no as usize
    }

    /// The target horizontal coordinate (left).
    #[inline]
    #[must_use]
    pub const fn in_horiz(&self) -> i16 {
        self.in_horiz
    }

    /// The target vertical coordinate (top).
    #[inline]
    #[must_use]
    pub const fn in_vert(&self) -> i16 {
        self.in_vert
    }
}

/// Evaluates parameters for `MoveDialogItem`.
///
/// Returns `Ok(MoveDialogItemParameters)` if `dialog_ptr != 0` and `item_no > 0`,
/// or `Err(DIALOG_PARAM_ERR)` otherwise.
#[inline]
#[must_use]
pub const fn evaluate_move_dialog_item_parameters(
    dialog_ptr: u32,
    item_no: i16,
    in_horiz: i16,
    in_vert: i16,
) -> Result<MoveDialogItemParameters, i16> {
    if dialog_ptr == 0 || item_no <= 0 {
        Err(DIALOG_PARAM_ERR)
    } else {
        Ok(MoveDialogItemParameters::new(
            dialog_ptr,
            item_no,
            in_horiz,
            in_vert,
        ))
    }
}

/// Evaluates the transformed item rectangle for `MoveDialogItem`.
///
/// Moves the item's display rectangle so that its top-left corner is at `(in_vert, in_horiz)`,
/// preserving its existing width and height.
#[inline]
#[must_use]
pub const fn evaluate_move_dialog_item_rect(
    current_rect: (i16, i16, i16, i16),
    in_horiz: i16,
    in_vert: i16,
) -> (i16, i16, i16, i16) {
    let height = current_rect.2.saturating_sub(current_rect.0);
    let width = current_rect.3.saturating_sub(current_rect.1);
    (
        in_vert,
        in_horiz,
        in_vert.saturating_add(height),
        in_horiz.saturating_add(width),
    )
}

/// Canonical evaluated parameters for `SizeDialogItem`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`:
/// `EXTERN_API( OSErr ) SizeDialogItem(DialogRef inDialog, SInt16 inItemNo, SInt16 inWidth, SInt16 inHeight);`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SizeDialogItemParameters {
    dialog_ptr: u32,
    item_no: i16,
    in_width: i16,
    in_height: i16,
}

impl SizeDialogItemParameters {
    /// Constructs a new `SizeDialogItemParameters`.
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, item_no: i16, in_width: i16, in_height: i16) -> Self {
        Self {
            dialog_ptr,
            item_no,
            in_width,
            in_height,
        }
    }

    /// The target dialog pointer.
    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// The 1-based dialog item number.
    #[inline]
    #[must_use]
    pub const fn item_no(&self) -> i16 {
        self.item_no
    }

    /// The 1-based dialog item number as a `usize`.
    #[inline]
    #[must_use]
    pub const fn item_number(&self) -> usize {
        self.item_no as usize
    }

    /// The target width.
    #[inline]
    #[must_use]
    pub const fn in_width(&self) -> i16 {
        self.in_width
    }

    /// The target height.
    #[inline]
    #[must_use]
    pub const fn in_height(&self) -> i16 {
        self.in_height
    }
}

/// Evaluates parameters for `SizeDialogItem`.
///
/// Returns `Ok(SizeDialogItemParameters)` if `dialog_ptr != 0` and `item_no > 0`,
/// or `Err(DIALOG_PARAM_ERR)` otherwise.
#[inline]
#[must_use]
pub const fn evaluate_size_dialog_item_parameters(
    dialog_ptr: u32,
    item_no: i16,
    in_width: i16,
    in_height: i16,
) -> Result<SizeDialogItemParameters, i16> {
    if dialog_ptr == 0 || item_no <= 0 {
        Err(DIALOG_PARAM_ERR)
    } else {
        Ok(SizeDialogItemParameters::new(
            dialog_ptr,
            item_no,
            in_width,
            in_height,
        ))
    }
}

/// Evaluates the transformed item rectangle for `SizeDialogItem`.
///
/// Resizes the item's display rectangle so that its top-left corner is preserved,
/// and its bottom-right corner becomes `(top + in_height, left + in_width)`.
#[inline]
#[must_use]
pub const fn evaluate_size_dialog_item_rect(
    current_rect: (i16, i16, i16, i16),
    in_width: i16,
    in_height: i16,
) -> (i16, i16, i16, i16) {
    (
        current_rect.0,
        current_rect.1,
        current_rect.0.saturating_add(in_height),
        current_rect.1.saturating_add(in_width),
    )
}

/// Evaluated parameters for a `SetDialogTracksCursor` request.
///
/// Macintosh Toolbox Essentials (1992), p. 6-166:
/// `FUNCTION SetDialogTracksCursor(theDialog: DialogPtr; tracks: BOOLEAN): OSErr;`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetDialogTracksCursorParameters {
    dialog_ptr: u32,
    tracks: bool,
}

impl SetDialogTracksCursorParameters {
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, tracks: bool) -> Self {
        Self {
            dialog_ptr,
            tracks,
        }
    }

    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn tracks(&self) -> bool {
        self.tracks
    }

    /// Returns `true` if `theDialog` is `NIL` (0), meaning cursor tracking
    /// applies globally to all dialogs.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn tracks_all_dialogs(&self) -> bool {
        self.dialog_ptr == 0
    }
}

/// Evaluates `SetDialogTracksCursor` parameters.
///
/// Macintosh Toolbox Essentials (1992), p. 6-166.
/// Passing `NIL` (0) for `theDialog` sets tracking for all dialogs.
/// Returns `Ok(SetDialogTracksCursorParameters)`.
pub fn evaluate_set_dialog_tracks_cursor_parameters(
    dialog_ptr: u32,
    tracks: bool,
) -> Result<SetDialogTracksCursorParameters, i16> {
    Ok(SetDialogTracksCursorParameters::new(dialog_ptr, tracks))
}

/// Evaluated parameters for a `GetDialogTracksCursor` request.
///
/// Macintosh Toolbox Essentials (1992), p. 6-166:
/// `FUNCTION GetDialogTracksCursor(theDialog: DialogRef; VAR tracks: Boolean): OSErr;`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetDialogTracksCursorParameters {
    dialog_ptr: u32,
    out_tracks_ptr: u32,
}

impl GetDialogTracksCursorParameters {
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, out_tracks_ptr: u32) -> Self {
        Self {
            dialog_ptr,
            out_tracks_ptr,
        }
    }

    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    #[inline]
    #[must_use]
    pub const fn out_tracks_ptr(&self) -> u32 {
        self.out_tracks_ptr
    }
}

/// Evaluates `GetDialogTracksCursor` parameter validation.
pub fn evaluate_get_dialog_tracks_cursor_parameters(
    dialog_ptr: u32,
    out_tracks_ptr: u32,
    can_write: bool,
) -> Result<GetDialogTracksCursorParameters, i16> {
    if out_tracks_ptr == 0 || !can_write {
        Err(DIALOG_PARAM_ERR)
    } else {
        Ok(GetDialogTracksCursorParameters::new(
            dialog_ptr,
            out_tracks_ptr,
        ))
    }
}

/// Evaluates `IsDialogTracksCursor` parameter validation.
pub fn evaluate_is_dialog_tracks_cursor_parameters(
    dialog_ptr: u32,
) -> Result<u32, i16> {
    Ok(dialog_ptr)
}

/// Evaluated parameters for an `AutoPositionDialog` / `PositionDialog` request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AutoPositionDialogParameters {
    dialog_ptr: u32,
    parent_ptr: u32,
    method: u16,
}

impl AutoPositionDialogParameters {
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, parent_ptr: u32, method: u16) -> Self {
        Self {
            dialog_ptr,
            parent_ptr,
            method,
        }
    }

    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    #[inline]
    #[must_use]
    pub const fn parent_ptr(&self) -> u32 {
        self.parent_ptr
    }

    #[inline]
    #[must_use]
    pub const fn method(&self) -> u16 {
        self.method
    }
}

/// Evaluates `AutoPositionDialog` / `PositionDialog` parameter validation.
pub fn evaluate_auto_position_dialog_parameters(
    dialog_ptr: u32,
    parent_ptr: u32,
    position: u16,
) -> Result<AutoPositionDialogParameters, i16> {
    if dialog_ptr == 0 {
        return Err(DIALOG_PARAM_ERR);
    }
    let Some(method) = normalize_dialog_position_method(position) else {
        return Err(DIALOG_PARAM_ERR);
    };
    Ok(AutoPositionDialogParameters::new(
        dialog_ptr, parent_ptr, method,
    ))
}

/// Evaluated parameters for a `GetStdFilterProc` request.
///
/// Macintosh Toolbox Essentials (1992), p. 6-163; Apple Dialog Manager Reference (2007), p. 38:
/// `FUNCTION GetStdFilterProc(VAR theProc: ModalFilterUPP): OSErr;`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetStdFilterProcParameters {
    out_proc: u32,
}

impl GetStdFilterProcParameters {
    #[inline]
    #[must_use]
    pub const fn new(out_proc: u32) -> Self {
        Self { out_proc }
    }

    #[inline]
    #[must_use]
    pub const fn out_proc(&self) -> u32 {
        self.out_proc
    }
}

/// Evaluates `GetStdFilterProc` output pointer validation.
///
/// Macintosh Toolbox Essentials (1992), p. 6-163; Apple Dialog Manager Reference (2007), p. 38.
/// Returns `Ok(GetStdFilterProcParameters)` if `out_proc != 0` and `can_write` is true, or `Err(DIALOG_PARAM_ERR)` otherwise.
pub fn evaluate_get_std_filter_proc_parameters(
    out_proc: u32,
    can_write: bool,
) -> Result<GetStdFilterProcParameters, i16> {
    if out_proc == 0 || !can_write {
        Err(DIALOG_PARAM_ERR)
    } else {
        Ok(GetStdFilterProcParameters::new(out_proc))
    }
}

/// Evaluates `GetStdFilterProc` output pointer validation.
///
/// Macintosh Toolbox Essentials (1992), p. 6-163; Apple Dialog Manager Reference (2007), p. 38.
/// Returns `Ok(DIALOG_NO_ERR)` if `out_proc != 0` and `can_write` is true, or `Err(DIALOG_PARAM_ERR)` otherwise.
#[allow(dead_code)]
#[inline]
pub fn evaluate_get_std_filter_proc(out_proc: u32, can_write: bool) -> Result<i16, i16> {
    evaluate_get_std_filter_proc_parameters(out_proc, can_write).map(|_| DIALOG_NO_ERR)
}

/// The evaluated parameters for a `CountDITL` / `CountDitl` operation.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-128--6-129:
/// `FUNCTION CountDITL (theDialog: DialogPtr): Integer;`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CountDitlParameters {
    dialog_ptr: u32,
}

#[allow(dead_code)]
impl CountDitlParameters {
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32) -> Self {
        Self { dialog_ptr }
    }

    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// Whether the dialog pointer is non-null.
    #[inline]
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.dialog_ptr != 0
    }
}

/// Evaluates `CountDITL` / `CountDitl` parameters from the dialog pointer.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-128--6-129.
/// Returns `None` if `dialog_ptr == 0`.
#[inline]
pub const fn evaluate_count_ditl_parameters(dialog_ptr: u32) -> Option<CountDitlParameters> {
    if dialog_ptr == 0 {
        return None;
    }
    Some(CountDitlParameters::new(dialog_ptr))
}

/// Evaluates the total number of items in a dialog.
///
/// Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 6-128--6-129.
/// Uses the DITL header count word (where `count = word.wrapping_add(1)`) if present
/// and non-zero; otherwise falls back to `record_count`.
pub fn evaluate_count_ditl(ditl_count_word: Option<u16>, record_count: usize) -> u16 {
    if let Some(word) = ditl_count_word {
        let count = word.wrapping_add(1);
        if count != 0 {
            return count;
        }
    }
    record_count.min(u16::MAX as usize) as u16
}

/// Hit-tests a dialog-local point against dialog items, returning the 0-based index of the first matching item.
///
/// If `enabled_only` is true, disabled items (`is_dialog_item_disabled`) are skipped.
/// For each item whose bounding rectangle contains `(local_v, local_h)`, `control_part_hit` is called
/// if the item is a control (`is_dialog_item_control`). If `control_part_hit` returns `false`, the item
/// is skipped (allowing clicks on inactive controls or transparent group box bodies to fall through).
///
/// Inside Macintosh Volume I, pp. I-416--I-418;
/// Macintosh Toolbox Essentials (1992), pp. 6-125, 6-138--6-139.
pub fn find_dialog_item_hit<I, F>(
    items: I,
    local_v: i16,
    local_h: i16,
    enabled_only: bool,
    mut control_part_hit: F,
) -> Option<usize>
where
    I: IntoIterator<Item = ((i16, i16, i16, i16), u8)>,
    F: FnMut(usize) -> bool,
{
    for (idx, (rect, raw_type)) in items.into_iter().enumerate() {
        if enabled_only && is_dialog_item_disabled(raw_type) {
            continue;
        }
        if !rect_contains_point(rect, local_v, local_h) {
            continue;
        }
        if is_dialog_item_control(raw_type) && !control_part_hit(idx) {
            continue;
        }
        return Some(idx);
    }
    None
}

/// Hit-tests a global screen point against dialog items given the dialog window bounds.
#[allow(dead_code)]
pub fn find_dialog_item_at_global_point<I, F>(
    bounds: (i16, i16, i16, i16),
    screen_v: i16,
    screen_h: i16,
    items: I,
    enabled_only: bool,
    control_part_hit: F,
) -> Option<usize>
where
    I: IntoIterator<Item = ((i16, i16, i16, i16), u8)>,
    F: FnMut(usize) -> bool,
{
    let (local_v, local_h) = global_to_dialog_local_point(bounds, screen_v, screen_h);
    find_dialog_item_hit(items, local_v, local_h, enabled_only, control_part_hit)
}

/// Offset the bounding rectangles of compiled DITL items directly within the raw binary buffer.
pub fn offset_ditl_bytes(
    bytes: &mut [u8],
    item_offsets: impl IntoIterator<Item = usize>,
    dv: i16,
    dh: i16,
) {
    if dv == 0 && dh == 0 {
        return;
    }
    for item_offset in item_offsets {
        for (coordinate_offset, delta) in [(4usize, dv), (6, dh), (8, dv), (10, dh)] {
            let Some(start) = item_offset.checked_add(coordinate_offset) else {
                continue;
            };
            let Some(pair) = bytes.get(start..start.saturating_add(2)) else {
                continue;
            };
            let value = i16::from_be_bytes([pair[0], pair[1]]).saturating_add(delta);
            if let Some(destination) = bytes.get_mut(start..start.saturating_add(2)) {
                destination.copy_from_slice(&value.to_be_bytes());
            }
        }
    }
}

/// Size in bytes of the fixed header prefix of a compiled DITL item
/// (4-byte handle + 8-byte Rect + 1-byte item type).
#[allow(dead_code)]
pub const DITL_ITEM_HEADER_SIZE: usize = 13;

/// Writes the standard 13-byte DITL item header (4-byte handle, 8-byte rect, 1-byte item type)
/// directly into a byte buffer at `offset`.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-120--6-123.
/// Returns `true` if the buffer was large enough and the header was written; `false` otherwise.
#[allow(dead_code)]
pub fn write_ditl_item_header_bytes(
    bytes: &mut [u8],
    offset: usize,
    handle: u32,
    rect: (i16, i16, i16, i16),
    item_type: u8,
) -> bool {
    let Some(end) = offset.checked_add(DITL_ITEM_HEADER_SIZE) else {
        return false;
    };
    let Some(slice) = bytes.get_mut(offset..end) else {
        return false;
    };
    slice[0..4].copy_from_slice(&handle.to_be_bytes());
    slice[4..6].copy_from_slice(&rect.0.to_be_bytes());
    slice[6..8].copy_from_slice(&rect.1.to_be_bytes());
    slice[8..10].copy_from_slice(&rect.2.to_be_bytes());
    slice[10..12].copy_from_slice(&rect.3.to_be_bytes());
    slice[12] = item_type;
    true
}

/// Target action for dialog template purgeability operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogTemplatePurgeabilityAction {
    /// Make already-loaded resources purgeable (`FreeDialog` / `FreeAlert`).
    MakePurgeable,
    /// Load resources if missing and make them unpurgeable (`CouldDialog` / `CouldAlert`).
    MakeUnpurgeableLoadIfMissing,
}

/// Evaluated parameters for a dialog or alert template purgeability operation.
///
/// Inside Macintosh Volume I, pp. I-415, I-420.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialogTemplatePurgeabilityParameters {
    template_type: [u8; 4],
    template_id: i16,
    action: DialogTemplatePurgeabilityAction,
}

impl DialogTemplatePurgeabilityParameters {
    /// Constructs a new `DialogTemplatePurgeabilityParameters`.
    #[inline]
    #[must_use]
    pub const fn new(
        template_type: [u8; 4],
        template_id: i16,
        action: DialogTemplatePurgeabilityAction,
    ) -> Self {
        Self {
            template_type,
            template_id,
            action,
        }
    }

    /// Four-character resource type code (`DLOG` or `ALRT`).
    #[inline]
    #[must_use]
    pub const fn template_type(&self) -> [u8; 4] {
        self.template_type
    }

    /// Template resource ID.
    #[inline]
    #[must_use]
    pub const fn template_id(&self) -> i16 {
        self.template_id
    }

    /// The requested purgeability action.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn action(&self) -> DialogTemplatePurgeabilityAction {
        self.action
    }

    /// Whether missing resources should be loaded.
    #[inline]
    #[must_use]
    pub const fn load_if_missing(&self) -> bool {
        matches!(
            self.action,
            DialogTemplatePurgeabilityAction::MakeUnpurgeableLoadIfMissing
        )
    }

    /// Whether resources should be marked purgeable.
    #[inline]
    #[must_use]
    pub const fn purgeable(&self) -> bool {
        matches!(
            self.action,
            DialogTemplatePurgeabilityAction::MakePurgeable
        )
    }

    /// Whether this purgeability operation targets an alert template (`ALRT`).
    #[inline]
    #[must_use]
    pub const fn is_alert(&self) -> bool {
        self.template_type[0] == b'A'
            && self.template_type[1] == b'L'
            && self.template_type[2] == b'R'
            && self.template_type[3] == b'T'
    }

    /// Whether this purgeability operation targets a dialog template (`DLOG`).
    #[inline]
    #[must_use]
    pub const fn is_dialog(&self) -> bool {
        self.template_type[0] == b'D'
            && self.template_type[1] == b'L'
            && self.template_type[2] == b'O'
            && self.template_type[3] == b'G'
    }
}

/// Backward compatibility alias for `DialogTemplatePurgeabilityParameters`.
#[allow(dead_code)]
pub type DialogTemplatePurgeabilityQuery = DialogTemplatePurgeabilityParameters;

/// Evaluates and validates input parameters for a dialog or alert template purgeability request.
///
/// Inside Macintosh Volume I, pp. I-415, I-420.
#[inline]
#[must_use]
#[allow(dead_code)]
pub const fn evaluate_dialog_template_purgeability_parameters(
    template_type: [u8; 4],
    template_id: i16,
    load_if_missing: bool,
    purgeable: bool,
) -> DialogTemplatePurgeabilityParameters {
    let action = if purgeable {
        DialogTemplatePurgeabilityAction::MakePurgeable
    } else if load_if_missing {
        DialogTemplatePurgeabilityAction::MakeUnpurgeableLoadIfMissing
    } else {
        DialogTemplatePurgeabilityAction::MakePurgeable
    };
    DialogTemplatePurgeabilityParameters::new(template_type, template_id, action)
}

/// Convenience evaluation for dialog template purgeability (`CouldDialog` / `FreeDialog`).
#[inline]
#[must_use]
pub const fn evaluate_dialog_purgeability_parameters(
    dialog_id: i16,
    load_if_missing: bool,
    purgeable: bool,
) -> DialogTemplatePurgeabilityParameters {
    evaluate_dialog_template_purgeability_parameters(
        *b"DLOG",
        dialog_id,
        load_if_missing,
        purgeable,
    )
}

/// Convenience evaluation for alert template purgeability (`CouldAlert` / `FreeAlert`).
#[inline]
#[must_use]
pub const fn evaluate_alert_purgeability_parameters(
    alert_id: i16,
    load_if_missing: bool,
    purgeable: bool,
) -> DialogTemplatePurgeabilityParameters {
    evaluate_dialog_template_purgeability_parameters(
        *b"ALRT",
        alert_id,
        load_if_missing,
        purgeable,
    )
}

/// Evaluates parameters for a `CouldDialog` request.
///
/// Inside Macintosh Volume I, p. I-415.
#[inline]
#[must_use]
pub const fn evaluate_could_dialog_parameters(dialog_id: i16) -> DialogTemplatePurgeabilityParameters {
    evaluate_dialog_purgeability_parameters(dialog_id, true, false)
}

/// Evaluates parameters for a `FreeDialog` request.
///
/// Inside Macintosh Volume I, p. I-415.
#[inline]
#[must_use]
pub const fn evaluate_free_dialog_parameters(dialog_id: i16) -> DialogTemplatePurgeabilityParameters {
    evaluate_dialog_purgeability_parameters(dialog_id, false, true)
}

/// Evaluates parameters for a `CouldAlert` request.
///
/// Inside Macintosh Volume I, p. I-420.
#[inline]
#[must_use]
pub const fn evaluate_could_alert_parameters(alert_id: i16) -> DialogTemplatePurgeabilityParameters {
    evaluate_alert_purgeability_parameters(alert_id, true, false)
}

/// Evaluates parameters for a `FreeAlert` request.
///
/// Inside Macintosh Volume I, p. I-420.
#[inline]
#[must_use]
pub const fn evaluate_free_alert_parameters(alert_id: i16) -> DialogTemplatePurgeabilityParameters {
    evaluate_alert_purgeability_parameters(alert_id, false, true)
}

/// Backward compatibility alias for `evaluate_dialog_template_purgeability_parameters`.
#[inline]
#[must_use]
#[allow(dead_code)]
pub const fn evaluate_dialog_template_purgeability_query(
    template_type: [u8; 4],
    template_id: i16,
    load_if_missing: bool,
    purgeable: bool,
) -> DialogTemplatePurgeabilityParameters {
    evaluate_dialog_template_purgeability_parameters(
        template_type,
        template_id,
        load_if_missing,
        purgeable,
    )
}

/// Parsed representation of a Macintosh dialog template (`DLOG` resource).
///
/// Inside Macintosh Volume I, pp. I-437--I-438;
/// Macintosh Toolbox Essentials (1992), pp. 6-113--6-114, 6-148.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogTemplate {
    pub bounds: (i16, i16, i16, i16),
    pub proc_id: i16,
    pub visible: bool,
    pub go_away: bool,
    pub ref_con: u32,
    pub items_id: i16,
    pub title: Vec<u8>,
    pub position: u16,
}

impl DialogTemplate {
    /// Decode the Pascal title bytes as a Mac Roman string.
    pub fn title_string(&self) -> String {
        decode_mac_roman(&self.title)
    }
}

/// Parse a dialog template (`DLOG` resource) from compiled binary bytes.
///
/// Inside Macintosh Volume I, pp. I-437--I-438;
/// Macintosh Toolbox Essentials (1992), pp. 6-113--6-114, 6-148.
pub fn parse_dialog_template(bytes: &[u8]) -> Option<DialogTemplate> {
    if bytes.len() < 20 {
        return None;
    }
    let bounds = (
        i16::from_be_bytes(bytes.get(0..2)?.try_into().ok()?),
        i16::from_be_bytes(bytes.get(2..4)?.try_into().ok()?),
        i16::from_be_bytes(bytes.get(4..6)?.try_into().ok()?),
        i16::from_be_bytes(bytes.get(6..8)?.try_into().ok()?),
    );
    let proc_id = i16::from_be_bytes(bytes.get(8..10)?.try_into().ok()?);
    let visible = *bytes.get(10)? != 0;
    let go_away = bytes.get(12).copied().unwrap_or(0) != 0;
    let ref_con = bytes
        .get(14..18)
        .and_then(|slice| slice.try_into().ok())
        .map(u32::from_be_bytes)
        .unwrap_or(0);
    let items_id = i16::from_be_bytes(bytes.get(18..20)?.try_into().ok()?);
    let title_len = usize::from(bytes.get(20).copied().unwrap_or(0));
    let title_start = 21.min(bytes.len());
    let title_end = 21usize.saturating_add(title_len).min(bytes.len());
    let title = bytes.get(title_start..title_end).unwrap_or(&[]).to_vec();
    let nominal_title_end = 21usize.saturating_add(title_len);
    let position_offset = (nominal_title_end + 1) & !1;
    let position = bytes
        .get(position_offset..position_offset.saturating_add(2))
        .and_then(|value| value.try_into().ok())
        .map(u16::from_be_bytes)
        .unwrap_or(0);

    Some(DialogTemplate {
        bounds,
        proc_id,
        visible,
        go_away,
        ref_con,
        items_id,
        title,
        position,
    })
}

/// Parsed representation of a Macintosh alert template (`ALRT` resource).
///
/// Inside Macintosh Volume I, pp. I-425--I-426;
/// Macintosh Toolbox Essentials (1992), p. 6-150.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AlertTemplate {
    pub bounds: (i16, i16, i16, i16),
    pub items_id: i16,
    pub stages: u16,
    pub position: u16,
}

/// Parse an alert template (`ALRT` resource) from compiled binary bytes.
///
/// Inside Macintosh Volume I, pp. I-425--I-426;
/// Macintosh Toolbox Essentials (1992), p. 6-150.
pub fn parse_alert_template(bytes: &[u8]) -> Option<AlertTemplate> {
    if bytes.len() < 8 {
        return None;
    }
    let bounds = (
        i16::from_be_bytes(bytes.get(0..2)?.try_into().ok()?),
        i16::from_be_bytes(bytes.get(2..4)?.try_into().ok()?),
        i16::from_be_bytes(bytes.get(4..6)?.try_into().ok()?),
        i16::from_be_bytes(bytes.get(6..8)?.try_into().ok()?),
    );
    let items_id = bytes
        .get(8..10)
        .and_then(|slice| slice.try_into().ok())
        .map(i16::from_be_bytes)
        .unwrap_or(0);
    let stages = bytes
        .get(10..12)
        .and_then(|slice| slice.try_into().ok())
        .map(u16::from_be_bytes)
        .unwrap_or(0);
    let position = bytes
        .get(12..14)
        .and_then(|slice| slice.try_into().ok())
        .map(u16::from_be_bytes)
        .unwrap_or(0);

    Some(AlertTemplate {
        bounds,
        items_id,
        stages,
        position,
    })
}

/// Information about an alert stage extracted from an `ALRT` resource's stage word.
///
/// Inside Macintosh Volume I, pp. I-422--I-424;
/// Macintosh Toolbox Essentials (1992), pp. 6-106, 6-150.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AlertStageInfo {
    /// Zero-based stage index (0..=3), clamped from guest stage counter.
    pub stage_index: u16,
    /// The 4-bit nibble for this stage.
    pub stage_nibble: u8,
    /// Whether the alert window is drawn (bit 2 / 0x04 of the nibble).
    pub box_drawn: bool,
    /// The default button item index: 1 (OK) if bit 3 is 0, or 2 (Cancel) if bit 3 is 1.
    pub default_item: i16,
    /// Sound number (0..=3) from bits 0..=1.
    pub sound_number: u8,
}

/// Decode alert stage parameters for the specified stage counter (0..=3).
pub fn alert_stage_info(stages: u16, stage_counter: u16) -> AlertStageInfo {
    let stage_index = stage_counter.min(3);
    let stage_nibble = ((stages >> (stage_index * 4)) & 0x0F) as u8;
    let box_drawn = (stage_nibble & 0x04) != 0;
    let default_item = if (stage_nibble & 0x08) == 0 { 1 } else { 2 };
    let sound_number = stage_nibble & 0x03;
    AlertStageInfo {
        stage_index,
        stage_nibble,
        box_drawn,
        default_item,
        sound_number,
    }
}

/// Canonical result returned by alert traps when the alert window is suppressed (box drawn is false).
/// Inside Macintosh Volume I, p. I-418, I-422.
pub const ALERT_SUPPRESSED_RESULT: i16 = -1;

/// Initial alert stage counter value (`0`).
///
/// Inside Macintosh Volume I, p. I-423.
pub const INITIAL_ALERT_STAGE: u16 = 0;

/// Evaluates the alert stage counter after `ResetAlertStage`.
///
/// Resets the alert stage counter (`AlertStage` / `ACount` at `$0A9A`) back to stage 0.
/// Inside Macintosh Volume I, p. I-423.
#[inline]
pub const fn evaluate_reset_alert_stage() -> u16 {
    INITIAL_ALERT_STAGE
}

/// Evaluates querying the active alert stage from low-memory global `AlertStage` / `ACount` ($0A9A).
///
/// Universal Interfaces 3.4.1 `Dialogs.h`:
/// `EXTERN_API( SInt16 ) GetAlertStage(void);`
/// `#define GetAlertStage() (* (short*) 0x0A9A)`
///
/// Inside Macintosh Volume I, p. I-422.
#[inline]
#[must_use]
pub const fn evaluate_get_alert_stage(raw_acount: u16) -> i16 {
    raw_acount as i16
}

/// Evaluates setting the active alert stage into low-memory global `AlertStage` / `ACount` ($0A9A).
///
/// Universal Interfaces 3.4.1 `LowMem.h`:
/// `EXTERN_API( void ) LMSetACount(short value);`
#[inline]
#[must_use]
pub const fn evaluate_set_alert_stage(stage: i16) -> u16 {
    stage as u16
}

/// Evaluated alert stage state including sound number, default item, and stage counter progression.
///
/// Inside Macintosh Volume I, pp. I-417--I-424;
/// Macintosh Toolbox Essentials (1992), pp. 6-105--6-119, 6-150.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AlertStageEvaluation {
    /// Zero-based stage index (0..=3), clamped from guest stage counter.
    stage_index: u16,
    /// Raw 4-bit nibble for this stage.
    stage_nibble: u8,
    /// Whether the alert window is drawn (bit 2 / 0x04 of the nibble).
    box_drawn: bool,
    /// The default button item index: 1 (OK) if bit 3 is 0, or 2 (Cancel) if bit 3 is 1.
    default_item: i16,
    /// Sound number (0..=3) from bits 0..=1.
    sound_number: u8,
    /// Next alert stage counter to be written to `AlertStage` ($0A9A), clamped to 3.
    next_stage: u16,
}

#[allow(dead_code)]
impl AlertStageEvaluation {
    /// Creates a new evaluated alert stage.
    #[inline]
    pub const fn new(
        stage_index: u16,
        stage_nibble: u8,
        box_drawn: bool,
        default_item: i16,
        sound_number: u8,
        next_stage: u16,
    ) -> Self {
        Self {
            stage_index,
            stage_nibble,
            box_drawn,
            default_item,
            sound_number,
            next_stage,
        }
    }

    /// Zero-based stage index (0..=3), clamped from guest stage counter.
    #[inline]
    pub const fn stage_index(&self) -> u16 {
        self.stage_index
    }

    /// Raw 4-bit nibble for this stage.
    #[inline]
    pub const fn stage_nibble(&self) -> u8 {
        self.stage_nibble
    }

    /// Whether the alert window is drawn (bit 2 / 0x04 of the nibble).
    #[inline]
    pub const fn box_drawn(&self) -> bool {
        self.box_drawn
    }

    /// The default button item index: 1 (OK) if bit 3 is 0, or 2 (Cancel) if bit 3 is 1.
    #[inline]
    pub const fn default_item(&self) -> i16 {
        self.default_item
    }

    /// The effective default button item if the alert window is drawn, or `None` if suppressed.
    #[inline]
    pub const fn effective_default_item(&self) -> Option<i16> {
        if self.box_drawn {
            Some(self.default_item)
        } else {
            None
        }
    }

    /// Sound number (0..=3) from bits 0..=1.
    #[inline]
    pub const fn sound_number(&self) -> u8 {
        self.sound_number
    }

    /// Whether an audible alert sound is specified (sound number > 0).
    #[inline]
    pub const fn has_sound(&self) -> bool {
        self.sound_number != 0
    }

    /// Next alert stage counter to be written to `AlertStage` ($0A9A), clamped to 3.
    #[inline]
    pub const fn next_stage(&self) -> u16 {
        self.next_stage
    }

    /// Canonical result code returned when the alert window is suppressed (`ALERT_SUPPRESSED_RESULT` / `-1`).
    #[inline]
    pub const fn suppressed_result(&self) -> i16 {
        ALERT_SUPPRESSED_RESULT
    }
}

/// Variant kind of classic Alert family invocation.
///
/// Inside Macintosh Volume I, pp. I-418, I-422;
/// Macintosh Toolbox Essentials (1992), pp. 6-105..6-107.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlertKind {
    /// Plain `Alert` ($A985) without default icon.
    Alert,
    /// `StopAlert` ($A986) with Stop icon (ID 0).
    Stop,
    /// `NoteAlert` ($A987) with Note icon (ID 1).
    Note,
    /// `CautionAlert` ($A988) with Caution icon (ID 2).
    Caution,
}

impl AlertKind {
    /// Associated system icon resource ID (`stopIcon`=0, `noteIcon`=1, `cautionIcon`=2), or `None` for plain alert.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn icon_id(&self) -> Option<i16> {
        match self {
            Self::Alert => None,
            Self::Stop => Some(0),
            Self::Note => Some(1),
            Self::Caution => Some(2),
        }
    }

    /// Whether this is a plain alert without an associated alert icon.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn is_plain(&self) -> bool {
        matches!(self, Self::Alert)
    }

    /// Human-readable diagnostic routine name.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Alert => "Alert",
            Self::Stop => "StopAlert",
            Self::Note => "NoteAlert",
            Self::Caution => "CautionAlert",
        }
    }
}

/// Evaluated parameters for a classic Alert family invocation (`Alert`, `StopAlert`, `NoteAlert`, `CautionAlert`).
///
/// Inside Macintosh Volume I, pp. I-418, I-422;
/// Macintosh Toolbox Essentials (1992), pp. 6-105..6-107.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AlertParameters {
    alert_id: i16,
    filter_proc: u32,
    kind: AlertKind,
}

impl AlertParameters {
    /// Constructs a new `AlertParameters`.
    #[inline]
    #[must_use]
    pub const fn new(alert_id: i16, filter_proc: u32, kind: AlertKind) -> Self {
        Self {
            alert_id,
            filter_proc,
            kind,
        }
    }

    /// The requested alert resource ID.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn alert_id(&self) -> i16 {
        self.alert_id
    }

    /// The optional modal filter procedure pointer or UPP.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn filter_proc(&self) -> u32 {
        self.filter_proc
    }

    /// Whether a non-nil modal filter procedure pointer was supplied.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn has_filter_proc(&self) -> bool {
        self.filter_proc != 0
    }

    /// The alert kind variant.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn kind(&self) -> AlertKind {
        self.kind
    }

    /// Associated system icon resource ID, if any.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn icon_id(&self) -> Option<i16> {
        self.kind.icon_id()
    }
}

/// Evaluates and validates input parameters for an Alert family invocation.
///
/// Inside Macintosh Volume I, pp. I-418, I-422.
#[inline]
#[must_use]
pub const fn evaluate_alert_parameters(
    alert_id: i16,
    filter_proc: u32,
    kind: AlertKind,
) -> AlertParameters {
    AlertParameters::new(alert_id, filter_proc, kind)
}

/// Evaluated invocation of an Alert family trap (`Alert`, `StopAlert`, `NoteAlert`, `CautionAlert`).
///
/// Encapsulates the alert resource ID, evaluated stage parameters, next stage counter,
/// and low-memory `ANumber` updates.
/// Inside Macintosh Volume I, pp. I-417--I-424.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AlertInvocationEvaluation {
    alert_id: i16,
    stage: AlertStageEvaluation,
}

#[allow(dead_code)]
impl AlertInvocationEvaluation {
    /// Creates a new alert invocation evaluation.
    #[inline]
    pub const fn new(alert_id: i16, stage: AlertStageEvaluation) -> Self {
        Self { alert_id, stage }
    }

    /// The alert resource ID being invoked.
    #[inline]
    pub const fn alert_id(&self) -> i16 {
        self.alert_id
    }

    /// Evaluated stage details.
    #[inline]
    pub const fn stage(&self) -> &AlertStageEvaluation {
        &self.stage
    }

    /// Next alert stage counter to write to `AlertStage` (`$0A9A`).
    #[inline]
    pub const fn next_stage(&self) -> u16 {
        self.stage.next_stage()
    }

    /// Value to write to low-memory `ANumber` (`$0A98`), recording the last alert resource ID.
    #[inline]
    pub const fn anumber(&self) -> u16 {
        self.alert_id as u16
    }

    /// The effective default item number if the alert window is drawn, or `None` if suppressed.
    #[inline]
    pub const fn default_item(&self) -> Option<i16> {
        self.stage.effective_default_item()
    }

    /// Sound number (0..=3) associated with this alert stage.
    #[inline]
    pub const fn sound_number(&self) -> u8 {
        self.stage.sound_number()
    }

    /// Whether an audible alert sound is specified for this stage.
    #[inline]
    pub const fn has_sound(&self) -> bool {
        self.stage.has_sound()
    }

    /// Whether the alert box display is suppressed (box drawn is false).
    #[inline]
    pub const fn is_suppressed(&self) -> bool {
        !self.stage.box_drawn()
    }

    /// Canonical result code returned when the alert window is suppressed (`ALERT_SUPPRESSED_RESULT` / `-1`).
    #[inline]
    pub const fn suppressed_result(&self) -> i16 {
        self.stage.suppressed_result()
    }
}

/// Evaluated parameter for `ErrorSound` ($A98C).
///
/// Sets the error-sound procedure for alerts in `DABeeper` ($0A9C).
/// Passing NIL (0) indicates no sound (and no menu bar blinking) at all.
/// Inside Macintosh Volume I, p. I-411.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ErrorSoundEvaluation {
    sound_proc: u32,
}

/// Canonical alias for error sound parameter evaluation.
pub type ErrorSoundParameters = ErrorSoundEvaluation;

#[allow(dead_code)]
impl ErrorSoundEvaluation {
    /// Creates a new error sound evaluation.
    #[inline]
    pub const fn new(sound_proc: u32) -> Self {
        Self { sound_proc }
    }

    /// The guest procedure pointer to store in `DABeeper` ($0A9C).
    #[inline]
    pub const fn sound_proc(&self) -> u32 {
        self.sound_proc
    }

    /// Whether the sound procedure is NIL (0), silencing alert sounds.
    #[inline]
    pub const fn is_silent(&self) -> bool {
        self.sound_proc == 0
    }
}

/// Evaluates alert stage parameters and stage counter advancement for an alert template.
///
/// Inside Macintosh Volume I, pp. I-422--I-424.
#[inline]
pub fn evaluate_alert_stage(stages: u16, current_stage: u16) -> AlertStageEvaluation {
    let info = alert_stage_info(stages, current_stage);
    let next_stage = next_alert_stage(current_stage);
    AlertStageEvaluation::new(
        info.stage_index,
        info.stage_nibble,
        info.box_drawn,
        info.default_item,
        info.sound_number,
        next_stage,
    )
}

/// Evaluates invocation of an alert given its resource ID, stages word, and current stage counter.
///
/// Inside Macintosh Volume I, pp. I-417--I-424.
#[inline]
pub fn evaluate_alert_invocation(
    alert_id: i16,
    stages: u16,
    current_stage: u16,
) -> AlertInvocationEvaluation {
    let stage = evaluate_alert_stage(stages, current_stage);
    AlertInvocationEvaluation::new(alert_id, stage)
}

/// Evaluates `ErrorSound` ($A98C) parameter.
///
/// Inside Macintosh Volume I, p. I-411.
#[inline]
pub const fn evaluate_error_sound(sound_proc: u32) -> ErrorSoundEvaluation {
    ErrorSoundEvaluation::new(sound_proc)
}

/// Evaluates `ErrorSound` ($A98C / InterfaceLib) parameter.
///
/// Inside Macintosh Volume I, p. I-411.
#[inline]
pub const fn evaluate_error_sound_parameters(sound_proc: u32) -> ErrorSoundParameters {
    evaluate_error_sound(sound_proc)
}

/// Evaluated parameters for a `StandardAlert` or alert compatibility invocation.
///
/// Apple Dialog Manager Reference, pp. 65, 75–76, 82–83.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StandardAlertParameters {
    alert_type: i16,
    error_ptr: u32,
    explanation_ptr: u32,
    alert_param_ptr: u32,
    item_hit_out_ptr: u32,
    is_standard: bool,
}

impl StandardAlertParameters {
    /// Constructs a new `StandardAlertParameters`.
    #[inline]
    #[must_use]
    pub const fn new(
        alert_type: i16,
        error_ptr: u32,
        explanation_ptr: u32,
        alert_param_ptr: u32,
        item_hit_out_ptr: u32,
        is_standard: bool,
    ) -> Self {
        Self {
            alert_type,
            error_ptr,
            explanation_ptr,
            alert_param_ptr,
            item_hit_out_ptr,
            is_standard,
        }
    }

    /// Alert type or template resource ID.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn alert_type(&self) -> i16 {
        self.alert_type
    }

    /// Alert template resource ID (alias for `alert_type`).
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn alert_id(&self) -> i16 {
        self.alert_type
    }

    /// Pointer to Pascal error message string.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn error_ptr(&self) -> u32 {
        self.error_ptr
    }

    /// Pointer to Pascal explanation string.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn explanation_ptr(&self) -> u32 {
        self.explanation_ptr
    }

    /// Pointer to `AlertStdAlertParamRec` parameter block.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn alert_param_ptr(&self) -> u32 {
        self.alert_param_ptr
    }

    /// Pointer to output variable receiving the item hit.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn item_hit_out_ptr(&self) -> u32 {
        self.item_hit_out_ptr
    }

    /// Whether this is a `StandardAlert` call (true) or classic `Alert` compatibility call (false).
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn is_standard(&self) -> bool {
        self.is_standard
    }
}

/// Evaluates and validates input parameters for a `StandardAlert` or alert compatibility invocation.
///
/// If `is_standard` is true, `item_hit_out_ptr` must be non-null and writable (`can_write` must be true),
/// returning `Err(DIALOG_PARAM_ERR)` otherwise.
///
/// Apple Dialog Manager Reference, pp. 65, 75–76, 82–83.
#[inline]
pub fn evaluate_standard_alert_parameters(
    alert_type: i16,
    error_ptr: u32,
    explanation_ptr: u32,
    alert_param_ptr: u32,
    item_hit_out_ptr: u32,
    is_standard: bool,
    can_write: bool,
) -> Result<StandardAlertParameters, i16> {
    if is_standard && (item_hit_out_ptr == 0 || !can_write) {
        return Err(DIALOG_PARAM_ERR);
    }
    let effective_output = if is_standard { item_hit_out_ptr } else { 0 };
    Ok(StandardAlertParameters::new(
        alert_type,
        error_ptr,
        explanation_ptr,
        alert_param_ptr,
        effective_output,
        is_standard,
    ))
}

/// Replace `^0`..`^3` placeholders in dialog text bytes with the corresponding ParamText slot contents.
///
/// Returns `Cow::Borrowed` when no `^` character is present, avoiding allocations.
/// Inside Macintosh Volume I, p. I-422;
/// Macintosh Toolbox Essentials (1992), pp. 6-129--6-130.
pub fn apply_param_text<'a>(text: &'a [u8], slots: &[impl AsRef<[u8]>]) -> Cow<'a, [u8]> {
    if !text.contains(&b'^') {
        return Cow::Borrowed(text);
    }
    let mut expanded = Vec::with_capacity(text.len());
    let mut offset = 0;
    while offset < text.len() {
        if text[offset] == b'^' {
            if let Some(index) = text
                .get(offset + 1)
                .copied()
                .filter(u8::is_ascii_digit)
                .map(|b| usize::from(b - b'0'))
                .filter(|&idx| idx < slots.len())
            {
                expanded.extend_from_slice(slots[index].as_ref());
                offset += 2;
                continue;
            }
        }
        expanded.push(text[offset]);
        offset += 1;
    }
    Cow::Owned(expanded)
}

/// String-oriented wrapper for `apply_param_text` operating on Mac Roman text strings.
pub fn apply_param_text_str<'a, S: AsRef<[u8]>>(text: &'a str, slots: &[S]) -> Cow<'a, str> {
    if !text.contains('^') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '^' {
            if let Some(&next) = chars.peek() {
                if let Some(digit) = next.to_digit(10) {
                    let idx = digit as usize;
                    if idx < slots.len() {
                        chars.next();
                        out.push_str(&decode_mac_roman(slots[idx].as_ref()));
                        continue;
                    }
                }
            }
        }
        out.push(ch);
    }
    Cow::Owned(out)
}

/// The number of parameter text slots supported by `ParamText` (^0..^3).
pub const PARAM_TEXT_SLOT_COUNT: usize = 4;

/// Canonical evaluated parameters for `ParamText`.
///
/// Inside Macintosh Volume I, p. I-422;
/// Macintosh Toolbox Essentials (1992), pp. 6-129--6-130:
/// `PROCEDURE ParamText(param0, param1, param2, param3: Str255);`
/// Passing NIL (0) for any parameter leaves that slot's previous value unchanged.
/// Non-NIL parameters point to a Pascal string in guest memory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParamTextParameters {
    param_ptrs: [u32; PARAM_TEXT_SLOT_COUNT],
}

#[allow(dead_code)]
impl ParamTextParameters {
    /// Constructs a new `ParamTextParameters` with 4 parameter string pointers.
    #[inline]
    pub const fn new(param0: u32, param1: u32, param2: u32, param3: u32) -> Self {
        Self {
            param_ptrs: [param0, param1, param2, param3],
        }
    }

    /// Pointer for param0 (`^0`).
    #[inline]
    pub const fn param0(&self) -> u32 {
        self.param_ptrs[0]
    }

    /// Pointer for param1 (`^1`).
    #[inline]
    pub const fn param1(&self) -> u32 {
        self.param_ptrs[1]
    }

    /// Pointer for param2 (`^2`).
    #[inline]
    pub const fn param2(&self) -> u32 {
        self.param_ptrs[2]
    }

    /// Pointer for param3 (`^3`).
    #[inline]
    pub const fn param3(&self) -> u32 {
        self.param_ptrs[3]
    }

    /// Pointer for the specified 0-indexed parameter slot (`0..3`).
    #[inline]
    pub fn param(&self, index: usize) -> u32 {
        self.param_ptrs.get(index).copied().unwrap_or(0)
    }

    /// All 4 parameter pointers as a fixed-size array reference.
    #[inline]
    pub const fn param_ptrs(&self) -> &[u32; PARAM_TEXT_SLOT_COUNT] {
        &self.param_ptrs
    }

    /// Whether the specified slot has a non-NIL pointer.
    #[inline]
    pub fn has_param(&self, index: usize) -> bool {
        self.param(index) != 0
    }
}

/// Evaluates and validates input parameters for `ParamText`.
#[inline]
pub const fn evaluate_param_text_parameters(
    param0: u32,
    param1: u32,
    param2: u32,
    param3: u32,
) -> ParamTextParameters {
    ParamTextParameters::new(param0, param1, param2, param3)
}

/// The evaluated outcome of a `ParamText` invocation.
///
/// Inside Macintosh Volume I, p. I-422;
/// Macintosh Toolbox Essentials (1992), pp. 6-129--6-130:
/// `PROCEDURE ParamText(param0, param1, param2, param3: Str255);`
/// Passing NIL (or `None`) for any parameter leaves that slot's previous value unchanged.
/// Non-NIL parameters provide a replacement string (clamped to at most 255 bytes).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParamTextEvaluation {
    slots: [Option<Vec<u8>>; PARAM_TEXT_SLOT_COUNT],
}

impl ParamTextEvaluation {
    /// Constructs a new evaluation with the given optional slot byte replacements.
    pub const fn new(slots: [Option<Vec<u8>>; PARAM_TEXT_SLOT_COUNT]) -> Self {
        Self { slots }
    }

    /// Accesses the optional replacement bytes for the given 0-indexed slot.
    pub fn slot(&self, index: usize) -> Option<&[u8]> {
        self.slots.get(index).and_then(|opt| opt.as_deref())
    }

    /// Whether the specified slot has a replacement value in this invocation.
    #[allow(dead_code)]
    pub fn has_update(&self, index: usize) -> bool {
        self.slots.get(index).is_some_and(Option::is_some)
    }

    /// Applies non-`None` replacement slots in-place into an existing 4-slot array.
    pub fn apply_to(&self, target: &mut [Vec<u8>; PARAM_TEXT_SLOT_COUNT]) {
        for (index, slot_opt) in self.slots.iter().enumerate() {
            if let Some(bytes) = slot_opt {
                target[index] = bytes.clone();
            }
        }
    }

    /// Returns a new 4-slot array with non-`None` replacement slots merged into `current`.
    #[allow(dead_code)]
    pub fn merged_with(
        &self,
        current: &[Vec<u8>; PARAM_TEXT_SLOT_COUNT],
    ) -> [Vec<u8>; PARAM_TEXT_SLOT_COUNT] {
        let mut result = current.clone();
        self.apply_to(&mut result);
        result
    }

    /// Decodes all four current or updated slots as Mac OS Roman strings for debugging or tracing.
    pub fn decoded_strings(
        &self,
        current: &[Vec<u8>; PARAM_TEXT_SLOT_COUNT],
    ) -> [String; PARAM_TEXT_SLOT_COUNT] {
        let merged = self.merged_with(current);
        [
            decode_mac_roman(&merged[0]),
            decode_mac_roman(&merged[1]),
            decode_mac_roman(&merged[2]),
            decode_mac_roman(&merged[3]),
        ]
    }
}

/// Evaluates `ParamText` arguments from 4 optional byte slices.
///
/// Any `Some(bytes)` argument is clamped to at most 255 bytes (Pascal string capacity limit).
/// Any `None` argument indicates a NIL pointer, preserving the slot's existing value.
pub fn evaluate_param_text(
    slots: [Option<&[u8]>; PARAM_TEXT_SLOT_COUNT],
) -> ParamTextEvaluation {
    let mut evaluated = [None, None, None, None];
    for (i, opt) in slots.into_iter().enumerate() {
        if let Some(bytes) = opt {
            let clamped = if bytes.len() > 255 {
                bytes[..255].to_vec()
            } else {
                bytes.to_vec()
            };
            evaluated[i] = Some(clamped);
        }
    }
    ParamTextEvaluation::new(evaluated)
}

/// The evaluated outcome of an `InitDialogs` invocation.
///
/// Inside Macintosh Volume I, p. I-411:
/// `PROCEDURE InitDialogs(resumeProc: ProcPtr);`
/// Initializes the Dialog Manager. Saves the application resume procedure (if non-null),
/// resets the sound beeper procedure to default (NIL), resets the alert stage count to 0
/// (so the next alert begins at stage 1), clears the four `DAStrings` `ParamText` handles,
/// and resets the dialog font to the system font (stores 0 into `DlgFont` at `$0AFA`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitDialogsEvaluation {
    resume_proc: u32,
}

impl InitDialogsEvaluation {
    /// The guest procedure pointer to invoke on system error resume, or 0 if none.
    pub const fn resume_proc(&self) -> u32 {
        self.resume_proc
    }

    /// The initial alert stage count written to low memory (`ACount` / `AlertStage` at `$0A9A`), which is 0.
    pub const fn initial_alert_stage(&self) -> i16 {
        INITIAL_ALERT_STAGE as i16
    }

    /// The initial sound beeper procedure pointer (`DABeeper` at `$0A9C`), which is 0 (default sound).
    pub const fn da_beeper(&self) -> u32 {
        0
    }

    /// The number of `ParamText` handle slots in `DAStrings` (`$0AA0..$0AAF`) cleared on initialization (4).
    pub const fn da_strings_count(&self) -> usize {
        PARAM_TEXT_SLOT_COUNT
    }

    /// The initial dialog font written to low memory (`DlgFont` at `$0AFA`), which is 0 (system font).
    /// Inside Macintosh Volume I, pp. I-411..I-412.
    pub const fn initial_dialog_font(&self) -> i16 {
        DIALOG_INITIAL_FONT
    }
}

/// Evaluates an `InitDialogs` invocation with the given `resumeProc` pointer.
pub const fn evaluate_init_dialogs(resume_proc: u32) -> InitDialogsEvaluation {
    InitDialogsEvaluation { resume_proc }
}

/// Evaluates the dialog text font from the raw `DlgFont` low-memory word.
#[inline]
#[must_use]
pub const fn evaluate_dialog_font(raw_dlg_font: u16) -> i16 {
    raw_dlg_font as i16
}

/// Canonical Macintosh key codes for modal dialog navigation.
pub const KEY_RETURN: u8 = 0x24;
pub const KEY_NUMPAD_ENTER: u8 = 0x4C;
pub const KEY_ESCAPE: u8 = 0x35;
pub const KEY_TAB: u8 = 0x30;
pub const KEY_PERIOD: u8 = 0x2F;

/// Canonical Macintosh ASCII character codes for modal dialog navigation.
pub const CHAR_RETURN: u8 = 0x0D;
pub const CHAR_ENTER: u8 = 0x03;
pub const CHAR_ESCAPE: u8 = 0x1B;
pub const CHAR_TAB: u8 = 0x09;
pub const CHAR_PERIOD: u8 = b'.';

/// Event modifier bit for Command key (`cmdKey`).
pub const MODIFIER_CMD_KEY: u16 = 0x0100;

/// Event `what` codes for Toolbox events.
///
/// Inside Macintosh Volume I (1985), pp. I-249--I-250;
/// Macintosh Toolbox Essentials (1992), pp. 2-83--2-84.
pub const EVENT_NULL: u16 = 0;
pub const EVENT_MOUSE_DOWN: u16 = 1;
#[allow(dead_code)]
pub const EVENT_MOUSE_UP: u16 = 2;
pub const EVENT_KEY_DOWN: u16 = 3;
#[allow(dead_code)]
pub const EVENT_KEY_UP: u16 = 4;
pub const EVENT_AUTO_KEY: u16 = 5;
pub const EVENT_UPDATE: u16 = 6;
#[allow(dead_code)]
pub const EVENT_DISK: u16 = 7;
pub const EVENT_ACTIVATE: u16 = 8;

/// Decision produced by standard modal dialog keyboard filtering.
///
/// Inside Macintosh Volume I (1985), pp. I-415--I-416;
/// Macintosh Toolbox Essentials (1992), pp. 6-86--6-90, 6-138.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogFilterDecision {
    /// Return or Enter key activated: triggers the dialog's default button.
    TriggerDefaultButton,
    /// Escape or Command-Period key activated: triggers the dialog's cancel button.
    TriggerCancelButton,
    /// Tab key activated: advances focus to the next `editText` item.
    AdvanceEditTextFocus,
    /// Key event not handled by standard dialog filter.
    Unhandled,
}

/// Determine whether the specified character and key code correspond to the default button activation key (Return or Enter).
pub const fn is_dialog_default_key(char_code: u8, key_code: u8) -> bool {
    matches!(char_code, CHAR_RETURN | CHAR_ENTER)
        || matches!(key_code, KEY_RETURN | KEY_NUMPAD_ENTER)
}

/// Determine whether the specified character, key code, and modifier word correspond to the cancel button activation key (Escape or Command-Period).
pub const fn is_dialog_cancel_key(char_code: u8, key_code: u8, modifiers: u16) -> bool {
    char_code == CHAR_ESCAPE
        || key_code == KEY_ESCAPE
        || ((modifiers & MODIFIER_CMD_KEY) != 0
            && (char_code == CHAR_PERIOD || key_code == KEY_PERIOD))
}

/// Determine whether the specified character and key code correspond to the Tab key.
pub const fn is_dialog_tab_key(char_code: u8, key_code: u8) -> bool {
    char_code == CHAR_TAB || key_code == KEY_TAB
}

/// Evaluated input parameters for a `ModalDialog` invocation.
///
/// Inside Macintosh Volume I, p. I-415;
/// Macintosh Toolbox Essentials (1992), pp. 6-135--6-141.
/// `PROCEDURE ModalDialog (filterProc: ProcPtr; VAR itemHit: INTEGER);`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModalDialogParameters {
    filter_proc: u32,
    item_hit_ptr: u32,
}

impl ModalDialogParameters {
    /// Constructs a new `ModalDialogParameters` instance.
    #[inline]
    #[must_use]
    pub const fn new(filter_proc: u32, item_hit_ptr: u32) -> Self {
        Self {
            filter_proc,
            item_hit_ptr,
        }
    }

    /// Guest pointer to the filter procedure, or 0 (NIL) if none.
    #[inline]
    #[must_use]
    pub const fn filter_proc(&self) -> u32 {
        self.filter_proc
    }

    /// Whether an explicit non-NIL filter procedure was provided.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub const fn has_filter_proc(&self) -> bool {
        self.filter_proc != 0
    }

    /// Guest pointer to the output `itemHit` variable.
    #[inline]
    #[must_use]
    pub const fn item_hit_ptr(&self) -> u32 {
        self.item_hit_ptr
    }
}

/// Evaluates and validates input parameters for a `ModalDialog` invocation.
///
/// Inside Macintosh Volume I, p. I-415.
/// Returns `None` if `itemHit` pointer is NIL (0) or cannot be written.
#[inline]
pub fn evaluate_modal_dialog_parameters(
    filter_proc: u32,
    item_hit_ptr: u32,
    can_write_item_hit: bool,
) -> Option<ModalDialogParameters> {
    if item_hit_ptr == 0 || !can_write_item_hit {
        None
    } else {
        Some(ModalDialogParameters::new(filter_proc, item_hit_ptr))
    }
}

/// Evaluate a keyboard event for standard modal dialog filtering.
///
/// Inside Macintosh Volume I, pp. I-415--I-416;
/// Macintosh Toolbox Essentials (1992), pp. 6-86--6-90, 6-138.
pub fn evaluate_modal_dialog_key(
    event_what: u16,
    message: u32,
    modifiers: u16,
) -> DialogFilterDecision {
    if !matches!(event_what, EVENT_KEY_DOWN | EVENT_AUTO_KEY) {
        return DialogFilterDecision::Unhandled;
    }
    let char_code = (message & 0xFF) as u8;
    let key_code = ((message >> 8) & 0xFF) as u8;

    if is_dialog_default_key(char_code, key_code) {
        DialogFilterDecision::TriggerDefaultButton
    } else if is_dialog_cancel_key(char_code, key_code, modifiers) {
        DialogFilterDecision::TriggerCancelButton
    } else if is_dialog_tab_key(char_code, key_code) {
        DialogFilterDecision::AdvanceEditTextFocus
    } else {
        DialogFilterDecision::Unhandled
    }
}

/// Evaluated parameters for a `StdFilterProc` invocation.
///
/// Inside Macintosh Volume I (1985), p. I-415;
/// Macintosh Toolbox Essentials (1992), p. 6-144;
/// Apple Dialog Manager Reference (2007), p. 43:
/// `FUNCTION StdFilterProc (theDialog: DialogPtr; VAR theEvent: EventRecord; VAR itemHit: INTEGER): BOOLEAN;`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StdFilterProcParameters {
    dialog_ptr: u32,
    event_ptr: u32,
    item_hit_ptr: u32,
}

impl StdFilterProcParameters {
    /// Constructs a new `StdFilterProcParameters` instance.
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, event_ptr: u32, item_hit_ptr: u32) -> Self {
        Self {
            dialog_ptr,
            event_ptr,
            item_hit_ptr,
        }
    }

    /// Guest pointer to the target `DialogRecord`.
    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// Guest pointer to the incoming `EventRecord`.
    #[inline]
    #[must_use]
    pub const fn event_ptr(&self) -> u32 {
        self.event_ptr
    }

    /// Guest pointer to the output `itemHit` variable.
    #[inline]
    #[must_use]
    pub const fn item_hit_ptr(&self) -> u32 {
        self.item_hit_ptr
    }
}

/// Evaluates and validates input parameters for a `StdFilterProc` invocation.
///
/// Inside Macintosh Volume I (1985), p. I-415;
/// Macintosh Toolbox Essentials (1992), p. 6-144;
/// Apple Dialog Manager Reference (2007), p. 43.
/// Returns `None` if `dialog_ptr == 0`, `event_ptr == 0`, or `item_hit_ptr == 0`.
#[inline]
pub const fn evaluate_std_filter_proc_parameters(
    dialog_ptr: u32,
    event_ptr: u32,
    item_hit_ptr: u32,
) -> Option<StdFilterProcParameters> {
    if dialog_ptr == 0 || event_ptr == 0 || item_hit_ptr == 0 {
        None
    } else {
        Some(StdFilterProcParameters::new(
            dialog_ptr,
            event_ptr,
            item_hit_ptr,
        ))
    }
}

/// Canonical evaluation result for `StdFilterProc`.
///
/// Inside Macintosh Volume I (1985), p. I-415;
/// Macintosh Toolbox Essentials (1992), p. 6-144;
/// Apple Dialog Manager Reference (2007), p. 43:
/// `StdFilterProc` returns `TRUE` if the user presses Return or Enter and the dialog
/// box contains an enabled default button, returning that item number in `itemHit`.
/// Otherwise, returns `FALSE`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StdFilterProcEvaluation {
    handled: bool,
    item_hit: Option<i16>,
}

impl StdFilterProcEvaluation {
    /// Result indicating the event was not handled by the standard filter procedure.
    #[inline]
    #[must_use]
    pub const fn unhandled() -> Self {
        Self {
            handled: false,
            item_hit: None,
        }
    }

    /// Result indicating the event was handled by triggering the default button.
    #[inline]
    #[must_use]
    pub const fn handled(item_hit: i16) -> Self {
        Self {
            handled: true,
            item_hit: Some(item_hit),
        }
    }

    /// Returns `true` if the event was handled by the standard filter procedure.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn is_handled(&self) -> bool {
        self.handled
    }

    /// The item number hit if handled, or `None` if unhandled.
    #[inline]
    #[must_use]
    pub const fn item_hit(&self) -> Option<i16> {
        self.item_hit
    }

    /// Returns the boolean result as a 32-bit integer (1 for `TRUE`, 0 for `FALSE`).
    #[inline]
    #[must_use]
    pub const fn boolean_result(&self) -> u32 {
        if self.handled {
            1
        } else {
            0
        }
    }
}

/// Evaluates `StdFilterProc` event handling given a filter decision and default item number.
///
/// Inside Macintosh Volume I (1985), p. I-415;
/// Macintosh Toolbox Essentials (1992), p. 6-144;
/// Apple Dialog Manager Reference (2007), p. 43:
/// Returns handled if `decision` is `DialogFilterDecision::TriggerDefaultButton`
/// and `default_item` contains a valid 1-indexed item number (> 0).
#[inline]
pub fn evaluate_std_filter_proc(
    decision: DialogFilterDecision,
    default_item: Option<i16>,
) -> StdFilterProcEvaluation {
    if decision == DialogFilterDecision::TriggerDefaultButton {
        if let Some(item) = default_item.filter(|&item| item > 0) {
            return StdFilterProcEvaluation::handled(item);
        }
    }
    StdFilterProcEvaluation::unhandled()
}

/// Evaluates `StdFilterProc` for a given raw event and default item number.
///
/// Inside Macintosh Volume I (1985), p. I-415;
/// Macintosh Toolbox Essentials (1992), p. 6-144;
/// Apple Dialog Manager Reference (2007), p. 43.
#[inline]
pub fn evaluate_std_filter_proc_event(
    event_what: u16,
    message: u32,
    modifiers: u16,
    default_item: Option<i16>,
) -> StdFilterProcEvaluation {
    let decision = evaluate_modal_dialog_key(event_what, message, modifiers);
    evaluate_std_filter_proc(decision, default_item)
}

/// Find the next `editText` item in item-list order, wrapping cyclically.
///
/// Takes an iterator of item types (raw `u8`) and the current 1-indexed
/// edit item number (0 if no edit item is active). Returns the 1-indexed item
/// number of the next `editText` item, or `None` if the dialog has no `editText` items.
///
/// Inside Macintosh Volume I, p. I-416;
/// Macintosh Toolbox Essentials (1992), p. 6-88.
pub fn find_next_edit_text_item(
    item_types: impl IntoIterator<Item = u8>,
    current_edit_item_1_indexed: usize,
) -> Option<usize> {
    let types: Vec<u8> = item_types.into_iter().collect();
    if types.is_empty() {
        return None;
    }
    let start = current_edit_item_1_indexed;
    for offset in 0..types.len() {
        let idx = (start + offset) % types.len();
        let base_type = dialog_item_base_type(types[idx]);
        if base_type == DIALOG_ITEM_EDIT_TEXT {
            return Some(idx + 1);
        }
    }
    None
}

/// Helper to find the next `editText` item directly from a slice of `DialogItemRecord`s.
#[allow(dead_code)]
pub fn find_next_edit_text_in_items(
    items: &[DialogItemRecord],
    current_edit_item_1_indexed: usize,
) -> Option<usize> {
    find_next_edit_text_item(
        items.iter().map(|item| item.item_type),
        current_edit_item_1_indexed,
    )
}

/// Clamps a text length to 255 bytes (Pascal string maximum payload).
#[allow(dead_code)]
pub fn clamp_dialog_item_text_len(len: usize) -> u8 {
    len.min(255) as u8
}

/// Clamps a byte slice to at most 255 bytes (Pascal string maximum payload).
pub fn clamp_dialog_item_text_bytes(text_bytes: &[u8]) -> &[u8] {
    let len = text_bytes.len().min(255);
    &text_bytes[..len]
}

/// Clamps a UTF-8 string so its byte length does not exceed 255 bytes,
/// ensuring character boundary safety.
#[allow(dead_code)]
pub fn clamp_dialog_item_text_str(text: &str) -> &str {
    if text.len() <= 255 {
        return text;
    }
    let mut boundary = 255;
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &text[..boundary]
}

/// Encodes raw text bytes into Pascal string format: `(length_byte, clamped_payload)`.
pub fn encode_dialog_item_pstring(text_bytes: &[u8]) -> (u8, &[u8]) {
    let clamped = clamp_dialog_item_text_bytes(text_bytes);
    (clamped.len() as u8, clamped)
}

/// Formats text bytes into a Pascal string byte vector with length prefix: `[len, byte0, byte1, ...]`.
#[allow(dead_code)]
pub fn format_dialog_item_pstring(text_bytes: &[u8]) -> Vec<u8> {
    let clamped = clamp_dialog_item_text_bytes(text_bytes);
    let mut result = Vec::with_capacity(1 + clamped.len());
    result.push(clamped.len() as u8);
    result.extend_from_slice(clamped);
    result
}

/// Decodes a Pascal string slice (where the first byte is length) into payload bytes.
#[allow(dead_code)]
pub fn decode_dialog_item_pstring(bytes: &[u8]) -> Option<&[u8]> {
    let &len = bytes.first()?;
    let end = 1 + len as usize;
    if bytes.len() >= end {
        Some(&bytes[1..end])
    } else {
        None
    }
}

/// Decodes a Pascal string slice (where the first byte is length) into a Mac Roman string.
#[allow(dead_code)]
pub fn decode_dialog_item_pstring_to_string(bytes: &[u8]) -> Option<String> {
    decode_dialog_item_pstring(bytes).map(decode_mac_roman)
}

/// Prepares input text bytes for `SetDialogItemText`, clamping to at most 255 bytes
/// and returning both the clamped byte slice and the decoded Mac Roman string.
#[allow(dead_code)]
pub fn prepare_set_dialog_item_text(text_bytes: &[u8]) -> (&[u8], String) {
    let clamped = clamp_dialog_item_text_bytes(text_bytes);
    let decoded = decode_mac_roman(clamped);
    (clamped, decoded)
}

/// Encodes raw text bytes into Pascal string format for `GetDialogItemText`: `(length_byte, clamped_payload)`.
#[allow(dead_code)]
pub fn prepare_get_dialog_item_text(text_bytes: &[u8]) -> (u8, &[u8]) {
    encode_dialog_item_pstring(text_bytes)
}

/// Canonical evaluated parameters for `GetDialogItemText` / `GetIText`.
///
/// Inside Macintosh Volume I (1985), p. I-422;
/// Macintosh Toolbox Essentials (1992), pp. 6-130--6-131.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetDialogItemTextParameters {
    item_handle: u32,
    text_out_ptr: u32,
}

#[allow(dead_code)]
impl GetDialogItemTextParameters {
    /// Constructs a new `GetDialogItemTextParameters`.
    #[inline]
    pub const fn new(item_handle: u32, text_out_ptr: u32) -> Self {
        Self {
            item_handle,
            text_out_ptr,
        }
    }

    /// Target text item handle (`Handle`).
    #[inline]
    pub const fn item_handle(&self) -> u32 {
        self.item_handle
    }

    /// Alias for `item_handle`.
    #[inline]
    pub const fn handle(&self) -> u32 {
        self.item_handle
    }

    /// Target Str255 output pointer.
    #[inline]
    pub const fn text_out_ptr(&self) -> u32 {
        self.text_out_ptr
    }

    /// Alias for `text_out_ptr`.
    #[inline]
    pub const fn text_ptr(&self) -> u32 {
        self.text_out_ptr
    }

    /// Whether a non-null item handle was supplied.
    #[inline]
    pub const fn has_handle(&self) -> bool {
        self.item_handle != 0
    }
}

/// Evaluates and validates input parameters for `GetDialogItemText` / `GetIText`.
///
/// Returns `None` if `text_out_ptr == 0` or `!can_write`.
#[inline]
pub const fn evaluate_get_dialog_item_text_parameters(
    item_handle: u32,
    text_out_ptr: u32,
    can_write: bool,
) -> Option<GetDialogItemTextParameters> {
    if text_out_ptr == 0 || !can_write {
        None
    } else {
        Some(GetDialogItemTextParameters {
            item_handle,
            text_out_ptr,
        })
    }
}

/// Architecture-neutral evaluation outcome for a `GetDialogItemText` request.
///
/// Inside Macintosh Volume I (1985), p. I-422;
/// Macintosh Toolbox Essentials (1992), pp. 6-130--6-131.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetDialogItemTextEvaluation {
    /// Target dialog item handle.
    pub item_handle: u32,
    /// Destination pointer for the Str255 output parameter.
    pub text_out_ptr: u32,
    /// Length of the Pascal string payload (clamped to 255).
    pub len: u8,
    /// Raw byte payload of the text.
    pub text: Vec<u8>,
}

#[allow(dead_code)]
impl GetDialogItemTextEvaluation {
    /// Target dialog item handle.
    pub fn item_handle(&self) -> u32 {
        self.item_handle
    }

    /// Destination pointer for the Str255 output parameter.
    pub fn text_out_ptr(&self) -> u32 {
        self.text_out_ptr
    }

    /// Length byte of the Pascal string.
    pub fn len(&self) -> u8 {
        self.len
    }

    /// Raw text bytes.
    pub fn text(&self) -> &[u8] {
        &self.text
    }

    /// Decoded Mac Roman string.
    pub fn decoded_text(&self) -> String {
        decode_mac_roman(&self.text)
    }

    /// Complete Pascal string representation `[len, text...]`.
    pub fn pstring_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(usize::from(self.len) + 1);
        out.push(self.len);
        out.extend_from_slice(&self.text);
        out
    }
}

/// Evaluates a `GetDialogItemText` request.
///
/// Returns `None` if `text_out_ptr == 0` (unwritable destination).
///
/// Inside Macintosh Volume I (1985), p. I-422;
/// Macintosh Toolbox Essentials (1992), pp. 6-130--6-131.
pub fn evaluate_get_dialog_item_text(
    item_handle: u32,
    text_out_ptr: u32,
    source_bytes: &[u8],
) -> Option<GetDialogItemTextEvaluation> {
    if text_out_ptr == 0 {
        None
    } else {
        let (len, text) = prepare_get_dialog_item_text(source_bytes);
        Some(GetDialogItemTextEvaluation {
            item_handle,
            text_out_ptr,
            len,
            text: text.to_vec(),
        })
    }
}

/// Canonical evaluated parameters for `SetDialogItemText` / `SetIText`.
///
/// Inside Macintosh Volume I (1985), p. I-422;
/// Macintosh Toolbox Essentials (1992), p. 6-131.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetDialogItemTextParameters {
    item_handle: u32,
    text_ptr: u32,
}

#[allow(dead_code)]
impl SetDialogItemTextParameters {
    /// Constructs a new `SetDialogItemTextParameters`.
    #[inline]
    pub const fn new(item_handle: u32, text_ptr: u32) -> Self {
        Self {
            item_handle,
            text_ptr,
        }
    }

    /// Target text item handle (`Handle`).
    #[inline]
    pub const fn item_handle(&self) -> u32 {
        self.item_handle
    }

    /// Alias for `item_handle`.
    #[inline]
    pub const fn handle(&self) -> u32 {
        self.item_handle
    }

    /// Source Str255 input pointer.
    #[inline]
    pub const fn text_ptr(&self) -> u32 {
        self.text_ptr
    }

    /// Whether a non-null item handle was supplied.
    #[inline]
    pub const fn has_handle(&self) -> bool {
        self.item_handle != 0
    }
}

/// Evaluates and validates input parameters for `SetDialogItemText` / `SetIText`.
///
/// Returns `None` if `text_ptr == 0`.
#[inline]
pub const fn evaluate_set_dialog_item_text_parameters(
    item_handle: u32,
    text_ptr: u32,
) -> Option<SetDialogItemTextParameters> {
    if text_ptr == 0 {
        None
    } else {
        Some(SetDialogItemTextParameters {
            item_handle,
            text_ptr,
        })
    }
}

/// Architecture-neutral evaluation outcome for a `SetDialogItemText` request.
///
/// Inside Macintosh Volume I (1985), p. I-422;
/// Macintosh Toolbox Essentials (1992), p. 6-131.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetDialogItemTextEvaluation {
    /// Target dialog item handle.
    pub item_handle: u32,
    /// Source pointer for the Str255 input string.
    pub text_ptr: u32,
    /// Raw byte payload of the text (clamped to at most 255 bytes).
    pub bytes: Vec<u8>,
    /// Decoded Mac Roman string.
    pub text: String,
}

#[allow(dead_code)]
impl SetDialogItemTextEvaluation {
    /// Target dialog item handle.
    pub fn item_handle(&self) -> u32 {
        self.item_handle
    }

    /// Source pointer for the Str255 input string.
    pub fn text_ptr(&self) -> u32 {
        self.text_ptr
    }

    /// Raw text bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Number of text bytes.
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    /// Decoded text string.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Evaluates a `SetDialogItemText` request.
///
/// Returns `None` if `text_ptr == 0` (null source string pointer).
///
/// Inside Macintosh Volume I (1985), p. I-422;
/// Macintosh Toolbox Essentials (1992), p. 6-131.
pub fn evaluate_set_dialog_item_text(
    item_handle: u32,
    text_ptr: u32,
    raw_text: &[u8],
) -> Option<SetDialogItemTextEvaluation> {
    if text_ptr == 0 {
        None
    } else {
        let (bytes, text) = prepare_set_dialog_item_text(raw_text);
        Some(SetDialogItemTextEvaluation {
            item_handle,
            text_ptr,
            bytes: bytes.to_vec(),
            text,
        })
    }
}

/// Resolves the active dialog item text bytes from an optional handle buffer and fallback DITL payload.
///
/// If `handle_bytes` is provided, it takes precedence. Otherwise, falls back to `fallback_payload`
/// if the item type represents text or a titled control. Returns an empty slice for other item types.
pub fn extract_dialog_item_text_bytes<'a>(
    base_type: u8,
    handle_bytes: Option<&'a [u8]>,
    fallback_payload: &'a [u8],
) -> &'a [u8] {
    if let Some(bytes) = handle_bytes {
        bytes
    } else if is_dialog_item_text_or_title(base_type) {
        fallback_payload
    } else {
        &[]
    }
}

/// Resolves the active dialog item text string from an optional handle buffer and fallback DITL payload.
#[allow(dead_code)]
pub fn extract_dialog_item_text_string(
    base_type: u8,
    handle_bytes: Option<&[u8]>,
    fallback_payload: &[u8],
) -> String {
    decode_mac_roman(extract_dialog_item_text_bytes(
        base_type,
        handle_bytes,
        fallback_payload,
    ))
}

/// Retrieves a reference to an item from a 1-indexed slice (returns `None` if `item_number == 0` or out of bounds).
pub fn get_item_at_1_indexed<T>(items: &[T], item_number_1_indexed: usize) -> Option<&T> {
    item_number_1_indexed
        .checked_sub(1)
        .and_then(|idx| items.get(idx))
}

/// Retrieves a mutable reference to an item from a 1-indexed slice (returns `None` if `item_number == 0` or out of bounds).
#[allow(dead_code)]
pub fn get_item_at_1_indexed_mut<T>(
    items: &mut [T],
    item_number_1_indexed: usize,
) -> Option<&mut T> {
    item_number_1_indexed
        .checked_sub(1)
        .and_then(|idx| items.get_mut(idx))
}

/// Returns true if the raw item type represents a button, checkbox, radio button, or resource control.
pub const fn is_dialog_item_control(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_control()
}

/// Returns true if the raw item type represents an application user item (`userItem`, 0).
pub const fn is_dialog_item_user_item(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_user_item()
}

/// Returns true if the raw item type represents a standard pushbutton (`ctrlItem + btnCtrl`, 4).
pub const fn is_dialog_item_button(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_button()
}

/// Returns true if the raw item type represents a checkbox control (`ctrlItem + chkCtrl`, 5).
pub const fn is_dialog_item_checkbox(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_checkbox()
}

/// Returns true if the raw item type represents a radio button control (`ctrlItem + radCtrl`, 6).
pub const fn is_dialog_item_radio(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_radio()
}

/// Returns true if the raw item type represents a resource-defined control (`ctrlItem + resCtrl`, 7).
pub const fn is_dialog_item_resource_control(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_resource_control()
}

/// Returns true if the raw item type represents non-editable static text (`statText`, 8).
pub const fn is_dialog_item_static_text(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_static_text()
}

/// Returns true if the raw item type represents editable text (`editText`, 16).
pub const fn is_dialog_item_edit_text(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_edit_text()
}

/// Returns true if the raw item type represents a standard icon (`iconItem`, 32).
pub const fn is_dialog_item_icon(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_icon()
}

/// Returns true if the raw item type represents a QuickDraw picture (`picItem`, 64).
pub const fn is_dialog_item_picture(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_picture()
}

/// Returns true if the raw item type represents static text or edit text.
pub const fn is_dialog_item_text(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_text()
}

/// Returns true if the raw item type represents static text, edit text, or a titled control.
pub const fn is_dialog_item_text_or_title(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).has_text_or_title()
}

/// Returns true if the raw item type references an external resource (resource control, icon, or picture).
pub const fn is_dialog_item_resource(raw_type: u8) -> bool {
    DialogItemKind::from_raw_type(raw_type).is_resource()
}

/// Returns the 4-byte Mac OS resource type associated with a dialog item (`b"CNTL"`, `b"ICON"`, or `b"PICT"`).
pub const fn dialog_item_resource_type(raw_type: u8) -> Option<[u8; 4]> {
    DialogItemKind::from_raw_type(raw_type).resource_type()
}

/// Returns the 4-byte Mac OS resource type associated with a dialog item as a big-endian `u32`.
pub const fn dialog_item_resource_type_u32(raw_type: u8) -> Option<u32> {
    DialogItemKind::from_raw_type(raw_type).resource_type_u32()
}

/// Standard Macintosh Dialog icon size (32x32 pixels, `ICON` / `cicn`).
pub const DIALOG_ICON_SIZE: i16 = 32;

/// Small Macintosh Dialog icon size (16x16 pixels, `SICN`).
#[allow(dead_code)]
pub const DIALOG_SMALL_ICON_SIZE: i16 = 16;

/// Centers an icon of dimensions `(icon_width, icon_height)` within `item_rect` `(top, left, bottom, right)`.
///
/// If `item_rect` is larger than the icon, the icon is centered horizontally and vertically.
/// If `item_rect` is identical in size, `item_rect` is returned.
#[allow(dead_code)]
#[inline]
pub const fn dialog_icon_rect(
    item_rect: (i16, i16, i16, i16),
    icon_width: i16,
    icon_height: i16,
) -> (i16, i16, i16, i16) {
    let (top, left, bottom, right) = item_rect;
    let rect_w = right - left;
    let rect_h = bottom - top;
    let offset_x = (rect_w - icon_width) / 2;
    let offset_y = (rect_h - icon_height) / 2;
    let start_x = left + offset_x;
    let start_y = top + offset_y;
    (start_y, start_x, start_y + icon_height, start_x + icon_width)
}

/// Centers a standard 32x32 icon (`DIALOG_ICON_SIZE`) within `item_rect`.
#[allow(dead_code)]
#[inline]
pub const fn dialog_standard_icon_rect(item_rect: (i16, i16, i16, i16)) -> (i16, i16, i16, i16) {
    dialog_icon_rect(item_rect, DIALOG_ICON_SIZE, DIALOG_ICON_SIZE)
}

/// Returns true if the raw item type represents static or editable text requiring handle disposal.
pub const fn is_dialog_item_disposable_text(raw_type: u8) -> bool {
    is_dialog_item_text(raw_type)
}

/// Returns true if the raw item type represents a button, checkbox, radio, or resource control requiring control record disposal.
pub const fn is_dialog_item_disposable_control(raw_type: u8) -> bool {
    is_dialog_item_control(raw_type)
}

/// Maps a dialog item type (with or without disabled bit) to its standard Control Manager procID.
///
/// Inside Macintosh Volume I, pp. I-410, I-421:
/// - Button (4) -> 0 (`pushButProc`)
/// - Checkbox (5) -> 1 (`checkBoxProc`)
/// - Radio (6) -> 2 (`radioButProc`)
pub fn dialog_item_control_proc_id(item_type: u8) -> Option<i16> {
    match dialog_item_base_type(item_type) {
        DIALOG_ITEM_BUTTON => Some(0),
        DIALOG_ITEM_CHECKBOX => Some(1),
        DIALOG_ITEM_RADIO => Some(2),
        _ => None,
    }
}

/// Maps an alert type (Stop, Note, Caution, Plain) to standard icon resource ID (Stop=0, Note=1, Caution=2, Plain=None).
#[allow(dead_code)]
pub fn alert_icon_id(alert_type: u16) -> Option<i16> {
    match alert_type {
        ALERT_TYPE_STOP => Some(0),
        ALERT_TYPE_NOTE => Some(1),
        ALERT_TYPE_CAUTION => Some(2),
        _ => None,
    }
}

/// Computes the next AlertStage counter capped at 3, per Inside Macintosh Volume I, p. I-423.
pub fn next_alert_stage(current_stage: u16) -> u16 {
    ((current_stage as u32) + 1).min(3) as u16
}

/// Normalizes an arbitrary text selection range `(start, end)` within `0..=max_len`.
///
/// Clamps both bounds to `max_len` and ensures `start <= end`.
#[inline]
pub fn normalize_selection_bounds(start: usize, end: usize, max_len: usize) -> (usize, usize) {
    let s = start.min(max_len);
    let e = end.min(max_len);
    if s <= e {
        (s, e)
    } else {
        (e, s)
    }
}

/// Normalizes an edit text item selection range `(start_sel, end_sel)` against a given text length.
///
/// Inside Macintosh Volume I, p. I-414, and Macintosh Toolbox Essentials (1992), p. 6-132:
/// - Special case: `(0, -1)` or `(0, 32767)` indicates "select all" (`0..text_len`).
/// - Clamps start and end bounds to `0..=text_len`.
/// - Normalizes reversed bounds (`start > end` -> swapped to `end..start`).
pub fn normalize_dialog_item_selection(
    start_sel: i16,
    end_sel: i16,
    text_len: usize,
) -> (u16, u16) {
    let text_len = text_len.min(i16::MAX as usize);
    if start_sel == 0 && (end_sel == -1 || end_sel == i16::MAX) {
        (0, text_len as u16)
    } else {
        let (s, e) = normalize_selection_bounds(
            start_sel.max(0) as usize,
            end_sel.max(0) as usize,
            text_len,
        );
        (s as u16, e as u16)
    }
}

/// Evaluated parameters for selecting an editable text item in a dialog.
///
/// Inside Macintosh Volume I, p. I-414, and Macintosh Toolbox Essentials (1992), pp. 6-131--6-132.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectDialogItemTextEvaluation {
    /// 0-indexed edit field index for `DialogRecord.editField` (offset 164).
    pub edit_field: u16,
    /// Normalized selection start offset.
    pub sel_start: u16,
    /// Normalized selection end offset.
    pub sel_end: u16,
}

#[allow(dead_code)]
impl SelectDialogItemTextEvaluation {
    /// Returns the normalized selection range `(start, end)`.
    pub fn selection_range(&self) -> (u16, u16) {
        (self.sel_start, self.sel_end)
    }
}

/// Canonical evaluated parameters for a `SelectDialogItemText` / `SelIText` request.
///
/// Inside Macintosh Volume I, p. I-422;
/// Macintosh Toolbox Essentials (1992), pp. 6-131--6-132:
/// `PROCEDURE SelectDialogItemText(theDialog: DialogPtr; itemNo: INTEGER; strtSel, endSel: INTEGER);`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectDialogItemTextParameters {
    dialog_ptr: u32,
    item_number: usize,
    selection_start: i16,
    selection_end: i16,
}

impl SelectDialogItemTextParameters {
    /// Constructs a new `SelectDialogItemTextParameters` instance.
    #[inline]
    #[must_use]
    pub const fn new(
        dialog_ptr: u32,
        item_number: usize,
        selection_start: i16,
        selection_end: i16,
    ) -> Self {
        Self {
            dialog_ptr,
            item_number,
            selection_start,
            selection_end,
        }
    }

    /// Target dialog pointer.
    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// 1-based dialog item number.
    #[inline]
    #[must_use]
    pub const fn item_number(&self) -> usize {
        self.item_number
    }

    /// 1-based dialog item number as signed 16-bit integer.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn item_no(&self) -> i16 {
        self.item_number as i16
    }

    /// 0-based item index (`item_number - 1`).
    #[inline]
    #[must_use]
    pub const fn item_index(&self) -> Option<usize> {
        self.item_number.checked_sub(1)
    }

    /// Selection start offset.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn selection_start(&self) -> i16 {
        self.selection_start
    }

    /// Selection end offset.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn selection_end(&self) -> i16 {
        self.selection_end
    }

    /// Evaluates the selection bounds and active edit field against item text characteristics.
    /// Returns `None` if `!is_edit_text`.
    pub fn evaluate_selection(
        &self,
        is_edit_text: bool,
        text_len: usize,
    ) -> Option<SelectDialogItemTextEvaluation> {
        evaluate_select_dialog_item_text(
            self.dialog_ptr,
            self.item_number,
            is_edit_text,
            self.selection_start,
            self.selection_end,
            text_len,
        )
    }
}

/// Evaluates and validates input parameters for `SelectDialogItemText` / `SelIText`.
///
/// Returns `None` if `dialog_ptr == 0` or `item_number == 0`.
#[inline]
pub const fn evaluate_select_dialog_item_text_parameters(
    dialog_ptr: u32,
    item_number: usize,
    selection_start: i16,
    selection_end: i16,
) -> Option<SelectDialogItemTextParameters> {
    if dialog_ptr == 0 || item_number == 0 {
        return None;
    }
    Some(SelectDialogItemTextParameters::new(
        dialog_ptr,
        item_number,
        selection_start,
        selection_end,
    ))
}

/// Evaluates selecting an editable text item in a dialog, returning the normalized selection
/// and the 0-indexed edit field if valid.
///
/// Inside Macintosh Volume I, p. I-414, and Macintosh Toolbox Essentials (1992), pp. 6-131--6-132:
/// - If `dialog_ptr == 0`, returns `None`.
/// - If `item_number == 0`, returns `None`.
/// - If `is_edit_text == false`, returns `None` (non-editText items are ignored).
/// - Clamps and normalizes `start_sel` and `end_sel` bounds (including `0..-1` and `0..32767` select-all).
/// - Sets `edit_field` to `(item_number - 1) as u16`.
pub fn evaluate_select_dialog_item_text(
    dialog_ptr: u32,
    item_number: usize,
    is_edit_text: bool,
    start_sel: i16,
    end_sel: i16,
    text_len: usize,
) -> Option<SelectDialogItemTextEvaluation> {
    if dialog_ptr == 0 || item_number == 0 || !is_edit_text {
        return None;
    }
    let edit_field = (item_number - 1).min(i16::MAX as usize) as u16;
    let (sel_start, sel_end) = normalize_dialog_item_selection(start_sel, end_sel, text_len);
    Some(SelectDialogItemTextEvaluation {
        edit_field,
        sel_start,
        sel_end,
    })
}

/// Canonical evaluated parameters for `CloseDialog` and `DisposeDialog`.
///
/// Inside Macintosh Volume I, p. I-413, and Macintosh Toolbox Essentials (1992), pp. 6-119--6-120:
/// - `CloseDialog` removes the dialog's window from the window list and frees standard items/controls,
///   but retains the `DialogRecord` memory and the DITL handle (caller-supplied `dStorage`).
/// - `DisposeDialog` removes the dialog's window and frees standard items/controls, and additionally
///   disposes the copied DITL handle and the `DialogRecord` memory allocated by `GetNewDialog`/`NewDialog`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialogTeardownParameters {
    dialog_ptr: u32,
    dispose_record: bool,
}

#[allow(dead_code)]
impl DialogTeardownParameters {
    /// Constructs a new `DialogTeardownParameters`.
    #[inline]
    pub const fn new(dialog_ptr: u32, dispose_record: bool) -> Self {
        Self {
            dialog_ptr,
            dispose_record,
        }
    }

    /// Target dialog pointer.
    #[inline]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// Whether the dialog record and copied items handle should be released.
    #[inline]
    pub const fn dispose_record(&self) -> bool {
        self.dispose_record
    }

    /// Whether this teardown represents `DisposeDialog`.
    #[inline]
    pub const fn is_dispose(&self) -> bool {
        self.dispose_record
    }

    /// Whether this teardown represents `CloseDialog`.
    #[inline]
    pub const fn is_close(&self) -> bool {
        !self.dispose_record
    }
}

/// Evaluates tearing down a dialog via `CloseDialog` (`is_dispose == false`) or `DisposeDialog` (`is_dispose == true`).
///
/// Returns `None` if `dialog_ptr == 0`.
#[inline]
pub const fn evaluate_dialog_teardown_parameters(
    dialog_ptr: u32,
    is_dispose: bool,
) -> Option<DialogTeardownParameters> {
    if dialog_ptr == 0 {
        None
    } else {
        Some(DialogTeardownParameters {
            dialog_ptr,
            dispose_record: is_dispose,
        })
    }
}

/// Evaluates closing a dialog via `CloseDialog`.
///
/// Returns `None` if `dialog_ptr == 0`.
#[inline]
pub const fn evaluate_close_dialog_parameters(
    dialog_ptr: u32,
) -> Option<DialogTeardownParameters> {
    evaluate_dialog_teardown_parameters(dialog_ptr, false)
}

/// Evaluates disposing a dialog via `DisposeDialog`.
///
/// Returns `None` if `dialog_ptr == 0`.
#[inline]
pub const fn evaluate_dispose_dialog_parameters(
    dialog_ptr: u32,
) -> Option<DialogTeardownParameters> {
    evaluate_dialog_teardown_parameters(dialog_ptr, true)
}


/// Canonical evaluated parameters for `DrawDialog`.
///
/// Inside Macintosh Volume I (1985), p. I-417;
/// Macintosh Toolbox Essentials (1992), p. 6-142.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrawDialogParameters {
    dialog_ptr: u32,
}

#[allow(dead_code)]
impl DrawDialogParameters {
    /// Constructs a new `DrawDialogParameters`.
    #[inline]
    pub const fn new(dialog_ptr: u32) -> Self {
        Self { dialog_ptr }
    }

    /// Target dialog pointer.
    #[inline]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }
}

/// Evaluates and validates input parameters for `DrawDialog`.
///
/// Returns `None` if `dialog_ptr == 0`.
#[inline]
pub const fn evaluate_draw_dialog_parameters(dialog_ptr: u32) -> Option<DrawDialogParameters> {
    if dialog_ptr == 0 {
        None
    } else {
        Some(DrawDialogParameters { dialog_ptr })
    }
}

/// Canonical evaluated parameters for `UpdateDialog` / `UpdtDialog`.
///
/// Inside Macintosh Volume I (1985), p. I-415;
/// Macintosh Toolbox Essentials (1992), pp. 6-142--6-143.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UpdateDialogParameters {
    dialog_ptr: u32,
    update_rgn: u32,
}

#[allow(dead_code)]
impl UpdateDialogParameters {
    /// Constructs a new `UpdateDialogParameters`.
    #[inline]
    pub const fn new(dialog_ptr: u32, update_rgn: u32) -> Self {
        Self {
            dialog_ptr,
            update_rgn,
        }
    }

    /// Target dialog pointer.
    #[inline]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// Handle to the update region.
    #[inline]
    pub const fn update_rgn(&self) -> u32 {
        self.update_rgn
    }

    /// Converts this update request to a general `DrawDialogParameters`.
    #[inline]
    pub const fn as_draw_dialog(&self) -> DrawDialogParameters {
        DrawDialogParameters {
            dialog_ptr: self.dialog_ptr,
        }
    }
}

/// Evaluates and validates input parameters for `UpdateDialog` / `UpdtDialog`.
///
/// Returns `None` if `dialog_ptr == 0` or `update_rgn == 0`.
#[inline]
pub const fn evaluate_update_dialog_parameters(
    dialog_ptr: u32,
    update_rgn: u32,
) -> Option<UpdateDialogParameters> {
    if dialog_ptr == 0 || update_rgn == 0 {
        None
    } else {
        Some(UpdateDialogParameters {
            dialog_ptr,
            update_rgn,
        })
    }
}

/// Computes the outer bounding rectangle and corner oval radius for drawing the standard 3px bold
/// default button ring around a push button rectangle.
///
/// Macintosh Toolbox Essentials (1992), Listing 6-17:
/// Outer rect is outset by 4 pixels in all dimensions; corner oval diameter is `max(4, height / 2 - 4)`.
pub fn default_button_outline_geometry(
    button_rect: (i16, i16, i16, i16),
) -> ((i16, i16, i16, i16), i16) {
    let outer = (
        button_rect.0.saturating_sub(DEFAULT_BUTTON_OUTLINE_INSET),
        button_rect.1.saturating_sub(DEFAULT_BUTTON_OUTLINE_INSET),
        button_rect.2.saturating_add(DEFAULT_BUTTON_OUTLINE_INSET),
        button_rect.3.saturating_add(DEFAULT_BUTTON_OUTLINE_INSET),
    );
    let height = outer.2.saturating_sub(outer.0);
    let oval = (height / 2 - 4).max(4);
    (outer, oval)
}

/// Canonical geometry and rendering metrics for a dialog default button outline ring.
///
/// Macintosh Toolbox Essentials (1992), Listing 6-17, pp. 6-50--6-51:
/// The standard default button ring is outset by 4 pixels around the button rectangle,
/// has a corner diameter of `(height / 2 - 4).max(4)`, and a pen thickness of 3 pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialogDefaultButtonOutline {
    outer_rect: (i16, i16, i16, i16),
    oval_width: i16,
    oval_height: i16,
    thickness: i16,
}

impl DialogDefaultButtonOutline {
    /// Constructs a new `DialogDefaultButtonOutline`.
    #[inline]
    #[must_use]
    pub const fn new(
        outer_rect: (i16, i16, i16, i16),
        oval_width: i16,
        oval_height: i16,
        thickness: i16,
    ) -> Self {
        Self {
            outer_rect,
            oval_width,
            oval_height,
            thickness,
        }
    }

    /// Outer bounding rectangle `(top, left, bottom, right)`.
    #[inline]
    #[must_use]
    pub const fn outer_rect(&self) -> (i16, i16, i16, i16) {
        self.outer_rect
    }

    /// Corner oval width in pixels.
    #[inline]
    #[must_use]
    pub const fn oval_width(&self) -> i16 {
        self.oval_width
    }

    /// Corner oval height in pixels.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn oval_height(&self) -> i16 {
        self.oval_height
    }

    /// Corner oval diameter shortcut (equal to `oval_width()`).
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn oval(&self) -> i16 {
        self.oval_width
    }

    /// Pen frame thickness in pixels.
    #[inline]
    #[must_use]
    pub const fn thickness(&self) -> i16 {
        self.thickness
    }
}

/// Evaluates default button outline geometry for a given push button bounding rectangle.
///
/// Macintosh Toolbox Essentials (1992), Listing 6-17, pp. 6-50--6-51.
#[inline]
pub fn evaluate_dialog_default_button_outline(
    button_rect: (i16, i16, i16, i16),
) -> DialogDefaultButtonOutline {
    let (outer, oval) = default_button_outline_geometry(button_rect);
    DialogDefaultButtonOutline::new(
        outer,
        oval,
        oval,
        DEFAULT_BUTTON_OUTLINE_THICKNESS,
    )
}

/// Evaluates whether a dialog item is the default push button, returning its outline geometry if so.
///
/// Macintosh Toolbox Essentials (1992), Listing 6-17, pp. 6-50--6-51:
/// Returns `Some(DialogDefaultButtonOutline)` if `raw_type` represents a push button (`DIALOG_ITEM_BUTTON`)
/// and `is_dialog_default_button(item_no, default_item)` is `true`. Otherwise returns `None`.
#[inline]
pub fn evaluate_dialog_item_default_button_outline(
    item_no: i16,
    default_item: i16,
    raw_type: u8,
    button_rect: (i16, i16, i16, i16),
) -> Option<DialogDefaultButtonOutline> {
    if is_dialog_item_button(raw_type) && is_dialog_default_button(item_no, default_item) {
        Some(evaluate_dialog_default_button_outline(button_rect))
    } else {
        None
    }
}

/// Returns true if the title matches "Cancel" (case-insensitive ASCII), which identifies
/// the standard Cancel button in dialog boxes per Macintosh Toolbox Essentials (1992), p. 6-51.
pub fn is_dialog_cancel_button_title(title: &[u8]) -> bool {
    title.eq_ignore_ascii_case(b"cancel")
}

/// Returns true if the string title matches "Cancel" (case-insensitive ASCII).
#[allow(dead_code)]
pub fn is_dialog_cancel_button_title_str(title: &str) -> bool {
    title.eq_ignore_ascii_case("cancel")
}

/// Finds the 1-indexed item number of an enabled PushButton with title "Cancel" (case-insensitive).
pub fn find_dialog_cancel_item_index<I, T>(items: I) -> Option<u16>
where
    I: IntoIterator<Item = (u8, T)>,
    T: AsRef<[u8]>,
{
    items
        .into_iter()
        .enumerate()
        .find_map(|(idx, (raw_type, title))| {
            let is_enabled_button =
                is_dialog_item_enabled(raw_type) && is_dialog_item_button(raw_type);
            if is_enabled_button && is_dialog_cancel_button_title(title.as_ref()) {
                u16::try_from(idx + 1).ok()
            } else {
                None
            }
        })
}

/// Canonical resolution of a dialog's cancel item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialogCancelItemResolution {
    item_no: Option<i16>,
}

impl DialogCancelItemResolution {
    /// Constructs a `DialogCancelItemResolution` from an optional 1-indexed item number.
    #[inline]
    #[must_use]
    pub const fn new(item_no: Option<i16>) -> Self {
        Self { item_no }
    }

    /// Constructs an empty `DialogCancelItemResolution` indicating no cancel item.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn none() -> Self {
        Self { item_no: None }
    }

    /// 1-indexed item number if a cancel item was resolved, or `None`.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn item_no(&self) -> Option<i16> {
        self.item_no
    }

    /// 1-indexed item number as an `i16`, returning `0` if no cancel item was resolved.
    #[inline]
    #[must_use]
    pub const fn item_number(&self) -> i16 {
        match self.item_no {
            Some(item) => item,
            None => 0,
        }
    }

    /// 1-indexed item number as a `u16`, returning `None` if no cancel item was resolved.
    #[inline]
    #[must_use]
    pub fn to_u16(&self) -> Option<u16> {
        self.item_no.and_then(|item| u16::try_from(item).ok())
    }

    /// Returns `true` if a cancel item was resolved (`item_no > 0`).
    #[inline]
    #[must_use]
    pub const fn has_item(&self) -> bool {
        match self.item_no {
            Some(item) => item > 0,
            None => false,
        }
    }

    /// Returns `true` if `item_no` matches the resolved cancel item (`item_no == cancel_item && cancel_item > 0`).
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn matches_item(&self, item_no: i16) -> bool {
        match self.item_no {
            Some(cancel) => is_dialog_cancel_button(item_no, cancel),
            None => false,
        }
    }
}

/// Evaluates and resolves the 1-indexed cancel item number for a dialog.
///
/// Priority:
/// 1. An explicitly configured cancel item (`configured_cancel > 0`).
/// 2. An enabled PushButton with title "Cancel" (case-insensitive ASCII) found via `find_dialog_cancel_item_index`.
/// 3. None if no cancel item is configured or found.
///
/// Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 6-51, 6-86--6-90, 6-163.
pub fn evaluate_dialog_cancel_item<I, T>(
    configured_cancel: Option<i16>,
    items: I,
) -> DialogCancelItemResolution
where
    I: IntoIterator<Item = (u8, T)>,
    T: AsRef<[u8]>,
{
    if let Some(item) = configured_cancel {
        if item > 0 {
            return DialogCancelItemResolution::new(Some(item));
        }
    }
    let found = find_dialog_cancel_item_index(items).and_then(|idx| i16::try_from(idx).ok());
    DialogCancelItemResolution::new(found)
}

/// Evaluates and resolves the 1-indexed cancel item number for a modal dialog,
/// falling back to `ALERT_BUTTON_CANCEL` (2) if no explicit or titled cancel item is found.
///
/// Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 6-51.
pub fn evaluate_modal_dialog_cancel_item<I, T>(
    configured_cancel: Option<i16>,
    items: I,
) -> DialogCancelItemResolution
where
    I: IntoIterator<Item = (u8, T)>,
    T: AsRef<[u8]>,
{
    let resolution = evaluate_dialog_cancel_item(configured_cancel, items);
    if resolution.has_item() {
        resolution
    } else {
        DialogCancelItemResolution::new(Some(ALERT_BUTTON_CANCEL))
    }
}

/// Evaluates whether a dialog filter decision triggers a cancel item, returning its 1-indexed number if so.
#[inline]
pub fn evaluate_dialog_filter_cancel(
    decision: DialogFilterDecision,
    cancel_item: Option<i16>,
) -> Option<i16> {
    if decision == DialogFilterDecision::TriggerCancelButton {
        cancel_item.filter(|&item| item > 0)
    } else {
        None
    }
}

/// Resolves the 1-indexed cancel item number for a dialog.
///
/// Priority:
/// 1. An explicitly configured cancel item (`configured_cancel > 0`).
/// 2. An enabled PushButton with title "Cancel" (case-insensitive ASCII) found via `find_dialog_cancel_item_index`.
/// 3. Returns 0 if no cancel item is configured or found.
///
/// Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 6-51, 6-86--6-90, 6-163.
#[allow(dead_code)]
#[inline]
pub fn resolve_dialog_cancel_item<I, T>(configured_cancel: Option<i16>, items: I) -> i16
where
    I: IntoIterator<Item = (u8, T)>,
    T: AsRef<[u8]>,
{
    evaluate_dialog_cancel_item(configured_cancel, items).item_number()
}

/// Resolves the 1-indexed cancel item number for a modal dialog, falling back to
/// `ALERT_BUTTON_CANCEL` (2) if no explicit or titled cancel item is found, per classic Mac OS convention.
#[allow(dead_code)]
#[inline]
pub fn resolve_modal_dialog_cancel_item<I, T>(configured_cancel: Option<i16>, items: I) -> i16
where
    I: IntoIterator<Item = (u8, T)>,
    T: AsRef<[u8]>,
{
    evaluate_modal_dialog_cancel_item(configured_cancel, items).item_number()
}

/// Resolves the 1-indexed default item number for a dialog.
///
/// If `configured_default` is `Some(item)` with `item > 0`, it is returned directly.
/// Otherwise, defaults to `ALERT_BUTTON_OK` (1) if `item_count >= 1`, or 0 if empty.
///
/// Inside Macintosh Volume I, p. I-415; Macintosh Toolbox Essentials (1992), pp. 6-86--6-90, 6-163.
pub fn resolve_dialog_default_item(configured_default: Option<i16>, item_count: usize) -> i16 {
    if let Some(item) = configured_default {
        if item > 0 {
            return item;
        }
    }
    if item_count >= 1 {
        ALERT_BUTTON_OK
    } else {
        0
    }
}

/// Returns true if `item_no` matches the active default item (`item_no == default_item && default_item > 0`).
#[allow(dead_code)]
#[inline]
pub const fn is_dialog_default_button(item_no: i16, default_item: i16) -> bool {
    item_no > 0 && item_no == default_item
}

/// Returns true if `item_no` matches the active cancel item (`item_no == cancel_item && cancel_item > 0`).
#[allow(dead_code)]
#[inline]
pub const fn is_dialog_cancel_button(item_no: i16, cancel_item: i16) -> bool {
    item_no > 0 && item_no == cancel_item
}

/// Computes the `(dv, dh)` translation offset for items being appended to a dialog via `AppendDITL`.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-108, 6-153:
/// - `overlayDITL` (0): (0, 0)
/// - `appendDITLRight` (1): (0, dialog_width)
/// - `appendDITLBottom` (2): (dialog_height, 0)
/// - negative `item_no` (< 0): offset relative to the upper-left of item `-method` (1-indexed).
pub fn append_ditl_offset_delta<F>(
    method: i16,
    dialog_height: i16,
    dialog_width: i16,
    item_origin_lookup: F,
) -> (i16, i16)
where
    F: FnOnce(usize) -> Option<(i16, i16)>,
{
    match method {
        APPEND_DITL_OVERLAY => (0, 0),
        APPEND_DITL_RIGHT => (0, dialog_width),
        APPEND_DITL_BOTTOM => (dialog_height, 0),
        relative_item if relative_item < 0 => {
            let item_no = usize::from(relative_item.unsigned_abs());
            item_origin_lookup(item_no).unwrap_or((0, 0))
        }
        _ => (0, 0),
    }
}

/// Calculates the number of retained items and the new DITL count-minus-one header word
/// after removing `remove_count` items from a dialog with `current_count` items.
///
/// Macintosh Toolbox Essentials (1992), p. 6-154:
/// - If `remove_count >= current_count`, all items are removed (0 retained, header word is -1 / 0xFFFF).
/// - Otherwise, `retained = current_count - remove_count`, and header word is `(retained - 1) as i16`.
pub fn shorten_ditl_counts(current_count: usize, remove_count: usize) -> (usize, i16) {
    let remove = remove_count.min(current_count);
    let retained = current_count.saturating_sub(remove);
    let count_minus_one = if retained == 0 {
        -1
    } else {
        retained.saturating_sub(1).min(i16::MAX as usize) as i16
    };
    (retained, count_minus_one)
}

/// Canonical evaluated parameters for `AppendDITL`.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-108, 6-153.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppendDitlParameters {
    dialog_ptr: u32,
    ditl_handle: u32,
    method: i16,
}

#[allow(dead_code)]
impl AppendDitlParameters {
    /// Constructs a new `AppendDitlParameters`.
    #[inline]
    pub const fn new(dialog_ptr: u32, ditl_handle: u32, method: i16) -> Self {
        Self {
            dialog_ptr,
            ditl_handle,
            method,
        }
    }

    /// The target dialog pointer.
    #[inline]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// The handle to the DITL resource data to append.
    #[inline]
    pub const fn ditl_handle(&self) -> u32 {
        self.ditl_handle
    }

    /// The placement method (`overlayDITL`, `appendDITLRight`, `appendDITLBottom`, or relative item).
    #[inline]
    pub const fn method(&self) -> i16 {
        self.method
    }
}

/// Evaluates and validates input parameters for `AppendDITL`.
///
/// Returns `None` if `dialog_ptr == 0` or `ditl_handle == 0`.
#[inline]
pub const fn evaluate_append_ditl_parameters(
    dialog_ptr: u32,
    ditl_handle: u32,
    method: i16,
) -> Option<AppendDitlParameters> {
    if dialog_ptr == 0 || ditl_handle == 0 {
        return None;
    }
    Some(AppendDitlParameters {
        dialog_ptr,
        ditl_handle,
        method,
    })
}

/// Canonical evaluated parameters for `AppendDialogItemList`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`:
/// `EXTERN_API( OSErr ) AppendDialogItemList(DialogRef dialog, SInt16 ditlID, DITLMethod method);`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppendDialogItemListParameters {
    dialog_ptr: u32,
    ditl_id: i16,
    method: i16,
}

impl AppendDialogItemListParameters {
    /// Constructs a new `AppendDialogItemListParameters`.
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, ditl_id: i16, method: i16) -> Self {
        Self {
            dialog_ptr,
            ditl_id,
            method,
        }
    }

    /// The target dialog pointer.
    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// The resource ID of the `'DITL'` resource to append.
    #[inline]
    #[must_use]
    pub const fn ditl_id(&self) -> i16 {
        self.ditl_id
    }

    /// The placement method (`overlayDITL`, `appendDITLRight`, `appendDITLBottom`, or relative item).
    #[inline]
    #[must_use]
    pub const fn method(&self) -> i16 {
        self.method
    }
}

/// Evaluates and validates input parameters for `AppendDialogItemList`.
///
/// Returns `Ok(AppendDialogItemListParameters)` if `dialog_ptr != 0`, or `Err(DIALOG_PARAM_ERR)` otherwise.
#[inline]
#[must_use]
pub const fn evaluate_append_dialog_item_list_parameters(
    dialog_ptr: u32,
    ditl_id: i16,
    method: i16,
) -> Result<AppendDialogItemListParameters, i16> {
    if dialog_ptr == 0 || ditl_id == 0 {
        Err(DIALOG_PARAM_ERR)
    } else {
        Ok(AppendDialogItemListParameters::new(
            dialog_ptr,
            ditl_id,
            method,
        ))
    }
}

/// Evaluates expanded dialog window bounds to encompass newly appended item rectangles.
///
/// Macintosh Toolbox Essentials (1992), p. 6-108:
/// `AppendDialogItemList` expands the dialog box, if necessary, to encompass the new items.
#[inline]
#[must_use]
pub fn evaluate_appended_dialog_bounds(
    current_bounds: (i16, i16, i16, i16),
    appended_item_rects: impl IntoIterator<Item = (i16, i16, i16, i16)>,
) -> (i16, i16, i16, i16) {
    let mut bounds = current_bounds;
    for rect in appended_item_rects {
        bounds.2 = bounds.2.max(rect.2);
        bounds.3 = bounds.3.max(rect.3);
    }
    bounds
}

/// Canonical evaluated parameters for `AutoSizeDialog`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`:
/// `EXTERN_API( OSStatus ) AutoSizeDialog(DialogRef inDialog);`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AutoSizeDialogParameters {
    dialog_ptr: u32,
}

impl AutoSizeDialogParameters {
    /// Constructs a new `AutoSizeDialogParameters`.
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32) -> Self {
        Self { dialog_ptr }
    }

    /// The target dialog pointer.
    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }
}

/// Evaluates and validates input parameters for `AutoSizeDialog`.
///
/// Returns `Ok(AutoSizeDialogParameters)` if `dialog_ptr != 0`, or `Err(DIALOG_PARAM_ERR)` otherwise.
#[inline]
#[must_use]
pub const fn evaluate_auto_size_dialog_parameters(
    dialog_ptr: u32,
) -> Result<AutoSizeDialogParameters, i16> {
    if dialog_ptr == 0 {
        Err(DIALOG_PARAM_ERR)
    } else {
        Ok(AutoSizeDialogParameters::new(dialog_ptr))
    }
}

/// Evaluates resized dialog window bounds to tightly encompass all dialog item rectangles.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`: `AutoSizeDialog` calculates the minimum content bounds
/// required to enclose all items in the dialog box, preserving the top-left origin.
/// If the item list is empty, the current bounds are preserved.
#[inline]
#[must_use]
pub fn evaluate_auto_size_dialog_bounds(
    current_bounds: (i16, i16, i16, i16),
    item_rects: impl IntoIterator<Item = (i16, i16, i16, i16)>,
) -> (i16, i16, i16, i16) {
    let mut max_bottom = current_bounds.0;
    let mut max_right = current_bounds.1;
    let mut has_items = false;
    for rect in item_rects {
        has_items = true;
        max_bottom = max_bottom.max(rect.2);
        max_right = max_right.max(rect.3);
    }
    if has_items {
        (current_bounds.0, current_bounds.1, max_bottom, max_right)
    } else {
        current_bounds
    }
}

/// Canonical evaluated parameters for `SetDialogFont` / `SetDAFont`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`:
/// `EXTERN_API( void ) SetDialogFont(SInt16 value);`
/// `#define SetDAFont(fontNum) SetDialogFont(fontNum)`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetDialogFontParameters {
    font_num: i16,
}

impl SetDialogFontParameters {
    /// Constructs a new `SetDialogFontParameters`.
    #[inline]
    #[must_use]
    pub const fn new(font_num: i16) -> Self {
        Self { font_num }
    }

    /// The target dialog font family ID to store in `DlgFont` ($0AFA).
    #[inline]
    #[must_use]
    pub const fn font_num(&self) -> i16 {
        self.font_num
    }
}

/// Evaluates and validates input parameters for `SetDialogFont` / `SetDAFont`.
#[inline]
#[must_use]
pub const fn evaluate_set_dialog_font_parameters(font_num: i16) -> SetDialogFontParameters {
    SetDialogFontParameters::new(font_num)
}

/// Evaluates reading the `ResumeProc` low-memory global ($0A8C).
///
/// Universal Interfaces 3.4.1 `LowMem.h`:
/// `EXTERN_API( Handle ) LMGetResumeProc(void);`
#[inline]
#[must_use]
pub const fn evaluate_get_resume_proc(raw: u32) -> u32 {
    raw
}

/// Evaluates writing the `ResumeProc` low-memory global ($0A8C).
///
/// Universal Interfaces 3.4.1 `LowMem.h`:
/// `EXTERN_API( void ) LMSetResumeProc(Handle value);`
#[inline]
#[must_use]
pub const fn evaluate_set_resume_proc(resume_proc: u32) -> u32 {
    resume_proc
}

/// Evaluates reading the `ANumber` low-memory global ($0A98).
///
/// Universal Interfaces 3.4.1 `LowMem.h`:
/// `EXTERN_API( short ) LMGetANumber(void);`
#[inline]
#[must_use]
pub const fn evaluate_get_anumber(raw_anumber: u16) -> i16 {
    raw_anumber as i16
}

/// Evaluates writing the `ANumber` low-memory global ($0A98).
///
/// Universal Interfaces 3.4.1 `LowMem.h`:
/// `EXTERN_API( void ) LMSetANumber(short value);`
#[inline]
#[must_use]
pub const fn evaluate_set_anumber(anumber: i16) -> u16 {
    anumber as u16
}

/// Evaluates reading the `DABeeper` low-memory global ($0A9C).
///
/// Universal Interfaces 3.4.1 `LowMem.h`:
/// `EXTERN_API( SoundProcPtr ) LMGetDABeeper(void);`
#[inline]
#[must_use]
pub const fn evaluate_get_da_beeper(raw: u32) -> u32 {
    raw
}

/// Evaluates writing the `DABeeper` low-memory global ($0A9C).
///
/// Universal Interfaces 3.4.1 `LowMem.h`:
/// `EXTERN_API( void ) LMSetDABeeper(SoundProcPtr value);`
#[inline]
#[must_use]
pub const fn evaluate_set_da_beeper(sound_proc: u32) -> u32 {
    evaluate_error_sound(sound_proc).sound_proc()
}

/// Returns the low-memory address of the `DAStrings` handles array ($0AA0).
///
/// Universal Interfaces 3.4.1 `LowMem.h`:
/// `EXTERN_API( Ptr ) LMGetDAStrings(void);`
#[inline]
#[must_use]
pub const fn evaluate_get_da_strings_addr() -> u32 {
    crate::memory::globals::addr::DA_STRINGS
}

/// Evaluates `GetDialogPort` (Universal Interfaces 3.4.1 `Dialogs.h`).
///
/// In classic Mac OS, `DialogRecord` begins with `WindowRecord`, which begins
/// with `GrafPort` / `CGrafPort`. Therefore `GetDialogPort` returns the dialog's
/// port pointer directly, or 0 if NULL.
#[inline]
#[must_use]
pub const fn evaluate_get_dialog_port(dialog_ptr: u32) -> u32 {
    dialog_ptr
}

/// Evaluates `GetDialogWindow` (Universal Interfaces 3.4.1 `Dialogs.h`).
///
/// In classic Mac OS, `DialogRecord` begins with `WindowRecord`, so the dialog
/// pointer is also the window pointer. Returns 0 if NULL.
#[inline]
#[must_use]
pub const fn evaluate_get_dialog_window(dialog_ptr: u32) -> u32 {
    dialog_ptr
}

/// Evaluates `GetDialogFromWindow` (Universal Interfaces 3.4.1 `Dialogs.h`).
///
/// Inside Macintosh Volume I, p. I-274: `dialogKind` is 2 (`DIALOG_WINDOW_KIND`).
/// If `window_ptr` is non-NULL and `window_kind` is `Some(DIALOG_WINDOW_KIND as i16)`,
/// the window is an active dialog and `window_ptr` is returned as `DialogRef`.
/// Otherwise, returns 0 (NULL).
#[inline]
#[must_use]
pub const fn evaluate_get_dialog_from_window(
    window_ptr: u32,
    window_kind: Option<i16>,
) -> u32 {
    if window_ptr == 0 {
        return 0;
    }
    match window_kind {
        Some(kind) if kind == DIALOG_WINDOW_KIND as i16 => window_ptr,
        _ => 0,
    }
}

/// Evaluates `SetPortDialogPort` (Universal Interfaces 3.4.1 `Dialogs.h`).
///
/// Returns the target port pointer to make current.
#[inline]
#[must_use]
pub const fn evaluate_set_port_dialog_port(dialog_ptr: u32) -> u32 {
    evaluate_get_dialog_port(dialog_ptr)
}

/// Canonical evaluated parameters for a dialog clipboard editing command (`DialogCut`, `DialogCopy`, `DialogPaste`, and `DialogDelete`).
///
/// Inside Macintosh Volume I, p. I-418; Macintosh Toolbox Essentials 1992, pp. 6-132..6-134:
/// Checks whether the dialog has an active editable text item (`editField >= 0`).
/// If so, retrieves the `TEHandle` from `textH` (offset 160) to target the TextEdit editing command.
/// If `theDialog` is NULL (0), `editField < 0`, or `textH == 0`, no editing command is performed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialogEditCommandEvaluation {
    text_handle: u32,
}

impl DialogEditCommandEvaluation {
    /// Constructs a new `DialogEditCommandEvaluation` with the target `TEHandle`.
    #[inline]
    #[must_use]
    pub const fn new(text_handle: u32) -> Self {
        Self { text_handle }
    }

    /// The target `TEHandle` for the TextEdit editing command.
    #[inline]
    #[must_use]
    pub const fn text_handle(&self) -> u32 {
        self.text_handle
    }
}

/// Evaluates a dialog clipboard editing command (`DialogCut`, `DialogCopy`, `DialogPaste`, and `DialogDelete`).
///
/// Returns `Some(DialogEditCommandEvaluation)` with the target `TEHandle` if `dialog_ptr != 0`, `edit_field >= 0`, and `text_handle != 0`.
/// Returns `None` if the dialog is NULL, no editable text item is active, or the text handle is NULL.
#[inline]
#[must_use]
pub const fn evaluate_dialog_edit_command(
    dialog_ptr: u32,
    edit_field: i16,
    text_handle: u32,
) -> Option<DialogEditCommandEvaluation> {
    if dialog_ptr == 0 || edit_field < 0 || text_handle == 0 {
        None
    } else {
        Some(DialogEditCommandEvaluation::new(text_handle))
    }
}

/// Canonical evaluated parameters for `ShortenDITL`.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-153--6-154.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShortenDitlParameters {
    dialog_ptr: u32,
    number_items: usize,
}

#[allow(dead_code)]
impl ShortenDitlParameters {
    /// Constructs a new `ShortenDitlParameters`.
    #[inline]
    pub const fn new(dialog_ptr: u32, number_items: usize) -> Self {
        Self {
            dialog_ptr,
            number_items,
        }
    }

    /// The target dialog pointer.
    #[inline]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// The number of items to remove from the end of the DITL.
    #[inline]
    pub const fn number_items(&self) -> usize {
        self.number_items
    }
}

/// Evaluates and validates input parameters for `ShortenDITL`.
///
/// Returns `None` if `dialog_ptr == 0`.
#[inline]
pub const fn evaluate_shorten_ditl_parameters(
    dialog_ptr: u32,
    number_items: usize,
) -> Option<ShortenDitlParameters> {
    if dialog_ptr == 0 {
        return None;
    }
    Some(ShortenDitlParameters {
        dialog_ptr,
        number_items,
    })
}

/// Evaluates `GetDialogKeyboardFocusItem` (Universal Interfaces 3.4.1 `Dialogs.h`).
///
/// Inside Macintosh Volume I, p. I-411:
/// `editField` in `DialogRecord` contains the 0-based item number of the editable text item
/// that currently has keyboard focus, or -1 (`DIALOG_INITIAL_EDIT_FIELD`) if none.
/// `GetDialogKeyboardFocusItem` returns the 1-based dialog item number (`editField + 1`),
/// or 0 if `dialog_ptr == 0` or no editable text item currently has focus.
#[inline]
#[must_use]
pub const fn evaluate_get_dialog_keyboard_focus_item(
    dialog_ptr: u32,
    edit_field: Option<i16>,
) -> i16 {
    if dialog_ptr == 0 {
        return 0;
    }
    match edit_field {
        Some(field) if field >= 0 => field.saturating_add(1),
        _ => 0,
    }
}

/// Canonical evaluated parameters for `SetDialogKeyboardFocusItem`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`:
/// Sets the active edit field in `DialogRecord` (`editField` at offset 164).
/// An `item_index <= 0` clears the focus (-1), while `item_index > 0` sets `editField = item_index - 1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetDialogKeyboardFocusItemParameters {
    dialog_ptr: u32,
    item_index: i16,
}

impl SetDialogKeyboardFocusItemParameters {
    /// Constructs a new `SetDialogKeyboardFocusItemParameters`.
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, item_index: i16) -> Self {
        Self {
            dialog_ptr,
            item_index,
        }
    }

    /// The target dialog pointer.
    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// The 1-based target dialog item index (or <= 0 to clear focus).
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn item_index(&self) -> i16 {
        self.item_index
    }

    /// The calculated 0-based `editField` value to write to `DialogRecord` (or -1 if clearing focus).
    #[inline]
    #[must_use]
    pub const fn target_edit_field(&self) -> i16 {
        if self.item_index <= 0 {
            DIALOG_INITIAL_EDIT_FIELD
        } else {
            self.item_index.saturating_sub(1)
        }
    }
}

/// Evaluates `SetDialogKeyboardFocusItem` parameters.
///
/// Returns `Ok(SetDialogKeyboardFocusItemParameters)` if `dialog_ptr != 0`,
/// or `Err(DIALOG_PARAM_ERR)` if `dialog_ptr == 0`.
#[inline]
pub const fn evaluate_set_dialog_keyboard_focus_item_parameters(
    dialog_ptr: u32,
    item_index: i16,
) -> Result<SetDialogKeyboardFocusItemParameters, i16> {
    if dialog_ptr == 0 {
        Err(DIALOG_PARAM_ERR)
    } else {
        Ok(SetDialogKeyboardFocusItemParameters::new(
            dialog_ptr,
            item_index,
        ))
    }
}

/// Evaluates `GetDialogTextEditHandle` (Universal Interfaces 3.4.1 `Dialogs.h`).
///
/// Inside Macintosh Volume I, p. I-411:
/// In `DialogRecord`, `textH` (offset 160) holds the handle to the `TERec` used for
/// active text editing. Returns `text_handle` if `dialog_ptr != 0`, or 0 if NULL.
#[inline]
#[must_use]
pub const fn evaluate_get_dialog_text_edit_handle(
    dialog_ptr: u32,
    text_handle: Option<u32>,
) -> u32 {
    if dialog_ptr == 0 {
        0
    } else {
        match text_handle {
            Some(handle) => handle,
            None => 0,
        }
    }
}

/// Evaluates `GetParamText` parameters.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`:
/// `PROCEDURE GetParamText(VAR param0, param1, param2, param3: Str255);`
#[inline]
#[must_use]
pub const fn evaluate_get_param_text_parameters(
    param0: u32,
    param1: u32,
    param2: u32,
    param3: u32,
) -> ParamTextParameters {
    ParamTextParameters::new(param0, param1, param2, param3)
}

/// Canonical evaluated parameters for `SetDialogTimeout`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`:
/// `OSStatus SetDialogTimeout(DialogRef inDialog, DialogItemIndex inButtonToPress, UInt32 inSecondsToWait);`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetDialogTimeoutParameters {
    dialog_ptr: u32,
    button_to_press: i16,
    seconds_to_wait: u32,
}

impl SetDialogTimeoutParameters {
    /// Constructs a new `SetDialogTimeoutParameters`.
    #[inline]
    #[must_use]
    pub const fn new(dialog_ptr: u32, button_to_press: i16, seconds_to_wait: u32) -> Self {
        Self {
            dialog_ptr,
            button_to_press,
            seconds_to_wait,
        }
    }

    /// The target dialog pointer.
    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// The button item index to press on timeout expiration.
    #[inline]
    #[must_use]
    pub const fn button_to_press(&self) -> i16 {
        self.button_to_press
    }

    /// The duration to wait before timeout expiration (in seconds).
    #[inline]
    #[must_use]
    pub const fn seconds_to_wait(&self) -> u32 {
        self.seconds_to_wait
    }
}

/// Evaluates `SetDialogTimeout` parameters.
///
/// Returns `Ok(SetDialogTimeoutParameters)` if `dialog_ptr != 0`,
/// or `Err(DIALOG_PARAM_ERR)` if `dialog_ptr == 0`.
#[inline]
pub const fn evaluate_set_dialog_timeout_parameters(
    dialog_ptr: u32,
    button_to_press: i16,
    seconds_to_wait: u32,
) -> Result<SetDialogTimeoutParameters, i16> {
    if dialog_ptr == 0 {
        Err(DIALOG_PARAM_ERR)
    } else {
        Ok(SetDialogTimeoutParameters::new(
            dialog_ptr,
            button_to_press,
            seconds_to_wait,
        ))
    }
}

/// Canonical evaluated parameters for `GetDialogTimeout`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`:
/// `OSStatus GetDialogTimeout(DialogRef inDialog, DialogItemIndex *outButtonToPress, UInt32 *outSecondsToWait, UInt32 *outSecondsRemaining);`
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetDialogTimeoutParameters {
    dialog_ptr: u32,
    out_button_ptr: u32,
    out_seconds_ptr: u32,
    out_remaining_ptr: u32,
}

impl GetDialogTimeoutParameters {
    /// Constructs a new `GetDialogTimeoutParameters`.
    #[inline]
    #[must_use]
    pub const fn new(
        dialog_ptr: u32,
        out_button_ptr: u32,
        out_seconds_ptr: u32,
        out_remaining_ptr: u32,
    ) -> Self {
        Self {
            dialog_ptr,
            out_button_ptr,
            out_seconds_ptr,
            out_remaining_ptr,
        }
    }

    /// The target dialog pointer.
    #[inline]
    #[must_use]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    /// The output pointer for the button item index.
    #[inline]
    #[must_use]
    pub const fn out_button_ptr(&self) -> u32 {
        self.out_button_ptr
    }

    /// The output pointer for the configured seconds duration.
    #[inline]
    #[must_use]
    pub const fn out_seconds_ptr(&self) -> u32 {
        self.out_seconds_ptr
    }

    /// The output pointer for the remaining seconds duration.
    #[inline]
    #[must_use]
    pub const fn out_remaining_ptr(&self) -> u32 {
        self.out_remaining_ptr
    }
}

/// Evaluates `GetDialogTimeout` parameters.
///
/// Returns `Ok(GetDialogTimeoutParameters)` if `dialog_ptr != 0` and all non-null output pointers are writable,
/// or `Err(DIALOG_PARAM_ERR)` otherwise.
pub fn evaluate_get_dialog_timeout_parameters(
    dialog_ptr: u32,
    out_button_ptr: u32,
    button_writable: bool,
    out_seconds_ptr: u32,
    seconds_writable: bool,
    out_remaining_ptr: u32,
    remaining_writable: bool,
) -> Result<GetDialogTimeoutParameters, i16> {
    if dialog_ptr == 0 {
        return Err(DIALOG_PARAM_ERR);
    }
    if (out_button_ptr != 0 && !button_writable)
        || (out_seconds_ptr != 0 && !seconds_writable)
        || (out_remaining_ptr != 0 && !remaining_writable)
    {
        return Err(DIALOG_PARAM_ERR);
    }
    Ok(GetDialogTimeoutParameters::new(
        dialog_ptr,
        out_button_ptr,
        out_seconds_ptr,
        out_remaining_ptr,
    ))
}

/// Evaluates the remaining countdown duration for a dialog timeout given start tick and current tick.
#[inline]
#[must_use]
pub const fn evaluate_dialog_timeout_remaining(
    seconds_to_wait: u32,
    start_tick: u32,
    current_tick: u32,
) -> u32 {
    if seconds_to_wait == 0 {
        0
    } else {
        let elapsed_ticks = current_tick.saturating_sub(start_tick);
        let elapsed_seconds = elapsed_ticks / 60;
        seconds_to_wait.saturating_sub(elapsed_seconds)
    }
}

/// Canonical evaluated parameters for `CreateStandardAlert` and `CreateStandardSheet`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreateStandardAlertParameters {
    alert_type: i16,
    error_ptr: u32,
    explanation_ptr: u32,
    alert_param_ptr: u32,
    out_alert_ptr: u32,
}

impl CreateStandardAlertParameters {
    pub const fn new(
        alert_type: i16,
        error_ptr: u32,
        explanation_ptr: u32,
        alert_param_ptr: u32,
        out_alert_ptr: u32,
    ) -> Self {
        Self {
            alert_type,
            error_ptr,
            explanation_ptr,
            alert_param_ptr,
            out_alert_ptr,
        }
    }

    #[allow(dead_code)]
    pub const fn alert_type(&self) -> i16 {
        self.alert_type
    }

    #[allow(dead_code)]
    pub const fn error_ptr(&self) -> u32 {
        self.error_ptr
    }

    #[allow(dead_code)]
    pub const fn explanation_ptr(&self) -> u32 {
        self.explanation_ptr
    }

    #[allow(dead_code)]
    pub const fn alert_param_ptr(&self) -> u32 {
        self.alert_param_ptr
    }

    #[allow(dead_code)]
    pub const fn out_alert_ptr(&self) -> u32 {
        self.out_alert_ptr
    }
}

/// Evaluates parameters for `CreateStandardAlert` and `CreateStandardSheet`.
#[inline]
pub fn evaluate_create_standard_alert_parameters(
    alert_type: i16,
    error_ptr: u32,
    explanation_ptr: u32,
    alert_param_ptr: u32,
    out_alert_ptr: u32,
    can_write: bool,
) -> Result<CreateStandardAlertParameters, i16> {
    if out_alert_ptr == 0 || !can_write {
        return Err(DIALOG_PARAM_ERR);
    }
    Ok(CreateStandardAlertParameters::new(
        alert_type,
        error_ptr,
        explanation_ptr,
        alert_param_ptr,
        out_alert_ptr,
    ))
}

/// Canonical evaluated parameters for `RunStandardAlert`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunStandardAlertParameters {
    dialog_ptr: u32,
    filter_proc: u32,
    out_item_hit_ptr: u32,
}

impl RunStandardAlertParameters {
    pub const fn new(dialog_ptr: u32, filter_proc: u32, out_item_hit_ptr: u32) -> Self {
        Self {
            dialog_ptr,
            filter_proc,
            out_item_hit_ptr,
        }
    }

    #[allow(dead_code)]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    #[allow(dead_code)]
    pub const fn filter_proc(&self) -> u32 {
        self.filter_proc
    }

    #[allow(dead_code)]
    pub const fn out_item_hit_ptr(&self) -> u32 {
        self.out_item_hit_ptr
    }
}

/// Evaluates parameters for `RunStandardAlert`.
#[inline]
pub fn evaluate_run_standard_alert_parameters(
    dialog_ptr: u32,
    filter_proc: u32,
    out_item_hit_ptr: u32,
    can_write: bool,
) -> Result<RunStandardAlertParameters, i16> {
    if dialog_ptr == 0 || out_item_hit_ptr == 0 || !can_write {
        return Err(DIALOG_PARAM_ERR);
    }
    Ok(RunStandardAlertParameters::new(
        dialog_ptr,
        filter_proc,
        out_item_hit_ptr,
    ))
}

/// Canonical evaluated parameters for `CloseStandardSheet`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseStandardSheetParameters {
    sheet_ptr: u32,
    result_command: u32,
}

impl CloseStandardSheetParameters {
    pub const fn new(sheet_ptr: u32, result_command: u32) -> Self {
        Self {
            sheet_ptr,
            result_command,
        }
    }

    #[allow(dead_code)]
    pub const fn sheet_ptr(&self) -> u32 {
        self.sheet_ptr
    }

    #[allow(dead_code)]
    pub const fn result_command(&self) -> u32 {
        self.result_command
    }
}

/// Evaluates parameters for `CloseStandardSheet`.
#[inline]
pub fn evaluate_close_standard_sheet_parameters(
    sheet_ptr: u32,
    result_command: u32,
) -> Result<CloseStandardSheetParameters, i16> {
    if sheet_ptr == 0 {
        return Err(DIALOG_PARAM_ERR);
    }
    Ok(CloseStandardSheetParameters::new(sheet_ptr, result_command))
}

/// Canonical evaluated parameters for `GetStandardAlertDefaultParams`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetStandardAlertDefaultParamsParameters {
    param_ptr: u32,
    version: u32,
}

impl GetStandardAlertDefaultParamsParameters {
    pub const fn new(param_ptr: u32, version: u32) -> Self {
        Self { param_ptr, version }
    }

    #[allow(dead_code)]
    pub const fn param_ptr(&self) -> u32 {
        self.param_ptr
    }

    #[allow(dead_code)]
    pub const fn version(&self) -> u32 {
        self.version
    }
}

/// Evaluates parameters for `GetStandardAlertDefaultParams`.
#[inline]
pub fn evaluate_get_standard_alert_default_params_parameters(
    param_ptr: u32,
    version: u32,
    can_write: bool,
) -> Result<GetStandardAlertDefaultParamsParameters, i16> {
    if param_ptr == 0 || !can_write || version != STD_CFSTRING_ALERT_VERSION_ONE {
        return Err(DIALOG_PARAM_ERR);
    }
    Ok(GetStandardAlertDefaultParamsParameters::new(
        param_ptr, version,
    ))
}

/// Canonical evaluated parameters for `GetModalDialogEventMask`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetModalDialogEventMaskParameters {
    dialog_ptr: u32,
    out_mask_ptr: u32,
}

impl GetModalDialogEventMaskParameters {
    pub const fn new(dialog_ptr: u32, out_mask_ptr: u32) -> Self {
        Self {
            dialog_ptr,
            out_mask_ptr,
        }
    }

    #[allow(dead_code)]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    #[allow(dead_code)]
    pub const fn out_mask_ptr(&self) -> u32 {
        self.out_mask_ptr
    }
}

/// Evaluates parameters for `GetModalDialogEventMask`.
#[inline]
pub fn evaluate_get_modal_dialog_event_mask_parameters(
    dialog_ptr: u32,
    out_mask_ptr: u32,
    can_write: bool,
) -> Result<GetModalDialogEventMaskParameters, i16> {
    if dialog_ptr == 0 || out_mask_ptr == 0 || !can_write {
        return Err(DIALOG_PARAM_ERR);
    }
    Ok(GetModalDialogEventMaskParameters::new(
        dialog_ptr,
        out_mask_ptr,
    ))
}

/// Canonical evaluated parameters for `SetModalDialogEventMask`.
///
/// Universal Interfaces 3.4.1 `Dialogs.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetModalDialogEventMaskParameters {
    dialog_ptr: u32,
    mask: u16,
}

impl SetModalDialogEventMaskParameters {
    pub const fn new(dialog_ptr: u32, mask: u16) -> Self {
        Self { dialog_ptr, mask }
    }

    #[allow(dead_code)]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    #[allow(dead_code)]
    pub const fn mask(&self) -> u16 {
        self.mask
    }
}

/// Evaluates parameters for `SetModalDialogEventMask`.
#[inline]
pub fn evaluate_set_modal_dialog_event_mask_parameters(
    dialog_ptr: u32,
    mask: u16,
) -> Result<SetModalDialogEventMaskParameters, i16> {
    if dialog_ptr == 0 {
        return Err(DIALOG_PARAM_ERR);
    }
    Ok(SetModalDialogEventMaskParameters::new(dialog_ptr, mask))
}

/// Canonical evaluated parameters for `FlashDialogControl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashDialogControlParameters {
    dialog_ptr: u32,
    item_index: i16,
}

impl FlashDialogControlParameters {
    pub const fn new(dialog_ptr: u32, item_index: i16) -> Self {
        Self {
            dialog_ptr,
            item_index,
        }
    }

    #[allow(dead_code)]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    #[allow(dead_code)]
    pub const fn item_index(&self) -> i16 {
        self.item_index
    }
}

/// Evaluates parameters for `FlashDialogControl`.
#[inline]
pub fn evaluate_flash_dialog_control_parameters(
    dialog_ptr: u32,
    item_index: i16,
) -> Result<FlashDialogControlParameters, i16> {
    if dialog_ptr == 0 || item_index <= 0 {
        return Err(DIALOG_PARAM_ERR);
    }
    Ok(FlashDialogControlParameters::new(dialog_ptr, item_index))
}

/// Canonical evaluated parameters for `GetDialogItemInit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetDialogItemInitParameters {
    dialog_ptr: u32,
    item_index: i16,
    out_type_ptr: u32,
    out_handle_ptr: u32,
    out_rect_ptr: u32,
}

impl GetDialogItemInitParameters {
    pub const fn new(
        dialog_ptr: u32,
        item_index: i16,
        out_type_ptr: u32,
        out_handle_ptr: u32,
        out_rect_ptr: u32,
    ) -> Self {
        Self {
            dialog_ptr,
            item_index,
            out_type_ptr,
            out_handle_ptr,
            out_rect_ptr,
        }
    }

    #[allow(dead_code)]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    #[allow(dead_code)]
    pub const fn item_index(&self) -> i16 {
        self.item_index
    }

    #[allow(dead_code)]
    pub const fn out_type_ptr(&self) -> u32 {
        self.out_type_ptr
    }

    #[allow(dead_code)]
    pub const fn out_handle_ptr(&self) -> u32 {
        self.out_handle_ptr
    }

    #[allow(dead_code)]
    pub const fn out_rect_ptr(&self) -> u32 {
        self.out_rect_ptr
    }
}

/// Evaluates parameters for `GetDialogItemInit`.
#[inline]
pub fn evaluate_get_dialog_item_init_parameters(
    dialog_ptr: u32,
    item_index: i16,
    out_type_ptr: u32,
    out_handle_ptr: u32,
    out_rect_ptr: u32,
) -> Result<GetDialogItemInitParameters, i16> {
    if dialog_ptr == 0 || item_index <= 0 {
        return Err(DIALOG_PARAM_ERR);
    }
    Ok(GetDialogItemInitParameters::new(
        dialog_ptr,
        item_index,
        out_type_ptr,
        out_handle_ptr,
        out_rect_ptr,
    ))
}

/// Canonical evaluated parameters for `SetDialogFilter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetDialogFilterParameters {
    dialog_ptr: u32,
    filter_proc: u32,
}

impl SetDialogFilterParameters {
    pub const fn new(dialog_ptr: u32, filter_proc: u32) -> Self {
        Self {
            dialog_ptr,
            filter_proc,
        }
    }

    #[allow(dead_code)]
    pub const fn dialog_ptr(&self) -> u32 {
        self.dialog_ptr
    }

    #[allow(dead_code)]
    pub const fn filter_proc(&self) -> u32 {
        self.filter_proc
    }
}

/// Evaluates parameters for `SetDialogFilter`.
#[inline]
pub fn evaluate_set_dialog_filter_parameters(
    dialog_ptr: u32,
    filter_proc: u32,
) -> Result<SetDialogFilterParameters, i16> {
    if dialog_ptr == 0 {
        return Err(DIALOG_PARAM_ERR);
    }
    Ok(SetDialogFilterParameters::new(dialog_ptr, filter_proc))
}

/// Returns true if two rectangles intersect.
pub fn rects_intersect(a: (i16, i16, i16, i16), b: (i16, i16, i16, i16)) -> bool {
    a.0 < b.2 && a.2 > b.0 && a.1 < b.3 && a.3 > b.1
}

/// Returns true if a dialog item's local rectangle intersects the dialog global bounds.
pub fn dialog_item_intersects_bounds(
    bounds: (i16, i16, i16, i16),
    item_rect: (i16, i16, i16, i16),
) -> bool {
    rects_intersect(dialog_rect_to_global(bounds, item_rect), bounds)
}

/// Returns the effective global update rectangle given dialog bounds and an update rectangle.
///
/// If `update_rect` already intersects `bounds` (or is in global coordinates), it is used as-is.
/// Otherwise, `update_rect` is assumed to be dialog-local coordinates and is converted to global coordinates.
pub fn dialog_effective_update_rect(
    bounds: (i16, i16, i16, i16),
    update_rect: (i16, i16, i16, i16),
) -> (i16, i16, i16, i16) {
    if rects_intersect(update_rect, bounds) {
        update_rect
    } else {
        dialog_rect_to_global(bounds, update_rect)
    }
}

/// Returns true if a dialog item's local rectangle intersects the dialog bounds and an optional update rectangle.
pub fn dialog_item_intersects_draw_area(
    bounds: (i16, i16, i16, i16),
    item_rect: (i16, i16, i16, i16),
    update_rect: Option<(i16, i16, i16, i16)>,
) -> bool {
    if !dialog_item_intersects_bounds(bounds, item_rect) {
        return false;
    }
    match update_rect {
        Some(rect) => {
            let global_rect = dialog_rect_to_global(bounds, item_rect);
            let effective_rect = dialog_effective_update_rect(bounds, rect);
            rects_intersect(global_rect, effective_rect)
        }
        None => true,
    }
}

/// Returns the 1-based dialog item numbers for all user items with an installed callback procedure
/// that intersect the dialog bounds and optional update rectangle.
///
/// Inside Macintosh Volume I (1985), pp. I-405, I-415, I-417;
/// Macintosh Toolbox Essentials (1992), pp. 6-142--6-143.
pub fn evaluate_dialog_user_item_numbers<I>(
    items: I,
    bounds: (i16, i16, i16, i16),
    update_rect: Option<(i16, i16, i16, i16)>,
) -> Vec<usize>
where
    I: IntoIterator<Item = (bool, (i16, i16, i16, i16))>,
{
    items
        .into_iter()
        .enumerate()
        .filter_map(|(index, (has_user_proc, item_rect))| {
            if has_user_proc && dialog_item_intersects_draw_area(bounds, item_rect, update_rect) {
                Some(index + 1)
            } else {
                None
            }
        })
        .collect()
}

/// Checks whether an in-bounds dialog is game-managed (all items intersecting bounds are user items).
pub fn is_dialog_game_managed<I>(bounds: (i16, i16, i16, i16), items: I) -> bool
where
    I: IntoIterator<Item = (u8, (i16, i16, i16, i16))>,
{
    let mut has_visible_item = false;
    for (item_type, item_rect) in items {
        if !dialog_item_intersects_bounds(bounds, item_rect) {
            continue;
        }
        has_visible_item = true;
        if !is_dialog_item_user_item(item_type) {
            return false;
        }
    }
    has_visible_item
}

/// Canonical evaluated parameters for `IsDialogEvent`.
///
/// Inside Macintosh Volume I, p. I-416;
/// Macintosh Toolbox Essentials (1992), p. 6-138.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IsDialogEventParameters {
    event_ptr: u32,
}

#[allow(dead_code)]
impl IsDialogEventParameters {
    /// Constructs a new `IsDialogEventParameters`.
    #[inline]
    pub const fn new(event_ptr: u32) -> Self {
        Self { event_ptr }
    }

    /// The guest pointer to the `EventRecord`.
    #[inline]
    pub const fn event_ptr(&self) -> u32 {
        self.event_ptr
    }

    /// Whether the pointer is non-null.
    #[inline]
    pub const fn is_valid(&self) -> bool {
        self.event_ptr != 0
    }
}

/// Evaluates and validates parameters for `IsDialogEvent`.
///
/// Returns `None` if `event_ptr == 0`.
#[inline]
pub const fn evaluate_is_dialog_event_parameters(event_ptr: u32) -> Option<IsDialogEventParameters> {
    if event_ptr == 0 {
        return None;
    }
    Some(IsDialogEventParameters { event_ptr })
}

/// Tests whether an event should be handled as part of an active modeless or movable modal dialog.
///
/// Inside Macintosh Volume I (1985), p. I-416;
/// Macintosh Toolbox Essentials (1992), p. 6-138:
/// - If there is no target dialog corresponding to the event, returns `false`.
/// - For `updateEvt` (6) and `activateEvt` (8), returns `true` if `message == dialog_ptr`.
/// - For `mouseDown` (1), returns `true` if the mouse point `(where_v, where_h)` falls within the dialog window bounds.
/// - For all other event types directed to the dialog, returns `true`.
pub fn is_dialog_event(
    what: u16,
    message: u32,
    where_v: i16,
    where_h: i16,
    target_dialog: Option<u32>,
    dialog_bounds: Option<(i16, i16, i16, i16)>,
) -> bool {
    let Some(dialog_ptr) = target_dialog else {
        return false;
    };
    match what {
        EVENT_UPDATE | EVENT_ACTIVATE => message == dialog_ptr,
        EVENT_MOUSE_DOWN => {
            dialog_bounds.is_some_and(|bounds| rect_contains_point(bounds, where_v, where_h))
        }
        _ => true,
    }
}

/// Validates whether a character is an editable character accepted by dialog edit fields.
///
/// Macintosh Toolbox Essentials (1992), p. 6-139:
/// Accepts backspace (0x08) and printable ASCII / Mac Roman characters (0x20..=0x7E).
pub fn is_dialog_edit_text_character(character: u8) -> bool {
    matches!(character, 0x08 | 0x20..=0x7E)
}

/// Describes the action that the host environment must take in response to a `DialogSelect` event.
///
/// Inside Macintosh Volume I, pp. I-417--I-418;
/// Macintosh Toolbox Essentials (1992), pp. 6-139--6-141.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogSelectAction {
    /// The event is not targeted at an active dialog, fell outside the dialog window,
    /// or was otherwise ignored by the Dialog Manager.
    ///
    /// Corresponds to `DialogSelect` returning `FALSE` (0).
    NoAction,

    /// An update event (`updateEvt`) targeted at `dialog`.
    ///
    /// The caller must redraw the dialog window contents.
    /// `theDialog` out-pointer is written with `dialog`.
    /// Returns `FALSE` (0).
    Update { dialog: u32 },

    /// An activate event (`activateEvt`) targeted at `dialog`.
    ///
    /// `theDialog` out-pointer is written with `dialog`.
    /// Returns `FALSE` (0).
    Activate { dialog: u32 },

    /// A null event (`nullEvent`) targeted at `dialog` while an editable text item is active.
    ///
    /// The caller must call `TEIdle` on the dialog's TextEdit record to blink the insertion caret.
    /// Returns `FALSE` (0).
    Idle { dialog: u32, edit_item: i16 },

    /// A mouse-down event (`mouseDown`) hit an enabled dialog item.
    ///
    /// The caller must:
    /// - If `is_edit_text`: activate or focus the edit field (e.g. `TEClick`).
    /// - If `is_resource_control`: track the control (e.g. scroll bar tracking).
    /// - Write `theDialog = dialog` and `itemHit = item_no`.
    /// Returns `TRUE` (1).
    ItemHit {
        dialog: u32,
        item_no: i16,
        is_edit_text: bool,
        is_resource_control: bool,
    },

    /// A mouse-down event (`mouseDown`) hit a disabled dialog item.
    ///
    /// Does not activate or return the item.
    /// Returns `FALSE` (0).
    DisabledItemHit { dialog: u32, item_no: i16 },

    /// A key-down or auto-key event directed to an active enabled editable text field.
    ///
    /// The caller must process the character via TextEdit (e.g. `TEKey`) and write:
    /// - `theDialog = dialog` and `itemHit = edit_item`.
    /// Returns `TRUE` (1).
    KeyStroke {
        dialog: u32,
        edit_item: i16,
        character: u8,
    },
}

#[allow(dead_code)]
impl DialogSelectAction {
    /// Whether this action represents a handled event that causes `DialogSelect` to return `TRUE` (1).
    #[inline]
    pub const fn is_handled(self) -> bool {
        matches!(self, Self::ItemHit { .. } | Self::KeyStroke { .. })
    }

    /// Whether this action sets `theDialog` output pointer if one was provided.
    #[inline]
    pub const fn should_set_dialog_ptr(self) -> bool {
        matches!(
            self,
            Self::Update { .. }
                | Self::Activate { .. }
                | Self::ItemHit { .. }
                | Self::KeyStroke { .. }
        )
    }

    /// The target dialog pointer if this action affects a dialog box.
    #[inline]
    pub const fn target_dialog(self) -> Option<u32> {
        match self {
            Self::NoAction => None,
            Self::Update { dialog }
            | Self::Activate { dialog }
            | Self::Idle { dialog, .. }
            | Self::ItemHit { dialog, .. }
            | Self::DisabledItemHit { dialog, .. }
            | Self::KeyStroke { dialog, .. } => Some(dialog),
        }
    }

    /// The affected 1-indexed item number if this action reports an item hit (`ItemHit` or `KeyStroke`).
    #[inline]
    pub const fn item_hit(self) -> Option<i16> {
        match self {
            Self::ItemHit { item_no, .. } => Some(item_no),
            Self::KeyStroke { edit_item, .. } => Some(edit_item),
            _ => None,
        }
    }
}

/// Canonical evaluated parameters for `DialogSelect`.
///
/// Inside Macintosh Volume I, pp. I-417--I-418;
/// Macintosh Toolbox Essentials (1992), pp. 6-139--6-141.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialogSelectParameters {
    event_ptr: u32,
    dialog_out_ptr: u32,
    item_hit_ptr: u32,
}

#[allow(dead_code)]
impl DialogSelectParameters {
    /// Constructs a new `DialogSelectParameters`.
    #[inline]
    pub const fn new(event_ptr: u32, dialog_out_ptr: u32, item_hit_ptr: u32) -> Self {
        Self {
            event_ptr,
            dialog_out_ptr,
            item_hit_ptr,
        }
    }

    /// The guest pointer to the input `EventRecord`.
    #[inline]
    pub const fn event_ptr(&self) -> u32 {
        self.event_ptr
    }

    /// The guest pointer to the output `DialogPtr` location.
    #[inline]
    pub const fn dialog_out_ptr(&self) -> u32 {
        self.dialog_out_ptr
    }

    /// The guest pointer to the output `itemHit` location.
    #[inline]
    pub const fn item_hit_ptr(&self) -> u32 {
        self.item_hit_ptr
    }

    /// Whether a non-null output `DialogPtr` location was provided.
    #[inline]
    pub const fn has_dialog_out(&self) -> bool {
        self.dialog_out_ptr != 0
    }

    /// Whether a non-null output `itemHit` location was provided.
    #[inline]
    pub const fn has_item_hit_out(&self) -> bool {
        self.item_hit_ptr != 0
    }
}

/// Evaluates and validates parameters for `DialogSelect`.
///
/// Returns `None` if `event_ptr == 0`.
#[inline]
pub const fn evaluate_dialog_select_parameters(
    event_ptr: u32,
    dialog_out_ptr: u32,
    item_hit_ptr: u32,
) -> Option<DialogSelectParameters> {
    if event_ptr == 0 {
        return None;
    }
    Some(DialogSelectParameters {
        event_ptr,
        dialog_out_ptr,
        item_hit_ptr,
    })
}

/// Evaluates an event record within the context of `DialogSelect`.
///
/// Macintosh Toolbox Essentials (1992), pp. 6-139--6-141:
/// - Update and activate events route to the named window (if it is a dialog).
/// - Null events advance the insertion-caret blink in active editable text fields via `Idle`.
/// - Mouse-down events inside the dialog bounds are hit-tested against dialog items.
/// - Key-down and auto-key events are forwarded to the active editable text field.
pub fn evaluate_dialog_select<F>(
    what: u16,
    message: u32,
    where_v: i16,
    where_h: i16,
    target_dialog: Option<u32>,
    dialog_bounds: Option<(i16, i16, i16, i16)>,
    active_edit_item: Option<(i16, u8)>,
    hit_test: F,
) -> DialogSelectAction
where
    F: FnOnce(i16, i16) -> Option<(i16, u8)>,
{
    let Some(dialog) = target_dialog else {
        return DialogSelectAction::NoAction;
    };

    match what {
        EVENT_UPDATE if message == dialog => DialogSelectAction::Update { dialog },
        EVENT_ACTIVATE if message == dialog => DialogSelectAction::Activate { dialog },
        EVENT_NULL => {
            if let Some((edit_item, raw_type)) = active_edit_item {
                if edit_item > 0
                    && is_dialog_item_enabled(raw_type)
                    && is_dialog_item_edit_text(raw_type)
                {
                    return DialogSelectAction::Idle { dialog, edit_item };
                }
            }
            DialogSelectAction::NoAction
        }
        EVENT_MOUSE_DOWN => {
            let Some(bounds) = dialog_bounds else {
                return DialogSelectAction::NoAction;
            };
            if !rect_contains_point(bounds, where_v, where_h) {
                return DialogSelectAction::NoAction;
            }
            if let Some((item_no, raw_type)) = hit_test(where_v, where_h) {
                if item_no > 0 {
                    if is_dialog_item_enabled(raw_type) {
                        return DialogSelectAction::ItemHit {
                            dialog,
                            item_no,
                            is_edit_text: is_dialog_item_edit_text(raw_type),
                            is_resource_control: is_dialog_item_resource_control(raw_type),
                        };
                    } else {
                        return DialogSelectAction::DisabledItemHit { dialog, item_no };
                    }
                }
            }
            DialogSelectAction::NoAction
        }
        EVENT_KEY_DOWN | EVENT_AUTO_KEY => {
            if let Some((edit_item, raw_type)) = active_edit_item {
                if edit_item > 0
                    && is_dialog_item_enabled(raw_type)
                    && is_dialog_item_edit_text(raw_type)
                {
                    let character = (message & 0xFF) as u8;
                    if is_dialog_edit_text_character(character) {
                        return DialogSelectAction::KeyStroke {
                            dialog,
                            edit_item,
                            character,
                        };
                    }
                }
            }
            DialogSelectAction::NoAction
        }
        _ => DialogSelectAction::NoAction,
    }
}

/// Calculates the updated text bytes and new insertion caret offset after processing a key press
/// (such as backspace or printable character) against an existing text buffer and selection range.
///
/// Macintosh Toolbox Essentials (1992), pp. 2-83--2-88, 6-139.
pub fn textedit_key_result(
    existing: &[u8],
    sel_start: usize,
    sel_end: usize,
    key: u8,
) -> (Vec<u8>, usize) {
    let text_len = existing.len();
    let s = sel_start.min(text_len);
    let e = sel_end.min(text_len);
    let (s, e) = if s > e { (e, s) } else { (s, e) };

    if key == 0x08 {
        if s != e {
            let mut merged = Vec::with_capacity(text_len - (e - s));
            merged.extend_from_slice(&existing[..s]);
            merged.extend_from_slice(&existing[e..]);
            return (merged, s);
        }
        if s > 0 {
            let mut merged = Vec::with_capacity(text_len - 1);
            merged.extend_from_slice(&existing[..s - 1]);
            merged.extend_from_slice(&existing[s..]);
            return (merged, s - 1);
        }
        return (existing.to_vec(), s);
    }

    let mut merged = Vec::with_capacity(s + 1 + text_len.saturating_sub(e));
    merged.extend_from_slice(&existing[..s]);
    merged.push(key);
    merged.extend_from_slice(&existing[e..]);
    (merged, s + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_kind_decoding_and_properties() {
        assert_eq!(DialogItemKind::from_raw_type(4), DialogItemKind::Button);
        assert_eq!(
            DialogItemKind::from_raw_type(4 | DIALOG_ITEM_DISABLED_FLAG),
            DialogItemKind::Button
        );
        assert!(DialogItemKind::Button.is_control());
        assert!(!DialogItemKind::Button.is_text());

        assert_eq!(DialogItemKind::from_raw_type(8), DialogItemKind::StaticText);
        assert!(DialogItemKind::StaticText.is_text());
        assert!(!DialogItemKind::StaticText.is_control());

        assert_eq!(DialogItemKind::from_raw_type(16), DialogItemKind::EditText);
        assert_eq!(DialogItemKind::from_raw_type(32), DialogItemKind::Icon);
        assert_eq!(DialogItemKind::from_raw_type(64), DialogItemKind::Picture);
        assert_eq!(DialogItemKind::from_raw_type(0), DialogItemKind::UserItem);
        assert_eq!(
            DialogItemKind::from_raw_type(99),
            DialogItemKind::Unknown(99)
        );
    }

    #[test]
    fn parse_ditl_items_round_trip() {
        let mut data = Vec::new();
        // Item count minus 1: 1 item (0x0000)
        data.extend_from_slice(&0i16.to_be_bytes());
        // Handle: 0
        data.extend_from_slice(&0u32.to_be_bytes());
        // Rect: (10, 20, 30, 40)
        data.extend_from_slice(&10i16.to_be_bytes());
        data.extend_from_slice(&20i16.to_be_bytes());
        data.extend_from_slice(&30i16.to_be_bytes());
        data.extend_from_slice(&40i16.to_be_bytes());
        // Type: Button (4), Length: 2, Payload: "OK"
        data.push(DIALOG_ITEM_BUTTON);
        data.push(2);
        data.extend_from_slice(b"OK");

        let items = parse_ditl_items(&data).expect("valid DITL");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].rect, (10, 20, 30, 40));
        assert_eq!(items[0].kind(), DialogItemKind::Button);
        assert!(items[0].is_enabled());
        assert_eq!(items[0].text(), "OK");
    }

    #[test]
    fn position_dialog_centering() {
        let bounds = (0, 0, 100, 200);
        // Centered position flag 0x280a
        let centered = position_dialog_bounds(bounds, 0x280a, 640, 480);
        assert_eq!(centered, (190, 220, 290, 420));

        // Non-centered flag leaves bounds unchanged
        let uncentered = position_dialog_bounds(bounds, 0x0000, 640, 480);
        assert_eq!(uncentered, bounds);
    }

    #[test]
    fn parse_ditl_items_empty_and_truncated() {
        let mut empty_data = Vec::new();
        empty_data.extend_from_slice(&(-1i16).to_be_bytes());
        let empty_items = parse_ditl_items(&empty_data).expect("valid empty DITL");
        assert!(empty_items.is_empty());
        assert_eq!(ditl_total_used_len(&empty_data), 2);

        // Truncated item: count 2, first valid button, second item truncated
        let mut data = Vec::new();
        data.extend_from_slice(&1i16.to_be_bytes()); // count-1 = 1 (2 items)
                                                     // Item 1: Button "OK"
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&[0, 1, 0, 2, 0, 11, 0, 42]);
        data.push(DIALOG_ITEM_BUTTON);
        data.push(2);
        data.extend_from_slice(b"OK");
        // Item 2: StatText declaring 6 bytes, but only 3 provided
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&[0, 12, 0, 2, 0, 22, 0, 72]);
        data.push(DIALOG_ITEM_STATIC_TEXT);
        data.push(6);
        data.extend_from_slice(b"Bad");

        let items = parse_ditl_items(&data).expect("parses valid items before truncated one");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text(), "OK");
    }

    #[test]
    fn parse_ditl_items_resource_ids_and_record_lengths() {
        let mut data = Vec::new();
        data.extend_from_slice(&1i16.to_be_bytes()); // 2 items
                                                     // Item 1: resCtrl (7) with reserved byte 0 followed by 128
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&[0, 1, 0, 2, 0, 11, 0, 42]);
        data.push(DIALOG_ITEM_RESOURCE_CONTROL);
        data.push(0); // reserved byte per MTE
        data.extend_from_slice(&128i16.to_be_bytes());

        // Item 2: icon (32) with length byte 2 followed by -42
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&[0, 12, 0, 2, 0, 28, 0, 34]);
        data.push(DIALOG_ITEM_ICON);
        data.push(2); // length byte 2 per IM:I
        data.extend_from_slice(&(-42i16).to_be_bytes());

        let len1 = ditl_item_record_len_at(&data, 2).expect("record 1 len");
        assert_eq!(len1, 16);
        let len2 = ditl_item_record_len_at(&data, 2 + len1).expect("record 2 len");
        assert_eq!(len2, 16);
        assert_eq!(ditl_total_used_len(&data), 2 + len1 + len2);

        let items = parse_ditl_items(&data).expect("valid items");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind(), DialogItemKind::ResourceControl);
        assert_eq!(items[0].resource_id(), Some(128));
        assert_eq!(items[1].kind(), DialogItemKind::Icon);
        assert_eq!(items[1].resource_id(), Some(-42));
    }

    #[test]
    fn dialog_item_rect_geometry_and_visibility() {
        let initial_rect = (10, 20, 30, 80);
        let offset = offset_rect(initial_rect, 5, -10);
        assert_eq!(offset, (15, 10, 35, 70));

        let bounds = (100, 200, 300, 500);
        let global = dialog_rect_to_global(bounds, initial_rect);
        assert_eq!(global, (110, 220, 130, 280));
        let local = dialog_rect_to_local(bounds, global);
        assert_eq!(local, initial_rect);

        assert!(!is_dialog_item_rect_hidden(initial_rect));
        let hidden = hide_dialog_item_rect(initial_rect);
        assert_eq!(hidden, (10, 16404, 30, 16464));
        assert!(is_dialog_item_rect_hidden(hidden));
        let shown = show_dialog_item_rect(hidden);
        assert_eq!(shown, initial_rect);

        // Enclosing rect: standard controls match exactly; editText extends 3px on all sides
        assert_eq!(
            dialog_item_enclosing_rect(DIALOG_ITEM_BUTTON, initial_rect),
            initial_rect
        );
        assert_eq!(
            dialog_item_enclosing_rect(DIALOG_ITEM_EDIT_TEXT, initial_rect),
            (7, 17, 33, 83)
        );

        // DialogItemRecord helpers
        let mut record = DialogItemRecord {
            item_offset: 2,
            item_type: DIALOG_ITEM_EDIT_TEXT,
            rect: initial_rect,
            handle: 0,
            payload: vec![],
        };
        assert!(!record.is_hidden());
        assert_eq!(record.enclosing_rect(), (7, 17, 33, 83));
        record.offset_rect(10, 20);
        assert_eq!(record.rect, (20, 40, 40, 100));
    }

    #[test]
    fn dialog_item_point_hit_testing_and_ditl_byte_offsetting() {
        let rect1 = (10, 20, 30, 50);
        let rect2 = (30, 20, 50, 50);
        let rects = [rect1, rect2];

        // rect_contains_point is half-open: [top, bottom) and [left, right)
        assert!(rect_contains_point(rect1, 10, 20));
        assert!(rect_contains_point(rect1, 29, 49));
        assert!(!rect_contains_point(rect1, 30, 20));
        assert!(!rect_contains_point(rect1, 10, 50));

        // find_dialog_item_at_local_point
        assert_eq!(find_dialog_item_at_local_point(&rects, 15, 25), Some(0));
        assert_eq!(find_dialog_item_at_local_point(&rects, 35, 25), Some(1));
        assert_eq!(find_dialog_item_at_local_point(&rects, 5, 5), None);

        // offset_ditl_bytes
        let mut data = Vec::new();
        data.extend_from_slice(&0i16.to_be_bytes()); // 1 item
        data.extend_from_slice(&0u32.to_be_bytes()); // handle at 2
        data.extend_from_slice(&10i16.to_be_bytes()); // top at 6
        data.extend_from_slice(&20i16.to_be_bytes()); // left at 8
        data.extend_from_slice(&30i16.to_be_bytes()); // bottom at 10
        data.extend_from_slice(&40i16.to_be_bytes()); // right at 12
        data.push(DIALOG_ITEM_BUTTON);
        data.push(0);

        offset_ditl_bytes(&mut data, [2], 5, 10);
        assert_eq!(i16::from_be_bytes([data[6], data[7]]), 15);
        assert_eq!(i16::from_be_bytes([data[8], data[9]]), 30);
        assert_eq!(i16::from_be_bytes([data[10], data[11]]), 35);
        assert_eq!(i16::from_be_bytes([data[12], data[13]]), 50);
    }

    #[test]
    fn dialog_template_parsing_and_positioning() {
        // Classic 22-byte DLOG (empty title, no position word)
        let mut dlog22 = Vec::new();
        dlog22.extend_from_slice(&228i16.to_be_bytes()); // top
        dlog22.extend_from_slice(&198i16.to_be_bytes()); // left
        dlog22.extend_from_slice(&372i16.to_be_bytes()); // bottom
        dlog22.extend_from_slice(&455i16.to_be_bytes()); // right
        dlog22.extend_from_slice(&2i16.to_be_bytes()); // proc_id
        dlog22.push(1); // visible
        dlog22.push(0); // filler
        dlog22.push(0); // go_away
        dlog22.push(0); // filler
        dlog22.extend_from_slice(&0u32.to_be_bytes()); // ref_con
        dlog22.extend_from_slice(&1013i16.to_be_bytes()); // items_id
        dlog22.push(0); // empty title length
        dlog22.push(0); // padding

        let template = parse_dialog_template(&dlog22).expect("valid 22-byte dlog");
        assert_eq!(template.bounds, (228, 198, 372, 455));
        assert_eq!(template.proc_id, 2);
        assert!(template.visible);
        assert!(!template.go_away);
        assert_eq!(template.ref_con, 0);
        assert_eq!(template.items_id, 1013);
        assert_eq!(template.title, b"");
        assert_eq!(template.title_string(), "");
        assert_eq!(template.position, 0);

        // 24-byte DLOG with position word 0x280A
        let mut dlog24 = dlog22.clone();
        dlog24.extend_from_slice(&0x280Au16.to_be_bytes());
        let template = parse_dialog_template(&dlog24).expect("valid 24-byte dlog");
        assert_eq!(template.position, 0x280A);

        // DLOG with odd title ("Odd" -> 3 bytes, padded to 24, position at 24..26)
        let mut dlog_odd = Vec::new();
        dlog_odd.extend_from_slice(&10i16.to_be_bytes());
        dlog_odd.extend_from_slice(&20i16.to_be_bytes());
        dlog_odd.extend_from_slice(&110i16.to_be_bytes());
        dlog_odd.extend_from_slice(&220i16.to_be_bytes());
        dlog_odd.extend_from_slice(&1i16.to_be_bytes());
        dlog_odd.push(1); // visible
        dlog_odd.push(0);
        dlog_odd.push(1); // go_away
        dlog_odd.push(0);
        dlog_odd.extend_from_slice(&42u32.to_be_bytes());
        dlog_odd.extend_from_slice(&701i16.to_be_bytes());
        dlog_odd.push(3); // title len
        dlog_odd.extend_from_slice(b"Odd");
        // title_end = 21 + 3 = 24 (even boundary), so position is at offset 24..26
        dlog_odd.extend_from_slice(&0x300Au16.to_be_bytes());
        let template = parse_dialog_template(&dlog_odd).expect("valid odd title dlog");
        assert_eq!(template.title_string(), "Odd");
        assert_eq!(template.items_id, 701);
        assert_eq!(template.position, 0x300A);
        assert!(template.go_away);

        // DLOG with even title ("Even" -> 4 bytes, padded to 26, position at 26..28)
        let mut dlog_even = Vec::new();
        dlog_even.extend_from_slice(&11i16.to_be_bytes());
        dlog_even.extend_from_slice(&21i16.to_be_bytes());
        dlog_even.extend_from_slice(&111i16.to_be_bytes());
        dlog_even.extend_from_slice(&221i16.to_be_bytes());
        dlog_even.extend_from_slice(&1i16.to_be_bytes());
        dlog_even.push(1);
        dlog_even.push(0);
        dlog_even.push(0);
        dlog_even.push(0);
        dlog_even.extend_from_slice(&0u32.to_be_bytes());
        dlog_even.extend_from_slice(&702i16.to_be_bytes());
        dlog_even.push(4); // title len
        dlog_even.extend_from_slice(b"Even");
        dlog_even.push(0); // alignment pad
        dlog_even.extend_from_slice(&0x700Au16.to_be_bytes());
        let template = parse_dialog_template(&dlog_even).expect("valid even title dlog");
        assert_eq!(template.title_string(), "Even");
        assert_eq!(template.items_id, 702);
        assert_eq!(template.position, 0x700A);

        // Truncated DLOG (< 20 bytes) returns None
        assert!(parse_dialog_template(&[0u8; 19]).is_none());
    }

    #[test]
    fn alert_template_parsing_and_stages() {
        // Classic 12-byte ALRT
        let mut alrt12 = Vec::new();
        alrt12.extend_from_slice(&10i16.to_be_bytes());
        alrt12.extend_from_slice(&20i16.to_be_bytes());
        alrt12.extend_from_slice(&90i16.to_be_bytes());
        alrt12.extend_from_slice(&220i16.to_be_bytes());
        alrt12.extend_from_slice(&123i16.to_be_bytes()); // items_id
        alrt12.extend_from_slice(&0x0008u16.to_be_bytes()); // stages

        let alert = parse_alert_template(&alrt12).expect("valid 12-byte alrt");
        assert_eq!(alert.bounds, (10, 20, 90, 220));
        assert_eq!(alert.items_id, 123);
        assert_eq!(alert.stages, 0x0008);
        assert_eq!(alert.position, 0);

        // 14-byte System 7 ALRT with position word 0xB00A
        let mut alrt14 = Vec::new();
        alrt14.extend_from_slice(&0i16.to_be_bytes());
        alrt14.extend_from_slice(&0i16.to_be_bytes());
        alrt14.extend_from_slice(&80i16.to_be_bytes());
        alrt14.extend_from_slice(&200i16.to_be_bytes());
        alrt14.extend_from_slice(&(-321i16).to_be_bytes()); // items_id
        alrt14.extend_from_slice(&0xF721u16.to_be_bytes()); // stages
        alrt14.extend_from_slice(&0xB00Au16.to_be_bytes()); // position

        let alert = parse_alert_template(&alrt14).expect("valid 14-byte alrt");
        assert_eq!(alert.items_id, -321);
        assert_eq!(alert.stages, 0xF721);
        assert_eq!(alert.position, 0xB00A);

        // Alert stages evaluation: 0xF721
        // Stage 0 (counter 0): nibble 1 (0b0001) -> box_drawn: false, default_item: 1, sound: 1
        let s0 = alert_stage_info(0xF721, 0);
        assert_eq!(s0.stage_index, 0);
        assert_eq!(s0.stage_nibble, 1);
        assert!(!s0.box_drawn);
        assert_eq!(s0.default_item, 1);
        assert_eq!(s0.sound_number, 1);

        // Stage 1 (counter 1): nibble 2 (0b0010) -> box_drawn: false, default_item: 1, sound: 2
        let s1 = alert_stage_info(0xF721, 1);
        assert_eq!(s1.stage_index, 1);
        assert_eq!(s1.stage_nibble, 2);
        assert!(!s1.box_drawn);
        assert_eq!(s1.default_item, 1);
        assert_eq!(s1.sound_number, 2);

        // Stage 2 (counter 2): nibble 7 (0b0111) -> box_drawn: true, default_item: 1, sound: 3
        let s2 = alert_stage_info(0xF721, 2);
        assert_eq!(s2.stage_index, 2);
        assert_eq!(s2.stage_nibble, 7);
        assert!(s2.box_drawn);
        assert_eq!(s2.default_item, 1);
        assert_eq!(s2.sound_number, 3);

        // Stage 3 (counter 3): nibble 0xF (0b1111) -> box_drawn: true, default_item: 2 (boldItm set), sound: 3
        let s3 = alert_stage_info(0xF721, 3);
        assert_eq!(s3.stage_index, 3);
        assert_eq!(s3.stage_nibble, 0xF);
        assert!(s3.box_drawn);
        assert_eq!(s3.default_item, 2);
        assert_eq!(s3.sound_number, 3);

        // Counter > 3 clamps to stage 3
        let s_clamped = alert_stage_info(0xF721, 10);
        assert_eq!(s_clamped, s3);

        // Truncated ALRT (< 8 bytes) returns None
        assert!(parse_alert_template(&[0u8; 7]).is_none());
    }

    #[test]
    fn param_text_byte_and_str_substitution() {
        let params: [Vec<u8>; 4] = [
            b"first".to_vec(),
            Vec::new(),
            b"third".to_vec(),
            b"fourth".to_vec(),
        ];

        // Byte substitution
        assert_eq!(
            apply_param_text(b"^0/^1/^2/^3", &params).as_ref(),
            b"first//third/fourth"
        );
        assert_eq!(
            apply_param_text(b"^^0 ^9 trailing^", &params).as_ref(),
            b"^first ^9 trailing^"
        );
        assert!(matches!(
            apply_param_text(b"plain", &params),
            Cow::Borrowed(_)
        ));

        // String substitution
        let str_params: [Vec<u8>; 4] = [
            b"MS UserKey".to_vec(),
            b"42".to_vec(),
            Vec::new(),
            Vec::new(),
        ];

        assert_eq!(
            apply_param_text_str("Unable to open the \"^0\" file.", &str_params),
            "Unable to open the \"MS UserKey\" file."
        );
        assert_eq!(apply_param_text_str("count: ^1", &str_params), "count: 42");
        assert_eq!(
            apply_param_text_str("plain text without placeholders", &str_params),
            "plain text without placeholders"
        );
        assert_eq!(
            apply_param_text_str("^0 ^1 ^2 ^3", &str_params),
            "MS UserKey 42  "
        );
        assert_eq!(
            apply_param_text_str("^A literal caret", &str_params),
            "^A literal caret"
        );
        assert_eq!(apply_param_text_str("trailing^", &str_params), "trailing^");
        assert_eq!(apply_param_text_str("^^0", &str_params), "^MS UserKey");
        assert_eq!(apply_param_text_str("", &str_params), "");
    }

    #[test]
    fn modal_dialog_keyboard_navigation_and_filter_evaluation() {
        // Return key: char 0x0D, keycode 0x24
        assert!(is_dialog_default_key(CHAR_RETURN, 0));
        assert!(is_dialog_default_key(0, KEY_RETURN));
        // Enter key: char 0x03, keycode 0x4C
        assert!(is_dialog_default_key(CHAR_ENTER, 0));
        assert!(is_dialog_default_key(0, KEY_NUMPAD_ENTER));
        assert!(!is_dialog_default_key(b'a', 0));

        // Escape: char 0x1B, keycode 0x35
        assert!(is_dialog_cancel_key(CHAR_ESCAPE, 0, 0));
        assert!(is_dialog_cancel_key(0, KEY_ESCAPE, 0));
        // Command-period: char '.' with MODIFIER_CMD_KEY
        assert!(is_dialog_cancel_key(CHAR_PERIOD, 0, MODIFIER_CMD_KEY));
        assert!(is_dialog_cancel_key(0, KEY_PERIOD, MODIFIER_CMD_KEY));
        // Period without Cmd is not cancel
        assert!(!is_dialog_cancel_key(CHAR_PERIOD, 0, 0));

        // Tab: char 0x09, keycode 0x30
        assert!(is_dialog_tab_key(CHAR_TAB, 0));
        assert!(is_dialog_tab_key(0, KEY_TAB));
        assert!(!is_dialog_tab_key(b' ', 0));

        // evaluate_modal_dialog_key
        let return_msg = (u32::from(KEY_RETURN) << 8) | u32::from(CHAR_RETURN);
        assert_eq!(
            evaluate_modal_dialog_key(EVENT_KEY_DOWN, return_msg, 0),
            DialogFilterDecision::TriggerDefaultButton
        );
        assert_eq!(
            evaluate_modal_dialog_key(EVENT_AUTO_KEY, return_msg, 0),
            DialogFilterDecision::TriggerDefaultButton
        );

        let esc_msg = (u32::from(KEY_ESCAPE) << 8) | u32::from(CHAR_ESCAPE);
        assert_eq!(
            evaluate_modal_dialog_key(EVENT_KEY_DOWN, esc_msg, 0),
            DialogFilterDecision::TriggerCancelButton
        );

        let cmd_period_msg = (u32::from(KEY_PERIOD) << 8) | u32::from(CHAR_PERIOD);
        assert_eq!(
            evaluate_modal_dialog_key(EVENT_KEY_DOWN, cmd_period_msg, MODIFIER_CMD_KEY),
            DialogFilterDecision::TriggerCancelButton
        );
        assert_eq!(
            evaluate_modal_dialog_key(EVENT_KEY_DOWN, cmd_period_msg, 0),
            DialogFilterDecision::Unhandled
        );

        let tab_msg = (u32::from(KEY_TAB) << 8) | u32::from(CHAR_TAB);
        assert_eq!(
            evaluate_modal_dialog_key(EVENT_KEY_DOWN, tab_msg, 0),
            DialogFilterDecision::AdvanceEditTextFocus
        );

        // Non-keyboard events (e.g. mouseDown = 1) are Unhandled
        assert_eq!(
            evaluate_modal_dialog_key(1, return_msg, 0),
            DialogFilterDecision::Unhandled
        );
    }

    #[test]
    fn find_next_edit_text_item_cycling() {
        assert_eq!(find_next_edit_text_item([], 0), None);
        assert_eq!(
            find_next_edit_text_item([DIALOG_ITEM_BUTTON, DIALOG_ITEM_STATIC_TEXT], 0),
            None
        );

        // Single editText at index 1 (item 2)
        let single = [
            DIALOG_ITEM_BUTTON,
            DIALOG_ITEM_EDIT_TEXT,
            DIALOG_ITEM_STATIC_TEXT,
        ];
        assert_eq!(find_next_edit_text_item(single, 0), Some(2));
        assert_eq!(find_next_edit_text_item(single, 1), Some(2));
        assert_eq!(find_next_edit_text_item(single, 2), Some(2));
        assert_eq!(find_next_edit_text_item(single, 3), Some(2));

        // Multiple editText items at item 2 and item 4
        let multiple = [
            DIALOG_ITEM_BUTTON,      // 1
            DIALOG_ITEM_EDIT_TEXT,   // 2
            DIALOG_ITEM_STATIC_TEXT, // 3
            DIALOG_ITEM_EDIT_TEXT,   // 4
        ];
        assert_eq!(find_next_edit_text_item(multiple, 0), Some(2));
        assert_eq!(find_next_edit_text_item(multiple, 1), Some(2));
        assert_eq!(find_next_edit_text_item(multiple, 2), Some(4));
        assert_eq!(find_next_edit_text_item(multiple, 3), Some(4));
        assert_eq!(find_next_edit_text_item(multiple, 4), Some(2)); // wraps!

        // Handles disabled flag (0x80)
        let disabled_edit = [
            DIALOG_ITEM_BUTTON,
            DIALOG_ITEM_EDIT_TEXT | DIALOG_ITEM_DISABLED_FLAG,
        ];
        assert_eq!(find_next_edit_text_item(disabled_edit, 0), Some(2));

        // find_next_edit_text_in_items helper
        let item_records = [
            DialogItemRecord {
                item_offset: 0,
                item_type: DIALOG_ITEM_BUTTON,
                rect: (0, 0, 0, 0),
                handle: 0,
                payload: vec![],
            },
            DialogItemRecord {
                item_offset: 0,
                item_type: DIALOG_ITEM_EDIT_TEXT,
                rect: (0, 0, 0, 0),
                handle: 0,
                payload: vec![],
            },
        ];
        assert_eq!(find_next_edit_text_in_items(&item_records, 0), Some(2));
        assert_eq!(find_next_edit_text_in_items(&item_records, 2), Some(2));
    }

    #[test]
    fn dialog_item_text_and_query_operations() {
        // Clamping length and bytes
        assert_eq!(clamp_dialog_item_text_len(10), 10);
        assert_eq!(clamp_dialog_item_text_len(300), 255);

        let short_bytes = b"Hello world";
        assert_eq!(clamp_dialog_item_text_bytes(short_bytes), short_bytes);
        let long_bytes = vec![b'A'; 300];
        let clamped = clamp_dialog_item_text_bytes(&long_bytes);
        assert_eq!(clamped.len(), 255);
        assert_eq!(clamped, &long_bytes[..255]);

        // Pascal string encode & decode
        let (len, text) = encode_dialog_item_pstring(b"Test");
        assert_eq!(len, 4);
        assert_eq!(text, b"Test");

        let mut pstr = vec![4];
        pstr.extend_from_slice(b"Test");
        assert_eq!(decode_dialog_item_pstring(&pstr), Some(&b"Test"[..]));
        assert_eq!(decode_dialog_item_pstring(&[]), None);
        assert_eq!(decode_dialog_item_pstring(&[10, 1, 2]), None); // truncated

        // 1-indexed lookups
        let mut items = vec!["First", "Second", "Third"];
        assert_eq!(get_item_at_1_indexed(&items, 0), None);
        assert_eq!(get_item_at_1_indexed(&items, 1), Some(&"First"));
        assert_eq!(get_item_at_1_indexed(&items, 2), Some(&"Second"));
        assert_eq!(get_item_at_1_indexed(&items, 3), Some(&"Third"));
        assert_eq!(get_item_at_1_indexed(&items, 4), None);

        *get_item_at_1_indexed_mut(&mut items, 2).unwrap() = "Modified";
        assert_eq!(items[1], "Modified");
        assert_eq!(get_item_at_1_indexed_mut(&mut items, 0), None);
        assert_eq!(get_item_at_1_indexed_mut(&mut items, 99), None);

        // Control and text predicates
        assert!(is_dialog_item_control(DIALOG_ITEM_BUTTON));
        assert!(is_dialog_item_control(
            DIALOG_ITEM_BUTTON | DIALOG_ITEM_DISABLED_FLAG
        ));
        assert!(is_dialog_item_control(DIALOG_ITEM_CHECKBOX));
        assert!(is_dialog_item_control(DIALOG_ITEM_RADIO));
        assert!(is_dialog_item_control(DIALOG_ITEM_RESOURCE_CONTROL));
        assert!(!is_dialog_item_control(DIALOG_ITEM_STATIC_TEXT));
        assert!(!is_dialog_item_control(DIALOG_ITEM_EDIT_TEXT));
        assert!(!is_dialog_item_control(DIALOG_ITEM_ICON));

        assert!(is_dialog_item_text(DIALOG_ITEM_STATIC_TEXT));
        assert!(is_dialog_item_text(DIALOG_ITEM_EDIT_TEXT));
        assert!(is_dialog_item_text(
            DIALOG_ITEM_EDIT_TEXT | DIALOG_ITEM_DISABLED_FLAG
        ));
        assert!(!is_dialog_item_text(DIALOG_ITEM_BUTTON));
        assert!(!is_dialog_item_text(DIALOG_ITEM_ICON));

        assert!(is_dialog_item_text_or_title(DIALOG_ITEM_STATIC_TEXT));
        assert!(is_dialog_item_text_or_title(DIALOG_ITEM_EDIT_TEXT));
        assert!(is_dialog_item_text_or_title(DIALOG_ITEM_BUTTON));
        assert!(is_dialog_item_text_or_title(DIALOG_ITEM_CHECKBOX));
        assert!(is_dialog_item_text_or_title(DIALOG_ITEM_RADIO));
        assert!(!is_dialog_item_text_or_title(DIALOG_ITEM_RESOURCE_CONTROL));
        assert!(!is_dialog_item_text_or_title(DIALOG_ITEM_ICON));
        assert!(!is_dialog_item_text_or_title(DIALOG_ITEM_PICTURE));
        assert!(!is_dialog_item_text_or_title(DIALOG_ITEM_USER_ITEM));

        // String clamping and formatting
        let short_str = "Dialog item text";
        assert_eq!(clamp_dialog_item_text_str(short_str), short_str);
        let long_str = "a".repeat(300);
        let clamped_str = clamp_dialog_item_text_str(&long_str);
        assert_eq!(clamped_str.len(), 255);

        let pstr_formatted = format_dialog_item_pstring(b"Sample");
        assert_eq!(pstr_formatted[0], 6);
        assert_eq!(&pstr_formatted[1..], b"Sample");
        assert_eq!(
            decode_dialog_item_pstring_to_string(&pstr_formatted),
            Some("Sample".to_string())
        );

        let (set_bytes, set_string) = prepare_set_dialog_item_text(b"Config");
        assert_eq!(set_bytes, b"Config");
        assert_eq!(set_string, "Config");

        let (get_len, get_bytes) = prepare_get_dialog_item_text(b"Result");
        assert_eq!(get_len, 6);
        assert_eq!(get_bytes, b"Result");

        // Active text resolution from handle vs fallback payload
        let payload = b"Default text";
        let handle_override = b"Active handle text";
        assert_eq!(
            extract_dialog_item_text_bytes(DIALOG_ITEM_EDIT_TEXT, Some(handle_override), payload),
            handle_override
        );
        assert_eq!(
            extract_dialog_item_text_bytes(DIALOG_ITEM_EDIT_TEXT, None, payload),
            payload
        );
        assert_eq!(
            extract_dialog_item_text_bytes(DIALOG_ITEM_BUTTON, None, b"Cancel"),
            b"Cancel"
        );
        assert_eq!(
            extract_dialog_item_text_bytes(DIALOG_ITEM_ICON, None, &[0x00, 0x80]),
            b""
        );
        assert_eq!(
            extract_dialog_item_text_string(DIALOG_ITEM_STATIC_TEXT, None, b"Info"),
            "Info"
        );

        // DialogItemRecord text queries and payload isolation
        let text_record = DialogItemRecord {
            item_offset: 0,
            item_type: DIALOG_ITEM_STATIC_TEXT,
            rect: (0, 0, 10, 50),
            handle: 0,
            payload: b"Hello".to_vec(),
        };
        assert!(text_record.has_text_or_title());
        assert_eq!(text_record.text_payload(), Some(&b"Hello"[..]));
        assert_eq!(text_record.text(), "Hello");

        let icon_record = DialogItemRecord {
            item_offset: 0,
            item_type: DIALOG_ITEM_ICON,
            rect: (0, 0, 32, 32),
            handle: 0,
            payload: vec![0x00, 0x80],
        };
        assert!(!icon_record.has_text_or_title());
        assert_eq!(icon_record.text_payload(), None);
        assert_eq!(icon_record.text(), "");

        // Control procIDs
        assert_eq!(dialog_item_control_proc_id(DIALOG_ITEM_BUTTON), Some(0));
        assert_eq!(
            dialog_item_control_proc_id(DIALOG_ITEM_BUTTON | DIALOG_ITEM_DISABLED_FLAG),
            Some(0)
        );
        assert_eq!(dialog_item_control_proc_id(DIALOG_ITEM_CHECKBOX), Some(1));
        assert_eq!(dialog_item_control_proc_id(DIALOG_ITEM_RADIO), Some(2));
        assert_eq!(dialog_item_control_proc_id(DIALOG_ITEM_STATIC_TEXT), None);
        assert_eq!(dialog_item_control_proc_id(DIALOG_ITEM_ICON), None);
    }

    #[test]
    fn dialog_item_resource_mapping_and_geometry() {
        // Classification
        assert!(is_dialog_item_resource(DIALOG_ITEM_RESOURCE_CONTROL));
        assert!(is_dialog_item_resource(DIALOG_ITEM_ICON));
        assert!(is_dialog_item_resource(DIALOG_ITEM_PICTURE));
        assert!(is_dialog_item_resource(
            DIALOG_ITEM_RESOURCE_CONTROL | DIALOG_ITEM_DISABLED_FLAG
        ));
        assert!(is_dialog_item_resource(
            DIALOG_ITEM_ICON | DIALOG_ITEM_DISABLED_FLAG
        ));
        assert!(is_dialog_item_resource(
            DIALOG_ITEM_PICTURE | DIALOG_ITEM_DISABLED_FLAG
        ));

        assert!(!is_dialog_item_resource(DIALOG_ITEM_USER_ITEM));
        assert!(!is_dialog_item_resource(DIALOG_ITEM_BUTTON));
        assert!(!is_dialog_item_resource(DIALOG_ITEM_CHECKBOX));
        assert!(!is_dialog_item_resource(DIALOG_ITEM_RADIO));
        assert!(!is_dialog_item_resource(DIALOG_ITEM_STATIC_TEXT));
        assert!(!is_dialog_item_resource(DIALOG_ITEM_EDIT_TEXT));
        assert!(!is_dialog_item_resource(99));

        // Resource type 4-character codes
        assert_eq!(
            dialog_item_resource_type(DIALOG_ITEM_RESOURCE_CONTROL),
            Some(*b"CNTL")
        );
        assert_eq!(
            dialog_item_resource_type(DIALOG_ITEM_RESOURCE_CONTROL | DIALOG_ITEM_DISABLED_FLAG),
            Some(*b"CNTL")
        );
        assert_eq!(dialog_item_resource_type(DIALOG_ITEM_ICON), Some(*b"ICON"));
        assert_eq!(
            dialog_item_resource_type(DIALOG_ITEM_ICON | DIALOG_ITEM_DISABLED_FLAG),
            Some(*b"ICON")
        );
        assert_eq!(
            dialog_item_resource_type(DIALOG_ITEM_PICTURE),
            Some(*b"PICT")
        );
        assert_eq!(
            dialog_item_resource_type(DIALOG_ITEM_PICTURE | DIALOG_ITEM_DISABLED_FLAG),
            Some(*b"PICT")
        );
        assert_eq!(dialog_item_resource_type(DIALOG_ITEM_STATIC_TEXT), None);
        assert_eq!(dialog_item_resource_type(DIALOG_ITEM_BUTTON), None);

        // Resource type as u32
        assert_eq!(
            dialog_item_resource_type_u32(DIALOG_ITEM_RESOURCE_CONTROL),
            Some(u32::from_be_bytes(*b"CNTL"))
        );
        assert_eq!(
            dialog_item_resource_type_u32(DIALOG_ITEM_ICON),
            Some(u32::from_be_bytes(*b"ICON"))
        );
        assert_eq!(
            dialog_item_resource_type_u32(DIALOG_ITEM_PICTURE),
            Some(u32::from_be_bytes(*b"PICT"))
        );
        assert_eq!(
            dialog_item_resource_type_u32(DIALOG_ITEM_STATIC_TEXT),
            None
        );

        // DialogItemRecord resource queries and guarded ID decoding
        let res_control_record = DialogItemRecord {
            item_offset: 0,
            item_type: DIALOG_ITEM_RESOURCE_CONTROL,
            rect: (10, 10, 30, 80),
            handle: 0,
            payload: 128i16.to_be_bytes().to_vec(),
        };
        assert!(res_control_record.is_resource());
        assert_eq!(res_control_record.resource_type(), Some(*b"CNTL"));
        assert_eq!(
            res_control_record.resource_type_u32(),
            Some(u32::from_be_bytes(*b"CNTL"))
        );
        assert_eq!(res_control_record.resource_id(), Some(128));

        let icon_record = DialogItemRecord {
            item_offset: 16,
            item_type: DIALOG_ITEM_ICON | DIALOG_ITEM_DISABLED_FLAG,
            rect: (20, 20, 52, 52),
            handle: 0,
            payload: (-42i16).to_be_bytes().to_vec(),
        };
        assert!(icon_record.is_resource());
        assert_eq!(icon_record.resource_type(), Some(*b"ICON"));
        assert_eq!(
            icon_record.resource_type_u32(),
            Some(u32::from_be_bytes(*b"ICON"))
        );
        assert_eq!(icon_record.resource_id(), Some(-42));

        let pict_record = DialogItemRecord {
            item_offset: 32,
            item_type: DIALOG_ITEM_PICTURE,
            rect: (0, 0, 100, 100),
            handle: 0,
            payload: 1024i16.to_be_bytes().to_vec(),
        };
        assert!(pict_record.is_resource());
        assert_eq!(pict_record.resource_type(), Some(*b"PICT"));
        assert_eq!(
            pict_record.resource_type_u32(),
            Some(u32::from_be_bytes(*b"PICT"))
        );
        assert_eq!(pict_record.resource_id(), Some(1024));

        // Short payload on resource item returns None
        let short_icon_record = DialogItemRecord {
            item_offset: 48,
            item_type: DIALOG_ITEM_ICON,
            rect: (0, 0, 32, 32),
            handle: 0,
            payload: vec![1],
        };
        assert!(short_icon_record.is_resource());
        assert_eq!(short_icon_record.resource_id(), None);

        // Text item with 2+ bytes must NOT return a resource ID
        let text_record = DialogItemRecord {
            item_offset: 64,
            item_type: DIALOG_ITEM_STATIC_TEXT,
            rect: (0, 0, 16, 100),
            handle: 0,
            payload: b"OK".to_vec(),
        };
        assert!(!text_record.is_resource());
        assert_eq!(text_record.resource_type(), None);
        assert_eq!(text_record.resource_type_u32(), None);
        assert_eq!(text_record.resource_id(), None);

        // Icon geometry and centering
        assert_eq!(DIALOG_ICON_SIZE, 32);
        assert_eq!(DIALOG_SMALL_ICON_SIZE, 16);

        // Standard 32x32 in exactly 32x32 rect -> unchanged
        assert_eq!(
            dialog_standard_icon_rect((10, 20, 42, 52)),
            (10, 20, 42, 52)
        );
        // Centered within larger rect (40x50)
        assert_eq!(
            dialog_standard_icon_rect((0, 0, 40, 50)),
            (4, 9, 36, 41)
        );
        // Small icon (16x16) centered within 20x20 rect
        assert_eq!(
            dialog_icon_rect(
                (10, 10, 30, 30),
                DIALOG_SMALL_ICON_SIZE,
                DIALOG_SMALL_ICON_SIZE
            ),
            (12, 12, 28, 28)
        );
    }

    #[test]
    fn dialog_selection_normalization_and_alert_helpers() {
        // Select all special cases: (0, -1) and (0, 32767)
        assert_eq!(normalize_dialog_item_selection(0, -1, 10), (0, 10));
        assert_eq!(normalize_dialog_item_selection(0, i16::MAX, 10), (0, 10));
        assert_eq!(normalize_dialog_item_selection(0, -1, 0), (0, 0));

        // Normal in-bounds range
        assert_eq!(normalize_dialog_item_selection(2, 5, 10), (2, 5));
        assert_eq!(normalize_dialog_item_selection(0, 0, 10), (0, 0));
        assert_eq!(normalize_dialog_item_selection(10, 10, 10), (10, 10));

        // Clamping out-of-bounds
        assert_eq!(normalize_dialog_item_selection(-5, 20, 10), (0, 10));
        assert_eq!(normalize_dialog_item_selection(15, 20, 10), (10, 10));

        // Reversed bounds swapping
        assert_eq!(normalize_dialog_item_selection(7, 3, 10), (3, 7));
        assert_eq!(normalize_dialog_item_selection(12, -2, 10), (0, 10));

        // Alert icon IDs
        assert_eq!(alert_icon_id(ALERT_TYPE_STOP), Some(0));
        assert_eq!(alert_icon_id(ALERT_TYPE_NOTE), Some(1));
        assert_eq!(alert_icon_id(ALERT_TYPE_CAUTION), Some(2));
        assert_eq!(alert_icon_id(ALERT_TYPE_PLAIN), None);
        assert_eq!(alert_icon_id(999), None);

        // Standard alert buttons
        assert_eq!(ALERT_BUTTON_OK, 1);
        assert_eq!(ALERT_BUTTON_CANCEL, 2);
        assert_eq!(ALERT_BUTTON_OTHER, 3);
        assert_eq!(ALERT_BUTTON_HELP, 4);

        // Next alert stage capped at 3
        assert_eq!(next_alert_stage(0), 1);
        assert_eq!(next_alert_stage(1), 2);
        assert_eq!(next_alert_stage(2), 3);
        assert_eq!(next_alert_stage(3), 3);
        assert_eq!(next_alert_stage(10), 3);

        // Disposal classification predicates
        assert!(is_dialog_item_disposable_text(DIALOG_ITEM_STATIC_TEXT));
        assert!(is_dialog_item_disposable_text(DIALOG_ITEM_EDIT_TEXT));
        assert!(is_dialog_item_disposable_text(
            DIALOG_ITEM_EDIT_TEXT | DIALOG_ITEM_DISABLED_FLAG
        ));
        assert!(!is_dialog_item_disposable_text(DIALOG_ITEM_BUTTON));
        assert!(!is_dialog_item_disposable_text(DIALOG_ITEM_ICON));

        assert!(is_dialog_item_disposable_control(DIALOG_ITEM_BUTTON));
        assert!(is_dialog_item_disposable_control(DIALOG_ITEM_CHECKBOX));
        assert!(is_dialog_item_disposable_control(DIALOG_ITEM_RADIO));
        assert!(is_dialog_item_disposable_control(
            DIALOG_ITEM_RESOURCE_CONTROL
        ));
        assert!(is_dialog_item_disposable_control(
            DIALOG_ITEM_BUTTON | DIALOG_ITEM_DISABLED_FLAG
        ));
        assert!(!is_dialog_item_disposable_control(DIALOG_ITEM_STATIC_TEXT));
        assert!(!is_dialog_item_disposable_control(DIALOG_ITEM_ICON));
    }

    #[test]
    fn dialog_dispatch_button_geometry_and_cancel_detection() {
        // DialogRecord chrome and window kind
        assert_eq!(DIALOG_WINDOW_KIND_OFFSET, 108);
        assert_eq!(DIALOG_WINDOW_KIND, 2);
        assert_eq!(DIALOG_GO_AWAY_FLAG_OFFSET, 112);

        // DialogDispatch selectors
        assert_eq!(DIALOG_DISPATCH_NEW_COLOR_DIALOG, 0x0000);
        assert_eq!(DIALOG_DISPATCH_GET_STD_FILTER_PROC, 0x0003);
        assert_eq!(DIALOG_DISPATCH_SET_DIALOG_DEFAULT_ITEM, 0x0004);
        assert_eq!(DIALOG_DISPATCH_SET_DIALOG_CANCEL_ITEM, 0x0005);
        assert_eq!(DIALOG_DISPATCH_SET_DIALOG_TRACKS_CURSOR, 0x0006);
        assert_eq!(DIALOG_DISPATCH_NEW_FEATURES_DIALOG, 0x000C);
        assert_eq!(DIALOG_DISPATCH_AUTO_SIZE_DIALOG, 0x000D);
        assert_eq!(DIALOG_DISPATCH_GET_DIALOG_ITEM_AS_CONTROL, 0x000F);
        assert_eq!(DIALOG_DISPATCH_MOVE_DIALOG_ITEM, 0x0010);
        assert_eq!(DIALOG_DISPATCH_SIZE_DIALOG_ITEM, 0x0011);
        assert_eq!(DIALOG_DISPATCH_APPEND_DIALOG_ITEM_LIST, 0x0012);
        assert_eq!(DIALOG_DISPATCH_GET_DIALOG_DEFAULT_ITEM, 0x0012);
        assert_eq!(DIALOG_DISPATCH_GET_DIALOG_CANCEL_ITEM, 0x0013);
        assert_eq!(DIALOG_RES_NOT_FOUND, -192);

        // Default button outline geometry
        assert_eq!(DEFAULT_BUTTON_OUTLINE_THICKNESS, 3);
        assert_eq!(DEFAULT_BUTTON_OUTLINE_INSET, 4);

        let button_rect = (50, 100, 70, 160);
        let (outer, oval) = default_button_outline_geometry(button_rect);
        assert_eq!(outer, (46, 96, 74, 164));
        // height = 74 - 46 = 28; oval = max(4, 28/2 - 4) = 10
        assert_eq!(oval, 10);

        // Cancel button title predicates
        assert!(is_dialog_cancel_button_title(b"Cancel"));
        assert!(is_dialog_cancel_button_title(b"cancel"));
        assert!(is_dialog_cancel_button_title(b"CANCEL"));
        assert!(!is_dialog_cancel_button_title(b"OK"));
        assert!(!is_dialog_cancel_button_title(b""));

        assert!(is_dialog_cancel_button_title_str("Cancel"));
        assert!(is_dialog_cancel_button_title_str("cancel"));
        assert!(!is_dialog_cancel_button_title_str("Dismiss"));

        // Cancel item search across items
        let items: Vec<(u8, &str)> = vec![
            (DIALOG_ITEM_BUTTON, "OK"),
            (DIALOG_ITEM_BUTTON, "Cancel"),
            (DIALOG_ITEM_STATIC_TEXT, "Cancel"),
        ];
        assert_eq!(
            find_dialog_cancel_item_index(items.iter().copied()),
            Some(2)
        );

        // Disabled cancel button is ignored
        let items_disabled: Vec<(u8, &str)> = vec![
            (DIALOG_ITEM_BUTTON, "OK"),
            (DIALOG_ITEM_BUTTON | DIALOG_ITEM_DISABLED_FLAG, "Cancel"),
        ];
        assert_eq!(
            find_dialog_cancel_item_index(items_disabled.iter().copied()),
            None
        );

        // No cancel button present
        let items_no_cancel: Vec<(u8, &str)> =
            vec![(DIALOG_ITEM_BUTTON, "OK"), (DIALOG_ITEM_BUTTON, "Help")];
        assert_eq!(
            find_dialog_cancel_item_index(items_no_cancel.iter().copied()),
            None
        );

        // evaluate_dialog_cancel_item & DialogCancelItemResolution
        let eval_explicit = evaluate_dialog_cancel_item(Some(3), items.iter().copied());
        assert_eq!(eval_explicit.item_no(), Some(3));
        assert_eq!(eval_explicit.item_number(), 3);
        assert_eq!(eval_explicit.to_u16(), Some(3));
        assert!(eval_explicit.has_item());
        assert!(eval_explicit.matches_item(3));
        assert!(!eval_explicit.matches_item(2));

        let eval_titled = evaluate_dialog_cancel_item(Some(0), items.iter().copied());
        assert_eq!(eval_titled.item_no(), Some(2));
        assert_eq!(eval_titled.item_number(), 2);
        assert_eq!(eval_titled.to_u16(), Some(2));
        assert!(eval_titled.has_item());

        let eval_none = evaluate_dialog_cancel_item(None, items_no_cancel.iter().copied());
        assert_eq!(eval_none.item_no(), None);
        assert_eq!(eval_none.item_number(), 0);
        assert_eq!(eval_none.to_u16(), None);
        assert!(!eval_none.has_item());
        assert!(!eval_none.matches_item(0));

        let eval_empty = DialogCancelItemResolution::none();
        assert_eq!(eval_empty.item_no(), None);
        assert!(!eval_empty.has_item());

        // evaluate_modal_dialog_cancel_item
        let modal_eval_explicit = evaluate_modal_dialog_cancel_item(Some(4), items.iter().copied());
        assert_eq!(modal_eval_explicit.item_no(), Some(4));
        assert_eq!(modal_eval_explicit.item_number(), 4);

        let modal_eval_titled = evaluate_modal_dialog_cancel_item(None, items.iter().copied());
        assert_eq!(modal_eval_titled.item_no(), Some(2));
        assert_eq!(modal_eval_titled.item_number(), 2);

        let modal_eval_fallback = evaluate_modal_dialog_cancel_item(None, items_no_cancel.iter().copied());
        assert_eq!(modal_eval_fallback.item_no(), Some(ALERT_BUTTON_CANCEL));
        assert_eq!(modal_eval_fallback.item_number(), ALERT_BUTTON_CANCEL);
        assert!(modal_eval_fallback.matches_item(ALERT_BUTTON_CANCEL));

        // evaluate_dialog_filter_cancel
        assert_eq!(
            evaluate_dialog_filter_cancel(DialogFilterDecision::TriggerCancelButton, Some(2)),
            Some(2)
        );
        assert_eq!(
            evaluate_dialog_filter_cancel(DialogFilterDecision::TriggerCancelButton, Some(0)),
            None
        );
        assert_eq!(
            evaluate_dialog_filter_cancel(DialogFilterDecision::TriggerCancelButton, None),
            None
        );
        assert_eq!(
            evaluate_dialog_filter_cancel(DialogFilterDecision::TriggerDefaultButton, Some(2)),
            None
        );
        assert_eq!(
            evaluate_dialog_filter_cancel(DialogFilterDecision::Unhandled, Some(2)),
            None
        );

        // resolve_dialog_cancel_item
        assert_eq!(
            resolve_dialog_cancel_item(Some(3), items.iter().copied()),
            3
        );
        assert_eq!(
            resolve_dialog_cancel_item(Some(0), items.iter().copied()),
            2
        );
        assert_eq!(
            resolve_dialog_cancel_item(None, items.iter().copied()),
            2
        );
        assert_eq!(
            resolve_dialog_cancel_item(None, items_no_cancel.iter().copied()),
            0
        );

        // resolve_modal_dialog_cancel_item
        assert_eq!(
            resolve_modal_dialog_cancel_item(Some(4), items.iter().copied()),
            4
        );
        assert_eq!(
            resolve_modal_dialog_cancel_item(None, items.iter().copied()),
            2
        );
        assert_eq!(
            resolve_modal_dialog_cancel_item(None, items_no_cancel.iter().copied()),
            ALERT_BUTTON_CANCEL
        );

        // resolve_dialog_default_item
        assert_eq!(resolve_dialog_default_item(Some(3), 5), 3);
        assert_eq!(resolve_dialog_default_item(Some(0), 5), 1);
        assert_eq!(resolve_dialog_default_item(None, 5), 1);
        assert_eq!(resolve_dialog_default_item(None, 0), 0);

        // is_dialog_default_button & is_dialog_cancel_button
        assert!(is_dialog_default_button(1, 1));
        assert!(!is_dialog_default_button(2, 1));
        assert!(!is_dialog_default_button(0, 0));
        assert!(!is_dialog_default_button(-1, -1));

        assert!(is_dialog_cancel_button(2, 2));
        assert!(!is_dialog_cancel_button(1, 2));
        assert!(!is_dialog_cancel_button(0, 0));
        assert!(!is_dialog_cancel_button(-1, -1));
    }

    #[test]
    fn append_and_shorten_ditl_operations_and_game_managed_check() {
        // AppendDITL constants
        assert_eq!(APPEND_DITL_OVERLAY, 0);
        assert_eq!(APPEND_DITL_RIGHT, 1);
        assert_eq!(APPEND_DITL_BOTTOM, 2);

        // append_ditl_offset_delta tests
        let height = 200;
        let width = 300;
        let item_origins = [(20, 30), (50, 60)];

        assert_eq!(
            append_ditl_offset_delta(APPEND_DITL_OVERLAY, height, width, |_| None),
            (0, 0)
        );
        assert_eq!(
            append_ditl_offset_delta(APPEND_DITL_RIGHT, height, width, |_| None),
            (0, 300)
        );
        assert_eq!(
            append_ditl_offset_delta(APPEND_DITL_BOTTOM, height, width, |_| None),
            (200, 0)
        );
        assert_eq!(
            append_ditl_offset_delta(-1, height, width, |idx| item_origins.get(idx - 1).copied()),
            (20, 30)
        );
        assert_eq!(
            append_ditl_offset_delta(-2, height, width, |idx| item_origins.get(idx - 1).copied()),
            (50, 60)
        );
        assert_eq!(
            append_ditl_offset_delta(-3, height, width, |idx| item_origins.get(idx - 1).copied()),
            (0, 0)
        );

        // shorten_ditl_counts tests
        assert_eq!(shorten_ditl_counts(10, 3), (7, 6));
        assert_eq!(shorten_ditl_counts(5, 5), (0, -1));
        assert_eq!(shorten_ditl_counts(5, 10), (0, -1));
        assert_eq!(shorten_ditl_counts(0, 2), (0, -1));

        // rects_intersect tests
        let r1 = (10, 10, 50, 50);
        let r2 = (20, 20, 60, 60);
        let r3 = (60, 60, 100, 100);
        let r4 = (50, 50, 100, 100);
        assert!(rects_intersect(r1, r2));
        assert!(!rects_intersect(r1, r3));
        assert!(!rects_intersect(r1, r4)); // Touching edge only

        // dialog_item_intersects_bounds tests
        let bounds = (100, 100, 300, 400);
        assert!(dialog_item_intersects_bounds(bounds, (10, 10, 50, 50)));
        assert!(!dialog_item_intersects_bounds(bounds, (300, 10, 350, 50)));

        // is_dialog_game_managed tests
        let all_user = vec![
            (DIALOG_ITEM_USER_ITEM, (10, 10, 50, 50)),
            (DIALOG_ITEM_USER_ITEM, (60, 60, 100, 100)),
        ];
        assert!(is_dialog_game_managed(bounds, all_user));

        let with_button = vec![
            (DIALOG_ITEM_USER_ITEM, (10, 10, 50, 50)),
            (DIALOG_ITEM_BUTTON, (60, 60, 100, 100)),
        ];
        assert!(!is_dialog_game_managed(bounds, with_button));

        let button_offscreen = vec![
            (DIALOG_ITEM_USER_ITEM, (10, 10, 50, 50)),
            (DIALOG_ITEM_BUTTON, (300, 300, 350, 350)),
        ];
        assert!(is_dialog_game_managed(bounds, button_offscreen));

        let empty: Vec<(u8, (i16, i16, i16, i16))> = vec![];
        assert!(!is_dialog_game_managed(bounds, empty));
    }

    #[test]
    fn dialog_event_classification_and_textedit_key_processing() {
        let dialog = 0x2000;
        let bounds = (50, 50, 200, 300);

        // is_dialog_event tests
        // None target dialog -> always false
        assert!(!is_dialog_event(
            EVENT_UPDATE,
            dialog,
            0,
            0,
            None,
            Some(bounds)
        ));
        assert!(!is_dialog_event(
            EVENT_MOUSE_DOWN,
            0,
            100,
            100,
            None,
            Some(bounds)
        ));

        // updateEvt (6) & activateEvt (8) -> match message == dialog
        assert!(is_dialog_event(
            EVENT_UPDATE,
            dialog,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));
        assert!(!is_dialog_event(
            EVENT_UPDATE,
            0x9999,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));
        assert!(is_dialog_event(
            EVENT_ACTIVATE,
            dialog,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));
        assert!(!is_dialog_event(
            EVENT_ACTIVATE,
            0x1111,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));

        // mouseDown (1) -> within bounds
        assert!(is_dialog_event(
            EVENT_MOUSE_DOWN,
            0,
            100,
            100,
            Some(dialog),
            Some(bounds)
        ));
        assert!(!is_dialog_event(
            EVENT_MOUSE_DOWN,
            0,
            10,
            10,
            Some(dialog),
            Some(bounds)
        ));
        assert!(!is_dialog_event(
            EVENT_MOUSE_DOWN,
            0,
            100,
            100,
            Some(dialog),
            None
        ));

        // other events (keyDown, autoKey, null, etc.) -> true if target dialog present
        assert!(is_dialog_event(
            EVENT_KEY_DOWN,
            b'a' as u32,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));
        assert!(is_dialog_event(
            EVENT_AUTO_KEY,
            b'a' as u32,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));
        assert!(is_dialog_event(
            EVENT_NULL,
            0,
            0,
            0,
            Some(dialog),
            Some(bounds)
        ));

        // is_dialog_edit_text_character tests
        assert!(is_dialog_edit_text_character(0x08)); // backspace
        assert!(is_dialog_edit_text_character(b' ')); // space (0x20)
        assert!(is_dialog_edit_text_character(b'A'));
        assert!(is_dialog_edit_text_character(b'~')); // 0x7E
        assert!(!is_dialog_edit_text_character(0x00)); // null
        assert!(!is_dialog_edit_text_character(0x0D)); // CR
        assert!(!is_dialog_edit_text_character(0x1B)); // ESC
        assert!(!is_dialog_edit_text_character(0x7F)); // DEL
        assert!(!is_dialog_edit_text_character(0x80));

        // textedit_key_result tests
        let initial = b"Hello World";
        // Insertion at caret (index 5)
        let (res, caret) = textedit_key_result(initial, 5, 5, b',');
        assert_eq!(res, b"Hello, World");
        assert_eq!(caret, 6);

        // Insertion replacing range [5, 11) (" World")
        let (res, caret) = textedit_key_result(initial, 5, 11, b'!');
        assert_eq!(res, b"Hello!");
        assert_eq!(caret, 6);

        // Insertion with reversed endpoints [11, 5)
        let (res, caret) = textedit_key_result(initial, 11, 5, b'!');
        assert_eq!(res, b"Hello!");
        assert_eq!(caret, 6);

        // Backspace at caret > 0
        let (res, caret) = textedit_key_result(initial, 5, 5, 0x08);
        assert_eq!(res, b"Hell World");
        assert_eq!(caret, 4);

        // Backspace deleting selection range [0, 5) ("Hello")
        let (res, caret) = textedit_key_result(initial, 0, 5, 0x08);
        assert_eq!(res, b" World");
        assert_eq!(caret, 0);

        // Backspace at caret 0 (noop)
        let (res, caret) = textedit_key_result(initial, 0, 0, 0x08);
        assert_eq!(res, b"Hello World");
        assert_eq!(caret, 0);
    }

    #[test]
    fn dialog_target_routing_and_frame_geometry() {
        // Target resolution for events
        let front = Some(0x1000);
        let dialog_window = 0x2000;
        let other_window = 0x3000;
        let is_dialog = |ptr| ptr == dialog_window;

        // Update events route to message only if it is a dialog
        assert_eq!(
            dialog_target_for_event(EVENT_UPDATE, dialog_window, is_dialog, front),
            Some(dialog_window)
        );
        assert_eq!(
            dialog_target_for_event(EVENT_UPDATE, other_window, is_dialog, front),
            None
        );

        // Activate events route to message only if it is a dialog
        assert_eq!(
            dialog_target_for_event(EVENT_ACTIVATE, dialog_window, is_dialog, front),
            Some(dialog_window)
        );
        assert_eq!(
            dialog_target_for_event(EVENT_ACTIVATE, other_window, is_dialog, front),
            None
        );

        // Mouse, key, and other events route to front dialog regardless of message
        assert_eq!(
            dialog_target_for_event(EVENT_MOUSE_DOWN, other_window, is_dialog, front),
            front
        );
        assert_eq!(
            dialog_target_for_event(EVENT_KEY_DOWN, other_window, is_dialog, front),
            front
        );
        assert_eq!(
            dialog_target_for_event(EVENT_AUTO_KEY, other_window, is_dialog, front),
            front
        );
        assert_eq!(
            dialog_target_for_event(EVENT_NULL, other_window, is_dialog, front),
            front
        );

        // Inset and outset math
        let base = (10, 20, 100, 200);
        assert_eq!(inset_rect(base, 2, 5), (12, 25, 98, 195));
        assert_eq!(outset_rect(base, 3, 4), (7, 16, 103, 204));

        // Modal dialog dBoxProc frame rect: 8px margin
        assert_eq!(
            dialog_dbox_frame_rect((50, 60, 150, 260)),
            (42, 52, 158, 268)
        );

        // Edit text frame rect: 3px outset
        assert_eq!(edit_text_frame_rect((10, 20, 30, 80)), (7, 17, 33, 83));

        // Dialog text rect: 1px left inset
        assert_eq!(dialog_text_rect((10, 20, 30, 80)), (10, 21, 30, 80));

        // Dialog item base type stripping 0x80
        assert_eq!(
            dialog_item_base_type(DIALOG_ITEM_USER_ITEM),
            DIALOG_ITEM_USER_ITEM
        );
        assert_eq!(
            dialog_item_base_type(DIALOG_ITEM_BUTTON),
            DIALOG_ITEM_BUTTON
        );
        assert_eq!(
            dialog_item_base_type(DIALOG_ITEM_BUTTON | DIALOG_ITEM_DISABLED_FLAG),
            DIALOG_ITEM_BUTTON
        );
        assert_eq!(
            dialog_item_base_type(DIALOG_ITEM_EDIT_TEXT | DIALOG_ITEM_DISABLED_FLAG),
            DIALOG_ITEM_EDIT_TEXT
        );
        assert_eq!(
            dialog_item_base_type(DIALOG_ITEM_PICTURE | DIALOG_ITEM_DISABLED_FLAG),
            DIALOG_ITEM_PICTURE
        );
    }

    #[test]
    fn dialog_record_initial_state_and_offsets() {
        assert_eq!(DIALOG_INITIAL_EDIT_FIELD, -1);
        assert_eq!(DIALOG_INITIAL_EDIT_FIELD as u16, 0xFFFF);
        assert_eq!(DIALOG_INITIAL_EDIT_OPEN, 0);
        assert_eq!(DIALOG_INITIAL_DEFAULT_ITEM, 1);
        assert_eq!(DIALOG_ITEMS_OFFSET, 156);
        assert_eq!(DIALOG_TEXT_HANDLE_OFFSET, 160);
        assert_eq!(DIALOG_EDIT_FIELD_OFFSET, 164);
        assert_eq!(DIALOG_EDIT_OPEN_OFFSET, 166);
        assert_eq!(DIALOG_DEFAULT_ITEM_OFFSET, 168);
        assert_eq!(DIALOG_RESOURCE_ID_OFFSET, 170);
        assert_eq!(DIALOG_CANCEL_ITEM_OFFSET, 172);
        assert_eq!(DIALOG_ALERT_HIT_OFFSET, 174);
        assert_eq!(DIALOG_STANDARD_ALERT_OUTPUT_OFFSET, 176);
        assert_eq!(DIALOG_STANDARD_ALERT_STACK_OFFSET, 180);
        assert_eq!(DIALOG_RECORD_SIZE, 256);
    }

    #[test]
    fn dialog_item_disabled_and_enabled_predicates() {
        assert!(is_dialog_item_enabled(DIALOG_ITEM_BUTTON));
        assert!(is_dialog_item_enabled(DIALOG_ITEM_EDIT_TEXT));
        assert!(is_dialog_item_enabled(DIALOG_ITEM_USER_ITEM));
        assert!(!is_dialog_item_disabled(DIALOG_ITEM_BUTTON));
        assert!(!is_dialog_item_disabled(DIALOG_ITEM_EDIT_TEXT));

        let disabled_btn = DIALOG_ITEM_BUTTON | DIALOG_ITEM_DISABLED_FLAG;
        assert!(is_dialog_item_disabled(disabled_btn));
        assert!(!is_dialog_item_enabled(disabled_btn));

        let disabled_text = DIALOG_ITEM_STATIC_TEXT | DIALOG_ITEM_DISABLED_FLAG;
        assert!(is_dialog_item_disabled(disabled_text));
        assert!(!is_dialog_item_enabled(disabled_text));

        let record_enabled = DialogItemRecord {
            item_offset: 0,
            item_type: DIALOG_ITEM_BUTTON,
            rect: (10, 10, 30, 80),
            handle: 0,
            payload: vec![],
        };
        assert!(record_enabled.is_enabled());
        assert!(!record_enabled.is_disabled());

        let record_disabled = DialogItemRecord {
            item_offset: 0,
            item_type: disabled_btn,
            rect: (10, 10, 30, 80),
            handle: 0,
            payload: vec![],
        };
        assert!(record_disabled.is_disabled());
        assert!(!record_disabled.is_enabled());
    }

    #[test]
    fn dialog_point_conversions_and_bounds() {
        let bounds = (50, 100, 250, 300);
        let screen_pt = (75, 160);
        let local_pt = global_to_dialog_local_point(bounds, screen_pt.0, screen_pt.1);
        assert_eq!(local_pt, (25, 60));

        let converted_back = dialog_local_to_global_point(bounds, local_pt.0, local_pt.1);
        assert_eq!(converted_back, screen_pt);

        assert_eq!(
            global_to_dialog_local_point((100, 100, 200, 200), i16::MIN, i16::MIN),
            (i16::MIN, i16::MIN)
        );
        assert_eq!(
            dialog_local_to_global_point((100, 100, 200, 200), i16::MAX, i16::MAX),
            (i16::MAX, i16::MAX)
        );
    }

    #[test]
    fn dialog_item_hit_testing() {
        assert_eq!(find_dialog_item_hit([], 10, 10, false, |_| true), None);

        let items = [
            // Item 0: disabled static text at (10, 10, 50, 100)
            (
                (10, 10, 50, 100),
                DIALOG_ITEM_STATIC_TEXT | DIALOG_ITEM_DISABLED_FLAG,
            ),
            // Item 1: enabled button at (20, 20, 40, 80)
            ((20, 20, 40, 80), DIALOG_ITEM_BUTTON),
            // Item 2: enabled resource control at (60, 10, 90, 100)
            ((60, 10, 90, 100), DIALOG_ITEM_RESOURCE_CONTROL),
        ];

        // Unfiltered (FindDItem semantics): disabled item 0 wins at (25, 25)
        assert_eq!(
            find_dialog_item_hit(items, 25, 25, false, |_| true),
            Some(0)
        );

        // Filtered enabled-only (DialogSelect / mouse tracking semantics):
        // disabled item 0 is skipped, hitting enabled button item 1!
        assert_eq!(find_dialog_item_hit(items, 25, 25, true, |_| true), Some(1));

        // Point outside all items
        assert_eq!(find_dialog_item_hit(items, 5, 5, true, |_| true), None);

        // Control part filtering: control at index 2 queried
        assert_eq!(
            find_dialog_item_hit(items, 70, 50, true, |_idx| false),
            None
        );
        assert_eq!(
            find_dialog_item_hit(items, 70, 50, true, |idx| idx == 2),
            Some(2)
        );

        // find_dialog_item_at_global_point coordinates test
        let dialog_bounds = (100, 200, 300, 400);
        assert_eq!(
            find_dialog_item_at_global_point(dialog_bounds, 125, 225, items, true, |_| true,),
            Some(1)
        );
    }

    #[test]
    fn normalize_selection_bounds_behavior() {
        assert_eq!(normalize_selection_bounds(2, 5, 10), (2, 5));
        assert_eq!(normalize_selection_bounds(5, 2, 10), (2, 5));
        assert_eq!(normalize_selection_bounds(0, 0, 10), (0, 0));
        assert_eq!(normalize_selection_bounds(10, 10, 10), (10, 10));
        assert_eq!(normalize_selection_bounds(5, 20, 10), (5, 10));
        assert_eq!(normalize_selection_bounds(25, 30, 10), (10, 10));
        assert_eq!(normalize_selection_bounds(30, 5, 10), (5, 10));
    }

    #[test]
    fn item_kind_predicates_and_record_methods() {
        let test_cases: [(
            u8,
            DialogItemKind,
            bool,
            bool,
            bool,
            bool,
            bool,
            bool,
            bool,
            bool,
            bool,
            bool,
            bool,
        ); 9] = [
            // (raw, kind, is_user, is_btn, is_chk, is_rad, is_res_ctrl, is_ctrl, is_stat, is_edit, is_text, is_icon, is_pict)
            (
                DIALOG_ITEM_USER_ITEM,
                DialogItemKind::UserItem,
                true,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
            ),
            (
                DIALOG_ITEM_BUTTON,
                DialogItemKind::Button,
                false,
                true,
                false,
                false,
                false,
                true,
                false,
                false,
                false,
                false,
                false,
            ),
            (
                DIALOG_ITEM_CHECKBOX,
                DialogItemKind::Checkbox,
                false,
                false,
                true,
                false,
                false,
                true,
                false,
                false,
                false,
                false,
                false,
            ),
            (
                DIALOG_ITEM_RADIO,
                DialogItemKind::RadioButton,
                false,
                false,
                false,
                true,
                false,
                true,
                false,
                false,
                false,
                false,
                false,
            ),
            (
                DIALOG_ITEM_RESOURCE_CONTROL,
                DialogItemKind::ResourceControl,
                false,
                false,
                false,
                false,
                true,
                true,
                false,
                false,
                false,
                false,
                false,
            ),
            (
                DIALOG_ITEM_STATIC_TEXT,
                DialogItemKind::StaticText,
                false,
                false,
                false,
                false,
                false,
                false,
                true,
                false,
                true,
                false,
                false,
            ),
            (
                DIALOG_ITEM_EDIT_TEXT,
                DialogItemKind::EditText,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                true,
                true,
                false,
                false,
            ),
            (
                DIALOG_ITEM_ICON,
                DialogItemKind::Icon,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                true,
                false,
            ),
            (
                DIALOG_ITEM_PICTURE,
                DialogItemKind::Picture,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                true,
            ),
        ];

        for (
            raw,
            kind,
            is_user,
            is_btn,
            is_chk,
            is_rad,
            is_res_ctrl,
            is_ctrl,
            is_stat,
            is_edit,
            is_text,
            is_icon,
            is_pict,
        ) in test_cases
        {
            for flag in [0, DIALOG_ITEM_DISABLED_FLAG] {
                let r = raw | flag;
                assert_eq!(is_dialog_item_user_item(r), is_user);
                assert_eq!(is_dialog_item_button(r), is_btn);
                assert_eq!(is_dialog_item_checkbox(r), is_chk);
                assert_eq!(is_dialog_item_radio(r), is_rad);
                assert_eq!(is_dialog_item_resource_control(r), is_res_ctrl);
                assert_eq!(is_dialog_item_control(r), is_ctrl);
                assert_eq!(is_dialog_item_static_text(r), is_stat);
                assert_eq!(is_dialog_item_edit_text(r), is_edit);
                assert_eq!(is_dialog_item_text(r), is_text);
                assert_eq!(is_dialog_item_icon(r), is_icon);
                assert_eq!(is_dialog_item_picture(r), is_pict);

                let record = DialogItemRecord {
                    item_offset: 0,
                    item_type: r,
                    rect: (0, 0, 10, 10),
                    handle: 0,
                    payload: Vec::new(),
                };
                assert_eq!(record.is_user_item(), is_user);
                assert_eq!(record.is_button(), is_btn);
                assert_eq!(record.is_checkbox(), is_chk);
                assert_eq!(record.is_radio(), is_rad);
                assert_eq!(record.is_resource_control(), is_res_ctrl);
                assert_eq!(record.is_control(), is_ctrl);
                assert_eq!(record.is_static_text(), is_stat);
                assert_eq!(record.is_edit_text(), is_edit);
                assert_eq!(record.is_text(), is_text);
                assert_eq!(record.is_icon(), is_icon);
                assert_eq!(record.is_picture(), is_pict);
            }

            assert_eq!(kind.is_user_item(), is_user);
            assert_eq!(kind.is_button(), is_btn);
            assert_eq!(kind.is_checkbox(), is_chk);
            assert_eq!(kind.is_radio(), is_rad);
            assert_eq!(kind.is_resource_control(), is_res_ctrl);
            assert_eq!(kind.is_control(), is_ctrl);
            assert_eq!(kind.is_static_text(), is_stat);
            assert_eq!(kind.is_edit_text(), is_edit);
            assert_eq!(kind.is_text(), is_text);
            assert_eq!(kind.is_icon(), is_icon);
            assert_eq!(kind.is_picture(), is_pict);
        }
    }

    #[test]
    fn dialog_select_action_and_evaluation() {
        let dialog = 0x2000;
        let bounds = (10, 20, 110, 120);

        // Action properties
        let no_act = DialogSelectAction::NoAction;
        assert!(!no_act.is_handled());
        assert!(!no_act.should_set_dialog_ptr());
        assert_eq!(no_act.target_dialog(), None);
        assert_eq!(no_act.item_hit(), None);

        let update_act = DialogSelectAction::Update { dialog };
        assert!(!update_act.is_handled());
        assert!(update_act.should_set_dialog_ptr());
        assert_eq!(update_act.target_dialog(), Some(dialog));
        assert_eq!(update_act.item_hit(), None);

        let activate_act = DialogSelectAction::Activate { dialog };
        assert!(!activate_act.is_handled());
        assert!(activate_act.should_set_dialog_ptr());
        assert_eq!(activate_act.target_dialog(), Some(dialog));
        assert_eq!(activate_act.item_hit(), None);

        let idle_act = DialogSelectAction::Idle {
            dialog,
            edit_item: 1,
        };
        assert!(!idle_act.is_handled());
        assert!(!idle_act.should_set_dialog_ptr());
        assert_eq!(idle_act.target_dialog(), Some(dialog));
        assert_eq!(idle_act.item_hit(), None);

        let hit_act = DialogSelectAction::ItemHit {
            dialog,
            item_no: 3,
            is_edit_text: false,
            is_resource_control: false,
        };
        assert!(hit_act.is_handled());
        assert!(hit_act.should_set_dialog_ptr());
        assert_eq!(hit_act.target_dialog(), Some(dialog));
        assert_eq!(hit_act.item_hit(), Some(3));

        let dis_act = DialogSelectAction::DisabledItemHit {
            dialog,
            item_no: 4,
        };
        assert!(!dis_act.is_handled());
        assert!(!dis_act.should_set_dialog_ptr());
        assert_eq!(dis_act.target_dialog(), Some(dialog));
        assert_eq!(dis_act.item_hit(), None);

        let key_act = DialogSelectAction::KeyStroke {
            dialog,
            edit_item: 2,
            character: b'Z',
        };
        assert!(key_act.is_handled());
        assert!(key_act.should_set_dialog_ptr());
        assert_eq!(key_act.target_dialog(), Some(dialog));
        assert_eq!(key_act.item_hit(), Some(2));

        // evaluate_dialog_select: No target dialog
        assert_eq!(
            evaluate_dialog_select(
                EVENT_NULL,
                0,
                0,
                0,
                None,
                Some(bounds),
                None,
                |_, _| None,
            ),
            DialogSelectAction::NoAction
        );

        // evaluate_dialog_select: Update event
        assert_eq!(
            evaluate_dialog_select(
                EVENT_UPDATE,
                dialog,
                0,
                0,
                Some(dialog),
                Some(bounds),
                None,
                |_, _| None,
            ),
            DialogSelectAction::Update { dialog }
        );
        // Update for different window
        assert_eq!(
            evaluate_dialog_select(
                EVENT_UPDATE,
                0x9999,
                0,
                0,
                Some(dialog),
                Some(bounds),
                None,
                |_, _| None,
            ),
            DialogSelectAction::NoAction
        );

        // evaluate_dialog_select: Activate event
        assert_eq!(
            evaluate_dialog_select(
                EVENT_ACTIVATE,
                dialog,
                0,
                0,
                Some(dialog),
                Some(bounds),
                None,
                |_, _| None,
            ),
            DialogSelectAction::Activate { dialog }
        );

        // evaluate_dialog_select: Null event with active edit text
        assert_eq!(
            evaluate_dialog_select(
                EVENT_NULL,
                0,
                0,
                0,
                Some(dialog),
                Some(bounds),
                Some((2, DIALOG_ITEM_EDIT_TEXT)),
                |_, _| None,
            ),
            DialogSelectAction::Idle {
                dialog,
                edit_item: 2
            }
        );
        // Null event with active edit text disabled
        assert_eq!(
            evaluate_dialog_select(
                EVENT_NULL,
                0,
                0,
                0,
                Some(dialog),
                Some(bounds),
                Some((2, DIALOG_ITEM_EDIT_TEXT | DIALOG_ITEM_DISABLED_FLAG)),
                |_, _| None,
            ),
            DialogSelectAction::NoAction
        );
        // Null event with active non-edit text (e.g. button)
        assert_eq!(
            evaluate_dialog_select(
                EVENT_NULL,
                0,
                0,
                0,
                Some(dialog),
                Some(bounds),
                Some((1, DIALOG_ITEM_BUTTON)),
                |_, _| None,
            ),
            DialogSelectAction::NoAction
        );

        // evaluate_dialog_select: Mouse down
        // Mouse down outside bounds
        assert_eq!(
            evaluate_dialog_select(
                EVENT_MOUSE_DOWN,
                0,
                5,
                5,
                Some(dialog),
                Some(bounds),
                None,
                |_, _| Some((1, DIALOG_ITEM_BUTTON)),
            ),
            DialogSelectAction::NoAction
        );
        // Mouse down inside bounds, hitting enabled button
        assert_eq!(
            evaluate_dialog_select(
                EVENT_MOUSE_DOWN,
                0,
                50,
                50,
                Some(dialog),
                Some(bounds),
                None,
                |_, _| Some((1, DIALOG_ITEM_BUTTON)),
            ),
            DialogSelectAction::ItemHit {
                dialog,
                item_no: 1,
                is_edit_text: false,
                is_resource_control: false,
            }
        );
        // Mouse down inside bounds, hitting enabled edit text
        assert_eq!(
            evaluate_dialog_select(
                EVENT_MOUSE_DOWN,
                0,
                50,
                50,
                Some(dialog),
                Some(bounds),
                None,
                |_, _| Some((2, DIALOG_ITEM_EDIT_TEXT)),
            ),
            DialogSelectAction::ItemHit {
                dialog,
                item_no: 2,
                is_edit_text: true,
                is_resource_control: false,
            }
        );
        // Mouse down inside bounds, hitting enabled res control
        assert_eq!(
            evaluate_dialog_select(
                EVENT_MOUSE_DOWN,
                0,
                50,
                50,
                Some(dialog),
                Some(bounds),
                None,
                |_, _| Some((3, DIALOG_ITEM_RESOURCE_CONTROL)),
            ),
            DialogSelectAction::ItemHit {
                dialog,
                item_no: 3,
                is_edit_text: false,
                is_resource_control: true,
            }
        );
        // Mouse down inside bounds, hitting disabled item
        assert_eq!(
            evaluate_dialog_select(
                EVENT_MOUSE_DOWN,
                0,
                50,
                50,
                Some(dialog),
                Some(bounds),
                None,
                |_, _| Some((4, DIALOG_ITEM_BUTTON | DIALOG_ITEM_DISABLED_FLAG)),
            ),
            DialogSelectAction::DisabledItemHit { dialog, item_no: 4 }
        );
        // Mouse down inside bounds, no item hit
        assert_eq!(
            evaluate_dialog_select(
                EVENT_MOUSE_DOWN,
                0,
                50,
                50,
                Some(dialog),
                Some(bounds),
                None,
                |_, _| None,
            ),
            DialogSelectAction::NoAction
        );

        // evaluate_dialog_select: KeyDown / AutoKey
        assert_eq!(
            evaluate_dialog_select(
                EVENT_KEY_DOWN,
                b'A' as u32,
                0,
                0,
                Some(dialog),
                Some(bounds),
                Some((2, DIALOG_ITEM_EDIT_TEXT)),
                |_, _| None,
            ),
            DialogSelectAction::KeyStroke {
                dialog,
                edit_item: 2,
                character: b'A',
            }
        );
        assert_eq!(
            evaluate_dialog_select(
                EVENT_AUTO_KEY,
                0x08,
                0,
                0,
                Some(dialog),
                Some(bounds),
                Some((2, DIALOG_ITEM_EDIT_TEXT)),
                |_, _| None,
            ),
            DialogSelectAction::KeyStroke {
                dialog,
                edit_item: 2,
                character: 0x08,
            }
        );
        // KeyDown when no active edit text
        assert_eq!(
            evaluate_dialog_select(
                EVENT_KEY_DOWN,
                b'A' as u32,
                0,
                0,
                Some(dialog),
                Some(bounds),
                None,
                |_, _| None,
            ),
            DialogSelectAction::NoAction
        );
    }

    #[test]
    fn dialog_item_visibility_transitions_and_find_dialog_item() {
        let visible_btn_rect = (10, 20, 30, 80);
        let hidden_btn_rect = (
            10,
            20 + DIALOG_ITEM_HIDDEN_OFFSET,
            30,
            80 + DIALOG_ITEM_HIDDEN_OFFSET,
        );
        let visible_edit_rect = (40, 50, 60, 150);
        let hidden_edit_rect = (
            40,
            50 + DIALOG_ITEM_HIDDEN_OFFSET,
            60,
            150 + DIALOG_ITEM_HIDDEN_OFFSET,
        );

        // evaluate_hide_dialog_item on visible button
        let hide_btn = evaluate_hide_dialog_item(DIALOG_ITEM_BUTTON, visible_btn_rect);
        assert_eq!(
            hide_btn,
            Some(DialogItemVisibilityChange {
                new_rect: hidden_btn_rect,
                enclosing_rect: visible_btn_rect,
            })
        );
        // evaluate_hide_dialog_item on already-hidden button -> None
        assert_eq!(
            evaluate_hide_dialog_item(DIALOG_ITEM_BUTTON, hidden_btn_rect),
            None
        );

        // evaluate_hide_dialog_item on visible edit text (outset 3px enclosing)
        let hide_edit = evaluate_hide_dialog_item(DIALOG_ITEM_EDIT_TEXT, visible_edit_rect);
        assert_eq!(
            hide_edit,
            Some(DialogItemVisibilityChange {
                new_rect: hidden_edit_rect,
                enclosing_rect: (37, 47, 63, 153),
            })
        );

        // evaluate_show_dialog_item on already-visible button -> None
        assert_eq!(
            evaluate_show_dialog_item(DIALOG_ITEM_BUTTON, visible_btn_rect, None),
            None
        );

        // evaluate_show_dialog_item on hidden button without original_rect
        let show_btn = evaluate_show_dialog_item(DIALOG_ITEM_BUTTON, hidden_btn_rect, None);
        assert_eq!(
            show_btn,
            Some(DialogItemVisibilityChange {
                new_rect: visible_btn_rect,
                enclosing_rect: visible_btn_rect,
            })
        );

        // evaluate_show_dialog_item on hidden button with original_rect
        let show_btn_orig = evaluate_show_dialog_item(
            DIALOG_ITEM_BUTTON,
            hidden_btn_rect,
            Some(visible_btn_rect),
        );
        assert_eq!(
            show_btn_orig,
            Some(DialogItemVisibilityChange {
                new_rect: visible_btn_rect,
                enclosing_rect: visible_btn_rect,
            })
        );

        // evaluate_show_dialog_item on hidden edit text (outset 3px enclosing)
        let show_edit = evaluate_show_dialog_item(DIALOG_ITEM_EDIT_TEXT, hidden_edit_rect, None);
        assert_eq!(
            show_edit,
            Some(DialogItemVisibilityChange {
                new_rect: visible_edit_rect,
                enclosing_rect: (37, 47, 63, 153),
            })
        );

        // DialogItemRecord method delegation
        let btn_record = DialogItemRecord {
            item_offset: 0,
            item_type: DIALOG_ITEM_BUTTON,
            rect: visible_btn_rect,
            handle: 0,
            payload: Vec::new(),
        };
        assert!(!btn_record.is_hidden());
        assert_eq!(btn_record.evaluate_hide(), hide_btn);
        assert_eq!(btn_record.evaluate_show(None), None);

        let mut hidden_record = btn_record;
        hidden_record.rect = hidden_btn_rect;
        assert!(hidden_record.is_hidden());
        assert_eq!(hidden_record.evaluate_hide(), None);
        assert_eq!(hidden_record.evaluate_show(None), show_btn);

        // evaluate_find_dialog_item_rects
        let rects = [
            (10, 10, 40, 60),       // Item 0: button
            (50, 10, 70, 100),      // Item 1: disabled static text
            (10, 16404, 30, 16464), // Item 2: hidden item
        ];
        // Point in item 0
        assert_eq!(evaluate_find_dialog_item_rects(&rects, 25, 25), 0);
        // Point in disabled item 1 (FindDItem includes disabled items per IM:IV-60!)
        assert_eq!(evaluate_find_dialog_item_rects(&rects, 60, 25), 1);
        // Point in item 2's visible coordinates (20, 20) hits item 0, not hidden item 2
        assert_eq!(evaluate_find_dialog_item_rects(&rects, 20, 20), 0);
        // Point outside all items
        assert_eq!(evaluate_find_dialog_item_rects(&rects, 200, 200), -1);
        // Empty item list
        let empty: [(i16, i16, i16, i16); 0] = [];
        assert_eq!(evaluate_find_dialog_item_rects(&empty, 10, 10), -1);

        // evaluate_find_dialog_item with control_part_hit and fall-through
        let ditl_items = [
            // Item 0: Group box control around item 1
            ((10, 10, 150, 250), DIALOG_ITEM_RESOURCE_CONTROL),
            // Item 1: Button inside group box
            ((40, 30, 60, 110), DIALOG_ITEM_BUTTON),
            // Item 2: Disabled static text
            ((160, 10, 180, 100), DIALOG_ITEM_STATIC_TEXT | DIALOG_ITEM_DISABLED_FLAG),
        ];
        let hit_test = |v, h| {
            evaluate_find_dialog_item(ditl_items, v, h, |idx| {
                // Group box body returns false (kControlNoPart)
                idx != 0
            })
        };
        // Point inside button: falls through group box body to button (item 1)
        assert_eq!(hit_test(50, 70), 1);
        // Point inside group box body only: falls through and hits nothing (-1)
        assert_eq!(hit_test(120, 200), -1);
        // Point inside disabled static text: returns item 2 (disabled items returned)
        assert_eq!(hit_test(170, 50), 2);
        // Point outside all items
        assert_eq!(hit_test(300, 300), -1);
    }

    #[test]
    fn dialog_item_header_query_update_and_control_evaluation() {
        assert_eq!(
            DialogItemHeader::ZERO,
            DialogItemHeader {
                item_type: 0,
                handle: 0,
                rect: (0, 0, 0, 0),
            }
        );
        let header = DialogItemHeader::new(4, 0x12345678, (10, 20, 30, 40));
        assert_eq!(header.item_type, 4);
        assert_eq!(header.handle, 0x12345678);
        assert_eq!(header.rect, (10, 20, 30, 40));

        let mut record = DialogItemRecord {
            item_offset: 2,
            item_type: DIALOG_ITEM_BUTTON,
            rect: (10, 20, 30, 40),
            handle: 0x1000,
            payload: b"OK".to_vec(),
        };
        assert_eq!(
            record.header(),
            DialogItemHeader {
                item_type: DIALOG_ITEM_BUTTON as u16,
                handle: 0x1000,
                rect: (10, 20, 30, 40),
            }
        );

        record.update_header(DIALOG_ITEM_CHECKBOX, 0x2000, (50, 60, 70, 80));
        assert_eq!(record.item_type, DIALOG_ITEM_CHECKBOX);
        assert_eq!(record.handle, 0x2000);
        assert_eq!(record.rect, (50, 60, 70, 80));
        assert_eq!(
            record.header(),
            DialogItemHeader {
                item_type: DIALOG_ITEM_CHECKBOX as u16,
                handle: 0x2000,
                rect: (50, 60, 70, 80),
            }
        );

        let records = [record];
        // 1-indexed item query: item 1 exists
        assert_eq!(
            evaluate_get_dialog_item(&records, 1, |r| r.header()),
            records[0].header()
        );
        // Item 0 is invalid -> DialogItemHeader::ZERO
        assert_eq!(
            evaluate_get_dialog_item(&records, 0, |r| r.header()),
            DialogItemHeader::ZERO
        );
        // Item 2 is out of bounds -> DialogItemHeader::ZERO
        assert_eq!(
            evaluate_get_dialog_item(&records, 2, |r| r.header()),
            DialogItemHeader::ZERO
        );

        // evaluate_get_dialog_item_as_control
        assert_eq!(
            evaluate_get_dialog_item_as_control(DIALOG_ITEM_BUTTON, 0x3000),
            Ok(0x3000)
        );
        assert_eq!(
            evaluate_get_dialog_item_as_control(DIALOG_ITEM_RESOURCE_CONTROL, 0x4000),
            Ok(0x4000)
        );
        // Control with null handle -> Err(-50)
        assert_eq!(
            evaluate_get_dialog_item_as_control(DIALOG_ITEM_BUTTON, 0),
            Err(-50)
        );
        // Static text (not a control) -> Err(-50)
        assert_eq!(
            evaluate_get_dialog_item_as_control(DIALOG_ITEM_STATIC_TEXT, 0x3000),
            Err(-50)
        );
        // User item (not a control) -> Err(-50)
        assert_eq!(
            evaluate_get_dialog_item_as_control(DIALOG_ITEM_USER_ITEM, 0x3000),
            Err(-50)
        );

        // write_ditl_item_header_bytes
        let mut buf = vec![0u8; 32];
        let written = write_ditl_item_header_bytes(
            &mut buf,
            4,
            0xAABBCCDD,
            (11, 22, 33, 44),
            DIALOG_ITEM_RADIO,
        );
        assert!(written);
        assert_eq!(&buf[4..8], &0xAABBCCDDu32.to_be_bytes());
        assert_eq!(&buf[8..10], &11i16.to_be_bytes());
        assert_eq!(&buf[10..12], &22i16.to_be_bytes());
        assert_eq!(&buf[12..14], &33i16.to_be_bytes());
        assert_eq!(&buf[14..16], &44i16.to_be_bytes());
        assert_eq!(buf[16], DIALOG_ITEM_RADIO);

        // write_ditl_item_header_bytes with buffer too small
        let mut small_buf = [0u8; 10];
        assert!(!write_ditl_item_header_bytes(
            &mut small_buf,
            0,
            0,
            (0, 0, 0, 0),
            0
        ));
    }

    #[test]
    fn dialog_dispatch_extension_routines_and_count_ditl_evaluation() {
        // evaluate_set_dialog_default_item_parameters
        assert!(evaluate_set_dialog_default_item_parameters(0x1000, 1).is_ok());
        assert!(evaluate_set_dialog_default_item_parameters(0x1000, -1).is_ok());
        assert_eq!(
            evaluate_set_dialog_default_item_parameters(0, 1),
            Err(DIALOG_PARAM_ERR)
        );

        // evaluate_set_dialog_cancel_item_parameters
        assert!(evaluate_set_dialog_cancel_item_parameters(0x1000, 2).is_ok());
        assert!(evaluate_set_dialog_cancel_item_parameters(0x1000, 0).is_ok());
        assert_eq!(
            evaluate_set_dialog_cancel_item_parameters(0, 2),
            Err(DIALOG_PARAM_ERR)
        );

        // evaluate_set_dialog_tracks_cursor_parameters
        assert!(evaluate_set_dialog_tracks_cursor_parameters(0x1000, true).is_ok());
        assert!(evaluate_set_dialog_tracks_cursor_parameters(0, false).is_ok());

        // evaluate_get_std_filter_proc
        assert_eq!(evaluate_get_std_filter_proc(0x2000, true), Ok(DIALOG_NO_ERR));
        assert_eq!(evaluate_get_std_filter_proc(0, true), Err(DIALOG_PARAM_ERR));
        assert_eq!(evaluate_get_std_filter_proc(0x2000, false), Err(DIALOG_PARAM_ERR));
        assert_eq!(evaluate_get_std_filter_proc(0, false), Err(DIALOG_PARAM_ERR));

        // evaluate_count_ditl
        // Header count word present: count = word + 1
        assert_eq!(evaluate_count_ditl(Some(0), 10), 1);
        assert_eq!(evaluate_count_ditl(Some(1), 10), 2);
        assert_eq!(evaluate_count_ditl(Some(4), 0), 5);
        // Header count word is 0xFFFF (-1), meaning 0 items: falls back to record_count
        assert_eq!(evaluate_count_ditl(Some(u16::MAX), 0), 0);
        assert_eq!(evaluate_count_ditl(Some(u16::MAX), 3), 3);
        // Header count word is None: falls back to record_count
        assert_eq!(evaluate_count_ditl(None, 0), 0);
        assert_eq!(evaluate_count_ditl(None, 7), 7);
    }

    #[test]
    fn select_dialog_item_text_evaluation() {
        // evaluate_select_dialog_item_text invalid inputs
        assert_eq!(
            evaluate_select_dialog_item_text(0, 1, true, 0, 5, 10),
            None
        );
        assert_eq!(
            evaluate_select_dialog_item_text(0x1000, 0, true, 0, 5, 10),
            None
        );
        assert_eq!(
            evaluate_select_dialog_item_text(0x1000, 1, false, 0, 5, 10),
            None
        );

        // evaluate_select_dialog_item_text normal selection
        let eval = evaluate_select_dialog_item_text(0x1000, 1, true, 2, 5, 10).unwrap();
        assert_eq!(eval.edit_field, 0);
        assert_eq!(eval.sel_start, 2);
        assert_eq!(eval.sel_end, 5);
        assert_eq!(eval.selection_range(), (2, 5));

        // evaluate_select_dialog_item_text item 3 (edit field index 2)
        let eval_item3 = evaluate_select_dialog_item_text(0x1000, 3, true, 1, 4, 10).unwrap();
        assert_eq!(eval_item3.edit_field, 2);
        assert_eq!(eval_item3.sel_start, 1);
        assert_eq!(eval_item3.sel_end, 4);

        // Select all special cases: (0, -1) and (0, 32767)
        let eval_all1 = evaluate_select_dialog_item_text(0x1000, 2, true, 0, -1, 15).unwrap();
        assert_eq!(eval_all1.edit_field, 1);
        assert_eq!(eval_all1.selection_range(), (0, 15));

        let eval_all2 = evaluate_select_dialog_item_text(0x1000, 2, true, 0, i16::MAX, 15).unwrap();
        assert_eq!(eval_all2.selection_range(), (0, 15));

        // Empty text with select all
        let eval_empty = evaluate_select_dialog_item_text(0x1000, 1, true, 0, -1, 0).unwrap();
        assert_eq!(eval_empty.selection_range(), (0, 0));

        // Clamping out-of-bounds
        let eval_clamped = evaluate_select_dialog_item_text(0x1000, 1, true, -10, 50, 12).unwrap();
        assert_eq!(eval_clamped.selection_range(), (0, 12));

        // Reversed bounds swapping
        let eval_swapped = evaluate_select_dialog_item_text(0x1000, 1, true, 8, 3, 12).unwrap();
        assert_eq!(eval_swapped.selection_range(), (3, 8));

        // DialogItemRecord::select_text and evaluate_select_text
        let edit_record = DialogItemRecord {
            item_offset: 0,
            item_type: DIALOG_ITEM_EDIT_TEXT,
            rect: (0, 0, 20, 100),
            handle: 0,
            payload: b"Hello World".to_vec(),
        };
        assert_eq!(edit_record.select_text(0, -1), Some((0, 11)));
        assert_eq!(edit_record.evaluate_select_text(2, 7), Some((2, 7)));

        let button_record = DialogItemRecord {
            item_offset: 0,
            item_type: DIALOG_ITEM_BUTTON,
            rect: (0, 0, 20, 100),
            handle: 0,
            payload: b"OK".to_vec(),
        };
        assert_eq!(button_record.select_text(0, -1), None);
        assert_eq!(button_record.evaluate_select_text(0, 1), None);
    }

    #[test]
    fn dialog_teardown_evaluation() {
        // Zero dialog pointer returns None for all variants
        assert_eq!(evaluate_dialog_teardown_parameters(0, false), None);
        assert_eq!(evaluate_dialog_teardown_parameters(0, true), None);
        assert_eq!(evaluate_close_dialog_parameters(0), None);
        assert_eq!(evaluate_dispose_dialog_parameters(0), None);

        // CloseDialog evaluation
        let close_params = evaluate_close_dialog_parameters(0x1000).unwrap();
        assert_eq!(close_params.dialog_ptr(), 0x1000);
        assert_eq!(close_params.dispose_record(), false);
        assert!(close_params.is_close());
        assert!(!close_params.is_dispose());
        assert_eq!(
            close_params,
            DialogTeardownParameters::new(0x1000, false)
        );

        // DisposeDialog evaluation
        let dispose_params = evaluate_dispose_dialog_parameters(0x2000).unwrap();
        assert_eq!(dispose_params.dialog_ptr(), 0x2000);
        assert_eq!(dispose_params.dispose_record(), true);
        assert!(dispose_params.is_dispose());
        assert!(!dispose_params.is_close());
        assert_eq!(
            dispose_params,
            DialogTeardownParameters::new(0x2000, true)
        );

        // evaluate_dialog_teardown_parameters equivalence
        assert_eq!(
            evaluate_dialog_teardown_parameters(0x1000, false),
            Some(close_params)
        );
        assert_eq!(
            evaluate_dialog_teardown_parameters(0x2000, true),
            Some(dispose_params)
        );
    }

    #[test]
    fn draw_and_update_dialog_evaluation() {
        // DrawDialog evaluation
        assert_eq!(evaluate_draw_dialog_parameters(0), None);
        let draw_params = evaluate_draw_dialog_parameters(0x5000).unwrap();
        assert_eq!(draw_params.dialog_ptr(), 0x5000);
        assert_eq!(draw_params, DrawDialogParameters::new(0x5000));

        // UpdateDialog evaluation
        assert_eq!(evaluate_update_dialog_parameters(0, 0x1000), None);
        assert_eq!(evaluate_update_dialog_parameters(0x5000, 0), None);
        assert_eq!(evaluate_update_dialog_parameters(0, 0), None);

        let update_params = evaluate_update_dialog_parameters(0x5000, 0x2000).unwrap();
        assert_eq!(update_params.dialog_ptr(), 0x5000);
        assert_eq!(update_params.update_rgn(), 0x2000);
        assert_eq!(update_params.as_draw_dialog(), draw_params);
        assert_eq!(
            update_params,
            UpdateDialogParameters::new(0x5000, 0x2000)
        );

        // has_user_proc
        let user_with_proc = DialogItemRecord {
            item_offset: 0,
            item_type: DIALOG_ITEM_USER_ITEM,
            rect: (10, 10, 50, 50),
            handle: 0x4000,
            payload: Vec::new(),
        };
        assert!(user_with_proc.has_user_proc());

        let user_without_proc = DialogItemRecord {
            item_offset: 0,
            item_type: DIALOG_ITEM_USER_ITEM,
            rect: (10, 10, 50, 50),
            handle: 0,
            payload: Vec::new(),
        };
        assert!(!user_without_proc.has_user_proc());

        let button_with_proc = DialogItemRecord {
            item_offset: 0,
            item_type: DIALOG_ITEM_BUTTON,
            rect: (10, 10, 50, 50),
            handle: 0x4000,
            payload: Vec::new(),
        };
        assert!(!button_with_proc.has_user_proc());

        // dialog_effective_update_rect
        let bounds = (100, 100, 300, 400);
        let intersecting = (150, 150, 250, 250);
        assert_eq!(dialog_effective_update_rect(bounds, intersecting), intersecting);
        let local_rect = (10, 10, 50, 50);
        assert_eq!(
            dialog_effective_update_rect(bounds, local_rect),
            (110, 110, 150, 150)
        );

        // dialog_item_intersects_draw_area
        let in_bounds_item_rect = (10, 10, 50, 50); // global: (110, 110, 150, 150)
        assert!(dialog_item_intersects_draw_area(bounds, in_bounds_item_rect, None));
        assert!(dialog_item_intersects_draw_area(
            bounds,
            in_bounds_item_rect,
            Some((100, 100, 200, 200))
        ));
        assert!(!dialog_item_intersects_draw_area(
            bounds,
            in_bounds_item_rect,
            Some((200, 200, 300, 300))
        ));

        let offscreen_item_rect = (500, 500, 550, 550);
        assert!(!dialog_item_intersects_draw_area(bounds, offscreen_item_rect, None));
        assert!(!dialog_item_intersects_draw_area(
            bounds,
            offscreen_item_rect,
            Some((100, 100, 200, 200))
        ));

        // evaluate_dialog_user_item_numbers
        let items = vec![
            (user_with_proc.has_user_proc(), user_with_proc.rect),
            (button_with_proc.has_user_proc(), button_with_proc.rect),
            (user_without_proc.has_user_proc(), user_without_proc.rect),
            (true, (500, 500, 550, 550)), // user with proc but offscreen
            (true, (60, 60, 100, 100)),   // user with proc in bounds (item 5, global 160..200)
        ];

        assert_eq!(
            evaluate_dialog_user_item_numbers(items.clone(), bounds, None),
            vec![1, 5]
        );
        assert_eq!(
            evaluate_dialog_user_item_numbers(
                items.clone(),
                bounds,
                Some((100, 100, 155, 155)) // covers item 1 (110..150) but not item 5 (160..200)
            ),
            vec![1]
        );
        assert_eq!(
            evaluate_dialog_user_item_numbers(
                items.clone(),
                bounds,
                Some((155, 155, 250, 250)) // covers item 5 (160..200) but not item 1 (110..150)
            ),
            vec![5]
        );
        assert_eq!(
            evaluate_dialog_user_item_numbers(
                items,
                bounds,
                Some((250, 250, 300, 300)) // covers neither item
            ),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn get_and_set_dialog_item_text_evaluation() {
        // evaluate_get_dialog_item_text_parameters
        assert_eq!(evaluate_get_dialog_item_text_parameters(0x1000, 0, true), None);
        assert_eq!(evaluate_get_dialog_item_text_parameters(0x1000, 0x2000, false), None);
        assert_eq!(evaluate_get_dialog_item_text_parameters(0x1000, 0, false), None);

        let get_params = evaluate_get_dialog_item_text_parameters(0x1000, 0x2000, true).unwrap();
        assert_eq!(get_params.item_handle(), 0x1000);
        assert_eq!(get_params.handle(), 0x1000);
        assert_eq!(get_params.text_out_ptr(), 0x2000);
        assert_eq!(get_params.text_ptr(), 0x2000);
        assert!(get_params.has_handle());

        let get_nil_handle_params = evaluate_get_dialog_item_text_parameters(0, 0x2000, true).unwrap();
        assert_eq!(get_nil_handle_params.item_handle(), 0);
        assert!(!get_nil_handle_params.has_handle());

        let direct_get_params = GetDialogItemTextParameters::new(0x4000, 0x5000);
        assert_eq!(direct_get_params.item_handle(), 0x4000);
        assert_eq!(direct_get_params.text_out_ptr(), 0x5000);

        // evaluate_set_dialog_item_text_parameters
        assert_eq!(evaluate_set_dialog_item_text_parameters(0x1000, 0), None);

        let set_params = evaluate_set_dialog_item_text_parameters(0x1000, 0x3000).unwrap();
        assert_eq!(set_params.item_handle(), 0x1000);
        assert_eq!(set_params.handle(), 0x1000);
        assert_eq!(set_params.text_ptr(), 0x3000);
        assert!(set_params.has_handle());

        let set_nil_handle_params = evaluate_set_dialog_item_text_parameters(0, 0x3000).unwrap();
        assert_eq!(set_nil_handle_params.item_handle(), 0);
        assert!(!set_nil_handle_params.has_handle());

        let direct_set_params = SetDialogItemTextParameters::new(0x6000, 0x7000);
        assert_eq!(direct_set_params.item_handle(), 0x6000);
        assert_eq!(direct_set_params.text_ptr(), 0x7000);

        // evaluate_get_dialog_item_text
        assert_eq!(evaluate_get_dialog_item_text(0x1000, 0, b"Hello"), None);

        let get_eval = evaluate_get_dialog_item_text(0x1000, 0x2000, b"Hello").unwrap();
        assert_eq!(get_eval.item_handle(), 0x1000);
        assert_eq!(get_eval.text_out_ptr(), 0x2000);
        assert_eq!(get_eval.len(), 5);
        assert_eq!(get_eval.text(), b"Hello");
        assert_eq!(get_eval.decoded_text(), "Hello");
        assert_eq!(get_eval.pstring_bytes(), b"\x05Hello");

        let empty_get = evaluate_get_dialog_item_text(0x1000, 0x2000, b"").unwrap();
        assert_eq!(empty_get.len(), 0);
        assert_eq!(empty_get.text(), b"");
        assert_eq!(empty_get.pstring_bytes(), vec![0]);

        let long_bytes = vec![b'A'; 300];
        let clamped_get = evaluate_get_dialog_item_text(0x1000, 0x2000, &long_bytes).unwrap();
        assert_eq!(clamped_get.len(), 255);
        assert_eq!(clamped_get.text().len(), 255);
        assert_eq!(clamped_get.pstring_bytes().len(), 256);

        // evaluate_set_dialog_item_text
        assert_eq!(evaluate_set_dialog_item_text(0x1000, 0, b"Submit"), None);

        let set_eval = evaluate_set_dialog_item_text(0x1000, 0x3000, b"Submit").unwrap();
        assert_eq!(set_eval.item_handle(), 0x1000);
        assert_eq!(set_eval.text_ptr(), 0x3000);
        assert_eq!(set_eval.bytes(), b"Submit");
        assert_eq!(set_eval.byte_len(), 6);
        assert_eq!(set_eval.text(), "Submit");

        let long_raw = vec![b'Z'; 300];
        let clamped_set = evaluate_set_dialog_item_text(0x1000, 0x3000, &long_raw).unwrap();
        assert_eq!(clamped_set.byte_len(), 255);
        assert_eq!(clamped_set.bytes().len(), 255);

        // DialogItemRecord::set_text
        let mut edit_record = DialogItemRecord {
            item_offset: 0,
            item_type: DIALOG_ITEM_EDIT_TEXT,
            rect: (10, 10, 30, 100),
            handle: 0,
            payload: b"Original".to_vec(),
        };
        edit_record.set_text("Modified");
        assert_eq!(edit_record.text(), "Modified");

        let mut button_record = DialogItemRecord {
            item_offset: 0,
            item_type: DIALOG_ITEM_BUTTON,
            rect: (10, 10, 30, 100),
            handle: 0,
            payload: b"OK".to_vec(),
        };
        button_record.set_text("Cancel");
        assert_eq!(button_record.text(), "Cancel");

        let mut icon_record = DialogItemRecord {
            item_offset: 0,
            item_type: DIALOG_ITEM_ICON,
            rect: (10, 10, 42, 42),
            handle: 0,
            payload: vec![0, 128], // icon res id 128
        };
        icon_record.set_text("NotText");
        assert_eq!(icon_record.payload, vec![0, 128]);
    }

    #[test]
    fn param_text_and_init_dialogs_evaluation() {
        // InitDialogs evaluation
        let init_null = evaluate_init_dialogs(0);
        assert_eq!(init_null.resume_proc(), 0);
        assert_eq!(init_null.initial_alert_stage(), 0);
        assert_eq!(init_null.da_beeper(), 0);
        assert_eq!(init_null.da_strings_count(), 4);
        assert_eq!(init_null.initial_dialog_font(), DIALOG_INITIAL_FONT);

        let init_custom = evaluate_init_dialogs(0x0012_3456);
        assert_eq!(init_custom.resume_proc(), 0x0012_3456);
        assert_eq!(init_custom.initial_alert_stage(), 0);
        assert_eq!(init_custom.da_beeper(), 0);
        assert_eq!(init_custom.da_strings_count(), 4);
        assert_eq!(init_custom.initial_dialog_font(), 0);
        assert_eq!(evaluate_dialog_font(3), 3);
        assert_eq!(evaluate_dialog_font(0), 0);

        // ParamText evaluation: all NIL
        let all_nil = evaluate_param_text([None, None, None, None]);
        for i in 0..4 {
            assert!(!all_nil.has_update(i));
            assert_eq!(all_nil.slot(i), None);
        }
        let initial_slots = [
            b"apple".to_vec(),
            b"banana".to_vec(),
            b"cherry".to_vec(),
            b"date".to_vec(),
        ];
        assert_eq!(all_nil.merged_with(&initial_slots), initial_slots);
        let mut target = initial_slots.clone();
        all_nil.apply_to(&mut target);
        assert_eq!(target, initial_slots);

        // ParamText evaluation: selective update
        let selective = evaluate_param_text([Some(b"avocado"), None, Some(b"cantaloupe"), None]);
        assert!(selective.has_update(0));
        assert!(!selective.has_update(1));
        assert!(selective.has_update(2));
        assert!(!selective.has_update(3));
        assert_eq!(selective.slot(0), Some(b"avocado".as_slice()));
        assert_eq!(selective.slot(1), None);
        assert_eq!(selective.slot(2), Some(b"cantaloupe".as_slice()));
        assert_eq!(selective.slot(3), None);

        let merged = selective.merged_with(&initial_slots);
        assert_eq!(merged[0], b"avocado");
        assert_eq!(merged[1], b"banana");
        assert_eq!(merged[2], b"cantaloupe");
        assert_eq!(merged[3], b"date");

        let mut target = initial_slots.clone();
        selective.apply_to(&mut target);
        assert_eq!(target, merged);

        // Decoded strings
        let decoded = selective.decoded_strings(&initial_slots);
        assert_eq!(
            decoded,
            [
                "avocado".to_string(),
                "banana".to_string(),
                "cantaloupe".to_string(),
                "date".to_string()
            ]
        );

        // Clamping to 255 bytes
        let long_bytes = vec![b'X'; 300];
        let clamped = evaluate_param_text([Some(&long_bytes), None, None, None]);
        assert_eq!(clamped.slot(0).unwrap().len(), 255);
        assert_eq!(clamped.slot(0).unwrap(), &vec![b'X'; 255][..]);

        // ParamTextParameters evaluation
        let params = evaluate_param_text_parameters(0x1000, 0, 0x2000, 0x3000);
        assert_eq!(params.param0(), 0x1000);
        assert_eq!(params.param1(), 0);
        assert_eq!(params.param2(), 0x2000);
        assert_eq!(params.param3(), 0x3000);
        assert_eq!(params.param(0), 0x1000);
        assert_eq!(params.param(1), 0);
        assert_eq!(params.param(2), 0x2000);
        assert_eq!(params.param(3), 0x3000);
        assert_eq!(params.param(4), 0);
        assert!(params.has_param(0));
        assert!(!params.has_param(1));
        assert!(params.has_param(2));
        assert!(params.has_param(3));
        assert!(!params.has_param(4));
        assert_eq!(params.param_ptrs(), &[0x1000, 0, 0x2000, 0x3000]);
        assert_eq!(
            params,
            ParamTextParameters::new(0x1000, 0, 0x2000, 0x3000)
        );
    }

    #[test]
    fn find_dialog_item_and_dialog_record_init_evaluation() {
        // DialogRecordInitEvaluation
        let init = evaluate_dialog_record_init(0x1234_5678);
        assert_eq!(init.window_kind(), 2);
        assert_eq!(init.items_handle(), 0x1234_5678);
        assert_eq!(init.text_handle(), 0);
        assert_eq!(init.edit_field(), -1);
        assert_eq!(init.edit_open(), 0);
        assert_eq!(init.default_item(), 1);
        assert_eq!(init.cancel_item(), 0);
        assert_eq!(init.alert_hit(), 0);
        assert_eq!(init.resource_id(), 0);

        // AlertDialogRecordInitEvaluation
        let alert_init = evaluate_alert_dialog_record_init(0x9876_5432, 128, 1, 2);
        assert_eq!(alert_init.window_kind(), 2);
        assert_eq!(alert_init.items_handle(), 0x9876_5432);
        assert_eq!(alert_init.text_handle(), 0);
        assert_eq!(alert_init.edit_field(), -1);
        assert_eq!(alert_init.edit_open(), 0);
        assert_eq!(alert_init.alert_id(), 128);
        assert_eq!(alert_init.default_item(), 1);
        assert_eq!(alert_init.cancel_item(), 2);
        assert_eq!(alert_init.alert_hit(), 0);
        assert_eq!(
            alert_init,
            AlertDialogRecordInitEvaluation::new(0x9876_5432, 128, 1, 2)
        );

        // FindDialogItemParameters: null dialog pointer returns None
        assert_eq!(evaluate_find_dialog_item_parameters(0, 10, 20), None);
        assert_eq!(evaluate_find_dialog_item_parameters_packed(0, 0x000A_0014), None);

        // FindDialogItemParameters: valid parameters and constructor
        let params = evaluate_find_dialog_item_parameters(0x1000, 15, 25).unwrap();
        assert_eq!(params.dialog_ptr(), 0x1000);
        assert_eq!(params.pt_v(), 15);
        assert_eq!(params.pt_h(), 25);
        assert_eq!(params.point(), (15, 25));
        assert_eq!(params.packed_point(), 0x000F_0019);
        assert_eq!(params, FindDialogItemParameters::new(0x1000, 15, 25));

        // FindDialogItemParameters: from packed point
        let packed_params = evaluate_find_dialog_item_parameters_packed(0x2000, 0x000F_0019).unwrap();
        assert_eq!(packed_params.dialog_ptr(), 0x2000);
        assert_eq!(packed_params.pt_v(), 15);
        assert_eq!(packed_params.pt_h(), 25);
        assert_eq!(packed_params.point(), (15, 25));
        assert_eq!(packed_params.packed_point(), 0x000F_0019);
        assert_eq!(packed_params, FindDialogItemParameters::new(0x2000, 15, 25));

        // Negative coordinates packed correctly
        let neg_params = evaluate_find_dialog_item_parameters(0x3000, -10, -20).unwrap();
        assert_eq!(neg_params.pt_v(), -10);
        assert_eq!(neg_params.pt_h(), -20);
        let from_packed_neg =
            evaluate_find_dialog_item_parameters_packed(0x3000, neg_params.packed_point()).unwrap();
        assert_eq!(from_packed_neg.pt_v(), -10);
        assert_eq!(from_packed_neg.pt_h(), -20);
        assert_eq!(from_packed_neg, neg_params);
    }

    #[test]
    fn alert_stage_and_sound_evaluation() {
        // Stages word 0xF721:
        // Stage 0 (nibble 0x1): boldItm=0 (default 1), boxDrwn=0 (suppressed), sound=1
        // Stage 1 (nibble 0x2): boldItm=0 (default 1), boxDrwn=0 (suppressed), sound=2
        // Stage 2 (nibble 0x7): boldItm=0 (default 1), boxDrwn=1 (drawn), sound=3
        // Stage 3 (nibble 0xF): boldItm=1 (default 2), boxDrwn=1 (drawn), sound=3
        let stages = 0xF721;

        // Stage 0: box drawn is false, suppressed
        let eval0 = evaluate_alert_stage(stages, 0);
        assert_eq!(eval0.stage_index(), 0);
        assert_eq!(eval0.stage_nibble(), 0x01);
        assert!(!eval0.box_drawn());
        assert_eq!(eval0.default_item(), 1);
        assert_eq!(eval0.effective_default_item(), None);
        assert_eq!(eval0.sound_number(), 1);
        assert!(eval0.has_sound());
        assert_eq!(eval0.next_stage(), 1);
        assert_eq!(eval0.suppressed_result(), ALERT_SUPPRESSED_RESULT);

        // Stage 1: box drawn is false, sound 2, next stage 2
        let eval1 = evaluate_alert_stage(stages, 1);
        assert_eq!(eval1.stage_index(), 1);
        assert_eq!(eval1.stage_nibble(), 0x02);
        assert!(!eval1.box_drawn());
        assert_eq!(eval1.default_item(), 1);
        assert_eq!(eval1.effective_default_item(), None);
        assert_eq!(eval1.sound_number(), 2);
        assert!(eval1.has_sound());
        assert_eq!(eval1.next_stage(), 2);

        // Stage 2: box drawn is true, sound 3, next stage 3
        let eval2 = evaluate_alert_stage(stages, 2);
        assert_eq!(eval2.stage_index(), 2);
        assert_eq!(eval2.stage_nibble(), 0x07);
        assert!(eval2.box_drawn());
        assert_eq!(eval2.default_item(), 1);
        assert_eq!(eval2.effective_default_item(), Some(1));
        assert_eq!(eval2.sound_number(), 3);
        assert!(eval2.has_sound());
        assert_eq!(eval2.next_stage(), 3);

        // Stage 3: box drawn is true, default item 2 (Cancel), sound 3, next stage capped at 3
        let eval3 = evaluate_alert_stage(stages, 3);
        assert_eq!(eval3.stage_index(), 3);
        assert_eq!(eval3.stage_nibble(), 0x0F);
        assert!(eval3.box_drawn());
        assert_eq!(eval3.default_item(), 2);
        assert_eq!(eval3.effective_default_item(), Some(2));
        assert_eq!(eval3.sound_number(), 3);
        assert!(eval3.has_sound());
        assert_eq!(eval3.next_stage(), 3);

        // Stage > 3 clamped to stage 3
        let eval_clamped = evaluate_alert_stage(stages, 99);
        assert_eq!(eval_clamped.stage_index(), 3);
        assert_eq!(eval_clamped.next_stage(), 3);

        // Silent stage: nibble 0x04 -> box drawn true, sound 0, default 1
        let silent_eval = evaluate_alert_stage(0x0004, 0);
        assert_eq!(silent_eval.sound_number(), 0);
        assert!(!silent_eval.has_sound());
        assert!(silent_eval.box_drawn());
        assert_eq!(silent_eval.effective_default_item(), Some(1));

        // AlertInvocationEvaluation
        let inv_drawn = evaluate_alert_invocation(128, stages, 2);
        assert_eq!(inv_drawn.alert_id(), 128);
        assert_eq!(inv_drawn.anumber(), 128);
        assert_eq!(inv_drawn.next_stage(), 3);
        assert!(!inv_drawn.is_suppressed());
        assert_eq!(inv_drawn.default_item(), Some(1));
        assert_eq!(inv_drawn.sound_number(), 3);
        assert!(inv_drawn.has_sound());
        assert_eq!(inv_drawn.suppressed_result(), -1);

        let inv_suppressed = evaluate_alert_invocation(-300, stages, 0);
        assert_eq!(inv_suppressed.alert_id(), -300);
        assert_eq!(inv_suppressed.anumber(), (-300i16) as u16);
        assert_eq!(inv_suppressed.next_stage(), 1);
        assert!(inv_suppressed.is_suppressed());
        assert_eq!(inv_suppressed.default_item(), None);
        assert_eq!(inv_suppressed.sound_number(), 1);
        assert!(inv_suppressed.has_sound());
        assert_eq!(inv_suppressed.suppressed_result(), -1);

        // ErrorSoundEvaluation / ErrorSoundParameters
        let err_sound = evaluate_error_sound(0x00AB_CDEF);
        assert_eq!(err_sound.sound_proc(), 0x00AB_CDEF);
        assert!(!err_sound.is_silent());

        let err_param: ErrorSoundParameters = evaluate_error_sound_parameters(0x00AB_CDEF);
        assert_eq!(err_param, err_sound);
        assert_eq!(err_param.sound_proc(), 0x00AB_CDEF);
        assert!(!err_param.is_silent());

        let err_silent = evaluate_error_sound(0);
        assert_eq!(err_silent.sound_proc(), 0);
        assert!(err_silent.is_silent());

        let err_silent_param = evaluate_error_sound_parameters(0);
        assert_eq!(err_silent_param, err_silent);
        assert!(err_silent_param.is_silent());

        // ResetAlertStage evaluation
        assert_eq!(INITIAL_ALERT_STAGE, 0);
        assert_eq!(evaluate_reset_alert_stage(), 0);
        let init_eval = evaluate_init_dialogs(0x1234);
        assert_eq!(init_eval.initial_alert_stage(), 0);

        // GetAlertStage evaluation
        assert_eq!(evaluate_get_alert_stage(0), 0);
        assert_eq!(evaluate_get_alert_stage(1), 1);
        assert_eq!(evaluate_get_alert_stage(2), 2);
        assert_eq!(evaluate_get_alert_stage(3), 3);
        assert_eq!(evaluate_get_alert_stage(10), 10);
    }

    #[test]
    fn new_dialog_parameters_and_storage_policy_evaluation() {
        // Storage policy
        let alloc_policy = evaluate_dialog_storage_policy(0);
        assert_eq!(alloc_policy, DialogStoragePolicy::AllocateNew);
        assert!(!alloc_policy.is_caller_supplied());
        assert!(alloc_policy.is_allocate_new());
        assert_eq!(alloc_policy.caller_storage(), None);

        let caller_policy = evaluate_dialog_storage_policy(0x0012_3456);
        assert_eq!(caller_policy, DialogStoragePolicy::CallerSupplied(0x0012_3456));
        assert!(caller_policy.is_caller_supplied());
        assert!(!caller_policy.is_allocate_new());
        assert_eq!(caller_policy.caller_storage(), Some(0x0012_3456));

        // Parameter evaluation: null bounds_ptr fails with DIALOG_PARAM_ERR (-50)
        let err = evaluate_new_dialog_parameters(
            0, 0, 0x1000, true, 1, 0xFFFF_FFFF, false, 0x2000, 0x3000, false,
        );
        assert_eq!(err, Err(DIALOG_PARAM_ERR));

        // Parameter evaluation: valid bounds_ptr succeeds with complete parameter extraction
        let params = evaluate_new_dialog_parameters(
            0x0001_0000,
            0x0002_0000,
            0x0003_0000,
            true,
            16,
            0xFFFF_FFFF,
            true,
            0xCAFE_BABE,
            0x0004_0000,
            true,
        )
        .expect("valid bounds should succeed");

        assert_eq!(params.storage(), 0x0001_0000);
        assert_eq!(params.storage_policy(), DialogStoragePolicy::CallerSupplied(0x0001_0000));
        assert_eq!(params.bounds_ptr(), 0x0002_0000);
        assert_eq!(params.title_ptr(), 0x0003_0000);
        assert!(params.is_visible());
        assert_eq!(params.proc_id(), 16);
        assert_eq!(params.behind(), 0xFFFF_FFFF);
        assert!(params.go_away());
        assert_eq!(params.ref_con(), 0xCAFE_BABE);
        assert_eq!(params.items(), 0x0004_0000);
        assert!(params.is_color());
        assert!(params.has_title());
        assert!(params.has_items());

        // Zero title and items handles
        let minimal_params = evaluate_new_dialog_parameters(
            0,
            0x0002_0000,
            0,
            false,
            0,
            0,
            false,
            0,
            0,
            false,
        )
        .expect("minimal parameters should succeed");

        assert_eq!(minimal_params.storage(), 0);
        assert_eq!(minimal_params.storage_policy(), DialogStoragePolicy::AllocateNew);
        assert!(!minimal_params.is_visible());
        assert_eq!(minimal_params.proc_id(), 0);
        assert_eq!(minimal_params.behind(), 0);
        assert!(!minimal_params.go_away());
        assert_eq!(minimal_params.ref_con(), 0);
        assert_eq!(minimal_params.items(), 0);
        assert!(!minimal_params.is_color());
        assert!(!minimal_params.has_title());
        assert!(!minimal_params.has_items());
    }

    #[test]
    fn set_dialog_item_parameters_evaluation() {
        // DITL item entry offsets
        assert_eq!(DITL_ITEM_HANDLE_OFFSET, 0);
        assert_eq!(DITL_ITEM_RECT_OFFSET, 4);
        assert_eq!(DITL_ITEM_TYPE_OFFSET, 12);
        assert_eq!(DITL_ITEM_DATA_LEN_OFFSET, 13);

        // evaluate_set_dialog_item_parameters
        assert_eq!(
            evaluate_set_dialog_item_parameters(0, 1, 4, 0x2000, (10, 20, 30, 40)),
            None
        );
        assert_eq!(
            evaluate_set_dialog_item_parameters(0x1000, 0, 4, 0x2000, (10, 20, 30, 40)),
            None
        );

        let enabled_btn = evaluate_set_dialog_item_parameters(
            0x0005_6780,
            2,
            DIALOG_ITEM_BUTTON as u16,
            0x000A_BC00,
            (50, 60, 70, 80),
        )
        .expect("valid set parameters should succeed");

        assert_eq!(enabled_btn.dialog_ptr(), 0x0005_6780);
        assert_eq!(enabled_btn.item_number(), 2);
        assert_eq!(enabled_btn.item_no(), 2);
        assert_eq!(enabled_btn.item_type(), DIALOG_ITEM_BUTTON);
        assert_eq!(enabled_btn.base_type(), DIALOG_ITEM_BUTTON);
        assert!(enabled_btn.is_enabled());
        assert_eq!(enabled_btn.item_handle(), 0x000A_BC00);
        assert_eq!(enabled_btn.rect(), (50, 60, 70, 80));

        let disabled_user = evaluate_set_dialog_item_parameters(
            0x0005_6780,
            5,
            (DIALOG_ITEM_USER_ITEM | DIALOG_ITEM_DISABLED_FLAG) as u16,
            0x000D_EF00,
            (100, 110, 120, 130),
        )
        .expect("valid disabled item parameters should succeed");

        assert_eq!(disabled_user.item_number(), 5);
        assert_eq!(disabled_user.base_type(), DIALOG_ITEM_USER_ITEM);
        assert!(!disabled_user.is_enabled());
        assert_eq!(disabled_user.item_handle(), 0x000D_EF00);
        assert_eq!(disabled_user.rect(), (100, 110, 120, 130));
    }

    #[test]
    fn dialog_item_visibility_parameters_and_evaluation() {
        // evaluate_dialog_item_visibility_parameters
        assert_eq!(evaluate_dialog_item_visibility_parameters(0, 1), None);
        assert_eq!(evaluate_dialog_item_visibility_parameters(0x1000, 0), None);
        assert_eq!(evaluate_dialog_item_visibility_parameters_signed(0, 1), None);
        assert_eq!(evaluate_dialog_item_visibility_parameters_signed(0x1000, 0), None);
        assert_eq!(evaluate_dialog_item_visibility_parameters_signed(0x1000, -1), None);

        let params = evaluate_dialog_item_visibility_parameters(0x0003_4560, 4)
            .expect("valid visibility parameters should succeed");
        assert_eq!(params.dialog_ptr(), 0x0003_4560);
        assert_eq!(params.item_number(), 4);
        assert_eq!(params.item_no(), 4);
        assert_eq!(params.item_index(), Some(3));

        let params_signed = evaluate_dialog_item_visibility_parameters_signed(0x0003_4560, 4)
            .expect("valid signed visibility parameters should succeed");
        assert_eq!(params_signed, params);

        let constructed = DialogItemVisibilityParameters::new(0x0003_4560, 4);
        assert_eq!(constructed, params);
        let zero_idx = DialogItemVisibilityParameters::new(0x1000, 0);
        assert_eq!(zero_idx.item_index(), None);

        // evaluate_hide_dialog_item and accessors
        let visible_rect = (20, 30, 40, 80);
        let hide_change = evaluate_hide_dialog_item(DIALOG_ITEM_BUTTON, visible_rect)
            .expect("visible item should produce hide change");
        assert_eq!(hide_change.new_rect(), (20, 30 + 16384, 40, 80 + 16384));
        assert_eq!(hide_change.enclosing_rect(), visible_rect);

        // Hiding already-hidden item is a no-op
        assert_eq!(
            evaluate_hide_dialog_item(DIALOG_ITEM_BUTTON, hide_change.new_rect()),
            None
        );

        // evaluate_show_dialog_item and accessors
        let show_change = evaluate_show_dialog_item(
            DIALOG_ITEM_BUTTON,
            hide_change.new_rect(),
            Some(visible_rect),
        )
        .expect("hidden item should produce show change");
        assert_eq!(show_change.new_rect(), visible_rect);
        assert_eq!(show_change.enclosing_rect(), visible_rect);

        // Showing already-visible item is a no-op
        assert_eq!(
            evaluate_show_dialog_item(DIALOG_ITEM_BUTTON, visible_rect, None),
            None
        );
    }

    #[test]
    fn append_and_shorten_ditl_parameters_evaluation() {
        // evaluate_append_ditl_parameters
        assert_eq!(evaluate_append_ditl_parameters(0, 0x2000, 0), None);
        assert_eq!(evaluate_append_ditl_parameters(0x1000, 0, 0), None);
        assert_eq!(evaluate_append_ditl_parameters(0, 0, 1), None);

        let append_params = evaluate_append_ditl_parameters(0x0002_4680, 0x0008_ACE0, APPEND_DITL_RIGHT)
            .expect("valid append ditl parameters should evaluate");
        assert_eq!(append_params.dialog_ptr(), 0x0002_4680);
        assert_eq!(append_params.ditl_handle(), 0x0008_ACE0);
        assert_eq!(append_params.method(), APPEND_DITL_RIGHT);

        let append_direct = AppendDitlParameters::new(0x0001_1110, 0x0002_2220, APPEND_DITL_BOTTOM);
        assert_eq!(append_direct.dialog_ptr(), 0x0001_1110);
        assert_eq!(append_direct.ditl_handle(), 0x0002_2220);
        assert_eq!(append_direct.method(), APPEND_DITL_BOTTOM);

        // evaluate_shorten_ditl_parameters
        assert_eq!(evaluate_shorten_ditl_parameters(0, 5), None);
        assert_eq!(evaluate_shorten_ditl_parameters(0, 0), None);

        let shorten_params = evaluate_shorten_ditl_parameters(0x0003_5790, 3)
            .expect("valid shorten ditl parameters should evaluate");
        assert_eq!(shorten_params.dialog_ptr(), 0x0003_5790);
        assert_eq!(shorten_params.number_items(), 3);

        let shorten_zero = evaluate_shorten_ditl_parameters(0x0003_5790, 0)
            .expect("shorten with 0 items should evaluate");
        assert_eq!(shorten_zero.number_items(), 0);

        let shorten_direct = ShortenDitlParameters::new(0x0004_4440, 7);
        assert_eq!(shorten_direct.dialog_ptr(), 0x0004_4440);
        assert_eq!(shorten_direct.number_items(), 7);
    }

    #[test]
    fn is_dialog_event_and_dialog_select_parameters_evaluation() {
        // evaluate_is_dialog_event_parameters
        assert_eq!(evaluate_is_dialog_event_parameters(0), None);

        let params = evaluate_is_dialog_event_parameters(0x0005_1234)
            .expect("valid is_dialog_event parameters should evaluate");
        assert_eq!(params.event_ptr(), 0x0005_1234);
        assert!(params.is_valid());

        let params_direct = IsDialogEventParameters::new(0x0007_5678);
        assert_eq!(params_direct.event_ptr(), 0x0007_5678);
        assert!(params_direct.is_valid());
        let params_zero = IsDialogEventParameters::new(0);
        assert!(!params_zero.is_valid());

        // evaluate_dialog_select_parameters
        assert_eq!(evaluate_dialog_select_parameters(0, 0x1000, 0x2000), None);

        let select_all = evaluate_dialog_select_parameters(0x0001_2340, 0x0002_3450, 0x0003_4560)
            .expect("valid select parameters should evaluate");
        assert_eq!(select_all.event_ptr(), 0x0001_2340);
        assert_eq!(select_all.dialog_out_ptr(), 0x0002_3450);
        assert_eq!(select_all.item_hit_ptr(), 0x0003_4560);
        assert!(select_all.has_dialog_out());
        assert!(select_all.has_item_hit_out());

        let select_none = evaluate_dialog_select_parameters(0x0001_2340, 0, 0)
            .expect("valid select with null outputs should evaluate");
        assert_eq!(select_none.event_ptr(), 0x0001_2340);
        assert_eq!(select_none.dialog_out_ptr(), 0);
        assert_eq!(select_none.item_hit_ptr(), 0);
        assert!(!select_none.has_dialog_out());
        assert!(!select_none.has_item_hit_out());

        let select_direct = DialogSelectParameters::new(0x000A_1110, 0x000B_2220, 0);
        assert_eq!(select_direct.event_ptr(), 0x000A_1110);
        assert_eq!(select_direct.dialog_out_ptr(), 0x000B_2220);
        assert_eq!(select_direct.item_hit_ptr(), 0);
        assert!(select_direct.has_dialog_out());
        assert!(!select_direct.has_item_hit_out());
    }

    #[test]
    fn set_dialog_default_and_cancel_item_parameters_evaluation() {
        // evaluate_set_dialog_default_item_parameters
        assert_eq!(
            evaluate_set_dialog_default_item_parameters(0, 1),
            Err(DIALOG_PARAM_ERR)
        );

        let default_params = evaluate_set_dialog_default_item_parameters(0x0004_1234, 1)
            .expect("valid default item parameters should evaluate");
        assert_eq!(default_params.dialog_ptr(), 0x0004_1234);
        assert_eq!(default_params.item_no(), 1);

        let default_negative = evaluate_set_dialog_default_item_parameters(0x0004_1234, -1)
            .expect("valid negative default item should evaluate");
        assert_eq!(default_negative.dialog_ptr(), 0x0004_1234);
        assert_eq!(default_negative.item_no(), -1);

        let default_direct = SetDialogDefaultItemParameters::new(0x0005_2345, 3);
        assert_eq!(default_direct.dialog_ptr(), 0x0005_2345);
        assert_eq!(default_direct.item_no(), 3);

        // evaluate_set_dialog_cancel_item_parameters
        assert_eq!(
            evaluate_set_dialog_cancel_item_parameters(0, 2),
            Err(DIALOG_PARAM_ERR)
        );

        let cancel_params = evaluate_set_dialog_cancel_item_parameters(0x0006_3456, 2)
            .expect("valid cancel item parameters should evaluate");
        assert_eq!(cancel_params.dialog_ptr(), 0x0006_3456);
        assert_eq!(cancel_params.item_no(), 2);

        let cancel_zero = evaluate_set_dialog_cancel_item_parameters(0x0006_3456, 0)
            .expect("valid zero cancel item should evaluate");
        assert_eq!(cancel_zero.dialog_ptr(), 0x0006_3456);
        assert_eq!(cancel_zero.item_no(), 0);

        let cancel_direct = SetDialogCancelItemParameters::new(0x0007_4567, 4);
        assert_eq!(cancel_direct.dialog_ptr(), 0x0007_4567);
        assert_eq!(cancel_direct.item_no(), 4);

        // evaluate_get_dialog_default_item_parameters
        assert_eq!(
            evaluate_get_dialog_default_item_parameters(0, 0x1000, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_dialog_default_item_parameters(0x0004_1234, 0, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_dialog_default_item_parameters(0x0004_1234, 0x1000, false),
            Err(DIALOG_PARAM_ERR)
        );
        let get_default_params = evaluate_get_dialog_default_item_parameters(0x0004_1234, 0x1000, true)
            .expect("valid get default item parameters should evaluate");
        assert_eq!(get_default_params.dialog_ptr(), 0x0004_1234);
        assert_eq!(get_default_params.out_default_item_ptr(), 0x1000);

        let get_default_direct = GetDialogDefaultItemParameters::new(0x0005_2345, 0x2000);
        assert_eq!(get_default_direct.dialog_ptr(), 0x0005_2345);
        assert_eq!(get_default_direct.out_default_item_ptr(), 0x2000);

        // evaluate_get_dialog_cancel_item_parameters
        assert_eq!(
            evaluate_get_dialog_cancel_item_parameters(0, 0x3000, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_dialog_cancel_item_parameters(0x0006_3456, 0, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_dialog_cancel_item_parameters(0x0006_3456, 0x3000, false),
            Err(DIALOG_PARAM_ERR)
        );
        let get_cancel_params = evaluate_get_dialog_cancel_item_parameters(0x0006_3456, 0x3000, true)
            .expect("valid get cancel item parameters should evaluate");
        assert_eq!(get_cancel_params.dialog_ptr(), 0x0006_3456);
        assert_eq!(get_cancel_params.out_cancel_item_ptr(), 0x3000);

        let get_cancel_direct = GetDialogCancelItemParameters::new(0x0007_4567, 0x4000);
        assert_eq!(get_cancel_direct.dialog_ptr(), 0x0007_4567);
        assert_eq!(get_cancel_direct.out_cancel_item_ptr(), 0x4000);

        // evaluate_dialog_default_item
        assert_eq!(DEFAULT_DIALOG_ITEM, 1);
        assert_eq!(evaluate_dialog_default_item(Some(2)), 2);
        assert_eq!(evaluate_dialog_default_item(Some(1)), 1);
        assert_eq!(evaluate_dialog_default_item(Some(0)), 1);
        assert_eq!(evaluate_dialog_default_item(Some(-1)), 1);
        assert_eq!(evaluate_dialog_default_item(None), 1);
    }

    #[test]
    fn move_and_size_dialog_item_parameters_evaluation() {
        // evaluate_move_dialog_item_parameters
        assert_eq!(
            evaluate_move_dialog_item_parameters(0, 1, 10, 20),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_move_dialog_item_parameters(0x0001_1110, 0, 10, 20),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_move_dialog_item_parameters(0x0001_1110, -1, 10, 20),
            Err(DIALOG_PARAM_ERR)
        );

        let move_params = evaluate_move_dialog_item_parameters(0x0001_1110, 2, 50, 60)
            .expect("valid move dialog item parameters should evaluate");
        assert_eq!(move_params.dialog_ptr(), 0x0001_1110);
        assert_eq!(move_params.item_no(), 2);
        assert_eq!(move_params.item_number(), 2);
        assert_eq!(move_params.in_horiz(), 50);
        assert_eq!(move_params.in_vert(), 60);

        let move_direct = MoveDialogItemParameters::new(0x0002_2220, 3, -10, -20);
        assert_eq!(move_direct.dialog_ptr(), 0x0002_2220);
        assert_eq!(move_direct.item_no(), 3);
        assert_eq!(move_direct.item_number(), 3);
        assert_eq!(move_direct.in_horiz(), -10);
        assert_eq!(move_direct.in_vert(), -20);

        // evaluate_move_dialog_item_rect
        let rect = (20, 30, 60, 90); // height=40, width=60
        let moved_rect = evaluate_move_dialog_item_rect(rect, 100, 150); // top=150, left=100
        assert_eq!(moved_rect, (150, 100, 190, 160));

        // evaluate_size_dialog_item_parameters
        assert_eq!(
            evaluate_size_dialog_item_parameters(0, 1, 100, 200),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_size_dialog_item_parameters(0x0001_1110, 0, 100, 200),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_size_dialog_item_parameters(0x0001_1110, -5, 100, 200),
            Err(DIALOG_PARAM_ERR)
        );

        let size_params = evaluate_size_dialog_item_parameters(0x0001_1110, 4, 80, 120)
            .expect("valid size dialog item parameters should evaluate");
        assert_eq!(size_params.dialog_ptr(), 0x0001_1110);
        assert_eq!(size_params.item_no(), 4);
        assert_eq!(size_params.item_number(), 4);
        assert_eq!(size_params.in_width(), 80);
        assert_eq!(size_params.in_height(), 120);

        let size_direct = SizeDialogItemParameters::new(0x0003_3330, 5, 45, 55);
        assert_eq!(size_direct.dialog_ptr(), 0x0003_3330);
        assert_eq!(size_direct.item_no(), 5);
        assert_eq!(size_direct.item_number(), 5);
        assert_eq!(size_direct.in_width(), 45);
        assert_eq!(size_direct.in_height(), 55);

        // evaluate_size_dialog_item_rect
        let rect2 = (10, 20, 50, 80);
        let sized_rect = evaluate_size_dialog_item_rect(rect2, 100, 150);
        assert_eq!(sized_rect, (10, 20, 160, 120));
    }

    #[test]
    fn append_dialog_item_list_parameters_and_bounds_evaluation() {
        // evaluate_append_dialog_item_list_parameters
        assert_eq!(
            evaluate_append_dialog_item_list_parameters(0, 128, APPEND_DITL_BOTTOM),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_append_dialog_item_list_parameters(0x0005_4320, 0, APPEND_DITL_BOTTOM),
            Err(DIALOG_PARAM_ERR)
        );

        let params = evaluate_append_dialog_item_list_parameters(0x0005_4320, 256, APPEND_DITL_RIGHT)
            .expect("valid parameters should evaluate");
        assert_eq!(params.dialog_ptr(), 0x0005_4320);
        assert_eq!(params.ditl_id(), 256);
        assert_eq!(params.method(), APPEND_DITL_RIGHT);

        let direct = AppendDialogItemListParameters::new(0x0006_7890, -100, -3);
        assert_eq!(direct.dialog_ptr(), 0x0006_7890);
        assert_eq!(direct.ditl_id(), -100);
        assert_eq!(direct.method(), -3);

        // evaluate_appended_dialog_bounds
        let initial_bounds = (0, 0, 100, 200);
        let items_within = [(10, 20, 50, 80), (60, 70, 90, 150)];
        assert_eq!(
            evaluate_appended_dialog_bounds(initial_bounds, items_within),
            (0, 0, 100, 200)
        );

        let items_expanding = [
            (10, 20, 50, 80),
            (60, 70, 150, 250), // bottom=150 (>100), right=250 (>200)
        ];
        assert_eq!(
            evaluate_appended_dialog_bounds(initial_bounds, items_expanding),
            (0, 0, 150, 250)
        );
    }

    #[test]
    fn auto_size_dialog_parameters_and_bounds_evaluation() {
        assert_eq!(
            evaluate_auto_size_dialog_parameters(0),
            Err(DIALOG_PARAM_ERR)
        );

        let params = evaluate_auto_size_dialog_parameters(0x0001_2345)
            .expect("valid dialog pointer should evaluate");
        assert_eq!(params.dialog_ptr(), 0x0001_2345);

        let direct = AutoSizeDialogParameters::new(0x0005_6789);
        assert_eq!(direct.dialog_ptr(), 0x0005_6789);

        // evaluate_auto_size_dialog_bounds
        // Empty items preserves initial bounds
        let initial_bounds = (0, 0, 100, 200);
        assert_eq!(
            evaluate_auto_size_dialog_bounds(initial_bounds, []),
            (0, 0, 100, 200)
        );

        // Enclosing items shrinks bounds to tight bounding rect
        let items_shrinking = [
            (10, 10, 40, 80),
            (20, 30, 50, 90),
        ];
        assert_eq!(
            evaluate_auto_size_dialog_bounds(initial_bounds, items_shrinking),
            (0, 0, 50, 90)
        );

        // Expanding items expands bounds
        let items_expanding = [
            (10, 10, 120, 80),
            (20, 30, 50, 250),
        ];
        assert_eq!(
            evaluate_auto_size_dialog_bounds(initial_bounds, items_expanding),
            (0, 0, 120, 250)
        );

        // Non-zero origin is preserved
        let offset_bounds = (50, 40, 200, 300);
        let items_offset = [
            (60, 50, 150, 220),
            (70, 80, 180, 210),
        ];
        assert_eq!(
            evaluate_auto_size_dialog_bounds(offset_bounds, items_offset),
            (50, 40, 180, 220)
        );
    }

    #[test]
    fn set_dialog_font_parameters_evaluation() {
        let params = evaluate_set_dialog_font_parameters(0);
        assert_eq!(params.font_num(), 0);

        let geneva = evaluate_set_dialog_font_parameters(3);
        assert_eq!(geneva.font_num(), 3);

        let direct = SetDialogFontParameters::new(-1);
        assert_eq!(direct.font_num(), -1);
    }

    #[test]
    fn dialog_low_memory_globals_evaluation() {
        assert_eq!(evaluate_get_resume_proc(0), 0);
        assert_eq!(evaluate_get_resume_proc(0x0012_3456), 0x0012_3456);
        assert_eq!(evaluate_set_resume_proc(0), 0);
        assert_eq!(evaluate_set_resume_proc(0x0012_3456), 0x0012_3456);

        assert_eq!(evaluate_get_anumber(0), 0);
        assert_eq!(evaluate_get_anumber(128), 128);
        assert_eq!(evaluate_get_anumber(0xFF80), -128);
        assert_eq!(evaluate_set_anumber(0), 0);
        assert_eq!(evaluate_set_anumber(128), 128);
        assert_eq!(evaluate_set_anumber(-128), 0xFF80);

        assert_eq!(evaluate_get_alert_stage(0), 0);
        assert_eq!(evaluate_get_alert_stage(3), 3);
        assert_eq!(evaluate_set_alert_stage(0), 0);
        assert_eq!(evaluate_set_alert_stage(3), 3);
        assert_eq!(evaluate_set_alert_stage(-1), 0xFFFF);

        assert_eq!(evaluate_get_da_beeper(0), 0);
        assert_eq!(evaluate_get_da_beeper(0x00AB_CDEF), 0x00AB_CDEF);
        assert_eq!(evaluate_set_da_beeper(0), 0);
        assert_eq!(evaluate_set_da_beeper(0x00AB_CDEF), 0x00AB_CDEF);

        assert_eq!(
            evaluate_get_da_strings_addr(),
            crate::memory::globals::addr::DA_STRINGS
        );
        assert_eq!(evaluate_dialog_font(0), 0);
        assert_eq!(evaluate_dialog_font(3), 3);
        assert_eq!(evaluate_dialog_font(0xFFFF), -1);
    }

    #[test]
    fn dialog_window_and_port_accessors_evaluation() {
        assert_eq!(evaluate_get_dialog_port(0), 0);
        assert_eq!(evaluate_get_dialog_port(0x0012_3456), 0x0012_3456);

        assert_eq!(evaluate_get_dialog_window(0), 0);
        assert_eq!(evaluate_get_dialog_window(0x0012_3456), 0x0012_3456);

        // GetDialogFromWindow: NULL window returns 0
        assert_eq!(evaluate_get_dialog_from_window(0, None), 0);
        assert_eq!(
            evaluate_get_dialog_from_window(0, Some(DIALOG_WINDOW_KIND as i16)),
            0
        );

        // GetDialogFromWindow: non-dialog window returns 0
        assert_eq!(evaluate_get_dialog_from_window(0x0012_3456, None), 0);
        assert_eq!(evaluate_get_dialog_from_window(0x0012_3456, Some(8)), 0);
        assert_eq!(evaluate_get_dialog_from_window(0x0012_3456, Some(0)), 0);

        // GetDialogFromWindow: dialog window returns window pointer
        assert_eq!(
            evaluate_get_dialog_from_window(0x0012_3456, Some(DIALOG_WINDOW_KIND as i16)),
            0x0012_3456
        );

        assert_eq!(evaluate_set_port_dialog_port(0), 0);
        assert_eq!(evaluate_set_port_dialog_port(0x0012_3456), 0x0012_3456);
    }

    #[test]
    fn dialog_edit_command_evaluation() {
        // NULL dialog pointer returns None
        assert_eq!(evaluate_dialog_edit_command(0, 0, 0x0001_0000), None);
        assert_eq!(evaluate_dialog_edit_command(0, -1, 0x0001_0000), None);

        // Negative edit_field returns None (e.g. DIALOG_INITIAL_EDIT_FIELD = -1)
        assert_eq!(
            evaluate_dialog_edit_command(0x0012_3456, -1, 0x0001_0000),
            None
        );
        assert_eq!(
            evaluate_dialog_edit_command(0x0012_3456, -10, 0x0001_0000),
            None
        );

        // Zero text_handle returns None
        assert_eq!(evaluate_dialog_edit_command(0x0012_3456, 0, 0), None);
        assert_eq!(evaluate_dialog_edit_command(0x0012_3456, 2, 0), None);

        // Valid dialog with active editText item (edit_field >= 0) and non-zero text_handle returns Some
        let eval = evaluate_dialog_edit_command(0x0012_3456, 0, 0x0002_0000)
            .expect("active edit text command should evaluate");
        assert_eq!(eval.text_handle(), 0x0002_0000);

        let eval2 = evaluate_dialog_edit_command(0x0012_3456, 3, 0x0003_5555)
            .expect("active edit text command should evaluate");
        assert_eq!(eval2.text_handle(), 0x0003_5555);

        let direct = DialogEditCommandEvaluation::new(0x0004_1111);
        assert_eq!(direct.text_handle(), 0x0004_1111);
    }

    #[test]
    fn count_ditl_and_set_dialog_tracks_cursor_evaluation() {
        // evaluate_count_ditl_parameters
        assert_eq!(evaluate_count_ditl_parameters(0), None);

        let params = evaluate_count_ditl_parameters(0x0008_1234)
            .expect("valid count_ditl parameters should evaluate");
        assert_eq!(params.dialog_ptr(), 0x0008_1234);
        assert!(params.is_valid());

        let params_direct = CountDitlParameters::new(0x0009_5678);
        assert_eq!(params_direct.dialog_ptr(), 0x0009_5678);
        assert!(params_direct.is_valid());
        let params_zero = CountDitlParameters::new(0);
        assert!(!params_zero.is_valid());

        // evaluate_set_dialog_tracks_cursor_parameters
        let tracks_dlg = evaluate_set_dialog_tracks_cursor_parameters(0x000A_1110, true)
            .expect("tracks cursor for dialog should evaluate");
        assert_eq!(tracks_dlg.dialog_ptr(), 0x000A_1110);
        assert!(tracks_dlg.tracks());
        assert!(!tracks_dlg.tracks_all_dialogs());

        let tracks_all = evaluate_set_dialog_tracks_cursor_parameters(0, false)
            .expect("tracks cursor for all dialogs should evaluate");
        assert_eq!(tracks_all.dialog_ptr(), 0);
        assert!(!tracks_all.tracks());
        assert!(tracks_all.tracks_all_dialogs());

        let direct_tracks = SetDialogTracksCursorParameters::new(0x000B_2220, true);
        assert_eq!(direct_tracks.dialog_ptr(), 0x000B_2220);
        assert!(direct_tracks.tracks());
        assert!(!direct_tracks.tracks_all_dialogs());
    }

    #[test]
    fn get_std_filter_proc_and_select_dialog_item_text_evaluation() {
        // evaluate_get_std_filter_proc_parameters
        assert_eq!(
            evaluate_get_std_filter_proc_parameters(0, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_std_filter_proc_parameters(0x2000, false),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_std_filter_proc_parameters(0, false),
            Err(DIALOG_PARAM_ERR)
        );

        let filter_params = evaluate_get_std_filter_proc_parameters(0x2000, true)
            .expect("valid filter proc pointer should evaluate");
        assert_eq!(filter_params.out_proc(), 0x2000);

        let filter_direct = GetStdFilterProcParameters::new(0x3000);
        assert_eq!(filter_direct.out_proc(), 0x3000);

        // evaluate_select_dialog_item_text_parameters
        assert_eq!(
            evaluate_select_dialog_item_text_parameters(0, 1, 0, 5),
            None
        );
        assert_eq!(
            evaluate_select_dialog_item_text_parameters(0x1000, 0, 0, 5),
            None
        );

        let sel_params = evaluate_select_dialog_item_text_parameters(0x1000, 2, 3, 8)
            .expect("valid select text params should evaluate");
        assert_eq!(sel_params.dialog_ptr(), 0x1000);
        assert_eq!(sel_params.item_number(), 2);
        assert_eq!(sel_params.item_no(), 2);
        assert_eq!(sel_params.item_index(), Some(1));
        assert_eq!(sel_params.selection_start(), 3);
        assert_eq!(sel_params.selection_end(), 8);

        // evaluate_selection
        assert_eq!(sel_params.evaluate_selection(false, 10), None);
        let eval = sel_params
            .evaluate_selection(true, 10)
            .expect("edit text selection should evaluate");
        assert_eq!(eval.edit_field, 1);
        assert_eq!(eval.sel_start, 3);
        assert_eq!(eval.sel_end, 8);

        let direct_sel = SelectDialogItemTextParameters::new(0x2000, 3, 0, -1);
        assert_eq!(direct_sel.dialog_ptr(), 0x2000);
        assert_eq!(direct_sel.item_number(), 3);
        assert_eq!(direct_sel.item_index(), Some(2));
        assert_eq!(direct_sel.selection_start(), 0);
        assert_eq!(direct_sel.selection_end(), -1);
        let direct_eval = direct_sel
            .evaluate_selection(true, 15)
            .expect("select all should evaluate");
        assert_eq!(direct_eval.edit_field, 2);
        assert_eq!(direct_eval.sel_start, 0);
        assert_eq!(direct_eval.sel_end, 15);
    }

    #[test]
    fn get_new_dialog_parameters_evaluation() {
        let params_alloc = evaluate_get_new_dialog_parameters(128, 0, 0xFFFF_FFFF);
        assert_eq!(params_alloc.dialog_id(), 128);
        assert_eq!(params_alloc.storage(), 0);
        assert_eq!(params_alloc.storage_policy(), DialogStoragePolicy::AllocateNew);
        assert_eq!(params_alloc.behind(), 0xFFFF_FFFF);

        let params_supplied = evaluate_get_new_dialog_parameters(129, 0x0002_0000, 0x0003_0000);
        assert_eq!(params_supplied.dialog_id(), 129);
        assert_eq!(params_supplied.storage(), 0x0002_0000);
        assert_eq!(
            params_supplied.storage_policy(),
            DialogStoragePolicy::CallerSupplied(0x0002_0000)
        );
        assert_eq!(params_supplied.behind(), 0x0003_0000);

        let direct = GetNewDialogParameters::new(200, 0, 0);
        assert_eq!(direct.dialog_id(), 200);
        assert_eq!(direct.storage(), 0);
        assert_eq!(direct.storage_policy(), DialogStoragePolicy::AllocateNew);
        assert_eq!(direct.behind(), 0);
    }

    #[test]
    fn get_dialog_item_as_control_and_modal_dialog_parameters_evaluation() {
        // evaluate_get_dialog_item_as_control_parameters
        assert_eq!(
            evaluate_get_dialog_item_as_control_parameters(0, 1, 0x1000, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_dialog_item_as_control_parameters(0x2000, 0, 0x1000, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_dialog_item_as_control_parameters(0x2000, 1, 0, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_dialog_item_as_control_parameters(0x2000, 1, 0x1000, false),
            Err(DIALOG_PARAM_ERR)
        );

        let control_params =
            evaluate_get_dialog_item_as_control_parameters(0x2000, 3, 0x3000, true).unwrap();
        assert_eq!(control_params.dialog_ptr(), 0x2000);
        assert_eq!(control_params.item_number(), 3);
        assert_eq!(control_params.item_no(), 3);
        assert_eq!(control_params.item_index(), 2);
        assert_eq!(control_params.control_out(), 0x3000);

        let direct_control = GetDialogItemAsControlParameters::new(0x4000, 1, 0x5000);
        assert_eq!(direct_control.dialog_ptr(), 0x4000);
        assert_eq!(direct_control.item_number(), 1);
        assert_eq!(direct_control.item_no(), 1);
        assert_eq!(direct_control.item_index(), 0);
        assert_eq!(direct_control.control_out(), 0x5000);

        // evaluate_modal_dialog_parameters
        assert_eq!(evaluate_modal_dialog_parameters(0, 0, true), None);
        assert_eq!(evaluate_modal_dialog_parameters(0x1000, 0x2000, false), None);

        let modal_params = evaluate_modal_dialog_parameters(0x1000, 0x2000, true).unwrap();
        assert_eq!(modal_params.filter_proc(), 0x1000);
        assert!(modal_params.has_filter_proc());
        assert_eq!(modal_params.item_hit_ptr(), 0x2000);

        let no_filter = evaluate_modal_dialog_parameters(0, 0x2000, true).unwrap();
        assert_eq!(no_filter.filter_proc(), 0);
        assert!(!no_filter.has_filter_proc());
        assert_eq!(no_filter.item_hit_ptr(), 0x2000);

        let direct_modal = ModalDialogParameters::new(0x3000, 0x4000);
        assert_eq!(direct_modal.filter_proc(), 0x3000);
        assert!(direct_modal.has_filter_proc());
        assert_eq!(direct_modal.item_hit_ptr(), 0x4000);
    }

    #[test]
    fn dialog_template_purgeability_and_standard_alert_evaluation() {
        // DialogTemplatePurgeabilityParameters evaluation
        let could_dlog = evaluate_dialog_template_purgeability_parameters(*b"DLOG", 128, true, false);
        assert_eq!(could_dlog.template_type(), *b"DLOG");
        assert_eq!(could_dlog.template_id(), 128);
        assert_eq!(
            could_dlog.action(),
            DialogTemplatePurgeabilityAction::MakeUnpurgeableLoadIfMissing
        );
        assert!(could_dlog.load_if_missing());
        assert!(!could_dlog.purgeable());
        assert!(could_dlog.is_dialog());
        assert!(!could_dlog.is_alert());

        // Convenience evaluator for CouldDialog
        let could_dlog_conv = evaluate_dialog_purgeability_parameters(128, true, false);
        assert_eq!(could_dlog, could_dlog_conv);

        let free_dlog = evaluate_dialog_template_purgeability_parameters(*b"DLOG", 128, false, true);
        assert_eq!(free_dlog.template_type(), *b"DLOG");
        assert_eq!(free_dlog.template_id(), 128);
        assert_eq!(
            free_dlog.action(),
            DialogTemplatePurgeabilityAction::MakePurgeable
        );
        assert!(!free_dlog.load_if_missing());
        assert!(free_dlog.purgeable());
        assert!(free_dlog.is_dialog());
        assert!(!free_dlog.is_alert());

        // Convenience evaluator for FreeDialog
        let free_dlog_conv = evaluate_dialog_purgeability_parameters(128, false, true);
        assert_eq!(free_dlog, free_dlog_conv);

        let could_alrt = evaluate_dialog_template_purgeability_parameters(*b"ALRT", 256, true, false);
        assert_eq!(could_alrt.template_type(), *b"ALRT");
        assert_eq!(could_alrt.template_id(), 256);
        assert_eq!(
            could_alrt.action(),
            DialogTemplatePurgeabilityAction::MakeUnpurgeableLoadIfMissing
        );
        assert!(could_alrt.load_if_missing());
        assert!(!could_alrt.purgeable());
        assert!(!could_alrt.is_dialog());
        assert!(could_alrt.is_alert());

        // Convenience evaluator for CouldAlert
        let could_alrt_conv = evaluate_alert_purgeability_parameters(256, true, false);
        assert_eq!(could_alrt, could_alrt_conv);

        let free_alrt = evaluate_dialog_template_purgeability_parameters(*b"ALRT", 256, false, true);
        assert_eq!(free_alrt.template_type(), *b"ALRT");
        assert_eq!(free_alrt.template_id(), 256);
        assert_eq!(
            free_alrt.action(),
            DialogTemplatePurgeabilityAction::MakePurgeable
        );
        assert!(!free_alrt.load_if_missing());
        assert!(free_alrt.purgeable());
        assert!(!free_alrt.is_dialog());
        assert!(free_alrt.is_alert());

        // Convenience evaluator for FreeAlert
        let free_alrt_conv = evaluate_alert_purgeability_parameters(256, false, true);
        assert_eq!(free_alrt, free_alrt_conv);

        // Specific routine evaluators
        assert_eq!(could_dlog, evaluate_could_dialog_parameters(128));
        assert_eq!(free_dlog, evaluate_free_dialog_parameters(128));
        assert_eq!(could_alrt, evaluate_could_alert_parameters(256));
        assert_eq!(free_alrt, evaluate_free_alert_parameters(256));

        // Direct constructor
        let direct_purge = DialogTemplatePurgeabilityParameters::new(
            *b"DLOG",
            42,
            DialogTemplatePurgeabilityAction::MakePurgeable,
        );
        assert_eq!(direct_purge.template_id(), 42);
        assert!(direct_purge.is_dialog());
        assert!(direct_purge.purgeable());


        // StandardAlertParameters evaluation
        // StandardAlert: requires valid writable item_hit_out_ptr
        assert_eq!(
            evaluate_standard_alert_parameters(0, 0x1000, 0x2000, 0x3000, 0, true, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_standard_alert_parameters(0, 0x1000, 0x2000, 0x3000, 0x4000, true, false),
            Err(DIALOG_PARAM_ERR)
        );

        let std_params = evaluate_standard_alert_parameters(
            1,
            0x1000,
            0x2000,
            0x3000,
            0x4000,
            true,
            true,
        )
        .unwrap();
        assert_eq!(std_params.alert_type(), 1);
        assert_eq!(std_params.alert_id(), 1);
        assert_eq!(std_params.error_ptr(), 0x1000);
        assert_eq!(std_params.explanation_ptr(), 0x2000);
        assert_eq!(std_params.alert_param_ptr(), 0x3000);
        assert_eq!(std_params.item_hit_out_ptr(), 0x4000);
        assert!(std_params.is_standard());

        // Direct constructor
        let direct_std = StandardAlertParameters::new(2, 0x10, 0x20, 0x30, 0x40, true);
        assert_eq!(direct_std.alert_type(), 2);
        assert_eq!(direct_std.alert_id(), 2);
        assert_eq!(direct_std.error_ptr(), 0x10);
        assert_eq!(direct_std.explanation_ptr(), 0x20);
        assert_eq!(direct_std.alert_param_ptr(), 0x30);
        assert_eq!(direct_std.item_hit_out_ptr(), 0x40);
        assert!(direct_std.is_standard());

        // Classic Alert compatibility: does not require item_hit_out_ptr or write permission
        let alert_compat = evaluate_standard_alert_parameters(
            128,
            0,
            0,
            0,
            0,
            false,
            false,
        )
        .unwrap();
        assert_eq!(alert_compat.alert_type(), 128);
        assert_eq!(alert_compat.alert_id(), 128);
        assert_eq!(alert_compat.error_ptr(), 0);
        assert_eq!(alert_compat.explanation_ptr(), 0);
        assert_eq!(alert_compat.alert_param_ptr(), 0);
        assert_eq!(alert_compat.item_hit_out_ptr(), 0);
        assert!(!alert_compat.is_standard());
    }

    #[test]
    fn get_and_set_dialog_item_parameters_evaluation() {
        // evaluate_get_dialog_item_parameters
        assert_eq!(
            evaluate_get_dialog_item_parameters(0, 1, 0x1000, 0x2000, 0x3000, true, true, true),
            None
        );
        assert_eq!(
            evaluate_get_dialog_item_parameters(
                0x4000, 0, 0x1000, 0x2000, 0x3000, true, true, true
            ),
            None
        );
        assert_eq!(
            evaluate_get_dialog_item_parameters(
                0x4000, 1, 0x1000, 0x2000, 0x3000, false, true, true
            ),
            None
        );
        assert_eq!(
            evaluate_get_dialog_item_parameters(
                0x4000, 1, 0x1000, 0x2000, 0x3000, true, false, true
            ),
            None
        );
        assert_eq!(
            evaluate_get_dialog_item_parameters(
                0x4000, 1, 0x1000, 0x2000, 0x3000, true, true, false
            ),
            None
        );

        // Null output pointers do not require write permission
        let params_null =
            evaluate_get_dialog_item_parameters(0x4000, 2, 0, 0, 0, false, false, false).unwrap();
        assert_eq!(params_null.dialog_ptr(), 0x4000);
        assert_eq!(params_null.item_number(), 2);
        assert_eq!(params_null.item_no(), 2);
        assert_eq!(params_null.item_index(), 1);
        assert_eq!(params_null.item_type_ptr(), 0);
        assert_eq!(params_null.item_handle_ptr(), 0);
        assert_eq!(params_null.item_rect_ptr(), 0);

        let params = evaluate_get_dialog_item_parameters(
            0x5000, 3, 0x1000, 0x2000, 0x3000, true, true, true,
        )
        .unwrap();
        assert_eq!(params.dialog_ptr(), 0x5000);
        assert_eq!(params.item_number(), 3);
        assert_eq!(params.item_no(), 3);
        assert_eq!(params.item_index(), 2);
        assert_eq!(params.item_type_ptr(), 0x1000);
        assert_eq!(params.item_handle_ptr(), 0x2000);
        assert_eq!(params.item_rect_ptr(), 0x3000);

        let direct = GetDialogItemParameters::new(0x6000, 4, 0x10, 0x20, 0x30);
        assert_eq!(direct.dialog_ptr(), 0x6000);
        assert_eq!(direct.item_number(), 4);
        assert_eq!(direct.item_no(), 4);
        assert_eq!(direct.item_index(), 3);
        assert_eq!(direct.item_type_ptr(), 0x10);
        assert_eq!(direct.item_handle_ptr(), 0x20);
        assert_eq!(direct.item_rect_ptr(), 0x30);

        // evaluate_set_dialog_item_parameters
        assert_eq!(
            evaluate_set_dialog_item_parameters(0, 1, 4, 0x2000, (10, 20, 30, 40)),
            None
        );
        assert_eq!(
            evaluate_set_dialog_item_parameters(0x1000, 0, 4, 0x2000, (10, 20, 30, 40)),
            None
        );
        let set_params =
            evaluate_set_dialog_item_parameters(0x1000, 1, 4, 0x2000, (10, 20, 30, 40)).unwrap();
        assert_eq!(set_params.dialog_ptr(), 0x1000);
        assert_eq!(set_params.item_number(), 1);
        assert_eq!(set_params.item_no(), 1);
        assert_eq!(set_params.item_type(), 4);
        assert_eq!(set_params.base_type(), 4);
        assert_eq!(set_params.item_handle(), 0x2000);
        assert_eq!(set_params.rect(), (10, 20, 30, 40));
    }

    #[test]
    fn alert_parameters_evaluation() {
        // AlertKind properties
        assert_eq!(AlertKind::Alert.icon_id(), None);
        assert!(AlertKind::Alert.is_plain());
        assert_eq!(AlertKind::Alert.name(), "Alert");

        assert_eq!(AlertKind::Stop.icon_id(), Some(0));
        assert!(!AlertKind::Stop.is_plain());
        assert_eq!(AlertKind::Stop.name(), "StopAlert");

        assert_eq!(AlertKind::Note.icon_id(), Some(1));
        assert!(!AlertKind::Note.is_plain());
        assert_eq!(AlertKind::Note.name(), "NoteAlert");

        assert_eq!(AlertKind::Caution.icon_id(), Some(2));
        assert!(!AlertKind::Caution.is_plain());
        assert_eq!(AlertKind::Caution.name(), "CautionAlert");

        // evaluate_alert_parameters without filter
        let params_plain = evaluate_alert_parameters(128, 0, AlertKind::Alert);
        assert_eq!(params_plain.alert_id(), 128);
        assert_eq!(params_plain.filter_proc(), 0);
        assert!(!params_plain.has_filter_proc());
        assert_eq!(params_plain.kind(), AlertKind::Alert);
        assert_eq!(params_plain.icon_id(), None);

        // evaluate_alert_parameters with filter and icon
        let params_stop = evaluate_alert_parameters(-300, 0x0012_3456, AlertKind::Stop);
        assert_eq!(params_stop.alert_id(), -300);
        assert_eq!(params_stop.filter_proc(), 0x0012_3456);
        assert!(params_stop.has_filter_proc());
        assert_eq!(params_stop.kind(), AlertKind::Stop);
        assert_eq!(params_stop.icon_id(), Some(0));

        let params_note = evaluate_alert_parameters(2000, 0x0078_9ABC, AlertKind::Note);
        assert_eq!(params_note.alert_id(), 2000);
        assert_eq!(params_note.filter_proc(), 0x0078_9ABC);
        assert!(params_note.has_filter_proc());
        assert_eq!(params_note.kind(), AlertKind::Note);
        assert_eq!(params_note.icon_id(), Some(1));

        let params_caution = evaluate_alert_parameters(2001, 0, AlertKind::Caution);
        assert_eq!(params_caution.alert_id(), 2001);
        assert_eq!(params_caution.filter_proc(), 0);
        assert!(!params_caution.has_filter_proc());
        assert_eq!(params_caution.kind(), AlertKind::Caution);
        assert_eq!(params_caution.icon_id(), Some(2));

        // Direct constructor
        let direct = AlertParameters::new(500, 0x4000, AlertKind::Stop);
        assert_eq!(direct.alert_id(), 500);
        assert_eq!(direct.filter_proc(), 0x4000);
        assert!(direct.has_filter_proc());
        assert_eq!(direct.kind(), AlertKind::Stop);
    }

    #[test]
    fn std_filter_proc_parameter_and_event_evaluation() {
        // evaluate_std_filter_proc_parameters validation
        assert_eq!(evaluate_std_filter_proc_parameters(0, 0x1000, 0x2000), None);
        assert_eq!(evaluate_std_filter_proc_parameters(0x3000, 0, 0x2000), None);
        assert_eq!(evaluate_std_filter_proc_parameters(0x3000, 0x1000, 0), None);

        let params = evaluate_std_filter_proc_parameters(0x3000, 0x1000, 0x2000).unwrap();
        assert_eq!(params.dialog_ptr(), 0x3000);
        assert_eq!(params.event_ptr(), 0x1000);
        assert_eq!(params.item_hit_ptr(), 0x2000);

        let direct = StdFilterProcParameters::new(0x4000, 0x5000, 0x6000);
        assert_eq!(direct.dialog_ptr(), 0x4000);
        assert_eq!(direct.event_ptr(), 0x5000);
        assert_eq!(direct.item_hit_ptr(), 0x6000);

        // StdFilterProcEvaluation accessors
        let unhandled = StdFilterProcEvaluation::unhandled();
        assert!(!unhandled.is_handled());
        assert_eq!(unhandled.item_hit(), None);
        assert_eq!(unhandled.boolean_result(), 0);

        let handled = StdFilterProcEvaluation::handled(1);
        assert!(handled.is_handled());
        assert_eq!(handled.item_hit(), Some(1));
        assert_eq!(handled.boolean_result(), 1);

        // evaluate_std_filter_proc decision evaluation
        assert_eq!(
            evaluate_std_filter_proc(DialogFilterDecision::TriggerDefaultButton, Some(1)),
            StdFilterProcEvaluation::handled(1)
        );
        assert_eq!(
            evaluate_std_filter_proc(DialogFilterDecision::TriggerDefaultButton, Some(2)),
            StdFilterProcEvaluation::handled(2)
        );
        assert_eq!(
            evaluate_std_filter_proc(DialogFilterDecision::TriggerDefaultButton, Some(0)),
            StdFilterProcEvaluation::unhandled()
        );
        assert_eq!(
            evaluate_std_filter_proc(DialogFilterDecision::TriggerDefaultButton, Some(-1)),
            StdFilterProcEvaluation::unhandled()
        );
        assert_eq!(
            evaluate_std_filter_proc(DialogFilterDecision::TriggerDefaultButton, None),
            StdFilterProcEvaluation::unhandled()
        );
        assert_eq!(
            evaluate_std_filter_proc(DialogFilterDecision::TriggerCancelButton, Some(1)),
            StdFilterProcEvaluation::unhandled()
        );
        assert_eq!(
            evaluate_std_filter_proc(DialogFilterDecision::AdvanceEditTextFocus, Some(1)),
            StdFilterProcEvaluation::unhandled()
        );
        assert_eq!(
            evaluate_std_filter_proc(DialogFilterDecision::Unhandled, Some(1)),
            StdFilterProcEvaluation::unhandled()
        );

        // evaluate_std_filter_proc_event event evaluation
        let return_msg = (u32::from(KEY_RETURN) << 8) | u32::from(CHAR_RETURN);
        let eval_return = evaluate_std_filter_proc_event(EVENT_KEY_DOWN, return_msg, 0, Some(2));
        assert!(eval_return.is_handled());
        assert_eq!(eval_return.item_hit(), Some(2));
        assert_eq!(eval_return.boolean_result(), 1);

        let eval_return_auto = evaluate_std_filter_proc_event(EVENT_AUTO_KEY, return_msg, 0, Some(3));
        assert!(eval_return_auto.is_handled());
        assert_eq!(eval_return_auto.item_hit(), Some(3));
        assert_eq!(eval_return_auto.boolean_result(), 1);

        let enter_msg = (u32::from(KEY_NUMPAD_ENTER) << 8) | u32::from(CHAR_ENTER);
        let eval_enter = evaluate_std_filter_proc_event(EVENT_KEY_DOWN, enter_msg, 0, Some(1));
        assert!(eval_enter.is_handled());
        assert_eq!(eval_enter.item_hit(), Some(1));
        assert_eq!(eval_enter.boolean_result(), 1);

        let esc_msg = (u32::from(KEY_ESCAPE) << 8) | u32::from(CHAR_ESCAPE);
        let eval_esc = evaluate_std_filter_proc_event(EVENT_KEY_DOWN, esc_msg, 0, Some(1));
        assert!(!eval_esc.is_handled());
        assert_eq!(eval_esc.item_hit(), None);
        assert_eq!(eval_esc.boolean_result(), 0);

        let eval_no_default = evaluate_std_filter_proc_event(EVENT_KEY_DOWN, return_msg, 0, None);
        assert!(!eval_no_default.is_handled());
        assert_eq!(eval_no_default.item_hit(), None);
        assert_eq!(eval_no_default.boolean_result(), 0);

        let eval_mouse = evaluate_std_filter_proc_event(1, return_msg, 0, Some(1));
        assert!(!eval_mouse.is_handled());
        assert_eq!(eval_mouse.item_hit(), None);
        assert_eq!(eval_mouse.boolean_result(), 0);
    }

    #[test]
    fn dialog_default_button_outline_evaluation() {
        let direct = DialogDefaultButtonOutline::new((10, 20, 30, 40), 6, 6, 3);
        assert_eq!(direct.outer_rect(), (10, 20, 30, 40));
        assert_eq!(direct.oval_width(), 6);
        assert_eq!(direct.oval_height(), 6);
        assert_eq!(direct.oval(), 6);
        assert_eq!(direct.thickness(), 3);

        // evaluate_dialog_default_button_outline
        let outline = evaluate_dialog_default_button_outline((10, 20, 30, 40));
        assert_eq!(outline.outer_rect(), (6, 16, 34, 44));
        assert_eq!(outline.oval_width(), 10);
        assert_eq!(outline.oval_height(), 10);
        assert_eq!(outline.oval(), 10);
        assert_eq!(outline.thickness(), DEFAULT_BUTTON_OUTLINE_THICKNESS);

        // evaluate_dialog_item_default_button_outline
        let item_match = evaluate_dialog_item_default_button_outline(
            1,
            1,
            DIALOG_ITEM_BUTTON,
            (10, 20, 30, 40),
        );
        assert_eq!(item_match, Some(outline));

        // Disabled button flag preserved
        let disabled_button = evaluate_dialog_item_default_button_outline(
            1,
            1,
            DIALOG_ITEM_BUTTON | DIALOG_ITEM_DISABLED_FLAG,
            (10, 20, 30, 40),
        );
        assert_eq!(disabled_button, Some(outline));

        // Non-matching item number
        assert_eq!(
            evaluate_dialog_item_default_button_outline(
                2,
                1,
                DIALOG_ITEM_BUTTON,
                (10, 20, 30, 40),
            ),
            None
        );

        // No default item (0)
        assert_eq!(
            evaluate_dialog_item_default_button_outline(
                1,
                0,
                DIALOG_ITEM_BUTTON,
                (10, 20, 30, 40),
            ),
            None
        );

        // Non-button item types
        assert_eq!(
            evaluate_dialog_item_default_button_outline(
                1,
                1,
                DIALOG_ITEM_CHECKBOX,
                (10, 20, 30, 40),
            ),
            None
        );
        assert_eq!(
            evaluate_dialog_item_default_button_outline(
                1,
                1,
                DIALOG_ITEM_STATIC_TEXT,
                (10, 20, 30, 40),
            ),
            None
        );
    }

    #[test]
    fn dialog_keyboard_focus_textedit_paramtext_and_timeout_evaluation() {
        // 1. GetDialogKeyboardFocusItem
        assert_eq!(evaluate_get_dialog_keyboard_focus_item(0, None), 0);
        assert_eq!(evaluate_get_dialog_keyboard_focus_item(0, Some(0)), 0);
        assert_eq!(evaluate_get_dialog_keyboard_focus_item(0x1000, None), 0);
        assert_eq!(evaluate_get_dialog_keyboard_focus_item(0x1000, Some(-1)), 0);
        assert_eq!(evaluate_get_dialog_keyboard_focus_item(0x1000, Some(0)), 1);
        assert_eq!(evaluate_get_dialog_keyboard_focus_item(0x1000, Some(2)), 3);

        // 2. SetDialogKeyboardFocusItem
        assert_eq!(
            evaluate_set_dialog_keyboard_focus_item_parameters(0, 1),
            Err(DIALOG_PARAM_ERR)
        );
        let params_clear = evaluate_set_dialog_keyboard_focus_item_parameters(0x1000, 0).unwrap();
        assert_eq!(params_clear.dialog_ptr(), 0x1000);
        assert_eq!(params_clear.item_index(), 0);
        assert_eq!(params_clear.target_edit_field(), -1);

        let params_neg = evaluate_set_dialog_keyboard_focus_item_parameters(0x1000, -5).unwrap();
        assert_eq!(params_neg.target_edit_field(), -1);

        let params_focus = evaluate_set_dialog_keyboard_focus_item_parameters(0x1000, 3).unwrap();
        assert_eq!(params_focus.dialog_ptr(), 0x1000);
        assert_eq!(params_focus.item_index(), 3);
        assert_eq!(params_focus.target_edit_field(), 2);

        // 3. GetDialogTextEditHandle
        assert_eq!(evaluate_get_dialog_text_edit_handle(0, None), 0);
        assert_eq!(evaluate_get_dialog_text_edit_handle(0, Some(0x2000)), 0);
        assert_eq!(evaluate_get_dialog_text_edit_handle(0x1000, None), 0);
        assert_eq!(evaluate_get_dialog_text_edit_handle(0x1000, Some(0x2000)), 0x2000);

        // 4. GetParamText
        let params_text = evaluate_get_param_text_parameters(0x10, 0x20, 0, 0x40);
        assert_eq!(params_text.param(0), 0x10);
        assert_eq!(params_text.param(1), 0x20);
        assert_eq!(params_text.param(2), 0);
        assert_eq!(params_text.param(3), 0x40);

        // 5. SetDialogTimeout
        assert_eq!(
            evaluate_set_dialog_timeout_parameters(0, 1, 10),
            Err(DIALOG_PARAM_ERR)
        );
        let timeout_params = evaluate_set_dialog_timeout_parameters(0x1000, 2, 30).unwrap();
        assert_eq!(timeout_params.dialog_ptr(), 0x1000);
        assert_eq!(timeout_params.button_to_press(), 2);
        assert_eq!(timeout_params.seconds_to_wait(), 30);

        // 6. GetDialogTimeout
        assert_eq!(
            evaluate_get_dialog_timeout_parameters(0, 0x2000, true, 0x2004, true, 0x2008, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_dialog_timeout_parameters(0x1000, 0x2000, false, 0x2004, true, 0x2008, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_dialog_timeout_parameters(0x1000, 0x2000, true, 0x2004, false, 0x2008, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_dialog_timeout_parameters(0x1000, 0x2000, true, 0x2004, true, 0x2008, false),
            Err(DIALOG_PARAM_ERR)
        );
        let get_timeout = evaluate_get_dialog_timeout_parameters(
            0x1000,
            0x2000,
            true,
            0x2004,
            true,
            0x2008,
            true,
        )
        .unwrap();
        assert_eq!(get_timeout.dialog_ptr(), 0x1000);
        assert_eq!(get_timeout.out_button_ptr(), 0x2000);
        assert_eq!(get_timeout.out_seconds_ptr(), 0x2004);
        assert_eq!(get_timeout.out_remaining_ptr(), 0x2008);

        // 7. evaluate_dialog_timeout_remaining
        assert_eq!(evaluate_dialog_timeout_remaining(0, 1000, 2000), 0);
        assert_eq!(evaluate_dialog_timeout_remaining(10, 1000, 1000), 10);
        // 120 ticks = 2 seconds elapsed
        assert_eq!(evaluate_dialog_timeout_remaining(10, 1000, 1120), 8);
        // 600 ticks = 10 seconds elapsed
        assert_eq!(evaluate_dialog_timeout_remaining(10, 1000, 1600), 0);
        // 1200 ticks = 20 seconds elapsed (saturates at 0)
        assert_eq!(evaluate_dialog_timeout_remaining(10, 1000, 2200), 0);
    }

    #[test]
    fn standard_alert_sheet_and_event_mask_evaluation() {
        // 1. evaluate_create_standard_alert_parameters
        assert_eq!(
            evaluate_create_standard_alert_parameters(1, 0x1000, 0x1010, 0x1020, 0, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_create_standard_alert_parameters(1, 0x1000, 0x1010, 0x1020, 0x2000, false),
            Err(DIALOG_PARAM_ERR)
        );
        let alert_params = evaluate_create_standard_alert_parameters(
            2, 0x1000, 0x1010, 0x1020, 0x2000, true,
        )
        .unwrap();
        assert_eq!(alert_params.alert_type(), 2);
        assert_eq!(alert_params.error_ptr(), 0x1000);
        assert_eq!(alert_params.explanation_ptr(), 0x1010);
        assert_eq!(alert_params.alert_param_ptr(), 0x1020);
        assert_eq!(alert_params.out_alert_ptr(), 0x2000);

        // 2. evaluate_run_standard_alert_parameters
        assert_eq!(
            evaluate_run_standard_alert_parameters(0, 0x3000, 0x4000, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_run_standard_alert_parameters(0x5000, 0x3000, 0, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_run_standard_alert_parameters(0x5000, 0x3000, 0x4000, false),
            Err(DIALOG_PARAM_ERR)
        );
        let run_params = evaluate_run_standard_alert_parameters(0x5000, 0x3000, 0x4000, true).unwrap();
        assert_eq!(run_params.dialog_ptr(), 0x5000);
        assert_eq!(run_params.filter_proc(), 0x3000);
        assert_eq!(run_params.out_item_hit_ptr(), 0x4000);

        // 3. evaluate_close_standard_sheet_parameters
        assert_eq!(
            evaluate_close_standard_sheet_parameters(0, 1),
            Err(DIALOG_PARAM_ERR)
        );
        let close_params = evaluate_close_standard_sheet_parameters(0x5000, 42).unwrap();
        assert_eq!(close_params.sheet_ptr(), 0x5000);
        assert_eq!(close_params.result_command(), 42);

        // 4. evaluate_get_standard_alert_default_params_parameters
        assert_eq!(
            evaluate_get_standard_alert_default_params_parameters(0, 1, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_standard_alert_default_params_parameters(0x1000, 0, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_standard_alert_default_params_parameters(0x1000, 2, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_standard_alert_default_params_parameters(0x1000, 1, false),
            Err(DIALOG_PARAM_ERR)
        );
        let def_params = evaluate_get_standard_alert_default_params_parameters(0x1000, 1, true).unwrap();
        assert_eq!(def_params.param_ptr(), 0x1000);
        assert_eq!(def_params.version(), 1);

        // 5. evaluate_get_modal_dialog_event_mask_parameters & evaluate_set_modal_dialog_event_mask_parameters
        assert_eq!(
            evaluate_get_modal_dialog_event_mask_parameters(0, 0x2000, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_modal_dialog_event_mask_parameters(0x1000, 0, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_modal_dialog_event_mask_parameters(0x1000, 0x2000, false),
            Err(DIALOG_PARAM_ERR)
        );
        let get_mask = evaluate_get_modal_dialog_event_mask_parameters(0x1000, 0x2000, true).unwrap();
        assert_eq!(get_mask.dialog_ptr(), 0x1000);
        assert_eq!(get_mask.out_mask_ptr(), 0x2000);

        assert_eq!(
            evaluate_set_modal_dialog_event_mask_parameters(0, 0x01FF),
            Err(DIALOG_PARAM_ERR)
        );
        let set_mask = evaluate_set_modal_dialog_event_mask_parameters(0x1000, 0x01FF).unwrap();
        assert_eq!(set_mask.dialog_ptr(), 0x1000);
        assert_eq!(set_mask.mask(), 0x01FF);

        // 6. evaluate_flash_dialog_control_parameters
        assert_eq!(evaluate_flash_dialog_control_parameters(0, 1), Err(DIALOG_PARAM_ERR));
        assert_eq!(evaluate_flash_dialog_control_parameters(0x1000, 0), Err(DIALOG_PARAM_ERR));
        assert_eq!(evaluate_flash_dialog_control_parameters(0x1000, -1), Err(DIALOG_PARAM_ERR));
        let flash = evaluate_flash_dialog_control_parameters(0x1000, 2).unwrap();
        assert_eq!(flash.dialog_ptr(), 0x1000);
        assert_eq!(flash.item_index(), 2);

        // 7. evaluate_get_dialog_item_init_parameters
        assert_eq!(
            evaluate_get_dialog_item_init_parameters(0, 1, 0x2000, 0x2004, 0x2008),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_dialog_item_init_parameters(0x1000, 0, 0x2000, 0x2004, 0x2008),
            Err(DIALOG_PARAM_ERR)
        );
        let item_init = evaluate_get_dialog_item_init_parameters(0x1000, 1, 0x2000, 0x2004, 0x2008).unwrap();
        assert_eq!(item_init.dialog_ptr(), 0x1000);
        assert_eq!(item_init.item_index(), 1);
        assert_eq!(item_init.out_type_ptr(), 0x2000);
        assert_eq!(item_init.out_handle_ptr(), 0x2004);
        assert_eq!(item_init.out_rect_ptr(), 0x2008);

        // 8. evaluate_set_dialog_filter_parameters
        assert_eq!(evaluate_set_dialog_filter_parameters(0, 0x3000), Err(DIALOG_PARAM_ERR));
        let set_filter = evaluate_set_dialog_filter_parameters(0x1000, 0x3000).unwrap();
        assert_eq!(set_filter.dialog_ptr(), 0x1000);
        assert_eq!(set_filter.filter_proc(), 0x3000);
    }

    #[test]
    fn dialog_auto_positioning_and_cursor_tracking_evaluation() {
        // 1. dialog_position_code_to_method & normalize_dialog_position_method
        assert_eq!(dialog_position_code_to_method(0x280A), Some(crate::window_manager::WINDOW_CENTER_ON_MAIN_SCREEN));
        assert_eq!(dialog_position_code_to_method(0x300A), Some(crate::window_manager::WINDOW_ALERT_POSITION_ON_MAIN_SCREEN));
        assert_eq!(dialog_position_code_to_method(0x380A), Some(crate::window_manager::WINDOW_STAGGER_ON_MAIN_SCREEN));
        assert_eq!(dialog_position_code_to_method(0xA80A), Some(crate::window_manager::WINDOW_CENTER_ON_PARENT_WINDOW));
        assert_eq!(dialog_position_code_to_method(0xB00A), Some(crate::window_manager::WINDOW_ALERT_POSITION_ON_PARENT_WINDOW));
        assert_eq!(dialog_position_code_to_method(0xB80A), Some(crate::window_manager::WINDOW_STAGGER_ON_PARENT_WINDOW));
        assert_eq!(dialog_position_code_to_method(0x680A), Some(crate::window_manager::WINDOW_CENTER_ON_PARENT_WINDOW_SCREEN));
        assert_eq!(dialog_position_code_to_method(0x700A), Some(crate::window_manager::WINDOW_ALERT_POSITION_ON_PARENT_WINDOW_SCREEN));
        assert_eq!(dialog_position_code_to_method(0x780A), Some(crate::window_manager::WINDOW_STAGGER_ON_PARENT_WINDOW_SCREEN));
        assert_eq!(dialog_position_code_to_method(0x0000), None);
        assert_eq!(dialog_position_code_to_method(0x1234), None);

        for method in 1..=9 {
            assert_eq!(normalize_dialog_position_method(method), Some(method));
        }
        assert_eq!(normalize_dialog_position_method(0x280A), Some(1));
        assert_eq!(normalize_dialog_position_method(0), None);

        // 2. evaluate_dialog_position_bounds
        let content = (0, 0, 100, 200); // height 100, width 200
        let screen_w = 800;
        let screen_h = 600;
        let menu_h = 20;

        // Center on main screen: target area 20..600 (height 580), 0..800 (width 800)
        let centered = evaluate_dialog_position_bounds(content, 0x280A, None, None, screen_w, screen_h, menu_h);
        assert_eq!(centered.0, 20 + (580 - 100) / 2); // 260
        assert_eq!(centered.1, (800 - 200) / 2); // 300

        // Alert on main screen: 1/5th from top
        let alert = evaluate_dialog_position_bounds(content, 0x300A, None, None, screen_w, screen_h, menu_h);
        assert_eq!(alert.0, 20 + (580 - 100) / 5); // 116
        assert_eq!(alert.1, (800 - 200) / 2); // 300

        // Stagger on main screen without parent window falls back to centering
        let stagger = evaluate_dialog_position_bounds(content, 0x380A, None, None, screen_w, screen_h, menu_h);
        assert_eq!(stagger, centered);

        let parent = (100, 100, 400, 500); // parent height 300, width 400

        // Stagger on main screen with parent window
        let stagger_parent = evaluate_dialog_position_bounds(content, 0x380A, None, Some(parent), screen_w, screen_h, menu_h);
        assert_eq!(stagger_parent.0, 100 + 20); // 120
        assert_eq!(stagger_parent.1, 100 + 20); // 120

        // Stagger on parent window (0xB80A)
        let parent_stagger = evaluate_dialog_position_bounds(content, 0xB80A, None, Some(parent), screen_w, screen_h, menu_h);
        assert_eq!(parent_stagger.0, 100 + 20); // 120
        assert_eq!(parent_stagger.1, 100 + 20); // 120

        // Center on parent window
        let parent_centered = evaluate_dialog_position_bounds(content, 0xA80A, None, Some(parent), screen_w, screen_h, menu_h);
        assert_eq!(parent_centered.0, 100 + (300 - 100) / 2); // 200
        assert_eq!(parent_centered.1, 100 + (400 - 200) / 2); // 200

        // Parent window fallback when None
        let parent_fallback = evaluate_dialog_position_bounds(content, 0xA80A, None, None, screen_w, screen_h, menu_h);
        assert_eq!(parent_fallback, centered);

        // Position 0 leaves unchanged
        let unpositioned = evaluate_dialog_position_bounds(content, 0x0000, None, None, screen_w, screen_h, menu_h);
        assert_eq!(unpositioned, content);

        // 3. Cursor tracking evaluation
        assert_eq!(
            evaluate_get_dialog_tracks_cursor_parameters(0x1000, 0, true),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_get_dialog_tracks_cursor_parameters(0x1000, 0x2000, false),
            Err(DIALOG_PARAM_ERR)
        );
        let get_tracks = evaluate_get_dialog_tracks_cursor_parameters(0x1000, 0x2000, true).unwrap();
        assert_eq!(get_tracks.dialog_ptr(), 0x1000);
        assert_eq!(get_tracks.out_tracks_ptr(), 0x2000);

        assert_eq!(evaluate_is_dialog_tracks_cursor_parameters(0x1000), Ok(0x1000));
        assert_eq!(evaluate_is_dialog_tracks_cursor_parameters(0), Ok(0));

        // 4. AutoPositionDialog evaluation
        assert_eq!(
            evaluate_auto_position_dialog_parameters(0, 0, 1),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_auto_position_dialog_parameters(0x1000, 0, 0),
            Err(DIALOG_PARAM_ERR)
        );
        assert_eq!(
            evaluate_auto_position_dialog_parameters(0x1000, 0, 99),
            Err(DIALOG_PARAM_ERR)
        );
        let auto_pos = evaluate_auto_position_dialog_parameters(0x1000, 0x2000, 0x280A).unwrap();
        assert_eq!(auto_pos.dialog_ptr(), 0x1000);
        assert_eq!(auto_pos.parent_ptr(), 0x2000);
        assert_eq!(auto_pos.method(), 1);
    }
}


