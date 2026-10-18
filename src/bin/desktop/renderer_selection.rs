//! Explicit rollout policy: auto remains platform until hardware qualification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub(super) enum Renderer {
    #[default]
    Auto,
    Wgpu,
    Platform,
    /// CPU presentation, without enabling GPU compute.
    Software,
}
impl Renderer {
    pub fn uses_wgpu(self, legacy_gpu: bool) -> bool {
        legacy_gpu || self == Self::Wgpu
    }
}

/// Desktop presentation after a terminal wgpu failure. Qualification keeps
/// the existing platform fallback unless software is explicitly requested.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub(super) enum GpuFallback {
    #[default]
    Platform,
    Software,
}
impl GpuFallback {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Platform => "platform",
            Self::Software => "software",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_selection_requires_a_separate_qualification_decision() {
        assert!(!Renderer::default().uses_wgpu(false));
        assert!(!Renderer::Platform.uses_wgpu(false));
        assert!(!Renderer::Software.uses_wgpu(false));
        assert!(Renderer::Wgpu.uses_wgpu(false));
        assert!(Renderer::Auto.uses_wgpu(true));
    }
}
