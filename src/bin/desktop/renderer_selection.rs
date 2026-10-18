//! Explicit rollout policy: auto remains platform until hardware qualification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub(super) enum Renderer {
    #[default]
    Auto,
    Wgpu,
    Platform,
}
impl Renderer {
    pub fn uses_wgpu(self, legacy_gpu: bool) -> bool {
        legacy_gpu || self == Self::Wgpu
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_selection_requires_a_separate_qualification_decision() {
        assert!(!Renderer::default().uses_wgpu(false));
        assert!(!Renderer::Platform.uses_wgpu(false));
        assert!(Renderer::Wgpu.uses_wgpu(false));
        assert!(Renderer::Auto.uses_wgpu(true));
    }
}
