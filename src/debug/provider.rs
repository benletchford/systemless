use super::capabilities::ProviderCapabilities;
use super::error::{DebugError, DebugResult};
use super::ids::ProviderId;
use super::model::{
    ByteOrder, DisplayOutputDescriptor, GraphicsSnapshot, GraphicsSurfaceDescriptor,
    InspectionProviderDescriptor, PixelFormat, ProviderSnapshotKind, SurfaceKind,
};
use crate::runner::FixtureRunner;
use std::sync::{Arc, OnceLock};

#[derive(Clone, Debug)]
pub struct ProviderArtifact {
    pub format: Option<String>,
    pub bytes: Vec<u8>,
    pub provenance: String,
    pub related: Vec<u64>,
}

/// Bounded, observational subsystem inspection, registered through
/// [`FixtureRunner::debug_register_provider`]. Borrow live state only for the
/// call and return owned snapshots/artifacts; providers may own configuration.
///
/// Descriptor and capability metadata must agree. Declare and obey collection
/// and artifact limits. The descriptor's required boundary applies to both live
/// reads (command safe point) and captures. Use [`ProviderSnapshot::Custom`] with
/// a schema version for additional subsystems; capture handling is shared.
pub trait InspectionProvider: std::fmt::Debug + Send + Sync {
    fn descriptor(&self) -> InspectionProviderDescriptor;
    fn capabilities(&self) -> ProviderCapabilities;
    fn snapshot(&self, runner: &FixtureRunner) -> DebugResult<ProviderSnapshot>;
    fn artifacts(&self, _runner: &FixtureRunner) -> DebugResult<Vec<ProviderArtifact>> {
        Ok(Vec::new())
    }
}

impl dyn InspectionProvider + '_ {
    pub(crate) fn validate_boundary(
        &self,
        boundary: super::model::CaptureBoundaryKind,
    ) -> DebugResult<()> {
        let descriptor = self.descriptor();
        if let Some(required) = descriptor.required_boundary {
            if required != boundary {
                return Err(DebugError::BoundaryUnavailable {
                    detail: format!("provider {} requires boundary {required:?}", descriptor.id),
                });
            }
        }
        Ok(())
    }

    pub(crate) fn snapshot_at(
        &self,
        runner: &FixtureRunner,
        boundary: super::model::CaptureBoundaryKind,
    ) -> DebugResult<ProviderSnapshot> {
        self.validate_boundary(boundary)?;
        self.snapshot(runner)
    }

    pub(crate) fn artifacts_at(
        &self,
        runner: &FixtureRunner,
        boundary: super::model::CaptureBoundaryKind,
    ) -> DebugResult<Vec<ProviderArtifact>> {
        self.validate_boundary(boundary)?;
        self.artifacts(runner)
    }
}

pub type ProviderRegistration = Arc<dyn InspectionProvider>;

pub type SnapshotFn = fn(&FixtureRunner) -> DebugResult<ProviderSnapshot>;
pub type ArtifactFn = fn(&FixtureRunner) -> DebugResult<Vec<ProviderArtifact>>;

#[derive(Clone, Debug)]
pub struct DeclaredProvider {
    descriptor: InspectionProviderDescriptor,
    capabilities: ProviderCapabilities,
    snapshot: Option<SnapshotFn>,
    artifacts: Option<ArtifactFn>,
}

impl DeclaredProvider {
    pub fn new(
        descriptor: InspectionProviderDescriptor,
        capabilities: ProviderCapabilities,
    ) -> Self {
        Self {
            descriptor,
            capabilities,
            snapshot: None,
            artifacts: None,
        }
    }

    pub fn with_snapshot(mut self, snapshot: SnapshotFn) -> Self {
        self.snapshot = Some(snapshot);
        self
    }

    pub fn with_artifacts(mut self, artifacts: ArtifactFn) -> Self {
        self.artifacts = Some(artifacts);
        self
    }
}

impl InspectionProvider for DeclaredProvider {
    fn descriptor(&self) -> InspectionProviderDescriptor {
        self.descriptor.clone()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    fn snapshot(&self, runner: &FixtureRunner) -> DebugResult<ProviderSnapshot> {
        match self.snapshot {
            Some(snapshot) => snapshot(runner),
            None => Err(DebugError::unsupported("provider snapshot")),
        }
    }

    fn artifacts(&self, runner: &FixtureRunner) -> DebugResult<Vec<ProviderArtifact>> {
        match self.artifacts {
            Some(artifacts) => artifacts(runner),
            None => Ok(Vec::new()),
        }
    }
}

pub(crate) fn validate_provider_contract(provider: &dyn InspectionProvider) -> DebugResult<()> {
    let descriptor = provider.descriptor();
    let capabilities = provider.capabilities();
    if descriptor.observational != capabilities.observational
        || descriptor.snapshots != capabilities.snapshots
        || descriptor.collection_limit != capabilities.collection_limit
        || descriptor.limitations != capabilities.limitations
    {
        return Err(DebugError::InvalidValue {
            detail: format!(
                "provider {} descriptor and capabilities metadata disagree",
                descriptor.id
            ),
        });
    }
    Ok(())
}

fn provider(
    id: u64,
    name: &str,
    schema_version: u32,
    snapshots: Vec<ProviderSnapshotKind>,
    limitations: &[&str],
    snapshot: SnapshotFn,
) -> DeclaredProvider {
    let mut capabilities = ProviderCapabilities {
        observational: true,
        snapshots: snapshots.clone(),
        ..ProviderCapabilities::default()
    };
    capabilities.limitations = limitations.iter().map(|line| line.to_string()).collect();
    DeclaredProvider::new(
        InspectionProviderDescriptor {
            id: ProviderId(id),
            name: name.to_string(),
            schema_version,
            observational: true,
            snapshots,
            collection_limit: capabilities.collection_limit,
            required_boundary: None,
            limitations: capabilities.limitations.clone(),
        },
        capabilities,
    )
    .with_snapshot(snapshot)
}

pub mod provider_ids {
    pub const GRAPHICS: u64 = 1;
    pub const WINDOWS: u64 = 2;
    pub const EVENTS: u64 = 3;
    pub const SYMBOLS: u64 = 4;
}

/// Entry cap a provider inherits unless it declares its own. Providers whose
/// natural collections are larger raise it; the byte budget below is what keeps
/// a snapshot from growing without bound.
pub const DEFAULT_COLLECTION_LIMIT: usize = 256;
/// Default snapshot byte budget. A provider truncates when either this or its
/// collection limit is reached, whichever comes first.
pub const DEFAULT_SNAPSHOT_BYTES: u64 = 4 * 1024 * 1024;
/// A symbol table is naturally thousands of entries; capping it at the default
/// 256 would discard most of a real one, so it gets its own entry limit and
/// relies on the byte budget for memory safety.
pub const SYMBOL_COLLECTION_LIMIT: usize = 16 * 1024;
pub const MAX_FRAME_ARTIFACT_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum ProviderSnapshot {
    Graphics {
        schema_version: u32,
        state: GraphicsSnapshot,
        truncated: bool,
    },
    Events {
        schema_version: u32,
        state: crate::event_queue::EventManagerSnapshot,
        truncated: bool,
    },
    Windows {
        schema_version: u32,
        windows: Vec<crate::runner::WindowSnapshot>,
        truncated: bool,
    },
    Symbols {
        schema_version: u32,
        symbols: Vec<super::symbols::SymbolEntry>,
        truncated: bool,
    },
    Custom {
        id: ProviderId,
        schema_version: u32,
        state: serde_json::Value,
        truncated: bool,
    },
}

impl ProviderSnapshot {
    pub fn is_truncated(&self) -> bool {
        match self {
            Self::Graphics { truncated, .. }
            | Self::Events { truncated, .. }
            | Self::Windows { truncated, .. }
            | Self::Symbols { truncated, .. }
            | Self::Custom { truncated, .. } => *truncated,
        }
    }
}

static BUILTIN_PROVIDERS: OnceLock<Vec<ProviderRegistration>> = OnceLock::new();

pub fn builtin_providers() -> &'static [ProviderRegistration] {
    BUILTIN_PROVIDERS.get_or_init(|| {
        let mut providers = vec![
            provider(provider_ids::GRAPHICS, "graphics", 1,
                vec![ProviderSnapshotKind::State, ProviderSnapshotKind::Image, ProviderSnapshotKind::Buffer],
                &["one guest-visible framebuffer and its host mirror; no GPU command or texture list; no physical-display completion guarantee; frame previews support 1/2/4/8/16 bpp"],
                snapshot_graphics).with_artifacts(artifacts_graphics),
            provider(provider_ids::WINDOWS, "windows", 1, vec![ProviderSnapshotKind::Collection],
                &["classic Window Manager records only; region bounding boxes, not full regions"],
                snapshot_windows),
            provider(provider_ids::EVENTS, "events", 1, vec![ProviderSnapshotKind::State],
                &["queue types limited to 256 entries; no queue consumption"],
                snapshot_events),
            provider(provider_ids::SYMBOLS, "symbols", 1, vec![ProviderSnapshotKind::Collection],
                &["MacsBug procedure names are optional compiler output; a binary built or stripped without them yields none",
                  "68K only; PowerPC code uses an unrelated convention",
                  "recovered by pattern match, so names may be missed or misidentified",
                  "a symbol trails the routine it names; it is not that routine's entry point",
                  "the same routine is often resident twice (a loaded CODE segment and the cached resource copy), so names repeat and only one copy executes",
                  "scans a bounded low range of guest RAM, not a discovered code segment list"],
                snapshot_symbols),
        ];
        for provider in &mut providers {
            provider.descriptor.collection_limit = DEFAULT_COLLECTION_LIMIT;
            provider.capabilities.collection_limit = DEFAULT_COLLECTION_LIMIT;
        }
        if let Some(graphics) = providers.iter_mut().find(|provider| {
            provider.descriptor.id == super::ids::ProviderId(provider_ids::GRAPHICS)
        }) {
            graphics.capabilities.max_artifact_bytes = MAX_FRAME_ARTIFACT_BYTES;
        }
        if let Some(symbols) = providers.iter_mut().find(|provider| {
            provider.descriptor.id == super::ids::ProviderId(provider_ids::SYMBOLS)
        }) {
            symbols.descriptor.collection_limit = SYMBOL_COLLECTION_LIMIT;
            symbols.capabilities.collection_limit = SYMBOL_COLLECTION_LIMIT;
        }
        for provider in &providers {
            validate_provider_contract(provider)
                .expect("builtin provider metadata must be internally consistent");
        }
        providers
            .into_iter()
            .map(|provider| Arc::new(provider) as ProviderRegistration)
            .collect()
    })
}

fn snapshot_events(runner: &FixtureRunner) -> DebugResult<ProviderSnapshot> {
    let state = runner.event_manager_snapshot_with_limit(DEFAULT_COLLECTION_LIMIT);
    let truncated = state.queue_len > state.queued_event_types.len();
    Ok(ProviderSnapshot::Events {
        schema_version: 1,
        state,
        truncated,
    })
}

fn snapshot_graphics(runner: &FixtureRunner) -> DebugResult<ProviderSnapshot> {
    let state = graphics_snapshot(runner);
    let truncated = state.truncated;
    Ok(ProviderSnapshot::Graphics {
        schema_version: 1,
        state,
        truncated,
    })
}

/// Range scanned for symbols. Application code is loaded low in the 68K space;
/// without a discovered segment list this bounded window is the honest default,
/// and the provider's limitations say so.
pub const SYMBOL_SCAN_START: u64 = 0;
pub const SYMBOL_SCAN_LENGTH: u64 = 16 * 1024 * 1024;

fn snapshot_symbols(runner: &FixtureRunner) -> DebugResult<ProviderSnapshot> {
    let (symbols, truncated) = super::symbols::scan(
        runner,
        SYMBOL_SCAN_START,
        SYMBOL_SCAN_LENGTH,
        SYMBOL_COLLECTION_LIMIT,
        DEFAULT_SNAPSHOT_BYTES,
    )?;
    Ok(ProviderSnapshot::Symbols {
        schema_version: 1,
        symbols,
        truncated,
    })
}

fn snapshot_windows(runner: &FixtureRunner) -> DebugResult<ProviderSnapshot> {
    let (windows, truncated) = runner.debug_window_snapshot(DEFAULT_COLLECTION_LIMIT);
    Ok(ProviderSnapshot::Windows {
        schema_version: 1,
        windows,
        truncated,
    })
}

fn artifacts_graphics(runner: &FixtureRunner) -> DebugResult<Vec<ProviderArtifact>> {
    match render_frame_rgba8(runner) {
        Ok(bytes) => {
            let (base, _, width, height, _) = runner.debug_screen_mode();
            Ok(vec![ProviderArtifact {
                format: Some("rgba8".to_string()),
                bytes,
                provenance: format!("guest-framebuffer:{base:#010x}:{width}x{height}"),
                related: Vec::new(),
            }])
        }
        Err(omission) => Err(DebugError::InvalidValue {
            detail: omission.note(),
        }),
    }
}

pub fn pixel_format_for_depth(depth: u16) -> PixelFormat {
    match depth {
        1 => PixelFormat::Mono1,
        2 => PixelFormat::Indexed2,
        4 => PixelFormat::Indexed4,
        8 => PixelFormat::Indexed8,
        16 => PixelFormat::Rgb555,
        24 => PixelFormat::Rgb888,
        32 => PixelFormat::Xrgb8888,
        _ => PixelFormat::Unknown,
    }
}

fn graphics_snapshot(runner: &FixtureRunner) -> GraphicsSnapshot {
    let (base, row_bytes, width, height, depth) = runner.debug_screen_mode();
    let indexed = matches!(depth, 1 | 2 | 4 | 8);
    let palette_argb = indexed.then(|| runner.debug_palette_argb().to_vec());
    let architecture = runner.debug_active_architecture();
    let (source, kind, base) = match architecture {
        super::adapters::ActiveArchitecture::PowerPc
        | super::adapters::ActiveArchitecture::PowerPcCompanion => (
            "native-host-mirror",
            SurfaceKind::HostPresentationMirror,
            None,
        ),
        super::adapters::ActiveArchitecture::M68k => (
            "classic-video",
            SurfaceKind::GuestFramebuffer,
            Some(super::model::DebugAddress::new(
                super::adapters::M68K_SPACE,
                u64::from(base),
            )),
        ),
        super::adapters::ActiveArchitecture::Blocked => ("unresolved", SurfaceKind::Other, None),
    };
    let surface = GraphicsSurfaceDescriptor {
        id: 1,
        name: "main-framebuffer".to_string(),
        kind,
        width: u32::from(width),
        height: u32::from(height),
        stride_bytes: row_bytes,
        pixel_format: pixel_format_for_depth(depth),
        byte_order: ByteOrder::Big,
        base,
        scale: 1,
        has_palette: indexed,
    };
    let output = DisplayOutputDescriptor {
        id: 1,
        name: "main-display".to_string(),
        width: u32::from(width),
        height: u32::from(height),
        depth,
        surfaces: vec![surface.id],
        palette_entries: if indexed { 256 } else { 0 },
        source: source.to_string(),
    };
    GraphicsSnapshot {
        schema_version: 1,
        outputs: vec![output],
        surfaces: vec![surface],
        palette_argb,
        truncated: false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameArtifactOmission {
    Empty,
    UnsupportedDepth(u16),
    TooLarge { bytes: u64, limit: u64 },
}

impl FrameArtifactOmission {
    pub fn note(self) -> String {
        match self {
            FrameArtifactOmission::Empty => "display geometry is empty".to_string(),
            FrameArtifactOmission::UnsupportedDepth(depth) => {
                format!("no frame preview for {depth}-bit direct color")
            }
            FrameArtifactOmission::TooLarge { bytes, limit } => {
                format!("frame artifact would be {bytes} bytes, above the {limit}-byte budget")
            }
        }
    }
}

fn render_frame_rgba8(runner: &FixtureRunner) -> Result<Vec<u8>, FrameArtifactOmission> {
    let (_, _, width, height, depth) = runner.debug_screen_mode();
    if width == 0 || height == 0 {
        return Err(FrameArtifactOmission::Empty);
    }
    // The software renderer implements 1/2/4/8/16 bpp. Deeper direct-color
    // modes have no faithful preview, so no artifact is offered for them.
    if !matches!(depth, 1 | 2 | 4 | 8 | 16) {
        return Err(FrameArtifactOmission::UnsupportedDepth(depth));
    }
    let byte_len = u64::from(width) * u64::from(height) * 4;
    if byte_len > MAX_FRAME_ARTIFACT_BYTES {
        return Err(FrameArtifactOmission::TooLarge {
            bytes: byte_len,
            limit: MAX_FRAME_ARTIFACT_BYTES,
        });
    }
    Ok(crate::display::render_screen_with_gamma(
        runner.bus(),
        runner.debug_screen_mode(),
        runner.debug_device_clut(),
        runner.debug_device_gamma(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_providers_have_unique_ids_and_are_observational() {
        let providers = builtin_providers();
        let mut ids: Vec<u64> = providers
            .iter()
            .map(|provider| provider.descriptor().id.0)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), providers.len());
        for provider in providers {
            assert!(provider.descriptor().observational);
        }
    }
}
