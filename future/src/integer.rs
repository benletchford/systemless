//! Shared signed word extraction for Macintosh mathematical utilities.
//! Inside Macintosh: Operating System Utilities, Mathematical and Logical
//! Utilities, “Extracting a Word From a Long Word”, p. 3-18.
pub fn word(value: u32, high: bool) -> i16 {
    if high {
        (value >> 16) as i16
    } else {
        value as i16
    }
}

/// Toolbox logical operations use all 32 bits and have no manager state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Logic {
    And,
    Or,
    Xor,
    Not,
}
impl Logic {
    pub fn apply(self, left: u32, right: u32) -> u32 {
        match self {
            Self::And => left & right,
            Self::Or => left | right,
            Self::Xor => left ^ right,
            Self::Not => !left,
        }
    }
}
