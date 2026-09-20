use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

use crate::catalogue::{GameArchitecture, LaunchModifier, RuntimePacing};
use crate::paths::asset_path;
use crate::save_store::{self, DownloadableSaveFile};
use systemless::debug_overlay::DebugOverlayFrameStats;
use systemless::loader::ppc::PpcQ3GpuFrame;
use systemless::runner::{
    FixtureRunner, MenuBarPolicy, VfsFileSnapshot, VfsFileStat, VfsFileSummary, DEFAULT_VBL_HZ,
};
use systemless::sound::OUTPUT_RATE;
use systemless::trap::dispatch::ScreenCopyBitsRect;
use systemless::ui_theme::UiThemeId;
use systemless::{display, game};
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AudioContext, AudioContextLatencyCategory, AudioContextOptions, AudioContextState,
    AudioProcessingEvent, AudioWorkletNode, MessagePort, ScriptProcessorNode,
};

const UNIX_TO_MAC_EPOCH_OFFSET: u64 = 2_082_844_800;

/// Safety cap on instructions per rAF callback. Real pacing is driven by
/// `effective_target`; this stops a runaway frame if the interpreter can't
/// keep up with wall-clock.
const MAX_STEPS_PER_PAINT: usize = 2_000_000;
/// CPU batches stay short enough that long startup/loading phases re-check the
/// browser frame budget frequently, but not so short that JS clock calls become
/// a significant part of the rAF slice.
const M68K_CPU_BATCH_INSTRUCTIONS: usize = 2_500;
/// Once a browser frame has reached its ordinary CPU budget, let 68k code
/// finish the current trap-free drawing burst before the canvas samples RAM.
/// This keeps direct framebuffer copies from being presented halfway through
/// while preserving a strict bound on extra main-thread work.
const M68K_PRESENTATION_GRACE_INSTRUCTIONS: usize = 100_000;
/// PPC slices amortize the comparatively expensive framebuffer, VFS, audio,
/// and event synchronization performed after every runner call. This matches
/// the desktop headless runner while keeping each browser slice bounded.
const PPC_CPU_BATCH_INSTRUCTIONS: usize = 100_000;
const SOUND_CALLBACK_SLICE_INSTRUCTIONS: usize = M68K_CPU_BATCH_INSTRUCTIONS;
const SOUND_CALLBACK_RESERVED_INSTRUCTIONS_PER_FRAME: usize = SOUND_CALLBACK_SLICE_INSTRUCTIONS;
const SCRIPT_PROCESSOR_CPU_MS_PER_PAINT: f64 = 5.0;
const AUDIO_WORKLET_CPU_MS_PER_PAINT: f64 = 7.0;
const PPC_AUDIO_WORKLET_CPU_MS_PER_PAINT: f64 = 12.0;
const PPC_AUDIO_WORKLET_CATCHUP_CPU_MS_PER_PAINT: f64 = 14.0;
const AUDIO_WORKLET_CATCHUP_CPU_MS_PER_PAINT: f64 = 10.0;
/// AudioWorklet keeps playback off the main thread. When its queue is already
/// healthy, spend more of the rAF window catching up guest ticks so CPU-heavy
/// games don't visibly run slow after ordinary browser scheduling jitter.
const AUDIO_WORKLET_HEALTHY_CATCHUP_CPU_MS_PER_PAINT: f64 = 13.0;
const AUDIO_CALLBACK_CHUNK_SAMPLES: usize = 128;
const AUDIO_FRONTLOAD_CHUNKS: usize = 2;
const MAX_AUDIO_MS_PER_PAINT: usize = 50;
const MAX_AUDIO_SAMPLES_PER_PAINT: usize = (OUTPUT_RATE as usize * MAX_AUDIO_MS_PER_PAINT) / 1000;
const MAX_AUDIO_QUEUE_MS: usize = 500;
const MAX_AUDIO_QUEUE_SAMPLES: usize = (OUTPUT_RATE as usize * MAX_AUDIO_QUEUE_MS) / 1000;
const HEALTHY_AUDIO_QUEUE_MS: usize = 150;
const HEALTHY_AUDIO_QUEUE_SAMPLES: usize = (OUTPUT_RATE as usize * HEALTHY_AUDIO_QUEUE_MS) / 1000;

/// Silence queued in the browser audio sink at startup, in ms. Gives the
/// queue a steady lead so main-thread jitter (slow rAFs, heavy render
/// frames) doesn't starve playback and cause skips.
const AUDIO_PREFILL_MS: usize = 400;
const AUDIO_BUFFER_SIZE: u32 = 1024;
const WEB_PACK_LOAD_YIELD_BYTES: usize = 256 * 1024;
const WEB_PACK_LOAD_FRAME_YIELD_BYTES: usize = 1024 * 1024;
const SAVE_SCAN_FRAME_INTERVAL: u8 = 30;
const LAUNCH_MODIFIER_HOLD_TICKS: u32 = 120;

thread_local! {
    static PENDING_AUDIO_BOOTSTRAP: RefCell<Option<AudioBootstrap>> = const { RefCell::new(None) };
}

pub fn begin_audio_from_user_gesture() {
    begin_audio_bootstrap(true);
}

pub fn prepare_audio_for_boot() {
    begin_audio_bootstrap(false);
}

fn begin_audio_bootstrap(resume_from_gesture: bool) {
    PENDING_AUDIO_BOOTSTRAP.with(|slot| {
        let mut slot = slot.borrow_mut();
        match slot.as_mut() {
            Some(bootstrap) if resume_from_gesture => bootstrap.resume_from_user_gesture(),
            Some(_) => {}
            None => {
                *slot = AudioBootstrap::new(resume_from_gesture);
            }
        }
    });
}

fn take_pending_audio_bootstrap() -> Option<AudioBootstrap> {
    PENDING_AUDIO_BOOTSTRAP.with(|slot| slot.borrow_mut().take())
}

pub struct Machine {
    game_id: String,
    runner: FixtureRunner,
    runtime_pacing: RuntimePacing,
    started_at_ms: f64,
    audio_started_at_ms: f64,
    last_presentation_at_ms: f64,
    audio_samples_target: u64,
    last_ticks_behind: u32,
    last_steps: usize,
    last_cpu_budget_ms: f64,
    last_audio_queue_ms: Option<f64>,
    frame_rgba: Vec<u8>,
    presented_rgba: Vec<u8>,
    output_scale: u32,
    presented_size: (u32, u32),
    frame_palette: display::RgbaPalette,
    frame_palette_clut: [[u16; 3]; 256],
    frame_palette_valid: bool,
    input_dirty: bool,
    audio_staging: Vec<u8>,
    audio: Option<WebAudioBackend>,
    worker_mode: bool,
    worker_audio_queue_samples: Option<usize>,
    archive_vfs_stats: HashMap<String, VfsFileStat>,
    last_vfs_fingerprints: HashMap<String, SaveFingerprint>,
    persisted_save_paths: HashSet<String>,
    save_files: Vec<DownloadableSaveFile>,
    save_files_version: u64,
    save_scan_frame: u8,
    pending_launch_modifiers: Vec<LaunchModifier>,
    launch_modifiers_release_tick: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SaveFingerprint {
    data_len: usize,
    resource_len: usize,
    data_hash: u64,
    resource_hash: u64,
    file_type: u32,
    creator: u32,
    finder_flags: u16,
    modified_date: u32,
}

impl From<&VfsFileSummary> for SaveFingerprint {
    fn from(summary: &VfsFileSummary) -> Self {
        Self {
            data_len: summary.data_len,
            resource_len: summary.resource_len,
            data_hash: summary.data_hash,
            resource_hash: summary.resource_hash,
            file_type: summary.file_type,
            creator: summary.creator,
            finder_flags: summary.finder_flags,
            modified_date: summary.modified_date,
        }
    }
}

impl From<&VfsFileSnapshot> for SaveFingerprint {
    fn from(snapshot: &VfsFileSnapshot) -> Self {
        Self {
            data_len: snapshot.data_fork.len(),
            resource_len: snapshot.resource_fork.len(),
            data_hash: save_fork_hash(&snapshot.data_fork),
            resource_hash: save_fork_hash(&snapshot.resource_fork),
            file_type: snapshot.file_type,
            creator: snapshot.creator,
            finder_flags: snapshot.finder_flags,
            modified_date: snapshot.modified_date,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootProgress {
    MountingArchive {
        loaded_bytes: usize,
        total_bytes: usize,
    },
    LoadingExecutable,
    StartingRuntime,
    PreparingAudio,
}

#[derive(Clone, Debug)]
pub struct PluginFile {
    pub mount_path: String,
    pub file: VfsFileSnapshot,
}

impl Machine {
    pub async fn new_with_progress<F>(
        game_id: &str,
        game_bytes: &[u8],
        plugin_files: &[PluginFile],
        architecture: GameArchitecture,
        launch_modifiers: &[LaunchModifier],
        show_menu_bar: bool,
        application_partition_size: Option<u32>,
        remove_paths: &[&str],
        runtime_pacing: RuntimePacing,
        mut on_progress: F,
    ) -> Result<Self, String>
    where
        F: FnMut(BootProgress),
    {
        let mut runner = new_web_runner();
        runner.set_prefer_powerpc_executables(architecture == GameArchitecture::PowerPc);
        runner.set_app_start_time(current_mac_epoch_seconds());
        runner.set_application_partition_size(application_partition_size);
        runner.set_menu_bar_policy(if show_menu_bar {
            MenuBarPolicy::GuestControlled
        } else {
            MenuBarPolicy::InitialKiosk
        });
        let ipt = web_instructions_per_tick(runtime_pacing);
        runner.set_instructions_per_tick(ipt);

        let app = if let Some(mut loader) =
            game::WebPackLoader::new_with_remove_paths(&mut runner, game_bytes, remove_paths)?
        {
            let mut loaded_at_last_frame_yield = loader.archive_bytes_loaded();
            on_progress(BootProgress::MountingArchive {
                loaded_bytes: loader.archive_bytes_loaded(),
                total_bytes: loader.archive_bytes_total(),
            });
            while !loader.load_next_chunk(&mut runner, WEB_PACK_LOAD_YIELD_BYTES)? {
                let loaded_bytes = loader.archive_bytes_loaded();
                on_progress(BootProgress::MountingArchive {
                    loaded_bytes,
                    total_bytes: loader.archive_bytes_total(),
                });
                if loaded_bytes.saturating_sub(loaded_at_last_frame_yield)
                    >= WEB_PACK_LOAD_FRAME_YIELD_BYTES
                {
                    loaded_at_last_frame_yield = loaded_bytes;
                    yield_to_browser_frame().await;
                } else {
                    yield_to_browser_task().await;
                }
            }
            on_progress(BootProgress::MountingArchive {
                loaded_bytes: loader.archive_bytes_loaded(),
                total_bytes: loader.archive_bytes_total(),
            });
            on_progress(BootProgress::LoadingExecutable);
            yield_to_browser_task().await;
            loader.finish(&mut runner)?
        } else {
            on_progress(BootProgress::LoadingExecutable);
            yield_to_browser_task().await;
            game::load_game(&mut runner, game_bytes)?
        };

        on_progress(BootProgress::StartingRuntime);
        yield_to_browser_task().await;
        for plugin in plugin_files {
            runner.import_vfs_file_relative_to_launched_app(&plugin.mount_path, &plugin.file)?;
        }
        let archive_vfs_stats = vfs_stats(&mut runner);
        let restored_saves = save_store::load_saved_files(game_id)
            .await
            .unwrap_or_default();
        let persisted_save_paths = restored_saves
            .iter()
            .map(|file| file.path.clone())
            .collect::<HashSet<_>>();
        for file in &restored_saves {
            runner.import_vfs_file(file);
        }
        // Modifier-only launch conditions are polled through GetKeys/KeyMap.
        // Inject them before init_app republishes the current key state into
        // low memory. Inside Macintosh Volume I (1985), pp. I-260..I-261.
        for modifier in launch_modifiers {
            runner.push_key_down(modifier.mac_key_code(), 0);
        }
        game::init_game(&mut runner, &app);
        let launch_modifiers_release_tick = (!launch_modifiers.is_empty()).then(|| {
            runner
                .guest_tick()
                .saturating_add(LAUNCH_MODIFIER_HOLD_TICKS)
        });
        on_progress(BootProgress::PreparingAudio);
        let audio = if let Some(bootstrap) = take_pending_audio_bootstrap() {
            match bootstrap.finish().await {
                Some(audio) => Some(audio),
                None => WebAudioBackend::new().await,
            }
        } else {
            WebAudioBackend::new().await
        };
        let audio_started_at_ms = performance_now();
        let started_at_ms =
            wall_clock_origin_for_guest_tick(audio_started_at_ms, runner.guest_tick());
        runner.prepare_text_presentation();
        let mut machine = Machine {
            game_id: game_id.to_string(),
            runner,
            runtime_pacing,
            started_at_ms,
            audio_started_at_ms,
            last_presentation_at_ms: audio_started_at_ms,
            audio_samples_target: 0,
            last_ticks_behind: 0,
            last_steps: 0,
            last_cpu_budget_ms: 0.0,
            last_audio_queue_ms: None,
            frame_rgba: Vec::new(),
            presented_rgba: Vec::new(),
            output_scale: 1,
            presented_size: (0, 0),
            frame_palette: [0; 256],
            frame_palette_clut: [[0; 3]; 256],
            frame_palette_valid: false,
            input_dirty: false,
            audio_staging: Vec::new(),
            audio,
            worker_mode: web_sys::window().is_none(),
            worker_audio_queue_samples: None,
            archive_vfs_stats,
            last_vfs_fingerprints: HashMap::new(),
            persisted_save_paths,
            save_files: Vec::new(),
            save_files_version: 0,
            save_scan_frame: 0,
            pending_launch_modifiers: launch_modifiers.to_vec(),
            launch_modifiers_release_tick,
        };
        machine.last_vfs_fingerprints = restored_saves
            .iter()
            .map(|file| (file.path.clone(), SaveFingerprint::from(file)))
            .collect();
        machine.sync_save_files_now();
        Ok(machine)
    }

    pub fn set_arrows_as_numpad(&mut self, enabled: bool) {
        self.runner.set_arrows_as_numpad(enabled);
    }

    pub fn arrows_as_numpad(&self) -> bool {
        self.runner.arrows_as_numpad()
    }

    pub fn screen_size(&self) -> (u32, u32) {
        let (_, _, w, h, _) = self.runner.dispatcher().screen_mode;
        (w as u32, h as u32)
    }

    pub fn set_output_scale(&mut self, scale: u32) {
        let scale = scale.clamp(1, 4);
        if self.output_scale != scale {
            self.output_scale = scale;
            self.input_dirty = true;
        }
    }

    pub fn presented_size(&self) -> (u32, u32) {
        self.presented_size
    }

    /// Advance the emulator toward the wall-clock-paced target tick, mix
    /// audio, and ship any produced samples to the browser audio sink. The
    /// sink outputs silence by itself when the queue underruns, so idle frames
    /// do not need to post zero-filled chunks from the animation thread.
    pub fn run_frame(&mut self) -> FrameRunResult {
        self.runner.prepare_text_presentation();
        let now = performance_now();
        // Menu feedback runs on host time even while tracking freezes guest ticks.
        // Toolbox Essentials (1992), SetMenuFlash, p. 3-142.
        self.runner
            .advance_menu_presentation_clock(std::time::Duration::from_secs_f64(
                (now - self.last_presentation_at_ms).max(0.0) / 1000.0,
            ));
        self.last_presentation_at_ms = now;
        let frame_start_ms = now;
        let elapsed_ms = now - self.started_at_ms;
        let wall_tick = (elapsed_ms * DEFAULT_VBL_HZ / 1000.0) as u32;
        let current_tick = self.runner.guest_tick();
        let ticks_behind = wall_tick.saturating_sub(current_tick);
        self.last_ticks_behind = ticks_behind;
        let (worklet_audio, queued_source_samples) = if self.worker_mode {
            (true, self.worker_audio_queue_samples)
        } else {
            self.audio
                .as_mut()
                .map(|audio| (audio.uses_worklet(), audio.queued_source_samples()))
                .unwrap_or((true, None))
        };
        let cpu_budget_ms = web_cpu_budget_ms(
            worklet_audio,
            self.runner.is_powerpc_app(),
            ticks_behind,
            queued_source_samples,
        );
        self.last_cpu_budget_ms = cpu_budget_ms;
        self.last_audio_queue_ms =
            queued_source_samples.map(|samples| samples as f64 * 1000.0 / OUTPUT_RATE as f64);

        if ticks_behind > self.runtime_pacing.reset_slack_ticks {
            self.started_at_ms = now
                - (current_tick.saturating_add(self.runtime_pacing.max_ticks_per_paint) as f64)
                    * 1000.0
                    / DEFAULT_VBL_HZ;
        }

        let effective_target =
            web_effective_target_tick(current_tick, ticks_behind, self.runtime_pacing);

        let (audio_samples, next_audio_target) = audio_samples_for_elapsed(
            self.audio_started_at_ms,
            self.audio_samples_target,
            now,
            queued_source_samples,
        );
        self.audio_samples_target = next_audio_target;
        let queue_idle_silence = browser_idle_silence_needed(queued_source_samples);

        let mut total_steps = 0usize;
        let mut visual_steps = 0usize;
        let mut audio_mixed =
            self.frontload_browser_audio(audio_samples, queued_source_samples, queue_idle_silence);
        let mut reserved_sound_steps = 0usize;
        let mut running = !self.runner.is_halted();

        let cpu_batch_instructions = browser_cpu_batch_instructions(self.runner.is_powerpc_app());
        while running
            && self.runner.guest_tick() < effective_target
            && total_steps < MAX_STEPS_PER_PAINT
            && web_cpu_budget_remaining(frame_start_ms, performance_now(), cpu_budget_ms)
        {
            let remaining_steps = MAX_STEPS_PER_PAINT.saturating_sub(total_steps);
            let batch_steps = remaining_steps.min(cpu_batch_instructions);
            let remaining_audio = audio_samples.saturating_sub(audio_mixed);
            let batch_audio = remaining_audio.min(AUDIO_CALLBACK_CHUNK_SAMPLES);

            let (steps, still_running) =
                self.runner.run_gui_cpu_slice(batch_steps, effective_target);
            total_steps = total_steps.saturating_add(steps);
            visual_steps = visual_steps.saturating_add(steps);
            running = still_running;

            if batch_audio > 0 {
                self.runner.mix_gui_audio_slice(batch_audio);
                audio_mixed = audio_mixed.saturating_add(batch_audio);
                self.service_pending_sound_work(&mut total_steps, &mut reserved_sound_steps);
                running = !self.runner.is_halted();
            }

            if steps == 0 {
                break;
            }
        }

        let presentation_trap_count = self.runner.dispatcher().trap_count;
        let mut presentation_grace_steps = 0usize;
        let presentation_grace_tick = effective_target.saturating_add(1);
        while m68k_presentation_grace_pending(
            self.runner.is_powerpc_app(),
            running,
            visual_steps,
            presentation_grace_steps,
            presentation_trap_count,
            self.runner.dispatcher().trap_count,
        ) {
            let batch_steps = M68K_CPU_BATCH_INSTRUCTIONS
                .min(M68K_PRESENTATION_GRACE_INSTRUCTIONS.saturating_sub(presentation_grace_steps));
            let (steps, still_running) = self
                .runner
                .run_gui_cpu_slice(batch_steps, presentation_grace_tick);
            total_steps = total_steps.saturating_add(steps);
            visual_steps = visual_steps.saturating_add(steps);
            presentation_grace_steps = presentation_grace_steps.saturating_add(steps);
            running = still_running;
            if steps == 0 {
                break;
            }
        }

        self.service_pending_sound_work(&mut total_steps, &mut reserved_sound_steps);
        running = !self.runner.is_halted();

        while audio_mixed < audio_samples && running {
            let chunk_audio = (audio_samples - audio_mixed).min(AUDIO_CALLBACK_CHUNK_SAMPLES);
            self.runner.mix_gui_audio_slice(chunk_audio);
            audio_mixed += chunk_audio;
            self.service_pending_sound_work(&mut total_steps, &mut reserved_sound_steps);
            running = !self.runner.is_halted();
        }

        self.service_pending_sound_work(&mut total_steps, &mut reserved_sound_steps);
        self.last_steps = total_steps;
        for modifier in take_due_launch_modifiers(
            self.runner.guest_tick(),
            &mut self.launch_modifiers_release_tick,
            &mut self.pending_launch_modifiers,
        ) {
            self.runner.push_key_up(modifier.mac_key_code(), 0);
        }

        let input_dirty = self.input_dirty;
        self.input_dirty = false;
        let visual_work = visual_steps > 0 || input_dirty;
        if visual_work {
            self.runner.prepare_text_presentation();
            self.runner.composite_frame();
        }

        // Drain whatever was mixed into the runner's audio buffer. Keep this
        // batched to one browser queue post per frame; Sound Manager still
        // mixes internally in small chunks above so callbacks stay responsive.
        let queued_audio = self.flush_audio();
        if queued_audio == 0 && queue_idle_silence && audio_samples > 0 {
            self.queue_browser_silence(audio_samples);
        }
        self.sync_save_files();

        FrameRunResult {
            running: running && !self.runner.is_halted(),
            visual_work,
        }
    }

    fn frontload_browser_audio(
        &mut self,
        audio_samples: usize,
        queued_source_samples: Option<usize>,
        queue_idle_silence: bool,
    ) -> usize {
        if audio_samples == 0 || !browser_audio_frontload_needed(queued_source_samples) {
            return 0;
        }

        let frontload = audio_samples.min(AUDIO_CALLBACK_CHUNK_SAMPLES * AUDIO_FRONTLOAD_CHUNKS);
        self.runner.mix_gui_audio_slice(frontload);
        if queue_idle_silence && self.runner.audio_buffer_len() == 0 {
            self.queue_browser_silence(frontload);
        }
        frontload
    }

    fn flush_audio(&mut self) -> usize {
        if self.worker_mode {
            self.audio_staging.clear();
            self.runner.drain_audio_into(&mut self.audio_staging);
            return self.audio_staging.len();
        }
        if let Some(ref mut audio) = self.audio {
            self.runner.drain_audio_into(&mut self.audio_staging);
            let mixed_samples = self.audio_staging.len();
            if mixed_samples > 0 {
                audio.queue_samples(&self.audio_staging);
                self.audio_staging.clear();
            }
            mixed_samples
        } else {
            0
        }
    }

    fn queue_browser_silence(&mut self, samples: usize) {
        if samples == 0 {
            return;
        }
        if self.worker_mode {
            self.audio_staging
                .resize(self.audio_staging.len().saturating_add(samples), 0x80);
        } else if let Some(ref mut audio) = self.audio {
            self.audio_staging.clear();
            self.audio_staging.resize(samples, 0x80);
            audio.queue_samples(&self.audio_staging);
            self.audio_staging.clear();
        }
    }

    pub fn is_ui_tracking_active(&self) -> bool {
        self.runner.is_ui_tracking_active()
    }

    pub fn perf_counters(&self) -> PerfCounters {
        PerfCounters {
            guest_tick: self.runner.guest_tick(),
            total_instructions: self.runner.total_instructions(),
            ticks_behind: self.last_ticks_behind,
            last_steps: self.last_steps,
            cpu_budget_ms: self.last_cpu_budget_ms,
            audio_queue_ms: self.last_audio_queue_ms,
        }
    }

    pub fn set_worker_audio_queue_samples(&mut self, samples: Option<usize>) {
        self.worker_audio_queue_samples = samples;
    }

    pub fn set_external_q3_renderer_enabled(&mut self, enabled: bool) {
        self.runner.set_external_q3_renderer_enabled(enabled);
    }

    pub fn take_q3_gpu_frame(&mut self) -> Option<PpcQ3GpuFrame> {
        self.runner.take_q3_gpu_frame()
    }

    pub fn take_worker_audio(&mut self) -> Vec<u8> {
        if self.worker_mode {
            std::mem::take(&mut self.audio_staging)
        } else {
            Vec::new()
        }
    }

    pub fn save_files(&self) -> &[DownloadableSaveFile] {
        &self.save_files
    }

    pub fn save_files_version(&self) -> u64 {
        self.save_files_version
    }

    pub fn import_save_file(&mut self, file: VfsFileSnapshot) -> Result<(), String> {
        if !is_user_save_path(&file.path) {
            return Err("Imported file is not a save file".to_string());
        }
        self.runner.import_vfs_file(&file);
        self.sync_save_files_now();
        Ok(())
    }

    pub fn import_save_file_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        let prefix = self.save_import_path_prefix();
        let file = save_store::decode_macbinary_save_file(&prefix, bytes)?;
        self.import_save_file(file)
    }

    pub fn delete_save_file(&mut self, path: &str) -> Result<(), String> {
        if !is_user_save_path(path) {
            return Err("Selected file is not a save file".to_string());
        }

        let was_visible = self.save_files.iter().any(|file| file.path == path);
        let was_persisted = self.persisted_save_paths.contains(path);
        let removed = self.runner.remove_vfs_file(path);
        if !removed && !was_visible && !was_persisted {
            return Err("Save file was not found".to_string());
        }

        remove_save_from_local_state(
            path,
            &mut self.persisted_save_paths,
            &mut self.last_vfs_fingerprints,
            &mut self.save_files,
        );
        self.bump_save_files_version();
        save_store::delete_save_file(self.game_id.clone(), path.to_string());
        self.sync_save_files_now();
        Ok(())
    }

    pub fn render_rgba(
        &mut self,
        debug_stats: Option<DebugOverlayFrameStats>,
    ) -> ((u32, u32), &[u8]) {
        let dispatcher = self.runner.dispatcher();
        let screen_mode = dispatcher.screen_mode;
        let clut = *dispatcher.device_clut;
        let cursor = dispatcher.cursor().cloned();
        let mouse_pos = dispatcher.mouse_position();
        if !self.frame_palette_valid || self.frame_palette_clut != clut {
            self.frame_palette = display::rgba_palette_from_clut(&clut);
            self.frame_palette_clut = clut;
            self.frame_palette_valid = true;
        }
        display::render_screen_with_rgba_palette_into(
            self.runner.bus(),
            screen_mode,
            &self.frame_palette,
            &mut self.frame_rgba,
        );
        let guest = self
            .runner
            .bus()
            .has_visible_outline_detail()
            .then(|| self.frame_rgba.clone());
        if let Some(cursor) = cursor.as_ref() {
            display::render_cursor(
                &mut self.frame_rgba,
                screen_mode.2 as u32,
                screen_mode.3 as u32,
                cursor,
                mouse_pos,
            );
        }
        if let Some(debug_stats) = debug_stats {
            let lines = self.runner.debug_overlay_snapshot(debug_stats).lines();
            display::render_debug_overlay_rgba(
                &mut self.frame_rgba,
                screen_mode.2 as u32,
                screen_mode.3 as u32,
                &lines,
            );
        }
        self.presented_size = (screen_mode.2 as u32, screen_mode.3 as u32);
        if let Some(size) = guest.as_ref().and_then(|guest| {
            self.runner.bus().presented_rgba_scaled(
                guest,
                &self.frame_rgba,
                self.output_scale,
                &mut self.presented_rgba,
            )
        }) {
            self.presented_size = size;
            (self.presented_size, &self.presented_rgba)
        } else {
            (self.presented_size, &self.frame_rgba)
        }
    }

    pub fn mouse_down(&mut self, v: i16, h: i16) {
        let (v, h) = self.map_mouse_input(v, h);
        self.runner.push_mouse_down(v, h);
        self.input_dirty = true;
    }
    pub fn mouse_up(&mut self, v: i16, h: i16) {
        let (v, h) = self.map_mouse_input(v, h);
        self.runner.push_mouse_up(v, h);
        self.input_dirty = true;
    }
    pub fn mouse_move(&mut self, v: i16, h: i16) {
        let (v, h) = self.map_mouse_input(v, h);
        self.runner.set_mouse_position(v, h);
        self.runner.dispatcher_mut().show_cursor();
        self.input_dirty = true;
    }
    pub fn key_down(&mut self, mac_key: u8, char_code: u8) {
        self.runner.push_key_down(mac_key, char_code);
    }
    pub fn key_up(&mut self, mac_key: u8, char_code: u8) {
        self.runner.push_key_up(mac_key, char_code);
    }

    /// Force-resume the audio context. Safe to call from user-gesture
    /// handlers — AudioContext autoplay policy requires resume from a
    /// gesture, and the initial `new()` call usually runs outside one
    /// (click → async fetch → ctor).
    pub fn resume_audio(&mut self) {
        if let Some(ref mut audio) = self.audio {
            audio.resume();
        }
    }

    fn map_mouse_input(&self, v: i16, h: i16) -> (i16, i16) {
        let dispatcher = self.runner.dispatcher();
        let (_, _, screen_width, screen_height, _) = dispatcher.screen_mode;
        map_playfield_mouse_input(
            v,
            h,
            dispatcher.fullscreen_input_transform(),
            screen_width,
            screen_height,
        )
    }

    fn service_pending_sound_work(
        &mut self,
        total_steps: &mut usize,
        reserved_sound_steps: &mut usize,
    ) {
        if !self.runner.has_pending_sound_work() || self.runner.is_halted() {
            return;
        }

        let remaining = MAX_STEPS_PER_PAINT.saturating_sub(*total_steps);
        let using_reserved_slice = remaining == 0;
        let callback_budget = if using_reserved_slice {
            let reserved_remaining = SOUND_CALLBACK_RESERVED_INSTRUCTIONS_PER_FRAME
                .saturating_sub(*reserved_sound_steps);
            if reserved_remaining == 0 {
                return;
            }
            reserved_remaining.min(SOUND_CALLBACK_SLICE_INSTRUCTIONS)
        } else {
            remaining.min(SOUND_CALLBACK_SLICE_INSTRUCTIONS)
        };

        let (steps, _running) = self.runner.run_pending_sound_work(callback_budget);
        if using_reserved_slice {
            *reserved_sound_steps = reserved_sound_steps.saturating_add(steps);
        }
        *total_steps = total_steps.saturating_add(steps);
    }

    fn sync_save_files(&mut self) {
        self.save_scan_frame = self.save_scan_frame.wrapping_add(1);
        if self.save_scan_frame % SAVE_SCAN_FRAME_INTERVAL != 0 {
            return;
        }
        self.sync_save_files_now();
    }

    fn sync_save_files_now(&mut self) {
        let stats = self.runner.vfs_file_stats_where(is_user_save_path);
        let mut current_fingerprints = HashMap::new();
        let previous_fingerprints = self.last_vfs_fingerprints.clone();
        let previous_persisted_paths = self.persisted_save_paths.clone();
        let mut existing_save_files = std::mem::take(&mut self.save_files)
            .into_iter()
            .map(|file| (file.path.clone(), file))
            .collect::<HashMap<_, _>>();
        let mut next_persisted_paths = HashSet::new();
        let mut save_files = Vec::new();

        for stat in stats {
            if !self.persisted_save_paths.contains(&stat.path)
                && self
                    .archive_vfs_stats
                    .get(&stat.path)
                    .is_some_and(|archive| vfs_stats_match(archive, &stat))
            {
                continue;
            }

            let Some(summary) = self.runner.vfs_file_summary(&stat.path) else {
                continue;
            };
            let fingerprint = SaveFingerprint::from(&summary);
            current_fingerprints.insert(summary.path.clone(), fingerprint.clone());

            if let Some(downloadable) = take_reusable_save_file(
                &summary.path,
                &fingerprint,
                &self.last_vfs_fingerprints,
                &self.persisted_save_paths,
                &mut existing_save_files,
            ) {
                next_persisted_paths.insert(summary.path.clone());
                save_files.push(downloadable);
                continue;
            }

            let Some(snapshot) = self.runner.vfs_file_snapshot(&summary.path) else {
                continue;
            };
            if self.last_vfs_fingerprints.get(&summary.path) != Some(&fingerprint)
                || !self.persisted_save_paths.contains(&summary.path)
            {
                save_store::persist_save_file(self.game_id.clone(), snapshot.clone());
            }
            next_persisted_paths.insert(summary.path.clone());
            save_files.push(save_store::downloadable_save_file(&snapshot));
        }

        for path in self.persisted_save_paths.difference(&next_persisted_paths) {
            save_store::delete_save_file(self.game_id.clone(), path.clone());
        }

        save_files.sort_by_key(|file| file.name.to_ascii_lowercase());
        let save_state_changed = save_state_changed(
            &previous_fingerprints,
            &current_fingerprints,
            &previous_persisted_paths,
            &next_persisted_paths,
        );
        self.persisted_save_paths = next_persisted_paths;
        self.last_vfs_fingerprints = current_fingerprints;
        self.save_files = save_files;
        if save_state_changed {
            self.bump_save_files_version();
        }
    }

    fn bump_save_files_version(&mut self) {
        self.save_files_version = self.save_files_version.wrapping_add(1);
    }

    fn save_import_path_prefix(&mut self) -> String {
        let summaries = self.runner.vfs_file_summaries();
        save_import_path_prefix_for_summaries(&summaries)
    }
}

fn take_due_launch_modifiers(
    current_tick: u32,
    release_tick: &mut Option<u32>,
    modifiers: &mut Vec<LaunchModifier>,
) -> Vec<LaunchModifier> {
    if release_tick.is_some_and(|tick| current_tick >= tick) {
        *release_tick = None;
        std::mem::take(modifiers)
    } else {
        Vec::new()
    }
}

fn new_web_runner() -> FixtureRunner {
    let mut runner = game::new_runner();
    runner.set_ui_theme(UiThemeId::ClassicSystem7);
    runner
}

fn vfs_stats(runner: &mut FixtureRunner) -> HashMap<String, VfsFileStat> {
    runner
        .vfs_file_stats_where(|_| true)
        .into_iter()
        .map(|stat| (stat.path.clone(), stat))
        .collect()
}

fn vfs_stats_match(left: &VfsFileStat, right: &VfsFileStat) -> bool {
    left.data_len == right.data_len
        && left.resource_len == right.resource_len
        && left.file_type == right.file_type
        && left.creator == right.creator
        && left.finder_flags == right.finder_flags
        && left.modified_date == right.modified_date
}

fn save_fork_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn take_reusable_save_file(
    path: &str,
    fingerprint: &SaveFingerprint,
    last_vfs_fingerprints: &HashMap<String, SaveFingerprint>,
    persisted_save_paths: &HashSet<String>,
    existing_save_files: &mut HashMap<String, DownloadableSaveFile>,
) -> Option<DownloadableSaveFile> {
    if last_vfs_fingerprints.get(path) == Some(fingerprint) && persisted_save_paths.contains(path) {
        existing_save_files.remove(path)
    } else {
        None
    }
}

fn save_state_changed(
    previous_fingerprints: &HashMap<String, SaveFingerprint>,
    current_fingerprints: &HashMap<String, SaveFingerprint>,
    previous_persisted_paths: &HashSet<String>,
    next_persisted_paths: &HashSet<String>,
) -> bool {
    previous_fingerprints != current_fingerprints
        || previous_persisted_paths != next_persisted_paths
}

fn remove_save_from_local_state(
    path: &str,
    persisted_save_paths: &mut HashSet<String>,
    last_vfs_fingerprints: &mut HashMap<String, SaveFingerprint>,
    save_files: &mut Vec<DownloadableSaveFile>,
) {
    persisted_save_paths.remove(path);
    last_vfs_fingerprints.remove(path);
    save_files.retain(|file| file.path != path);
}

fn is_user_save_path(path: &str) -> bool {
    let normalized = path.trim_matches('/');
    if normalized.is_empty() {
        return false;
    }

    let lower = normalized.to_ascii_lowercase();
    if lower.starts_with("system folder/preferences/")
        || lower.starts_with("system folder/temporary items/")
        || lower.starts_with("temporary items/")
        || lower.starts_with("trash/")
    {
        return false;
    }

    let name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    !matches!(name, "desktop db" | "desktop df" | "thevolume")
}

fn save_import_path_prefix_for_summaries(summaries: &[VfsFileSummary]) -> String {
    let mut candidates = summaries
        .iter()
        .filter(|summary| is_user_save_path(&summary.path))
        .filter_map(|summary| pilots_parent_path(&summary.path))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        path_depth(right)
            .cmp(&path_depth(left))
            .then_with(|| left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase()))
    });
    candidates
        .into_iter()
        .next()
        .unwrap_or_else(|| "Pilots".to_string())
}

fn pilots_parent_path(path: &str) -> Option<String> {
    let normalized = path.trim_matches('/');
    let (parent, name) = normalized.rsplit_once('/')?;
    if parent.is_empty() || name.is_empty() {
        return None;
    }
    let parent_name = parent.rsplit('/').next().unwrap_or(parent);
    parent_name
        .eq_ignore_ascii_case("pilots")
        .then(|| parent.to_string())
}

fn path_depth(path: &str) -> usize {
    path.split('/')
        .filter(|component| !component.is_empty())
        .count()
}

#[derive(Clone, Copy, Debug)]
pub struct PerfCounters {
    pub guest_tick: u32,
    pub total_instructions: u64,
    pub ticks_behind: u32,
    pub last_steps: usize,
    pub cpu_budget_ms: f64,
    pub audio_queue_ms: Option<f64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameRunResult {
    pub running: bool,
    pub visual_work: bool,
}

fn current_mac_epoch_seconds() -> u32 {
    let unix_ms = js_sys::Date::now();
    let unix_secs = (unix_ms / 1000.0) as u64;
    unix_secs
        .saturating_add(UNIX_TO_MAC_EPOCH_OFFSET)
        .min(u32::MAX as u64) as u32
}

fn performance_now() -> f64 {
    js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("performance"))
        .ok()
        .and_then(|value| value.dyn_into::<web_sys::Performance>().ok())
        .map(|performance| performance.now())
        .unwrap_or_else(js_sys::Date::now)
}

fn browser_cpu_batch_instructions(powerpc: bool) -> usize {
    if powerpc {
        PPC_CPU_BATCH_INSTRUCTIONS
    } else {
        M68K_CPU_BATCH_INSTRUCTIONS
    }
}

fn m68k_presentation_grace_pending(
    powerpc: bool,
    running: bool,
    visual_steps: usize,
    grace_steps: usize,
    initial_trap_count: u64,
    current_trap_count: u64,
) -> bool {
    !powerpc
        && running
        && visual_steps > 0
        && grace_steps < M68K_PRESENTATION_GRACE_INSTRUCTIONS
        && current_trap_count == initial_trap_count
}

fn wall_clock_origin_for_guest_tick(now_ms: f64, guest_tick: u32) -> f64 {
    now_ms - (guest_tick as f64 * 1000.0 / DEFAULT_VBL_HZ as f64)
}

async fn yield_to_browser_task() {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let Some(window) = web_sys::window() else {
            let _ = resolve.call0(&JsValue::UNDEFINED);
            return;
        };
        let resolve_for_timeout = resolve.clone();
        let timeout_cb = Closure::once(move || {
            let _ = resolve_for_timeout.call0(&JsValue::UNDEFINED);
        });
        if window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                timeout_cb.as_ref().unchecked_ref(),
                0,
            )
            .is_ok()
        {
            timeout_cb.forget();
        } else {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        }
    });
    let _ = JsFuture::from(promise).await;
}

async fn yield_to_browser_frame() {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let Some(window) = web_sys::window() else {
            let _ = resolve.call0(&JsValue::UNDEFINED);
            return;
        };
        let window_for_timeout = window.clone();
        let resolve_for_raf = resolve.clone();
        let cb = Closure::once(move || {
            let resolve_for_timeout = resolve_for_raf.clone();
            let timeout_cb = Closure::once(move || {
                let _ = resolve_for_timeout.call0(&JsValue::UNDEFINED);
            });
            if window_for_timeout
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    timeout_cb.as_ref().unchecked_ref(),
                    0,
                )
                .is_ok()
            {
                timeout_cb.forget();
            } else {
                let _ = resolve_for_raf.call0(&JsValue::UNDEFINED);
            }
        });
        if window
            .request_animation_frame(cb.as_ref().unchecked_ref())
            .is_ok()
        {
            cb.forget();
        } else {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        }
    });
    let _ = JsFuture::from(promise).await;
}

fn audio_samples_for_elapsed(
    audio_started_at_ms: f64,
    audio_samples_target: u64,
    now_ms: f64,
    queued_source_samples: Option<usize>,
) -> (usize, u64) {
    let elapsed_ms = (now_ms - audio_started_at_ms).max(0.0);
    let elapsed_target = (elapsed_ms * OUTPUT_RATE as f64 / 1000.0) as u64;
    let clamped_previous_target = audio_samples_target.min(elapsed_target);
    let catchup_base =
        clamped_previous_target.max(elapsed_target.saturating_sub(MAX_AUDIO_QUEUE_SAMPLES as u64));
    let elapsed_samples = elapsed_target
        .saturating_sub(catchup_base)
        .min(MAX_AUDIO_SAMPLES_PER_PAINT as u64) as usize;
    let queue_deficit = queued_source_samples
        .map(|samples| HEALTHY_AUDIO_QUEUE_SAMPLES.saturating_sub(samples))
        .unwrap_or(0);
    let queue_top_up =
        queue_deficit.min(MAX_AUDIO_SAMPLES_PER_PAINT.saturating_sub(elapsed_samples));
    let samples = elapsed_samples.saturating_add(queue_top_up);
    (samples, catchup_base.saturating_add(samples as u64))
}

fn web_cpu_budget_ms(
    worklet_audio: bool,
    powerpc: bool,
    ticks_behind: u32,
    queued_source_samples: Option<usize>,
) -> f64 {
    if !worklet_audio {
        SCRIPT_PROCESSOR_CPU_MS_PER_PAINT
    } else if powerpc && ticks_behind > 0 {
        PPC_AUDIO_WORKLET_CATCHUP_CPU_MS_PER_PAINT
    } else if ticks_behind > 0
        && queued_source_samples
            .map(|samples| samples >= HEALTHY_AUDIO_QUEUE_SAMPLES)
            .unwrap_or(false)
    {
        AUDIO_WORKLET_HEALTHY_CATCHUP_CPU_MS_PER_PAINT
    } else if ticks_behind > 1 {
        AUDIO_WORKLET_CATCHUP_CPU_MS_PER_PAINT
    } else if powerpc {
        PPC_AUDIO_WORKLET_CPU_MS_PER_PAINT
    } else {
        AUDIO_WORKLET_CPU_MS_PER_PAINT
    }
}

fn web_effective_target_tick(
    current_tick: u32,
    ticks_behind: u32,
    runtime_pacing: RuntimePacing,
) -> u32 {
    current_tick.saturating_add(ticks_behind.min(runtime_pacing.max_ticks_per_paint))
}

fn web_instructions_per_tick(runtime_pacing: RuntimePacing) -> u32 {
    ((runtime_pacing.cpu_mhz as f64 * 1_000_000.0) / DEFAULT_VBL_HZ)
        .round()
        .max(1.0) as u32
}

fn web_cpu_budget_remaining(frame_start_ms: f64, now_ms: f64, budget_ms: f64) -> bool {
    now_ms - frame_start_ms < budget_ms
}

fn audio_prefill_samples() -> usize {
    (OUTPUT_RATE as usize * AUDIO_PREFILL_MS) / 1000
}

fn browser_idle_silence_needed(queued_source_samples: Option<usize>) -> bool {
    queued_source_samples
        .map(|samples| samples < HEALTHY_AUDIO_QUEUE_SAMPLES)
        .unwrap_or(false)
}

fn browser_audio_frontload_needed(queued_source_samples: Option<usize>) -> bool {
    browser_idle_silence_needed(queued_source_samples)
}

fn map_playfield_mouse_input(
    v: i16,
    h: i16,
    rect: Option<ScreenCopyBitsRect>,
    screen_width: u16,
    screen_height: u16,
) -> (i16, i16) {
    let Some(rect) = rect else {
        return (v, h);
    };

    if rect.dst_right <= rect.dst_left
        || rect.dst_bottom <= rect.dst_top
        || rect.src_right <= rect.src_left
        || rect.src_bottom <= rect.src_top
    {
        return (v, h);
    }

    let screen_width = screen_width.min(i16::MAX as u16) as i16;
    let screen_height = screen_height.min(i16::MAX as u16) as i16;
    if rect.dst_left <= 0
        && rect.dst_top <= 0
        && rect.dst_right >= screen_width
        && rect.dst_bottom >= screen_height
        && rect.src_right - rect.src_left == rect.dst_right - rect.dst_left
        && rect.src_bottom - rect.src_top == rect.dst_bottom - rect.dst_top
    {
        return (v, h);
    }

    (
        map_axis_through_copybits(
            v,
            rect.dst_top,
            rect.dst_bottom,
            rect.src_top,
            rect.src_bottom,
        ),
        map_axis_through_copybits(
            h,
            rect.dst_left,
            rect.dst_right,
            rect.src_left,
            rect.src_right,
        ),
    )
}

fn map_axis_through_copybits(
    value: i16,
    dst_start: i16,
    dst_end: i16,
    src_start: i16,
    src_end: i16,
) -> i16 {
    let dst_len = i32::from(dst_end) - i32::from(dst_start);
    let src_len = i32::from(src_end) - i32::from(src_start);
    if dst_len <= 0 || src_len <= 0 {
        return value;
    }

    let clamped = i32::from(value).clamp(i32::from(dst_start), i32::from(dst_end) - 1);
    let mapped = i32::from(src_start) + (clamped - i32::from(dst_start)) * src_len / dst_len;
    mapped.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_runtime_uses_systemless_presentation_theme() {
        assert_eq!(new_web_runner().ui_theme_id(), UiThemeId::ClassicSystem7);
    }

    #[test]
    fn audio_clock_survives_late_visual_tick_resets() {
        let samples_per_frame = ((OUTPUT_RATE as f64 / DEFAULT_VBL_HZ as f64).round()) as usize;
        let (samples, next_target) = audio_samples_for_elapsed(1_000.0, 0, 1_016.7, None);

        assert!(
            samples >= samples_per_frame.saturating_sub(2),
            "normal frame should produce approximately one frame of audio"
        );
        assert!(next_target > 0);

        let (late_samples, late_target) =
            audio_samples_for_elapsed(1_000.0, next_target, 1_500.0, None);

        assert_eq!(late_samples, MAX_AUDIO_SAMPLES_PER_PAINT);
        assert_eq!(
            late_target,
            next_target + MAX_AUDIO_SAMPLES_PER_PAINT as u64
        );
        assert!(
            late_target > next_target,
            "late visual frames must preserve recoverable audio debt instead of dropping it"
        );
    }

    #[test]
    fn audio_clock_catches_up_after_late_browser_frame() {
        let (first_samples, first_target) = audio_samples_for_elapsed(1_000.0, 0, 1_500.0, None);
        assert_eq!(first_samples, MAX_AUDIO_SAMPLES_PER_PAINT);
        assert_eq!(first_target, MAX_AUDIO_SAMPLES_PER_PAINT as u64);

        let (second_samples, second_target) =
            audio_samples_for_elapsed(1_000.0, first_target, 1_500.0, None);
        assert_eq!(second_samples, MAX_AUDIO_SAMPLES_PER_PAINT);
        assert_eq!(
            second_target,
            first_target + MAX_AUDIO_SAMPLES_PER_PAINT as u64
        );
    }

    #[test]
    fn audio_clock_drops_stale_debt_after_long_pause() {
        let elapsed_target = OUTPUT_RATE as u64 * 2;
        let (samples, next_target) = audio_samples_for_elapsed(1_000.0, 0, 3_000.0, None);

        assert_eq!(samples, MAX_AUDIO_SAMPLES_PER_PAINT);
        assert_eq!(
            next_target,
            elapsed_target - MAX_AUDIO_QUEUE_SAMPLES as u64 + MAX_AUDIO_SAMPLES_PER_PAINT as u64
        );
    }

    #[test]
    fn audio_clock_tops_up_running_sink_below_healthy_queue() {
        let samples_per_frame = ((OUTPUT_RATE as f64 / DEFAULT_VBL_HZ as f64).round()) as usize;
        let deficit = samples_per_frame / 2;
        let queued = HEALTHY_AUDIO_QUEUE_SAMPLES - deficit;
        let (samples, next_target) = audio_samples_for_elapsed(1_000.0, 0, 1_016.7, Some(queued));

        assert!(
            samples >= samples_per_frame.saturating_add(deficit).saturating_sub(4),
            "normal browser frames should use spare audio budget to rebuild a healthy queue"
        );
        assert_eq!(next_target, samples as u64);
    }

    #[test]
    fn audio_clock_caps_queue_top_up_per_paint() {
        let (samples, next_target) = audio_samples_for_elapsed(1_000.0, 0, 1_016.7, Some(0));

        assert_eq!(
            samples, MAX_AUDIO_SAMPLES_PER_PAINT,
            "queue recovery should remain bounded to one frontend audio budget"
        );
        assert_eq!(next_target, MAX_AUDIO_SAMPLES_PER_PAINT as u64);
    }

    #[test]
    fn audio_clock_does_not_top_up_when_queue_is_healthy() {
        let (samples, next_target) =
            audio_samples_for_elapsed(1_000.0, 0, 1_016.7, Some(HEALTHY_AUDIO_QUEUE_SAMPLES));
        let (baseline_samples, baseline_target) =
            audio_samples_for_elapsed(1_000.0, 0, 1_016.7, None);

        assert_eq!(samples, baseline_samples);
        assert_eq!(next_target, baseline_target);
    }

    #[test]
    fn web_cpu_budget_caps_long_animation_callbacks() {
        assert!(web_cpu_budget_remaining(
            20.0,
            24.9,
            SCRIPT_PROCESSOR_CPU_MS_PER_PAINT
        ));
        assert!(!web_cpu_budget_remaining(
            20.0,
            25.0,
            SCRIPT_PROCESSOR_CPU_MS_PER_PAINT
        ));
        assert!(!web_cpu_budget_remaining(
            20.0,
            45.0,
            SCRIPT_PROCESSOR_CPU_MS_PER_PAINT
        ));
    }

    #[test]
    fn web_cpu_budget_expands_when_worklet_audio_can_keep_playing() {
        assert_eq!(
            web_cpu_budget_ms(false, false, 4, Some(HEALTHY_AUDIO_QUEUE_SAMPLES)),
            SCRIPT_PROCESSOR_CPU_MS_PER_PAINT
        );
        assert_eq!(
            web_cpu_budget_ms(true, false, 0, Some(HEALTHY_AUDIO_QUEUE_SAMPLES)),
            AUDIO_WORKLET_CPU_MS_PER_PAINT
        );
        assert_eq!(
            web_cpu_budget_ms(true, false, 1, Some(HEALTHY_AUDIO_QUEUE_SAMPLES)),
            AUDIO_WORKLET_HEALTHY_CATCHUP_CPU_MS_PER_PAINT
        );
        assert_eq!(
            web_cpu_budget_ms(true, false, 2, None),
            AUDIO_WORKLET_CATCHUP_CPU_MS_PER_PAINT
        );
        assert_eq!(
            web_cpu_budget_ms(true, false, 2, Some(HEALTHY_AUDIO_QUEUE_SAMPLES - 1)),
            AUDIO_WORKLET_CATCHUP_CPU_MS_PER_PAINT
        );
        assert_eq!(
            web_cpu_budget_ms(true, false, 2, Some(HEALTHY_AUDIO_QUEUE_SAMPLES)),
            AUDIO_WORKLET_HEALTHY_CATCHUP_CPU_MS_PER_PAINT
        );
        assert_eq!(
            web_cpu_budget_ms(true, true, 0, None),
            PPC_AUDIO_WORKLET_CPU_MS_PER_PAINT
        );
        assert_eq!(
            web_cpu_budget_ms(true, true, 1, None),
            PPC_AUDIO_WORKLET_CATCHUP_CPU_MS_PER_PAINT
        );
        assert_eq!(
            web_cpu_budget_ms(true, true, 1, Some(HEALTHY_AUDIO_QUEUE_SAMPLES)),
            PPC_AUDIO_WORKLET_CATCHUP_CPU_MS_PER_PAINT
        );
        assert!(
            AUDIO_WORKLET_HEALTHY_CATCHUP_CPU_MS_PER_PAINT > AUDIO_WORKLET_CATCHUP_CPU_MS_PER_PAINT,
            "healthy AudioWorklet queues should permit a larger catch-up slice"
        );

        assert!(web_cpu_budget_remaining(
            20.0,
            29.9,
            AUDIO_WORKLET_HEALTHY_CATCHUP_CPU_MS_PER_PAINT
        ));
        assert!(!web_cpu_budget_remaining(
            20.0,
            34.0,
            AUDIO_WORKLET_HEALTHY_CATCHUP_CPU_MS_PER_PAINT
        ));
    }

    #[test]
    fn standard_runtime_pacing_keeps_two_tick_browser_recovery() {
        let pacing = RuntimePacing::default();

        assert_eq!(web_effective_target_tick(100, 1, pacing), 101);
        assert_eq!(web_effective_target_tick(100, 8, pacing), 102);
    }

    #[test]
    fn browser_pacer_starts_at_seeded_guest_tick() {
        let origin = wall_clock_origin_for_guest_tick(20_000.0, 600);
        let elapsed_ms = 20_000.0 - origin;
        let wall_tick = (elapsed_ms * DEFAULT_VBL_HZ as f64 / 1000.0) as u32;

        assert_eq!(
            wall_tick, 600,
            "nonzero launch TickCount must not make the browser wait real time before running CPU"
        );
    }

    #[test]
    fn seeded_guest_tick_can_advance_on_first_browser_frame() {
        let pacing = RuntimePacing::default();
        let origin = wall_clock_origin_for_guest_tick(20_000.0, 600);
        let now = 20_000.0 + 20.0;
        let current_tick = 600;
        let wall_tick = ((now - origin) * DEFAULT_VBL_HZ as f64 / 1000.0) as u32;
        let ticks_behind = wall_tick.saturating_sub(current_tick);

        assert!(
            ticks_behind > 0,
            "the first post-boot browser frame should have runnable guest work"
        );
        assert_eq!(
            web_effective_target_tick(current_tick, ticks_behind, pacing),
            current_tick + 1
        );
    }

    #[test]
    fn launch_modifiers_release_at_the_configured_tick_once() {
        let mut release_tick = Some(720);
        let mut modifiers = vec![LaunchModifier::Option];

        assert!(take_due_launch_modifiers(719, &mut release_tick, &mut modifiers).is_empty());
        assert_eq!(release_tick, Some(720));
        assert_eq!(modifiers, [LaunchModifier::Option]);

        assert_eq!(
            take_due_launch_modifiers(720, &mut release_tick, &mut modifiers),
            [LaunchModifier::Option]
        );
        assert_eq!(release_tick, None);
        assert!(modifiers.is_empty());
        assert!(take_due_launch_modifiers(721, &mut release_tick, &mut modifiers).is_empty());
    }

    #[test]
    fn smooth_runtime_pacing_allows_faster_recovery() {
        let pacing = RuntimePacing {
            max_ticks_per_paint: 4,
            reset_slack_ticks: 6,
            cpu_mhz: 25,
        };

        assert_eq!(web_effective_target_tick(100, 2, pacing), 102);
        assert_eq!(web_effective_target_tick(100, 8, pacing), 104);
    }

    #[test]
    fn runtime_pacing_default_uses_oracle_cpu_budget() {
        assert_eq!(
            web_instructions_per_tick(RuntimePacing::default()),
            (25_000_000.0 / DEFAULT_VBL_HZ).round() as u32
        );
    }

    #[test]
    fn runtime_pacing_can_lower_browser_cpu_budget_per_tick() {
        let pacing = RuntimePacing {
            max_ticks_per_paint: 4,
            reset_slack_ticks: 6,
            cpu_mhz: 10,
        };

        assert_eq!(
            web_instructions_per_tick(pacing),
            (10_000_000.0 / DEFAULT_VBL_HZ).round() as u32
        );
    }

    #[test]
    fn browser_cpu_budgets_leave_canvas_time_for_startup_smoothness() {
        assert!(
            SCRIPT_PROCESSOR_CPU_MS_PER_PAINT <= 5.0,
            "legacy main-thread audio needs short emulator slices so WebAudio callbacks can run"
        );
        assert!(
            AUDIO_WORKLET_CPU_MS_PER_PAINT <= 7.0,
            "worklet audio still needs main-thread breathing room for canvas presentation"
        );
        assert!(
            AUDIO_WORKLET_CATCHUP_CPU_MS_PER_PAINT <= 10.0,
            "catch-up frames should not consume most of a 60 Hz browser frame"
        );
        assert!(
            AUDIO_WORKLET_HEALTHY_CATCHUP_CPU_MS_PER_PAINT <= 14.0,
            "healthy AudioWorklet queues can spend more time catching up while \
             still leaving room for canvas presentation"
        );
    }

    #[test]
    fn browser_cpu_batches_balance_responsiveness_and_ppc_sync_overhead() {
        assert!(
            M68K_CPU_BATCH_INSTRUCTIONS <= 2_500,
            "68k browser CPU batches must stay short enough that heavy startup traps \
             cannot monopolize a whole animation frame"
        );
        assert_eq!(
            SOUND_CALLBACK_SLICE_INSTRUCTIONS, M68K_CPU_BATCH_INSTRUCTIONS,
            "sound callback work should use the same short browser slice"
        );
        assert!(
            MAX_STEPS_PER_PAINT.div_ceil(M68K_CPU_BATCH_INSTRUCTIONS) >= 400,
            "max 68k paint work should be split into many interruptible browser batches"
        );
        assert_eq!(
            browser_cpu_batch_instructions(false),
            M68K_CPU_BATCH_INSTRUCTIONS
        );
        assert_eq!(
            browser_cpu_batch_instructions(true),
            PPC_CPU_BATCH_INSTRUCTIONS
        );
        assert!(
            PPC_CPU_BATCH_INSTRUCTIONS >= M68K_CPU_BATCH_INSTRUCTIONS * 20,
            "PPC batches must amortize runner synchronization overhead"
        );
        assert!(
            PPC_CPU_BATCH_INSTRUCTIONS <= MAX_STEPS_PER_PAINT / 10,
            "PPC work must remain interruptible within the browser frame budget"
        );
        assert!(
            MAX_AUDIO_SAMPLES_PER_PAINT.div_ceil(AUDIO_CALLBACK_CHUNK_SAMPLES) <= 9,
            "browser audio must be front-loaded in a small number of chunks so \
             startup CPU work cannot delay most of a frame's queued samples"
        );
        assert!(
            AUDIO_CALLBACK_CHUNK_SAMPLES * AUDIO_FRONTLOAD_CHUNKS <= MAX_AUDIO_SAMPLES_PER_PAINT,
            "startup audio front-load must stay inside the per-paint audio cap"
        );
    }

    #[test]
    fn presentation_grace_finishes_only_the_active_68k_drawing_burst() {
        assert!(m68k_presentation_grace_pending(false, true, 1, 0, 10, 10));
        assert!(!m68k_presentation_grace_pending(false, true, 1, 0, 10, 11));
        assert!(!m68k_presentation_grace_pending(true, true, 1, 0, 10, 10));
        assert!(!m68k_presentation_grace_pending(
            false,
            true,
            1,
            M68K_PRESENTATION_GRACE_INSTRUCTIONS,
            10,
            10,
        ));
    }

    #[test]
    fn web_pack_mounting_yields_after_bounded_copy_work() {
        assert_eq!(
            WEB_PACK_LOAD_YIELD_BYTES,
            256 * 1024,
            "EV/EVO web startup depends on mount slices staying comfortably below \
             long-task scale while larger frame-yield cadence controls total startup latency"
        );
        assert_eq!(
            WEB_PACK_LOAD_FRAME_YIELD_BYTES,
            1024 * 1024,
            "web-pack mounting should periodically hand a real frame to the browser \
             during large EV/EVO startup archives"
        );
        assert_eq!(
            WEB_PACK_LOAD_FRAME_YIELD_BYTES % WEB_PACK_LOAD_YIELD_BYTES,
            0,
            "frame-yield cadence should align with mount chunk boundaries"
        );
    }

    #[test]
    fn browser_audio_prefill_covers_heavy_startup_jitter() {
        assert_eq!(audio_prefill_samples(), 8_820);
        assert!(
            audio_prefill_samples() > HEALTHY_AUDIO_QUEUE_SAMPLES * 2,
            "startup prefill should leave a cushion above the steady-state healthy queue"
        );
        assert!(
            audio_prefill_samples() < MAX_AUDIO_QUEUE_SAMPLES,
            "startup prefill must stay below the latency cap"
        );
    }

    #[test]
    fn browser_idle_silence_only_tops_up_unhealthy_running_queues() {
        assert!(browser_idle_silence_needed(Some(0)));
        assert!(browser_idle_silence_needed(Some(
            HEALTHY_AUDIO_QUEUE_SAMPLES - 1
        )));
        assert!(!browser_idle_silence_needed(Some(
            HEALTHY_AUDIO_QUEUE_SAMPLES
        )));
        assert!(!browser_idle_silence_needed(None));
    }

    #[test]
    fn browser_audio_frontloads_when_running_queue_is_unhealthy() {
        assert!(browser_audio_frontload_needed(Some(0)));
        assert!(browser_audio_frontload_needed(Some(
            HEALTHY_AUDIO_QUEUE_SAMPLES - 1
        )));
        assert!(!browser_audio_frontload_needed(Some(
            HEALTHY_AUDIO_QUEUE_SAMPLES
        )));
        assert!(!browser_audio_frontload_needed(None));
    }

    #[test]
    fn script_processor_audio_queue_holds_source_edges_when_upsampling() {
        let mut queue = AudioQueue::new((OUTPUT_RATE * 2) as f64);
        queue.queue_samples(&[0x90, 0x90, 0xA0, 0xA0]);

        let output = queue.render(8).to_vec();
        let held_first = u8_pcm_to_f32(0x90);
        let held_second = u8_pcm_to_f32(0xA0);

        assert_eq!(
            &output[..4],
            &[held_first, held_first, held_first, held_first],
            "browser fallback upsampling must preserve low-rate effect edges"
        );
        assert_eq!(
            &output[4..],
            &[held_second, held_second, held_second, held_second],
            "browser fallback should advance to the next source sample without an interpolated midpoint"
        );
    }

    fn abuse_playfield_rect() -> ScreenCopyBitsRect {
        ScreenCopyBitsRect {
            src_top: 0,
            src_left: 0,
            src_bottom: 400,
            src_right: 640,
            dst_top: 100,
            dst_left: 80,
            dst_bottom: 500,
            dst_right: 720,
        }
    }

    #[test]
    fn playfield_mouse_mapping_keeps_screen_space_without_explicit_transform() {
        assert_eq!(
            map_playfield_mouse_input(340, 400, None, 800, 600),
            (340, 400)
        );
    }

    #[test]
    fn playfield_mouse_mapping_subtracts_centered_destination_offset() {
        assert_eq!(
            map_playfield_mouse_input(340, 400, Some(abuse_playfield_rect()), 800, 600),
            (240, 320)
        );
    }

    #[test]
    fn playfield_mouse_mapping_clamps_outer_letterbox_margins() {
        assert_eq!(
            map_playfield_mouse_input(40, 20, Some(abuse_playfield_rect()), 800, 600),
            (0, 0)
        );
        assert_eq!(
            map_playfield_mouse_input(560, 760, Some(abuse_playfield_rect()), 800, 600),
            (399, 639)
        );
    }

    #[test]
    fn save_candidates_skip_classic_system_support_files() {
        assert!(is_user_save_path("Pilots/Rick Hardslab"));
        assert!(is_user_save_path("Games/My Saved Game"));

        assert!(!is_user_save_path(""));
        assert!(!is_user_save_path(
            "System Folder/Preferences/EV Override License"
        ));
        assert!(!is_user_save_path(
            "System Folder/Preferences/thaumaturgy.log"
        ));
        assert!(!is_user_save_path("System Folder/Temporary Items/scratch"));
        assert!(!is_user_save_path("Temporary Items/scratch"));
        assert!(!is_user_save_path("Trash/Old Pilot"));
        assert!(!is_user_save_path("Desktop DB"));
        assert!(!is_user_save_path("Desktop DF"));
        assert!(!is_user_save_path("TheVolume"));
    }

    #[test]
    fn save_import_prefix_prefers_nested_pilots_directory() {
        let summaries = vec![
            vfs_summary("System Folder/Preferences/Pilots/License"),
            vfs_summary("Pilots/Root Pilot"),
            vfs_summary("EV Override 1.0.1/Pilots/Ben"),
        ];

        assert_eq!(
            save_import_path_prefix_for_summaries(&summaries),
            "EV Override 1.0.1/Pilots"
        );
    }

    #[test]
    fn save_import_prefix_falls_back_to_pilots_without_anchor_file() {
        let summaries = vec![vfs_summary("Games/My Saved Game")];

        assert_eq!(save_import_path_prefix_for_summaries(&summaries), "Pilots");
    }

    #[test]
    fn save_import_prefix_places_macbinary_in_nested_pilots_directory() {
        let summaries = vec![vfs_summary("EV Override 1.0.1/Pilots/Ben")];
        let exported = VfsFileSnapshot {
            path: "Pilots/Rick Hardslab".to_string(),
            data_fork: vec![1, 2, 3],
            resource_fork: vec![4, 5, 6],
            file_type: u32::from_be_bytes(*b"PIL "),
            creator: u32::from_be_bytes(*b"EVO!"),
            finder_flags: 0x4000,
            created_date: 123,
            modified_date: 456,
        };
        let macbinary = save_store::downloadable_save_file(&exported).macbinary;
        let prefix = save_import_path_prefix_for_summaries(&summaries);
        let imported = save_store::decode_macbinary_save_file(&prefix, &macbinary).unwrap();

        assert_eq!(imported.path, "EV Override 1.0.1/Pilots/Rick Hardslab");
        assert_eq!(imported.data_fork, exported.data_fork);
        assert_eq!(imported.resource_fork, exported.resource_fork);
        assert_eq!(imported.file_type, exported.file_type);
        assert_eq!(imported.creator, exported.creator);
    }

    #[test]
    fn save_delete_removes_visible_persisted_and_fingerprint_state() {
        let path = "Pilots/Rick Hardslab";
        let mut persisted = HashSet::from([path.to_string(), "Pilots/Ace".to_string()]);
        let mut fingerprints = HashMap::from([
            (
                path.to_string(),
                SaveFingerprint {
                    data_len: 12,
                    resource_len: 34,
                    data_hash: 1,
                    resource_hash: 2,
                    file_type: u32::from_be_bytes(*b"PIL "),
                    creator: u32::from_be_bytes(*b"EVO!"),
                    finder_flags: 0,
                    modified_date: 456,
                },
            ),
            (
                "Pilots/Ace".to_string(),
                SaveFingerprint {
                    data_len: 1,
                    resource_len: 2,
                    data_hash: 3,
                    resource_hash: 4,
                    file_type: u32::from_be_bytes(*b"PIL "),
                    creator: u32::from_be_bytes(*b"EV  "),
                    finder_flags: 0,
                    modified_date: 123,
                },
            ),
        ]);
        let mut files = vec![
            downloadable_save("Pilots/Ace"),
            downloadable_save("Pilots/Rick Hardslab"),
        ];

        remove_save_from_local_state(path, &mut persisted, &mut fingerprints, &mut files);

        assert!(!persisted.contains(path));
        assert!(persisted.contains("Pilots/Ace"));
        assert!(!fingerprints.contains_key(path));
        assert!(fingerprints.contains_key("Pilots/Ace"));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "Pilots/Ace");
    }

    #[test]
    fn unchanged_persisted_save_reuses_existing_downloadable_entry() {
        let path = "Pilots/Rick Hardslab";
        let fingerprint = SaveFingerprint {
            data_len: 12,
            resource_len: 34,
            data_hash: 1,
            resource_hash: 2,
            file_type: u32::from_be_bytes(*b"PIL "),
            creator: u32::from_be_bytes(*b"EVO!"),
            finder_flags: 0,
            modified_date: 456,
        };
        let mut existing = HashMap::from([(path.to_string(), downloadable_save(path))]);
        let reused = take_reusable_save_file(
            path,
            &fingerprint,
            &HashMap::from([(path.to_string(), fingerprint.clone())]),
            &HashSet::from([path.to_string()]),
            &mut existing,
        )
        .expect("unchanged persisted save should be reused");

        assert_eq!(reused.path, path);
        assert!(
            existing.is_empty(),
            "reusing by value avoids cloning or re-encoding the MacBinary payload"
        );

        let mut existing = HashMap::from([(path.to_string(), downloadable_save(path))]);
        let changed = SaveFingerprint {
            data_hash: 99,
            ..fingerprint
        };
        assert!(
            take_reusable_save_file(
                path,
                &changed,
                &HashMap::from([(path.to_string(), fingerprint)]),
                &HashSet::from([path.to_string()]),
                &mut existing,
            )
            .is_none(),
            "changed saves must be snapshotted and re-encoded"
        );
        assert!(
            existing.contains_key(path),
            "changed saves should stay available for the caller to replace after snapshotting"
        );
    }

    #[test]
    fn save_state_change_tracks_fingerprints_and_visible_paths() {
        let path = "Pilots/Rick Hardslab";
        let fingerprint = SaveFingerprint {
            data_len: 12,
            resource_len: 34,
            data_hash: 1,
            resource_hash: 2,
            file_type: u32::from_be_bytes(*b"PIL "),
            creator: u32::from_be_bytes(*b"EVO!"),
            finder_flags: 0,
            modified_date: 456,
        };
        let previous_fingerprints = HashMap::from([(path.to_string(), fingerprint.clone())]);
        let previous_paths = HashSet::from([path.to_string()]);

        assert!(!save_state_changed(
            &previous_fingerprints,
            &previous_fingerprints,
            &previous_paths,
            &previous_paths,
        ));

        let changed_fingerprints = HashMap::from([(
            path.to_string(),
            SaveFingerprint {
                data_hash: 99,
                ..fingerprint
            },
        )]);
        assert!(save_state_changed(
            &previous_fingerprints,
            &changed_fingerprints,
            &previous_paths,
            &previous_paths,
        ));
        assert!(save_state_changed(
            &previous_fingerprints,
            &previous_fingerprints,
            &previous_paths,
            &HashSet::new(),
        ));
    }

    fn downloadable_save(path: &str) -> DownloadableSaveFile {
        let snapshot = VfsFileSnapshot {
            path: path.to_string(),
            data_fork: vec![1],
            resource_fork: vec![2],
            file_type: u32::from_be_bytes(*b"PIL "),
            creator: u32::from_be_bytes(*b"EVO!"),
            finder_flags: 0,
            created_date: 123,
            modified_date: 456,
        };
        save_store::downloadable_save_file(&snapshot)
    }

    fn vfs_summary(path: &str) -> VfsFileSummary {
        VfsFileSummary {
            path: path.to_string(),
            data_len: 0,
            resource_len: 0,
            data_hash: 0,
            resource_hash: 0,
            file_type: 0,
            creator: 0,
            finder_flags: 0,
            created_date: 0,
            modified_date: 0,
        }
    }
}

/// Browser audio sink.
///
/// Prefer AudioWorklet so playback keeps running when the page event loop is
/// busy with startup/loading work. Fall back to ScriptProcessor for browsers
/// without a working worklet path.
pub(crate) enum WebAudioBackend {
    Worklet(WorkletAudioBackend),
    ScriptProcessor(ScriptProcessorAudioBackend),
}

pub(crate) struct WorkletAudioBackend {
    ctx: AudioContext,
    node: AudioWorkletNode,
    port: MessagePort,
    dropped_while_suspended: bool,
    estimated_queued_source_samples: usize,
    estimated_queue_clock_ms: f64,
}

pub(crate) struct ScriptProcessorAudioBackend {
    ctx: AudioContext,
    _node: ScriptProcessorNode,
    state: Rc<RefCell<AudioQueue>>,
    _on_audio_process: Closure<dyn FnMut(AudioProcessingEvent)>,
    dropped_while_suspended: bool,
}

struct AudioBootstrap {
    ctx: Option<AudioContext>,
    resume: Option<js_sys::Promise>,
    worklet_module: Option<js_sys::Promise>,
}

impl AudioBootstrap {
    fn new(resume_from_gesture: bool) -> Option<Self> {
        let ctx = create_audio_context()?;
        // Must run from a trusted user gesture. The audio sink can be attached
        // later, after archive fetch and boot, while preserving the already
        // unlocked context.
        let resume = if resume_from_gesture {
            ctx.resume().ok()
        } else {
            None
        };
        let worklet_module = ctx.audio_worklet().ok().and_then(|worklet| {
            worklet
                .add_module(&asset_path("/assets/audio-worklet.js"))
                .ok()
        });
        Some(Self {
            ctx: Some(ctx),
            resume,
            worklet_module,
        })
    }

    fn resume_from_user_gesture(&mut self) {
        if self.resume.is_none() {
            self.resume = self.ctx.as_ref().and_then(|ctx| ctx.resume().ok());
        }
    }

    async fn finish(mut self) -> Option<WebAudioBackend> {
        if let Some(resume) = self.resume.take() {
            let _ = JsFuture::from(resume).await;
        }
        WebAudioBackend::from_context_with_worklet_module(
            self.ctx.take()?,
            self.worklet_module.take(),
        )
        .await
    }
}

impl Drop for AudioBootstrap {
    fn drop(&mut self) {
        if let Some(ctx) = self.ctx.take() {
            let _ = ctx.close();
        }
    }
}

impl WebAudioBackend {
    pub(crate) async fn new() -> Option<Self> {
        let ctx = create_audio_context()?;
        // Context starts suspended outside a user-gesture microtask; kick
        // resume fire-and-forget — by the time playback begins (after the
        // prefill) it will have settled.
        let _ = ctx.resume();
        Self::from_context(ctx).await
    }

    async fn from_context(ctx: AudioContext) -> Option<Self> {
        Self::from_context_with_worklet_module(ctx, None).await
    }

    async fn from_context_with_worklet_module(
        ctx: AudioContext,
        worklet_module: Option<js_sys::Promise>,
    ) -> Option<Self> {
        if let Some(worklet) = WorkletAudioBackend::from_context(ctx.clone(), worklet_module).await
        {
            return Some(Self::Worklet(worklet));
        }
        ScriptProcessorAudioBackend::from_context(ctx)
            .await
            .map(Self::ScriptProcessor)
    }

    pub(crate) fn queue_samples(&mut self, samples: &[u8]) {
        match self {
            Self::Worklet(audio) => audio.queue_samples(samples),
            Self::ScriptProcessor(audio) => audio.queue_samples(samples),
        }
    }

    pub(crate) fn resume(&mut self) {
        match self {
            Self::Worklet(audio) => audio.resume(),
            Self::ScriptProcessor(audio) => audio.resume(),
        }
    }

    fn uses_worklet(&self) -> bool {
        matches!(self, Self::Worklet(_))
    }

    pub(crate) fn queued_source_samples(&mut self) -> Option<usize> {
        match self {
            Self::Worklet(audio) => audio.queued_source_samples(),
            Self::ScriptProcessor(audio) => audio.queued_source_samples(),
        }
    }
}

impl WorkletAudioBackend {
    async fn from_context(
        ctx: AudioContext,
        worklet_module: Option<js_sys::Promise>,
    ) -> Option<Self> {
        if let Some(module) = worklet_module {
            JsFuture::from(module).await.ok()?;
        } else {
            let worklet = ctx.audio_worklet().ok()?;
            let module_url = asset_path("/assets/audio-worklet.js");
            JsFuture::from(worklet.add_module(&module_url).ok()?)
                .await
                .ok()?;
        }
        let node = AudioWorkletNode::new(&ctx, "systemless-audio").ok()?;
        let port = node.port().ok()?;
        port.start();
        node.connect_with_audio_node(&ctx.destination()).ok()?;

        let mut backend = Self {
            ctx,
            node,
            port,
            dropped_while_suspended: false,
            estimated_queued_source_samples: 0,
            estimated_queue_clock_ms: performance_now(),
        };
        backend.prefill_silence();
        Some(backend)
    }

    fn prepare_queue(&mut self) -> bool {
        self.ensure_running_queue_ready()
    }

    fn ensure_running_queue_ready(&mut self) -> bool {
        if self.ctx.state() != AudioContextState::Running {
            self.dropped_while_suspended = true;
            return false;
        }
        if self.dropped_while_suspended {
            self.clear_queue();
            self.prefill_silence();
            self.dropped_while_suspended = false;
        }
        true
    }

    fn queue_samples(&mut self, samples: &[u8]) {
        if samples.is_empty() || !self.prepare_queue() {
            return;
        }

        self.post_samples(samples);
        self.record_queued_samples(samples.len());
    }

    fn resume(&mut self) {
        let should_reset =
            self.dropped_while_suspended || self.ctx.state() != AudioContextState::Running;
        let _ = self.ctx.resume();
        if should_reset {
            self.clear_queue();
            self.prefill_silence();
            self.dropped_while_suspended = false;
        }
    }

    fn clear_queue(&mut self) {
        let message = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &message,
            &JsValue::from_str("type"),
            &JsValue::from_str("clear"),
        );
        let _ = self.port.post_message(&JsValue::from(message));
        self.estimated_queued_source_samples = 0;
        self.estimated_queue_clock_ms = performance_now();
    }

    fn prefill_silence(&mut self) {
        let prefill_len = audio_prefill_samples();
        self.post_samples(&vec![0x80; prefill_len]);
        self.record_queued_samples(prefill_len);
    }

    fn post_samples(&self, samples: &[u8]) {
        let payload: JsValue = js_sys::Uint8Array::from(samples).into();
        let _ = self.port.post_message(&payload);
    }

    fn queued_source_samples(&mut self) -> Option<usize> {
        if !self.ensure_running_queue_ready() {
            return None;
        }
        self.refresh_estimated_queue();
        Some(self.estimated_queued_source_samples)
    }

    fn record_queued_samples(&mut self, samples: usize) {
        self.refresh_estimated_queue();
        self.estimated_queued_source_samples = self
            .estimated_queued_source_samples
            .saturating_add(samples)
            .min(MAX_AUDIO_QUEUE_SAMPLES);
    }

    fn refresh_estimated_queue(&mut self) {
        let now = performance_now();
        let elapsed_ms = (now - self.estimated_queue_clock_ms).max(0.0);
        let consumed = (elapsed_ms * OUTPUT_RATE as f64 / 1000.0) as usize;
        self.estimated_queued_source_samples = self
            .estimated_queued_source_samples
            .saturating_sub(consumed);
        self.estimated_queue_clock_ms = now;
    }
}

impl ScriptProcessorAudioBackend {
    async fn from_context(ctx: AudioContext) -> Option<Self> {
        let state = Rc::new(RefCell::new(AudioQueue::new(ctx.sample_rate() as f64)));
        state.borrow_mut().prefill_silence();

        let node = ctx
            .create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
                AUDIO_BUFFER_SIZE,
                0,
                1,
            )
            .ok()?;
        let state_for_audio = state.clone();
        let on_audio_process = Closure::wrap(Box::new(move |ev: AudioProcessingEvent| {
            let Ok(buffer) = ev.output_buffer() else {
                return;
            };
            let mut state = state_for_audio.borrow_mut();
            let samples = state.render(buffer.length() as usize);
            let _ = buffer.copy_to_channel(samples, 0);
        }) as Box<dyn FnMut(AudioProcessingEvent)>);
        node.set_onaudioprocess(Some(on_audio_process.as_ref().unchecked_ref()));
        node.connect_with_audio_node(&ctx.destination()).ok()?;

        Some(Self {
            ctx,
            _node: node,
            state,
            _on_audio_process: on_audio_process,
            dropped_while_suspended: false,
        })
    }

    fn prepare_queue(&mut self) -> bool {
        self.ensure_running_queue_ready()
    }

    fn ensure_running_queue_ready(&mut self) -> bool {
        // Chrome commonly creates/resumes AudioContext outside a trusted
        // user gesture as "suspended". Do not queue audio while suspended:
        // otherwise the browser sink can play stale samples after the first click
        // finally unlocks audio.
        if self.ctx.state() != AudioContextState::Running {
            self.dropped_while_suspended = true;
            return false;
        }
        if self.dropped_while_suspended {
            let mut state = self.state.borrow_mut();
            state.clear();
            state.prefill_silence();
            self.dropped_while_suspended = false;
        }
        true
    }

    fn queue_samples(&mut self, samples: &[u8]) {
        if samples.is_empty() || !self.prepare_queue() {
            return;
        }

        // Systemless emits mono u8 (silence = 0x80). Keep compact PCM queued
        // until WebAudio pulls the next output block.
        self.state.borrow_mut().queue_samples(samples);
    }

    fn resume(&mut self) {
        let should_reset =
            self.dropped_while_suspended || self.ctx.state() != AudioContextState::Running;
        let _ = self.ctx.resume();
        if should_reset {
            let mut state = self.state.borrow_mut();
            state.clear();
            state.prefill_silence();
            self.dropped_while_suspended = false;
        }
    }

    fn queued_source_samples(&mut self) -> Option<usize> {
        if !self.ensure_running_queue_ready() {
            return None;
        }
        Some(self.state.borrow().queued_source_samples())
    }
}

fn create_audio_context() -> Option<AudioContext> {
    // This is a game frontend. Ask WebAudio for the low-latency path; the
    // explicit prefill and queue cap below handle ordinary jitter.
    let opts = AudioContextOptions::new();
    opts.set_latency_hint_audio_context_latency_category(AudioContextLatencyCategory::Interactive);
    AudioContext::new_with_context_options(&opts).ok()
}

impl Drop for WorkletAudioBackend {
    fn drop(&mut self) {
        self.port.close();
        let _ = self.node.disconnect();
        let _ = self.ctx.close();
    }
}

impl Drop for ScriptProcessorAudioBackend {
    fn drop(&mut self) {
        self._node.set_onaudioprocess(None);
        let _ = self._node.disconnect();
        let _ = self.ctx.close();
    }
}

struct AudioQueue {
    chunks: VecDeque<Vec<u8>>,
    idx: usize,
    phase: f64,
    step: f64,
    queued_samples: usize,
    max_queued_source_samples: usize,
    scratch: Vec<f32>,
}

impl AudioQueue {
    fn new(output_rate: f64) -> Self {
        Self {
            chunks: VecDeque::new(),
            idx: 0,
            phase: 0.0,
            step: OUTPUT_RATE as f64 / output_rate.max(1.0),
            queued_samples: 0,
            max_queued_source_samples: MAX_AUDIO_QUEUE_SAMPLES,
            scratch: Vec::new(),
        }
    }

    fn prefill_silence(&mut self) {
        let prefill_len = audio_prefill_samples();
        self.queue_samples(&vec![0x80; prefill_len]);
    }

    fn clear(&mut self) {
        self.chunks.clear();
        self.idx = 0;
        self.phase = 0.0;
        self.queued_samples = 0;
    }

    fn queue_samples(&mut self, samples: &[u8]) {
        if samples.is_empty() {
            return;
        }
        self.queued_samples = self.queued_samples.saturating_add(samples.len());
        self.chunks.push_back(samples.to_vec());
        self.trim_queue();
    }

    fn queued_source_samples(&self) -> usize {
        self.queued_samples
    }

    fn render(&mut self, len: usize) -> &[f32] {
        if self.scratch.len() != len {
            self.scratch.resize(len, 0.0);
        }
        for i in 0..len {
            self.scratch[i] = self.next_sample();
        }
        &self.scratch
    }

    fn next_sample(&mut self) -> f32 {
        let Some(chunk) = self.chunks.front() else {
            return 0.0;
        };
        let a_sample = chunk.get(self.idx).copied().unwrap_or(0x80);
        let a = u8_pcm_to_f32(a_sample);
        let sample = if self.step < 1.0 {
            // Match the native backend: host-rate upsampling holds the 22 kHz
            // Sound Manager stream instead of smoothing classic effect edges.
            a
        } else {
            let b = chunk
                .get(self.idx + 1)
                .copied()
                .or_else(|| self.chunks.get(1).and_then(|next| next.first().copied()))
                .unwrap_or(a_sample);
            let b = u8_pcm_to_f32(b);
            a + (b - a) * self.phase as f32
        };

        self.phase += self.step;
        while self.phase >= 1.0 {
            self.advance_source_sample();
            self.phase -= 1.0;
            if self.chunks.is_empty() {
                self.phase = 0.0;
                break;
            }
        }
        sample
    }

    fn advance_source_sample(&mut self) {
        let Some(front_len) = self.chunks.front().map(Vec::len) else {
            self.idx = 0;
            return;
        };
        self.idx += 1;
        self.queued_samples = self.queued_samples.saturating_sub(1);
        if self.idx >= front_len {
            self.chunks.pop_front();
            self.idx = 0;
        }
    }

    fn trim_queue(&mut self) {
        while self.queued_samples > self.max_queued_source_samples && !self.chunks.is_empty() {
            let overflow = self.queued_samples - self.max_queued_source_samples;
            let Some(front_len) = self.chunks.front().map(Vec::len) else {
                break;
            };
            let remaining_in_first_chunk = front_len.saturating_sub(self.idx);
            if overflow >= remaining_in_first_chunk {
                self.chunks.pop_front();
                self.queued_samples = self.queued_samples.saturating_sub(remaining_in_first_chunk);
                self.idx = 0;
            } else {
                self.idx += overflow;
                self.queued_samples -= overflow;
            }
            self.phase = 0.0;
        }
    }
}

fn u8_pcm_to_f32(sample: u8) -> f32 {
    (sample as f32 - 128.0) / 128.0
}
