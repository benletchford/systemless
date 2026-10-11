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
pub struct StandardFileSnapshot {
    /// Address of the caller's reply record; pair with generation.
    pub guest_id: u32,
    /// Changes for each retained Standard File invocation.
    pub generation: u64,
    pub kind: StandardFileKind,
    pub bounds: (i16, i16, i16, i16),
    pub directory_id: u32,
    /// `None` means this adapter has not extracted the file list yet.
    pub entries: Option<Vec<StandardFileEntrySnapshot>>,
    pub selected: Option<usize>,
    pub prompt: Option<String>,
    pub name: Option<String>,
    pub name_selection: Option<(usize, usize)>,
}
