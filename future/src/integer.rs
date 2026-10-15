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
