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
}

#[wasm_bindgen]
pub struct WorkerMachine {
    machine: Machine,
    painted_once: bool,
    gpu_renderer_enabled: bool,
}

#[wasm_bindgen]
impl WorkerMachine {
    #[wasm_bindgen(js_name = create)]
    pub async fn create(game_bytes: Uint8Array, config: &str) -> Result<WorkerMachine, JsValue> {
        let config: BootConfig =
            serde_json::from_str(config).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let architecture = match config.architecture.as_str() {
            "68k" => GameArchitecture::M68k,
            "ppc" => GameArchitecture::PowerPc,
            _ => return Err(JsValue::from_str("Unsupported worker architecture")),
        };
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
            &[] as &[PluginFile],
            architecture,
            &config.launch_modifiers,
            config.show_menu_bar,
            config.screen_depth,
            config.application_partition_size,
            &paths,
            &mappings,
            config.runtime_pacing,
            |_| {},
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
    ) -> Object {
        self.machine.set_output_scale(output_scale);
        self.machine
            .set_external_q3_renderer_enabled(self.gpu_renderer_enabled && !debug);
        self.machine.set_worker_audio_queue_samples(
            (queued_audio_samples >= 0).then_some(queued_audio_samples as usize),
        );
        let frame_result = self.machine.run_frame();
        let gpu_frame = self.machine.take_q3_gpu_frame();
        let counters = self.machine.perf_counters();
        let logical_size = self.machine.screen_size();
        let should_render =
            gpu_frame.is_none() && (frame_result.visual_work || !self.painted_once || debug);
        let frame = should_render.then(|| {
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
            Uint8Array::from(self.machine.render_rgba(stats).1)
        });
        let (width, height) = if frame.is_some() {
            self.machine.presented_size()
        } else {
            logical_size
        };
        let audio = Uint8Array::from(self.machine.take_worker_audio().as_slice());

        let result = Object::new();
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
    pub fn import_save(&mut self, bytes: Uint8Array) -> Result<Array, JsValue> {
        self.machine
            .import_save_file_bytes(&bytes.to_vec())
            .map_err(|error| JsValue::from_str(&error))?;
        Ok(self.save_files())
    }

    #[wasm_bindgen(js_name = deleteSave)]
    pub fn delete_save(&mut self, path: &str) -> Result<Array, JsValue> {
        self.machine
            .delete_save_file(path)
            .map_err(|error| JsValue::from_str(&error))?;
        Ok(self.save_files())
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
