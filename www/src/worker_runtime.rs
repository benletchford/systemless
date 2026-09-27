use js_sys::{Array, Float32Array, Object, Reflect, Uint8Array};
use wasm_bindgen::{prelude::wasm_bindgen, JsValue};

use crate::catalogue::{GameArchitecture, LaunchModifier, RuntimePacing};
use crate::emulator::{Machine, PluginFile};
use systemless::debug_overlay::DebugOverlayFrameStats;

#[derive(serde::Deserialize)]
struct BootConfig {
    id: String,
    architecture: String,
    launch_modifiers: Vec<LaunchModifier>,
    show_menu_bar: bool,
    screen_depth: Option<u16>,
    application_partition_size: Option<u32>,
    remove_paths: Vec<String>,
    #[serde(default)]
    file_mappings: Vec<(String, String)>,
    runtime_pacing: RuntimePacing,
    arrows_as_numpad: bool,
    #[serde(default)]
    plugins: Vec<PluginMetadata>,
}

// Fork bytes travel separately in transfer lists. Keep every Finder field and
// the mount path together; the owner uses the same import path as compatibility mode.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct PluginMetadata {
    mount_path: String,
    path: String,
    file_type: u32,
    creator: u32,
    finder_flags: u16,
    created_date: u32,
    modified_date: u32,
}

impl From<&PluginFile> for PluginMetadata {
    fn from(plugin: &PluginFile) -> Self {
        let file = &plugin.file;
        Self {
            mount_path: plugin.mount_path.clone(),
            path: file.path.clone(),
            file_type: file.file_type,
            creator: file.creator,
            finder_flags: file.finder_flags,
            created_date: file.created_date,
            modified_date: file.modified_date,
        }
    }
}

impl PluginMetadata {
    fn with_forks(self, data_fork: Vec<u8>, resource_fork: Vec<u8>) -> PluginFile {
        PluginFile {
            mount_path: self.mount_path,
            file: systemless::runner::VfsFileSnapshot {
                path: self.path,
                data_fork,
                resource_fork,
                file_type: self.file_type,
                creator: self.creator,
                finder_flags: self.finder_flags,
                created_date: self.created_date,
                modified_date: self.modified_date,
            },
        }
    }
}

#[wasm_bindgen]
pub struct WorkerMachine {
    machine: Machine,
    painted_once: bool,
    gpu_renderer_enabled: bool,
}

#[wasm_bindgen]
impl WorkerMachine {
    #[wasm_bindgen(js_name = runtimeProtocolVersion)]
    pub fn runtime_protocol_version() -> u32 {
        7
    }

    #[wasm_bindgen(js_name = create)]
    pub async fn create(
        game_bytes: Uint8Array,
        config: &str,
        plugin_forks: Array,
        on_progress: js_sys::Function,
    ) -> Result<WorkerMachine, JsValue> {
        let config: BootConfig =
            serde_json::from_str(config).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let architecture = match config.architecture.as_str() {
            "68k" => GameArchitecture::M68k,
            "ppc" => GameArchitecture::PowerPc,
            _ => return Err(JsValue::from_str("Unsupported worker architecture")),
        };
        if plugin_forks.length() as usize != config.plugins.len() * 2 {
            return Err(JsValue::from_str(
                "Worker plugin fork count does not match metadata",
            ));
        }
        let plugins: Vec<PluginFile> = config
            .plugins
            .into_iter()
            .enumerate()
            .map(|(index, metadata)| {
                metadata.with_forks(
                    Uint8Array::new(&plugin_forks.get(index as u32 * 2)).to_vec(),
                    Uint8Array::new(&plugin_forks.get(index as u32 * 2 + 1)).to_vec(),
                )
            })
            .collect();
        let bytes = game_bytes.to_vec();
        let paths: Vec<&str> = config.remove_paths.iter().map(String::as_str).collect();
        let mappings: Vec<(&str, &str)> = config
            .file_mappings
            .iter()
            .map(|(source, destination)| (source.as_str(), destination.as_str()))
            .collect();
        let mut machine = Machine::new_with_progress(
            &config.id,
            &bytes,
            &plugins,
            architecture,
            &config.launch_modifiers,
            config.show_menu_bar,
            config.screen_depth,
            config.application_partition_size,
            &paths,
            &mappings,
            config.runtime_pacing,
            &std::cell::RefCell::new(None),
            |progress| {
                if let Ok(progress) = serde_json::to_string(&progress) {
                    let _ = on_progress.call1(&JsValue::UNDEFINED, &JsValue::from_str(&progress));
                }
            },
        )
        .await
        .map_err(|error| JsValue::from_str(&error))?;
        machine.set_arrows_as_numpad(config.arrows_as_numpad);
        machine.set_worker_audio_queue_samples(Some(0));
        Ok(Self {
            machine,
            painted_once: false,
            gpu_renderer_enabled: false,
        })
    }

    #[wasm_bindgen(js_name = runFrame)]
    pub fn run_frame(
        &mut self,
        queued_audio_samples: i32,
        debug: bool,
        output_scale: u32,
        force_render: bool,
        indexed_render: bool,
        compact_render: bool,
        measure_presentation: bool,
    ) -> Object {
        let started = measured_now(measure_presentation);
        self.machine.set_output_scale(output_scale);
        self.machine
            .set_external_q3_renderer_enabled(self.gpu_renderer_enabled && !debug);
        self.machine.set_worker_audio_queue_samples(
            (queued_audio_samples >= 0).then_some(queued_audio_samples as usize),
        );
        let frame_result = self.machine.run_frame();
        let executed = measured_now(measure_presentation);
        let mut js_copy_ms = 0.0;
        let gpu_frame = self.machine.take_q3_gpu_frame();
        let counters = self.machine.perf_counters();
        let logical_size = self.machine.screen_size();
        let should_render = gpu_frame.is_none()
            && (frame_result.visual_work || !self.painted_once || debug || force_render);
        let output_scale = output_scale.clamp(1, 4);
        let compact_frame = if should_render && compact_render && !debug {
            self.machine.render_compact().map(|frame| {
                let before_copy = measured_now(measure_presentation);
                let packet = compact_frame_object(frame, output_scale);
                js_copy_ms += measured_now(measure_presentation) - before_copy;
                packet
            })
        } else {
            None
        };
        let indexed_frame = if should_render && compact_frame.is_none() && indexed_render && !debug
        {
            self.machine.render_indexed().map(|frame| {
                let before_copy = measured_now(measure_presentation);
                let packet = indexed_frame_object(frame);
                js_copy_ms += measured_now(measure_presentation) - before_copy;
                packet
            })
        } else {
            None
        };
        let frame =
            (should_render && indexed_frame.is_none() && compact_frame.is_none()).then(|| {
                self.painted_once = true;
                let stats = debug.then_some(DebugOverlayFrameStats {
                    host_fps: None,
                    frame_ms: None,
                    guest_mips: None,
                    guest_ticks_per_sec: None,
                    ticks_behind: Some(counters.ticks_behind),
                    last_steps: Some(counters.last_steps),
                    cpu_budget_ms: Some(counters.cpu_budget_ms),
                    audio_queue_ms: counters.audio_queue_ms,
                });
                let pixels = self.machine.render_rgba(stats).1;
                let before_copy = measured_now(measure_presentation);
                let packet = Uint8Array::from(pixels);
                js_copy_ms += measured_now(measure_presentation) - before_copy;
                packet
            });
        let prepared = measured_now(measure_presentation);
        if indexed_frame.is_some() || compact_frame.is_some() {
            self.painted_once = true;
        }
        let (width, height) =
            if frame.is_some() || indexed_frame.is_some() || compact_frame.is_some() {
                self.machine.presented_size()
            } else {
                logical_size
            };
        let audio = Uint8Array::from(self.machine.take_worker_audio().as_slice());

        let result = Object::new();
        // Diagnostics use only this owner's clock. Snapshot includes presentation
        // bookkeeping and native export/conversion; copy includes JS packet
        // construction. Audio, saves and incremental QD3D are outside this scope.
        if measure_presentation {
            let metrics = Object::new();
            set_number(&metrics, "guestMs", (executed - started).max(0.0));
            set_number(
                &metrics,
                "snapshotMs",
                (prepared - executed - js_copy_ms).max(0.0),
            );
            set_number(&metrics, "jsCopyMs", js_copy_ms.max(0.0));
            set_bool(
                &metrics,
                "completeImage",
                frame.is_some() || indexed_frame.is_some() || compact_frame.is_some(),
            );
            let _ = Reflect::set(&result, &JsValue::from_str("presentationMetrics"), &metrics);
        }
        if let Some(error) = self.machine.take_save_error() {
            set_string(&result, "saveError", &error);
        }
        set_bool(&result, "running", frame_result.running);
        set_bool(&result, "uiTracking", self.machine.is_ui_tracking_active());
        set_bool(&result, "visualWork", frame_result.visual_work);
        set_number(
            &result,
            "outputScale",
            (width / logical_size.0.max(1)) as f64,
        );
        set_number(&result, "width", width as f64);
        set_number(&result, "height", height as f64);
        set_number(&result, "guestTick", counters.guest_tick as f64);
        set_number(
            &result,
            "totalInstructions",
            counters.total_instructions as f64,
        );
        set_number(&result, "ticksBehind", counters.ticks_behind as f64);
        set_number(&result, "lastSteps", counters.last_steps as f64);
        set_number(&result, "cpuBudgetMs", counters.cpu_budget_ms);
        if let Some(queue_ms) = counters.audio_queue_ms {
            set_number(&result, "audioQueueMs", queue_ms);
        }
        let _ = Reflect::set(result.as_ref(), &JsValue::from_str("audio"), audio.as_ref());
        if let Some(frame) = frame {
            let _ = Reflect::set(result.as_ref(), &JsValue::from_str("frame"), frame.as_ref());
        }
        if let Some(frame) = compact_frame {
            let _ = Reflect::set(
                result.as_ref(),
                &JsValue::from_str("compactFrame"),
                frame.as_ref(),
            );
        }
        if let Some(frame) = indexed_frame {
            let _ = Reflect::set(
                result.as_ref(),
                &JsValue::from_str("indexedFrame"),
                frame.as_ref(),
            );
        }
        if let Some(frame) = gpu_frame {
            self.painted_once = true;
            let packet = q3_gpu_frame_object(frame);
            let _ = Reflect::set(
                result.as_ref(),
                &JsValue::from_str("gpuFrame"),
                packet.as_ref(),
            );
        }
        result
    }

    #[wasm_bindgen(js_name = setGpuRendererEnabled)]
    pub fn set_gpu_renderer_enabled(&mut self, enabled: bool) {
        self.gpu_renderer_enabled = enabled;
        self.machine.set_external_q3_renderer_enabled(enabled);
    }

    #[wasm_bindgen(js_name = mouseDown)]
    pub fn mouse_down(&mut self, vertical: i16, horizontal: i16) {
        self.machine.mouse_down(vertical, horizontal);
    }

    #[wasm_bindgen(js_name = mouseUp)]
    pub fn mouse_up(&mut self, vertical: i16, horizontal: i16) {
        self.machine.mouse_up(vertical, horizontal);
    }

    #[wasm_bindgen(js_name = mouseMove)]
    pub fn mouse_move(&mut self, vertical: i16, horizontal: i16) {
        self.machine.mouse_move(vertical, horizontal);
    }

    #[wasm_bindgen(js_name = keyDown)]
    pub fn key_down(&mut self, mac_key: u8, char_code: u8) {
        self.machine.key_down(mac_key, char_code);
    }

    #[wasm_bindgen(js_name = keyUp)]
    pub fn key_up(&mut self, mac_key: u8, char_code: u8) {
        self.machine.key_up(mac_key, char_code);
    }

    #[wasm_bindgen(js_name = importSave)]
    pub fn import_save(&mut self, bytes: Uint8Array) -> Result<(), JsValue> {
        self.machine
            .import_save_file_bytes(&bytes.to_vec())
            .map_err(|error| JsValue::from_str(&error))?;
        Ok(())
    }

    #[wasm_bindgen(js_name = deleteSave)]
    pub fn delete_save(&mut self, path: &str) -> Result<(), JsValue> {
        self.machine
            .delete_save_file(path)
            .map_err(|error| JsValue::from_str(&error))?;
        Ok(())
    }

    #[wasm_bindgen(js_name = flushSaves)]
    pub async fn flush_saves(&mut self) -> Result<(), JsValue> {
        let id = self.machine.flush_save_files();
        crate::save_store::flush_pending_saves(&id)
            .await
            .map_err(|error| JsValue::from_str(&error))
    }

    #[wasm_bindgen(js_name = saveFilesVersion)]
    pub fn save_files_version(&self) -> String {
        self.machine.save_files_version().to_string()
    }

    #[wasm_bindgen(js_name = saveFiles)]
    pub fn save_files(&self) -> Array {
        let files = Array::new();
        for file in self.machine.save_files() {
            let value = Object::new();
            set_string(&value, "path", &file.path);
            set_string(&value, "name", &file.name);
            set_number(&value, "dataLen", file.data_len as f64);
            set_number(&value, "resourceLen", file.resource_len as f64);
            set_number(&value, "modifiedDate", file.modified_date as f64);
            let macbinary = Uint8Array::from(file.macbinary.as_slice());
            let _ = Reflect::set(
                value.as_ref(),
                &JsValue::from_str("macbinary"),
                macbinary.as_ref(),
            );
            files.push(value.as_ref());
        }
        files
    }
}

fn q3_gpu_frame_object(frame: systemless::loader::ppc::PpcQ3GpuFrame) -> Object {
    let result = Object::new();
    set_number(&result, "width", frame.width as f64);
    set_number(&result, "height", frame.height as f64);
    let viewport = Float32Array::from(frame.viewport.map(|value| value as f32).as_slice());
    let _ = Reflect::set(
        result.as_ref(),
        &JsValue::from_str("viewport"),
        viewport.as_ref(),
    );
    if let Some(clear) = frame.clear_color {
        let clear = Float32Array::from(clear.as_slice());
        let _ = Reflect::set(
            result.as_ref(),
            &JsValue::from_str("clearColor"),
            clear.as_ref(),
        );
    }

    let textures = Array::new();
    for texture in frame.textures {
        let value = Object::new();
        set_number(&value, "width", texture.width as f64);
        set_number(&value, "height", texture.height as f64);
        set_bool(&value, "wrapU", texture.wrap_u);
        set_bool(&value, "wrapV", texture.wrap_v);
        let rgba = Uint8Array::from(texture.rgba.as_slice());
        let _ = Reflect::set(value.as_ref(), &JsValue::from_str("rgba"), rgba.as_ref());
        textures.push(value.as_ref());
    }
    let _ = Reflect::set(
        result.as_ref(),
        &JsValue::from_str("textures"),
        textures.as_ref(),
    );

    let draws = Array::new();
    for draw in frame.draws {
        let value = Object::new();
        set_number(
            &value,
            "texture",
            draw.texture.map(|index| index as f64).unwrap_or(-1.0),
        );
        set_bool(&value, "blend", draw.blend);
        set_bool(&value, "writeDepth", draw.write_depth);
        let mut vertices = Vec::with_capacity(draw.vertices.len().saturating_mul(10));
        for vertex in draw.vertices {
            vertices.extend([
                vertex.screen_x,
                vertex.screen_y,
                vertex.depth,
                vertex.reciprocal_w,
                vertex.uv[0],
                vertex.uv[1],
                vertex.color[0],
                vertex.color[1],
                vertex.color[2],
                vertex.color[3],
            ]);
        }
        let vertices = Float32Array::from(vertices.as_slice());
        let _ = Reflect::set(
            value.as_ref(),
            &JsValue::from_str("vertices"),
            vertices.as_ref(),
        );
        draws.push(value.as_ref());
    }
    let _ = Reflect::set(result.as_ref(), &JsValue::from_str("draws"), draws.as_ref());
    result
}

fn set_number(object: &Object, property: &str, value: f64) {
    let _ = Reflect::set(
        object.as_ref(),
        &JsValue::from_str(property),
        &JsValue::from_f64(value),
    );
}

fn set_bool(object: &Object, property: &str, value: bool) {
    let _ = Reflect::set(
        object.as_ref(),
        &JsValue::from_str(property),
        &JsValue::from_bool(value),
    );
}

fn set_string(object: &Object, property: &str, value: &str) {
    let _ = Reflect::set(
        object.as_ref(),
        &JsValue::from_str(property),
        &JsValue::from_str(value),
    );
}

#[cfg(test)]
mod plugin_tests {
    use super::*;

    #[test]
    fn plugin_transport_preserves_both_forks_mount_path_and_finder_metadata() {
        let plugin = PluginFile {
            mount_path: "Plug-ins".into(),
            file: systemless::runner::VfsFileSnapshot {
                path: "Original Plugin".into(),
                data_fork: vec![0, 1, 128, 255],
                resource_fork: vec![255, 0, 2, 127],
                file_type: u32::from_be_bytes(*b"rsrc"),
                creator: u32::from_be_bytes(*b"TEST"),
                finder_flags: 0x8401,
                created_date: 123456789,
                modified_date: 234567890,
            },
        };
        let encoded = serde_json::to_string(&PluginMetadata::from(&plugin)).unwrap();
        let metadata: PluginMetadata = serde_json::from_str(&encoded).unwrap();
        let received = metadata.with_forks(
            plugin.file.data_fork.clone(),
            plugin.file.resource_fork.clone(),
        );
        assert_eq!(received.mount_path, plugin.mount_path);
        assert_eq!(received.file.path, plugin.file.path);
        assert_eq!(received.file.data_fork, plugin.file.data_fork);
        assert_eq!(received.file.resource_fork, plugin.file.resource_fork);
        assert_eq!(received.file.file_type, plugin.file.file_type);
        assert_eq!(received.file.creator, plugin.file.creator);
        assert_eq!(received.file.finder_flags, plugin.file.finder_flags);
        assert_eq!(received.file.created_date, plugin.file.created_date);
        assert_eq!(received.file.modified_date, plugin.file.modified_date);
    }
}

fn indexed_frame_object(frame: &crate::indexed_frame::IndexedFrame) -> Object {
    let result = Object::new();
    set_string(&result, "kind", "indexed8");
    set_bool(&result, "complete", true);
    set_number(&result, "width", f64::from(frame.screen.screen_mode.2));
    set_number(&result, "height", f64::from(frame.screen.screen_mode.3));
    set_number(&result, "stride", f64::from(frame.screen.screen_mode.1));
    let _ = Reflect::set(
        &result,
        &JsValue::from_str("pixels"),
        &Uint8Array::from(frame.screen.pixels.as_slice()),
    );
    let _ = Reflect::set(
        &result,
        &JsValue::from_str("palette"),
        &Uint8Array::from(frame.palette.as_slice()),
    );
    if let Some(cursor) = &frame.cursor {
        let patch = Object::new();
        for (name, value) in [
            ("x", cursor.x),
            ("y", cursor.y),
            ("width", cursor.width),
            ("height", cursor.height),
        ] {
            set_number(&patch, name, f64::from(value));
        }
        let _ = Reflect::set(
            &patch,
            &JsValue::from_str("pixels"),
            &Uint8Array::from(cursor.pixels.as_slice()),
        );
        let _ = Reflect::set(&result, &JsValue::from_str("cursor"), &patch);
    }
    result
}

fn compact_frame_object(
    frame: &systemless::memory::CompactPresentation,
    output_scale: u32,
) -> Object {
    let packet = Object::new();
    set_string(&packet, "kind", "compact");
    set_bool(&packet, "complete", true);
    set_number(&packet, "width", f64::from(frame.width * output_scale));
    set_number(&packet, "height", f64::from(frame.height * output_scale));
    let compact = Object::new();
    set_number(&compact, "width", f64::from(frame.width));
    set_number(&compact, "height", f64::from(frame.height));
    set_number(&compact, "scale", f64::from(frame.scale));
    let _ = Reflect::set(
        &compact,
        &JsValue::from_str("cells"),
        &js_sys::Uint32Array::from(frame.cells.as_slice()),
    );
    let _ = Reflect::set(
        &compact,
        &JsValue::from_str("detail"),
        &js_sys::Uint32Array::from(frame.detail.as_slice()),
    );
    let _ = Reflect::set(&packet, &JsValue::from_str("compact"), &compact);
    packet
}

fn measured_now(enabled: bool) -> f64 {
    if enabled {
        crate::emulator::performance_now()
    } else {
        0.0
    }
}
