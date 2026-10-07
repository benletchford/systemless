//! PowerPC loaded application execution model and process bindings.

use super::*;

#[derive(Debug, Clone)]
pub struct PpcLoadedApp {
    pub cpu: PpcCpu,
    /// Mapped guest bytes shared by native and emulated 68k execution.
    pub memory: crate::memory::GuestAddressSpace,
    pub entry_pc: u32,
    pub rtoc: u32,
    pub stack_base: u32,
    pub stack_size: u32,
    pub stack_pointer: u32,
    pub(crate) launch_partition_storage: PpcLaunchPartitionStorage,
    /// Process-scoped host pacing snapshot for the wrapping Macintosh clock.
    /// Guest-visible time is always read from low-memory `Ticks`; this handle
    /// only lets callback scheduling share the last observed value while a
    /// native slice is active.
    pub(crate) tick_state: SharedProcessTickState,
    pub clock_cycles_per_tick: u32,
    pub clock_cycle_phase: u32,
    /// Canonical system-owned trap gateways captured when this native adapter
    /// joins a materialized process. A live table entry equal to one of these
    /// identities still selects the HLE default; every other callable entry is
    /// an application patch and must run through Mixed Mode.
    pub(crate) trap_default_gateways: HashMap<u16, u32>,
    pub native_exception_handler: u32,
    pub(crate) native_exception_stack: Vec<PpcNativeExceptionContext>,
    pub(crate) stdc_qsort_stack: Vec<PpcQsortState>,
    pub(crate) dialog_callback_stack: Vec<PpcDialogCallbackState>,
    pub(crate) collection_callback_stack: Vec<PpcCollectionCallbackState>,
    pub(crate) pending_file_completions: VecDeque<(u32, u32)>,
    /// A callback that exhausted its current runner slice, in queue-front
    /// order, with the stack pointer of its private callback frame.
    pub(crate) file_completion_context: Option<(PpcExecutionContext, u32)>,
    /// A callback the runner entered asynchronously (an interrupt-level task
    /// or completion, or a Carbon event-loop timer) that is waiting on a 68K
    /// Mixed Mode call. It keeps the native CPU until it returns; what it
    /// interrupted waits here.
    pub(crate) parked_interrupt_callback: Option<PpcParkedInterruptCallback>,
    /// Number of such callbacks parked since launch.
    pub(crate) interrupt_callback_parks: u64,
    pub(crate) apple_events: PpcAppleEventState,
    /// Standalone CFM seed; None after a runner moves it into its process.
    /// Installed execution must receive the process service explicitly.
    pub cfm: Option<PpcCfmState>,
    pub(crate) controls: SharedProcessControlManager,
    pub aliases: Vec<PpcAliasRecord>,
    pub gworlds: Vec<PpcGWorldRecord>,
    pub(crate) agl: PpcAglState,
    /// Process-owned state bits keyed by PixMapHandle. GWorld geometry,
    /// allocation, and rendering records remain in `gworlds`.
    pub(crate) gworld_pixel_states: SharedProcessQuickDrawPixelStates,
    pub q3_objects: Vec<PpcQ3ObjectRecord>,
    pub q3_object_refs: Vec<PpcQ3ObjectReferenceRecord>,
    pub next_q3_object: u32,
    pub q3_error_state: PpcQ3ErrorState,
    pub q3_lifecycle: PpcQ3LifecycleState,
    pub q3_memory_storages: Vec<PpcQ3MemoryStorageRecord>,
    pub q3_files: Vec<PpcQ3FileRecord>,
    pub q3_group_memberships: Vec<PpcQ3GroupMembershipRecord>,
    pub q3_file_groups: Vec<PpcQ3FileGroupRecord>,
    pub q3_views: Vec<PpcQ3ViewStateRecord>,
    pub q3_submissions: Vec<PpcQ3SubmissionRecord>,
    pub q3_view_transforms: Vec<PpcQ3ViewTransformRecord>,
    pub q3_submission_transforms: Vec<PpcQ3SubmissionTransformRecord>,
    pub q3_view_materials: Vec<PpcQ3ViewMaterialRecord>,
    pub q3_submission_materials: Vec<PpcQ3SubmissionMaterialRecord>,
    pub q3_submission_lights: Vec<PpcQ3SubmissionLightRecord>,
    pub q3_view_state_stack: Vec<PpcQ3ViewStateSnapshotRecord>,
    pub q3_completed_frames: Vec<PpcQ3CompletedFrameRecord>,
    pub q3_retained_frames: Vec<PpcQ3RetainedFrameRecord>,
    pub q3_state_only_completed_frame_batches: Vec<PpcQ3StateOnlyCompletedFrameBatch>,
    pub q3_fog_styles: Vec<PpcQ3FogStyleRecord>,
    pub q3_attributes: Vec<PpcQ3AttributeRecord>,
    pub q3_shader_uv_transforms: Vec<PpcQ3ShaderUvTransformRecord>,
    pub q3_shader_boundaries: Vec<PpcQ3ShaderBoundaryRecord>,
    pub q3_mipmap_textures: Vec<PpcQ3MipmapTextureRecord>,
    pub q3_texture_shaders: Vec<PpcQ3TextureShaderRecord>,
    pub q3_renderer_preferences: Vec<PpcQ3RendererPreferenceRecord>,
    pub q3_draw_contexts: Vec<PpcQ3DrawContextRecord>,
    pub q3_trimeshes: Vec<PpcQ3TriMeshRecord>,
    pub q3_styles: Vec<PpcQ3StyleRecord>,
    pub q3_cameras: Vec<PpcQ3CameraRecord>,
    pub q3_lights: Vec<PpcQ3LightRecord>,
    pub input_sprocket: PpcInputSprocketState,
    pub input_sprocket_virtual_elements: Vec<PpcInputSprocketVirtualElementRecord>,
    pub toolbox_startup: PpcToolboxStartupState,
    pub quicktime: PpcQuickTimeState,
    pub sound: PpcSoundState,
    pub(crate) timer_tasks: SharedProcessTimerTasks,
    pub(crate) vbl_tasks: SharedProcessVblTasks,
    pub(crate) callback_scheduling: SharedProcessCallbackScheduling,
    pub(crate) process_file_system: SharedProcessFileSystem,
    pub(crate) current_gworld: SharedProcessGraphicsPort,
    pub(crate) current_gdevice: SharedProcessGraphicsDevice,
    pub(crate) quickdraw_op_colors: SharedProcessQuickDrawOpColors,
    pub(crate) quickdraw_hilite_colors: SharedProcessQuickDrawHiliteColors,
    pub screen_clut: SharedProcessDisplayClut,
    pub color_manager_clut: SharedProcessDisplayClut,
    pub(crate) display_gamma: SharedProcessDisplayGamma,
    /// Whether QuickDraw draw state is canonical in the attached process's
    /// current CGrafPort record and must be reloaded at each import boundary.
    pub(crate) process_quickdraw_port_state_attached: bool,
    pub quickdraw_fore_color: PpcRgbColor,
    pub(crate) quickdraw_fore_indices: HashMap<u32, u8>,
    pub quickdraw_back_color: PpcRgbColor,
    pub quickdraw_pen_h: i16,
    pub quickdraw_pen_v: i16,
    pub quickdraw_text_mode: i16,
    pub quickdraw_text_size: i16,
    pub(crate) cursor_state: SharedProcessCursorState,
    pub(crate) param_text: SharedProcessDialogText,
    pub scrap: PpcScrapState,
    pub(crate) list_manager: PpcListManagerState,
    pub(crate) collections: SharedProcessCollectionManager,
    pub halt_pc: u32,
    pub import_trap_base: u32,
    pub import_count: u32,
    pub imports: Vec<PpcImportBinding>,
    pub section_bases: Vec<Option<u32>>,
    pub input: PpcInputSnapshot,
    pub(crate) process_input: SharedProcessInputState,
    pub(crate) event_queue: SharedProcessEventQueue,
    pub(crate) window_list: crate::process_context::SharedProcessWindowList,
    pub(crate) process_memory_manager: PpcProcessMemoryManager,
    pub draw_sprocket: PpcDrawSprocketState,
    /// OpenGL memory configuration belongs to the loaded process.
    pub(crate) glm_mode: Option<u32>,
    pub(crate) glm_callbacks: [Option<PpcCallbackTarget>; 8],
    pub(crate) glm_callback_stack: Vec<PpcGlmCallbackState>,
    pub(crate) glm_allocations: HashMap<u32, (bool, u32)>,
    pub(crate) glm_page_free_all_queue: VecDeque<u32>,
    pub(crate) glm_error: u32,
}

/// Work a callback runner does right after an asynchronous callback returns,
/// deferred to the callback's real return when it parked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PpcInterruptReturnWork {
    None,
    /// The Vertical Retrace Manager removes a task whose callback left
    /// vblCount at zero.
    VblTask {
        task_ptr: u32,
    },
    /// The exhausted buffer stays marked pending until its refill returns.
    DoubleBack(PpcSoundDoubleBackRecord),
}

/// How a parked callback was entered, which decides what it masks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PpcCallbackLevel {
    /// VBL, DrawSprocket VBL, Time Manager, sound and File Manager callbacks
    /// run at interrupt time.
    Interrupt,
    /// Carbon event-loop timers run from the event loop at task level.
    Task,
}

/// A native routine that calls a 68K routine through Mixed Mode waits for the
/// emulated call to return. An asynchronously entered callback in that state
/// still owns the processor, so the interrupted context is set aside until
/// the callback itself returns.
#[derive(Debug, Clone)]
pub(crate) struct PpcParkedInterruptCallback {
    pub(crate) level: PpcCallbackLevel,
    /// Task that owns the callback's 68K call and its native frame.
    pub(crate) task: crate::guest_call::ExecutionTaskId,
    /// The 68K call the callback submitted when it parked. Every call the
    /// callback makes afterwards has a larger ID.
    pub(crate) awaited_call: crate::guest_call::CallId,
    /// Stack pointer the callback holds when it returns to `halt_pc`.
    pub(crate) callback_sp: u32,
    pub(crate) interrupted: PpcExecutionContext,
    /// Resource file to reselect, on the paths that restore it on return.
    pub(crate) interrupted_refnum: Option<i16>,
    pub(crate) on_return: PpcInterruptReturnWork,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PpcGlmCallbackState {
    pub(crate) import_pc: u32,
    pub(crate) final_pc: u32,
    pub(crate) restore_rtoc: u32,
    pub(crate) operation: PpcGlmCallbackOperation,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PpcGlmCallbackOperation {
    Allocate {
        size: u32,
        zero_on_return: bool,
        replace: Option<(u32, u32)>,
    },
    Free {
        pointer: u32,
        result: u32,
        replacement_size: Option<u32>,
        free_all: bool,
    },
}

/// Launch-time storage that `grow_application_partition` budgets around.
/// PowerPC System Software (1994), pp. 1-53--1-60.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PpcLaunchPartitionStorage {
    /// Heap bytes CFM keeps outside the partition: initial libraries' code
    /// sections and the container copies handed to fragment initializers.
    pub(crate) outside_partition: u32,
    /// The application fragment's data sections. They are mapped outside
    /// the native heap but belong to the application heap on a Power Mac.
    pub(crate) application_data: u32,
}

impl std::ops::Deref for PpcLoadedApp {
    type Target = ProcessFileSystemState;

    fn deref(&self) -> &Self::Target {
        &self.process_file_system
    }
}
