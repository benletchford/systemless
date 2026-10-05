//! PowerPC execution diagnostics, trace observers, and memory watch tooling.

use super::*;

const PPC_MATH_HOT_IMPORT_EXTRA_CYCLES: u64 = 128;
// TickCount is maintained by the vertical retrace interrupt, so it advances
// while the Toolbox draws on the application's behalf. Inside Macintosh:
// Processes (1993), p. 3-46. HLE QuickDraw and Resource Manager imports do
// that work on the host and otherwise charge no guest cycles, so a loop that
// redraws until TickCount changes sees drawing as free and repeats a full
// redraw many times within one tick at real host cost. Inside Macintosh
// gives no per-call timings; these fixed charges are a deliberate
// approximation of a 120 MHz 604 (DrawText ~6 us, CopyBits ~9 us,
// DrawPicture ~17 us). They are lower bounds -- large pictures and transfers
// take far longer on hardware -- so an application redrawing a few hundred
// items still has most of its tick left for its own code.
// A redraw of ~370 text calls, 165 pictures and 135 transfers costs under
// half of one tick's cycles.
const PPC_DRAW_TEXT_IMPORT_EXTRA_CYCLES: u64 = 768;
const PPC_MEASURE_TEXT_IMPORT_EXTRA_CYCLES: u64 = 256;
const PPC_DRAW_PICTURE_IMPORT_EXTRA_CYCLES: u64 = 2_048;
const PPC_BIT_TRANSFER_IMPORT_EXTRA_CYCLES: u64 = 1_024;
const PPC_DRAW_PRIMITIVE_IMPORT_EXTRA_CYCLES: u64 = 256;
const PPC_RESOURCE_IMPORT_EXTRA_CYCLES: u64 = 128;

pub(crate) struct PpcHleFetchObserver<'a> {
    pub(crate) histogram: Option<&'a mut PpcFetchHistogram>,
    pub(crate) trace_fetches: bool,
    pub(crate) trace_pc_range: Option<(u32, u32)>,
}

impl PpcFetchObserver for PpcHleFetchObserver<'_> {
    fn on_fetch(&mut self, pc: u32, word: u32) {
        if self.trace_fetches
            || self
                .trace_pc_range
                .map(|(start, end)| pc >= start && pc <= end)
                .unwrap_or(false)
        {
            eprintln!("{}", format_ppc_trace_fetch(pc, word));
        }
        if let Some(histogram) = self.histogram.as_deref_mut() {
            histogram.on_fetch(pc, word);
        }
    }

    fn on_fetch_cpu(&mut self, cpu: &PpcCpu, word: u32) {
        let pc = cpu.pc;
        let in_range = self
            .trace_pc_range
            .map(|(start, end)| pc >= start && pc <= end)
            .unwrap_or(false);
        let r27_matches = ppc_trace_regs_r27_filter()
            .map(|expected| cpu.gpr[27] == expected)
            .unwrap_or(true);
        let r3_matches = ppc_trace_regs_r3_filter()
            .map(|expected| cpu.gpr[3] == expected)
            .unwrap_or(true);
        if ppc_trace_regs_enabled() && in_range && r27_matches && r3_matches {
            eprintln!(
                "[PPC-TRACE] fetch pc=${:08X} word=${:08X} lr=${:08X} sp=${:08X} rtoc=${:08X} r3=${:08X} r4=${:08X} r5=${:08X} r12=${:08X} r27=${:08X} r28=${:08X} r29=${:08X} r30=${:08X} r31=${:08X}",
                pc,
                word,
                cpu.lr,
                cpu.gpr[1],
                cpu.gpr[2],
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5],
                cpu.gpr[12],
                cpu.gpr[27],
                cpu.gpr[28],
                cpu.gpr[29],
                cpu.gpr[30],
                cpu.gpr[31],
            );
            if let Some(histogram) = self.histogram.as_deref_mut() {
                histogram.on_fetch(pc, word);
            }
            return;
        }
        if ppc_trace_regs_enabled() && in_range {
            if let Some(histogram) = self.histogram.as_deref_mut() {
                histogram.on_fetch(pc, word);
            }
            return;
        }
        self.on_fetch(pc, word);
    }
}

static PPC_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_TRACE_PC_RANGE: OnceLock<Option<(u32, u32)>> = OnceLock::new();
static PPC_HLE_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_GWORLD_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static QD3D_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static QD3D_TRIMESH_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static QD3D_COLLISION_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static QD3D_DUMP_FRAME_ENABLED: OnceLock<bool> = OnceLock::new();
static QT_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_SOUND_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_TIMER_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_PT_IN_RECT_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_RECENT_IMPORTS_ON_HALT_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_TRACE_REGS_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_TRACE_REGS_R27_FILTER: OnceLock<Option<u32>> = OnceLock::new();
static PPC_TRACE_REGS_R3_FILTER: OnceLock<Option<u32>> = OnceLock::new();

pub(crate) fn ppc_trace_enabled() -> bool {
    *PPC_TRACE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_TRACE").is_some())
}

pub(crate) fn ppc_trace_pc_range() -> Option<(u32, u32)> {
    *PPC_TRACE_PC_RANGE.get_or_init(|| {
        let value = std::env::var("SYSTEMLESS_PPC_TRACE_PC_RANGE").ok()?;
        let mut parts = value.split(':');
        let start = parts.next()?.trim();
        let end = parts.next()?.trim();
        let parse = |s: &str| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok();
        Some((parse(start)?, parse(end)?))
    })
}

pub(crate) fn ppc_trace_regs_enabled() -> bool {
    *PPC_TRACE_REGS_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_TRACE_REGS").is_some())
}

pub(crate) fn ppc_trace_regs_r27_filter() -> Option<u32> {
    *PPC_TRACE_REGS_R27_FILTER.get_or_init(|| {
        let value = std::env::var("SYSTEMLESS_PPC_TRACE_REGS_R27").ok()?;
        let trimmed = value.trim();
        let hex = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .or_else(|| trimmed.strip_prefix('$'))
            .unwrap_or(trimmed);
        u32::from_str_radix(hex, 16).ok()
    })
}

pub(crate) fn ppc_trace_regs_r3_filter() -> Option<u32> {
    *PPC_TRACE_REGS_R3_FILTER.get_or_init(|| {
        let value = std::env::var("SYSTEMLESS_PPC_TRACE_REGS_R3").ok()?;
        let trimmed = value.trim();
        let hex = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .or_else(|| trimmed.strip_prefix('$'))
            .unwrap_or(trimmed);
        u32::from_str_radix(hex, 16).ok()
    })
}

pub(super) fn ppc_hle_trace_enabled() -> bool {
    *PPC_HLE_TRACE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_HLE_TRACE").is_some())
}

pub(super) fn ppc_gworld_trace_enabled() -> bool {
    *PPC_GWORLD_TRACE_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_GWORLD_TRACE").is_some())
}

pub(crate) fn ppc_recent_imports_on_halt_enabled() -> bool {
    *PPC_RECENT_IMPORTS_ON_HALT_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_RECENT_IMPORTS_ON_HALT").is_some())
}

pub(super) fn ppc_res_type_text(res_type: u32) -> String {
    let bytes = res_type.to_be_bytes();
    if bytes
        .iter()
        .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
    {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        format!("${res_type:08X}")
    }
}

pub(crate) fn qd3d_trace_enabled() -> bool {
    *QD3D_TRACE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_QD3D_TRACE").is_some())
}

pub(crate) fn qd3d_trimesh_trace_enabled() -> bool {
    *QD3D_TRIMESH_TRACE_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_QD3D_TRIMESH_TRACE").is_some())
}

pub(crate) fn qd3d_collision_trace_enabled() -> bool {
    *QD3D_COLLISION_TRACE_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_QD3D_COLLISION_TRACE").is_some())
}

pub(crate) fn qd3d_dump_frame_enabled() -> bool {
    *QD3D_DUMP_FRAME_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_QD3D_DUMP_FRAME").is_some())
}

pub(super) fn qt_trace_enabled() -> bool {
    *QT_TRACE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_QT_TRACE").is_some())
}

pub(super) fn ppc_sound_trace_enabled() -> bool {
    *PPC_SOUND_TRACE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_SOUND").is_some())
}

pub(crate) fn ppc_timer_trace_enabled() -> bool {
    *PPC_TIMER_TRACE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_TIMER").is_some())
}

pub(crate) fn ppc_pt_in_rect_trace_enabled() -> bool {
    *PPC_PT_IN_RECT_TRACE_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_TRACE_PT_IN_RECT").is_some())
}

pub(super) fn format_ppc_fourcc(value: u32) -> String {
    value
        .to_be_bytes()
        .into_iter()
        .map(|byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                char::from(byte)
            } else {
                '.'
            }
        })
        .collect()
}

pub(crate) fn format_ppc_trace_fetch(pc: u32, word: u32) -> String {
    format!("[PPC-TRACE] fetch pc=${:08X} word=${:08X}", pc, word)
}

pub(crate) fn format_ppc_trace_import(entry: &PpcHleImportTraceEntry) -> String {
    format!(
        "[PPC-TRACE] import #{} {}:{} pc=${:08X} lr=${:08X} rtoc=${:08X} sp=${:08X} target={:?}",
        entry.import_index,
        entry.library_name,
        entry.symbol_name,
        entry.pc,
        entry.lr,
        entry.rtoc,
        entry.sp,
        entry.dispatcher_target
    )
}

pub(crate) fn format_ppc_trace_unknown_import(
    index: u32,
    pc: u32,
    lr: u32,
    rtoc: u32,
    sp: u32,
) -> String {
    format!(
        "[PPC-TRACE] import #{} <unknown> pc=${:08X} lr=${:08X} rtoc=${:08X} sp=${:08X}",
        index, pc, lr, rtoc, sp
    )
}

pub(crate) fn is_sprocket_import(binding: &PpcImportBinding) -> bool {
    matches!(
        binding.library_name.as_str(),
        "DrawSprocketLib" | "InputSprocketLib"
    )
}

pub(crate) fn format_hex_opt(value: Option<u32>) -> String {
    value
        .map(|value| format!("${:08X}", value))
        .unwrap_or_else(|| "none".to_string())
}

pub(crate) fn format_hle_import_action(action: &PpcImportAction) -> String {
    format_ppc_import_action(action)
}

pub(crate) fn ppc_import_action_with_extra_cycles(
    action: PpcImportAction,
    extra_cycles: u64,
) -> PpcImportAction {
    if extra_cycles == 0 {
        return action;
    }
    match action {
        PpcImportAction::Return(value) => {
            PpcImportAction::ReturnWithExtraCycles(value, extra_cycles)
        }
        PpcImportAction::ReturnPreserve => {
            PpcImportAction::ReturnPreserveWithExtraCycles(extra_cycles)
        }
        PpcImportAction::ReturnPreserveWithExtraCycles(existing) => {
            PpcImportAction::ReturnPreserveWithExtraCycles(existing.saturating_add(extra_cycles))
        }
        PpcImportAction::ReturnWithExtraCycles(value, existing) => {
            PpcImportAction::ReturnWithExtraCycles(value, existing.saturating_add(extra_cycles))
        }
        _ => action,
    }
}

pub(crate) fn ppc_import_extra_cycles_for_target(target: &PpcImportDispatcherTarget) -> u64 {
    match target {
        PpcImportDispatcherTarget::Q3ViewEndRendering => 0,
        PpcImportDispatcherTarget::MathCeil
        | PpcImportDispatcherTarget::MathSqrt
        | PpcImportDispatcherTarget::MathExp
        | PpcImportDispatcherTarget::MathSin
        | PpcImportDispatcherTarget::MathCos
        | PpcImportDispatcherTarget::MathAsin
        | PpcImportDispatcherTarget::MathTan
        | PpcImportDispatcherTarget::MathAtan
        | PpcImportDispatcherTarget::MathAtan2
        | PpcImportDispatcherTarget::MathPow
        | PpcImportDispatcherTarget::MathFmod
        | PpcImportDispatcherTarget::MathLog
        | PpcImportDispatcherTarget::MathLog10 => PPC_MATH_HOT_IMPORT_EXTRA_CYCLES,
        // Rasterizing text: each call renders glyphs into the framebuffer.
        PpcImportDispatcherTarget::DrawChar
        | PpcImportDispatcherTarget::DrawText
        | PpcImportDispatcherTarget::DrawString => PPC_DRAW_TEXT_IMPORT_EXTRA_CYCLES,
        // Text measurement walks every glyph of the supplied bytes.
        PpcImportDispatcherTarget::MeasureText
        | PpcImportDispatcherTarget::TextWidth
        | PpcImportDispatcherTarget::TruncString
        | PpcImportDispatcherTarget::StringWidth
        | PpcImportDispatcherTarget::CharWidth
        | PpcImportDispatcherTarget::GetFontInfo
        | PpcImportDispatcherTarget::FontMetrics => PPC_MEASURE_TEXT_IMPORT_EXTRA_CYCLES,
        // PICT opcode interpretation plus rasterization.
        PpcImportDispatcherTarget::DrawPicture => PPC_DRAW_PICTURE_IMPORT_EXTRA_CYCLES,
        // Rectangular bit transfers between ports and GWorlds.
        PpcImportDispatcherTarget::CopyBits | PpcImportDispatcherTarget::StdBits => {
            PPC_BIT_TRANSFER_IMPORT_EXTRA_CYCLES
        }
        // Rect, region, oval and rounded-rect painting operations.
        PpcImportDispatcherTarget::PaintArc
        | PpcImportDispatcherTarget::PaintRect
        | PpcImportDispatcherTarget::PaintRoundRect
        | PpcImportDispatcherTarget::FillCRect
        | PpcImportDispatcherTarget::FillRgn
        | PpcImportDispatcherTarget::EraseOval
        | PpcImportDispatcherTarget::EraseRect
        | PpcImportDispatcherTarget::InvertRect
        | PpcImportDispatcherTarget::InvertRgn
        | PpcImportDispatcherTarget::FrameRect
        | PpcImportDispatcherTarget::FrameRgn => PPC_DRAW_PRIMITIVE_IMPORT_EXTRA_CYCLES,
        // Resource Manager fetches parse and copy resource data.
        PpcImportDispatcherTarget::GetIndString
        | PpcImportDispatcherTarget::GetString
        | PpcImportDispatcherTarget::GetResource
        | PpcImportDispatcherTarget::Get1Resource
        | PpcImportDispatcherTarget::Get1NamedResource
        | PpcImportDispatcherTarget::Get1IndResource
        | PpcImportDispatcherTarget::LoadResource
        | PpcImportDispatcherTarget::ReadPartialResource => PPC_RESOURCE_IMPORT_EXTRA_CYCLES,
        _ => 0,
    }
}

pub(crate) fn ppc_import_extra_cycles_for_binding(binding: &PpcImportBinding) -> u64 {
    if is_quickdraw_3d_library(&binding.library_name)
        || is_quickdraw_3d_accelerator_library(&binding.library_name)
    {
        return PPC_Q3_HOT_IMPORT_EXTRA_CYCLES;
    }
    ppc_import_extra_cycles_for_target(&binding.dispatcher_target)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Qd3dTraceSnapshot {
    pub(crate) objects: usize,
    pub(crate) views: usize,
    pub(crate) submissions: usize,
    pub(crate) completed_frames: usize,
    pub(crate) memory_storages: usize,
    pub(crate) files: usize,
    pub(crate) group_memberships: usize,
    pub(crate) trimeshes: usize,
    pub(crate) draw_contexts: usize,
    pub(crate) textures: usize,
    pub(crate) shaders: usize,
    pub(crate) styles: usize,
    pub(crate) cameras: usize,
    pub(crate) lights: usize,
    pub(crate) latest_object: Option<PpcQ3ObjectRecord>,
    pub(crate) latest_submission: Option<PpcQ3SubmissionRecord>,
    pub(crate) latest_frame_view: Option<u32>,
    pub(crate) latest_frame_submissions: usize,
}

pub(crate) fn qd3d_trace_snapshot(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_views: &[PpcQ3ViewStateRecord],
    q3_submissions: &[PpcQ3SubmissionRecord],
    q3_completed_frames: &[PpcQ3CompletedFrameRecord],
    q3_state_only_completed_frame_batches: &[PpcQ3StateOnlyCompletedFrameBatch],
    q3_memory_storages: &[PpcQ3MemoryStorageRecord],
    q3_files: &[PpcQ3FileRecord],
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_trimeshes: &[PpcQ3TriMeshRecord],
    q3_draw_contexts: &[PpcQ3DrawContextRecord],
    q3_mipmap_textures: &[PpcQ3MipmapTextureRecord],
    q3_texture_shaders: &[PpcQ3TextureShaderRecord],
    q3_styles: &[PpcQ3StyleRecord],
    q3_cameras: &[PpcQ3CameraRecord],
    q3_lights: &[PpcQ3LightRecord],
) -> Qd3dTraceSnapshot {
    let (latest_frame_view, latest_frame_submissions) = q3_completed_frames
        .last()
        .map(|frame| (Some(frame.view), frame.submissions.len()))
        .unwrap_or((None, 0));
    Qd3dTraceSnapshot {
        objects: q3_objects.len(),
        views: q3_views.len(),
        submissions: q3_submissions.len(),
        completed_frames: q3_completed_frames.len().saturating_add(
            q3_state_only_completed_frame_total(q3_state_only_completed_frame_batches),
        ),
        memory_storages: q3_memory_storages.len(),
        files: q3_files.len(),
        group_memberships: q3_group_memberships.len(),
        trimeshes: q3_trimeshes.len(),
        draw_contexts: q3_draw_contexts.len(),
        textures: q3_mipmap_textures.len(),
        shaders: q3_texture_shaders.len(),
        styles: q3_styles.len(),
        cameras: q3_cameras.len(),
        lights: q3_lights.len(),
        latest_object: q3_objects.last().copied(),
        latest_submission: q3_submissions.last().copied(),
        latest_frame_view,
        latest_frame_submissions,
    }
}

pub(crate) fn qd3d_trace_count(label: &str, before: usize, after: usize) -> String {
    let delta = after as isize - before as isize;
    format!("{}={}({:+})", label, after, delta)
}

pub(crate) fn q3_object_kind_name(kind: PpcQ3ObjectKind) -> &'static str {
    match kind {
        PpcQ3ObjectKind::Generic => "generic",
        PpcQ3ObjectKind::MemoryStorage => "memory-storage",
    }
}

pub(crate) fn q3_submission_kind_name(kind: PpcQ3SubmissionKind) -> &'static str {
    match kind {
        PpcQ3SubmissionKind::Shader => "shader",
        PpcQ3SubmissionKind::Style => "style",
        PpcQ3SubmissionKind::FogStyle => "fog-style",
        PpcQ3SubmissionKind::TriMesh => "trimesh",
        PpcQ3SubmissionKind::MatrixTransform => "matrix-transform",
        PpcQ3SubmissionKind::ResetTransform => "reset-transform",
        PpcQ3SubmissionKind::Push => "push",
        PpcQ3SubmissionKind::Pop => "pop",
        PpcQ3SubmissionKind::Object => "object",
    }
}

pub(crate) fn format_qd3d_latest_object(snapshot: &Qd3dTraceSnapshot) -> String {
    snapshot
        .latest_object
        .map(|object| {
            format!(
                "object={} kind={} type=${:08X} data=${:08X}/{}",
                format_hex_opt(Some(object.object)),
                q3_object_kind_name(object.kind),
                object.object_type,
                object.data_ptr,
                object.data_size
            )
        })
        .unwrap_or_else(|| "object=none".to_string())
}

pub(crate) fn format_qd3d_latest_submission(snapshot: &Qd3dTraceSnapshot) -> String {
    snapshot
        .latest_submission
        .map(|submission| {
            format!(
                "submission={} view={} primary={} secondary={}",
                q3_submission_kind_name(submission.kind),
                format_hex_opt(Some(submission.view)),
                format_hex_opt(Some(submission.primary)),
                format_hex_opt(Some(submission.secondary))
            )
        })
        .unwrap_or_else(|| "submission=none".to_string())
}

pub(crate) fn format_qd3d_latest_frame(snapshot: &Qd3dTraceSnapshot) -> String {
    snapshot
        .latest_frame_view
        .map(|view| {
            format!(
                "frame_view={} frame_submissions={}",
                format_hex_opt(Some(view)),
                snapshot.latest_frame_submissions
            )
        })
        .unwrap_or_else(|| "frame=none".to_string())
}

pub(crate) fn format_qd3d_trace(
    entry: &PpcHleImportTraceEntry,
    args: [u32; 6],
    action: &str,
    before: &Qd3dTraceSnapshot,
    after: &Qd3dTraceSnapshot,
) -> String {
    format!(
        "[QD3D-TRACE] {}:{} pc=${:08X} lr=${:08X} rtoc=${:08X} sp=${:08X} r3=${:08X} r4=${:08X} r5=${:08X} r6=${:08X} r7=${:08X} r8=${:08X} action={} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
        entry.library_name,
        entry.symbol_name,
        entry.pc,
        entry.lr,
        entry.rtoc,
        entry.sp,
        args[0],
        args[1],
        args[2],
        args[3],
        args[4],
        args[5],
        action,
        qd3d_trace_count("objects", before.objects, after.objects),
        qd3d_trace_count("views", before.views, after.views),
        qd3d_trace_count("submissions", before.submissions, after.submissions),
        qd3d_trace_count("frames", before.completed_frames, after.completed_frames),
        qd3d_trace_count("storages", before.memory_storages, after.memory_storages),
        qd3d_trace_count("files", before.files, after.files),
        qd3d_trace_count("groups", before.group_memberships, after.group_memberships),
        qd3d_trace_count("trimeshes", before.trimeshes, after.trimeshes),
        qd3d_trace_count("draw_contexts", before.draw_contexts, after.draw_contexts),
        qd3d_trace_count("textures", before.textures, after.textures),
        qd3d_trace_count("shaders", before.shaders, after.shaders),
        qd3d_trace_count("styles", before.styles, after.styles),
        qd3d_trace_count("cameras", before.cameras, after.cameras),
        qd3d_trace_count("lights", before.lights, after.lights),
        format_qd3d_latest_object(after),
        format_qd3d_latest_submission(after),
        format_qd3d_latest_frame(after)
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcWatchRange {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

impl PpcWatchRange {
    pub(crate) fn contains(self, addr: u32) -> bool {
        let start = u64::from(self.start);
        let end = start + u64::from(self.len);
        let addr = u64::from(addr);
        addr >= start && addr < end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcWatchWriteRecord {
    pub(crate) pc: u32,
    pub(crate) lr: u32,
    pub(crate) rtoc: u32,
    pub(crate) sp: u32,
    pub(crate) addr: u32,
    pub(crate) value: u8,
}

pub(crate) struct PpcWatchObserver {
    pub(crate) range: PpcWatchRange,
}

impl PpcMemoryWriteObserver for PpcWatchObserver {
    fn on_write(&mut self, pc: u32, lr: u32, rtoc: u32, sp: u32, addr: u32, value: u8) {
        if self.range.contains(addr) {
            eprintln!(
                "{}",
                format_ppc_watch_write(PpcWatchWriteRecord {
                    pc,
                    lr,
                    rtoc,
                    sp,
                    addr,
                    value,
                })
            );
        }
    }
}

static PPC_WATCH_RANGE: OnceLock<Option<PpcWatchRange>> = OnceLock::new();

pub(crate) fn ppc_watch_range() -> Option<PpcWatchRange> {
    *PPC_WATCH_RANGE.get_or_init(|| {
        let value = std::env::var_os("SYSTEMLESS_PPC_WATCH")?;
        parse_ppc_watch_range_value(&value)
    })
}

pub(crate) fn parse_ppc_watch_range_value(value: &std::ffi::OsStr) -> Option<PpcWatchRange> {
    let value = value.to_str()?.trim();
    if value.is_empty() {
        return None;
    }
    let mut parts = value.split(':');
    let start = parse_ppc_watch_addr(parts.next()?)?;
    let len = match parts.next() {
        Some(raw_len) => parse_ppc_watch_len(raw_len)?,
        None => 1,
    };
    if parts.next().is_some() {
        return None;
    }
    if len == 0 || u64::from(start) + u64::from(len) > (u64::from(u32::MAX) + 1) {
        return None;
    }
    Some(PpcWatchRange { start, len })
}

pub(crate) fn parse_ppc_watch_addr(value: &str) -> Option<u32> {
    let value = value
        .trim()
        .trim_start_matches('$')
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if value.is_empty() {
        return None;
    }
    u32::from_str_radix(value, 16).ok()
}

pub(crate) fn parse_ppc_watch_len(value: &str) -> Option<u32> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let len = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map_or_else(
            || value.parse().ok(),
            |hex| u32::from_str_radix(hex, 16).ok(),
        )?;
    (len != 0).then_some(len)
}

pub(crate) fn format_ppc_watch_write(record: PpcWatchWriteRecord) -> String {
    format!(
        "[PPC-WATCH] pc=${:08X} lr=${:08X} rtoc=${:08X} sp=${:08X} addr=${:08X} value=${:02X}",
        record.pc, record.lr, record.rtoc, record.sp, record.addr, record.value
    )
}
