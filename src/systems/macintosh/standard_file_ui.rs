//! Read-only presentation state for retained Standard File panels.
//!
//! Standard File owns its modal event loop and reply record. The host may
//! observe this state, but input must continue through the guest event path.
//! Inside Macintosh: Files (1992), pp. 3-3--3-13, 3-44--3-47.

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardFileKind {
    Get,
    Put,
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardFileEntrySnapshot {
    pub name: String,
    pub directory_id: u32,
    pub is_directory: bool,
    pub file_type: u32,
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardFilePutLayout {
    pub directory_label: (i16, i16, i16, i16),
    pub list: (i16, i16, i16, i16),
    pub scroll: (i16, i16, i16, i16),
    pub prompt: (i16, i16, i16, i16),
    pub name: (i16, i16, i16, i16),
    pub desktop: (i16, i16, i16, i16),
    pub cancel: (i16, i16, i16, i16),
    pub save: (i16, i16, i16, i16),
    pub row_height: i16,
    pub first_visible: usize,
    pub visible_rows: usize,
}

impl StandardFilePutLayout {
    pub(crate) fn global_rect(
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
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardFileSnapshot {
    /// Address of the caller's reply record; pair with generation.
    pub guest_id: u32,
    /// Changes for each retained Standard File invocation.
    pub generation: u64,
    pub kind: StandardFileKind,
    /// True only for the modern standard entry points. This alone does not
    /// qualify a panel for an overlay; its guest behavior must also be complete.
    pub standard_entry_point: bool,
    pub bounds: (i16, i16, i16, i16),
    pub directory_id: u32,
    /// `None` means the current guest panel has no exposed file list.
    pub entries: Option<Vec<StandardFileEntrySnapshot>>,
    pub selected: Option<usize>,
    pub prompt: Option<String>,
    pub name: Option<String>,
    pub name_selection: Option<(usize, usize)>,
    /// `None` for Open panels; Save reports where guest keyboard input goes.
    pub name_has_focus: Option<bool>,
    pub directory_label: Option<String>,
    pub put_layout: Option<StandardFilePutLayout>,
}
