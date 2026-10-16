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

/// Signed Integer/Integer to Fixed (16.16), using post-128K Macintosh
/// nearest rounding and signed saturation. IM: OS Utilities, pp. 3-12, 3-46–47.
pub fn fixed_ratio(numerator: i16, denominator: i16) -> i32 {
    if denominator == 0 {
        return if numerator < 0 { i32::MIN } else { i32::MAX };
    }
    let n = i64::from(numerator);
    let d = i64::from(denominator);
    let scaled = n.abs() * 65536;
    let divisor = d.abs();
    let magnitude = (scaled + divisor / 2) / divisor;
    let result = if (n < 0) != (d < 0) {
        -magnitude
    } else {
        magnitude
    };
    result.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// Signed LongInt to Fixed with saturation (IM: OS Utilities, p. 3-43).
pub fn long_to_fixed(value: i32) -> i32 {
    (i64::from(value) * 65536).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}
