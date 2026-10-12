//! Guest cursor presentation state shared by classic and native CPU adapters.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorSnapshot {
    /// Retained even when hidden; supports both monochrome and colour cursors.
    pub image: Option<crate::display::CursorImage>,
    pub visible: bool,
    pub level: i16,
    /// Macintosh global coordinates (vertical, horizontal).
    pub position: (i16, i16),
}
