//! Small contracts shared with embedding hosts. Guest-specific input and
//! configuration live with their guest implementation.

/// Maximum guest instructions to execute in one call. This is work, not
/// elapsed host time or a requested number of guest ticks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstructionBudget(pub usize);

/// Result of one bounded execution call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdvanceResult {
    pub instructions: usize,
    pub running: bool,
    /// Guest clock after this call, in the active system's tick unit.
    pub guest_tick: u32,
}

/// Owned pixels in row-major order. Callers may retain this across advances.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    Rgba8,
}

/// Owned audio samples. The format is explicit rather than a universal default.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioChunk {
    pub format: AudioFormat,
    pub samples: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioFormat {
    Unsigned8BitMono { sample_rate_hz: u32 },
}
