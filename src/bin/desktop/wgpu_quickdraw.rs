//! Guest-visible QuickDraw rasterization, enabled only by --gpu.
use systemless::quickdraw::gpu::{QuickDrawGpuCommand, QuickDrawGpuPixel, QuickDrawGpuRenderer};
use wgpu::util::DeviceExt;

pub(super) struct QuickDrawRenderer {
    state: Option<State>,
    initialization_error: Option<String>,
    last_error: Option<String>,
}
impl QuickDrawRenderer {
    pub fn new() -> Self {
        Self {
            state: None,
            initialization_error: None,
            last_error: None,
        }
    }
}
impl QuickDrawGpuRenderer for QuickDrawRenderer {
    fn render(
        &mut self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        if self.state.is_none() && self.initialization_error.is_none() {
            match State::new() {
                Ok(state) => self.state = Some(state),
                Err(error) => self.initialization_error = Some(error),
            }
        }
        let result = match self.state.as_ref() {
            Some(state) => state.render(command, background),
            None => Err(self.initialization_error.clone().unwrap()),
        };
        if let Err(error) = &result {
            if self.last_error.as_ref() != Some(error) {
                eprintln!("[GPU] QuickDraw CPU fallback: {error}");
                self.last_error = Some(error.clone());
            }
        }
        result
    }
}
struct State {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
}
impl State {
    fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .map_err(|e| e.to_string())?;
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
            .map_err(|e| e.to_string())?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("QuickDraw rectangle"),
            source: wgpu::ShaderSource::Wgsl(include_str!("wgpu_quickdraw.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("QuickDraw rasterizer"),
            layout: None,
            module: &shader,
            entry_point: Some("raster"),
            compilation_options: Default::default(),
            cache: None,
        });
        eprintln!("[GPU] QuickDraw rasterization: {}", adapter.get_info().name);
        Ok(Self {
            device,
            queue,
            pipeline,
        })
    }
    fn render(
        &self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        let internal = self.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = self.render_inner(command, background);
        let mut error = None;
        for scope in [validation, memory, internal] {
            if let Some(e) = pollster::block_on(scope.pop()) {
                error = Some(e.to_string());
            }
        }
        match error {
            Some(error) => Err(error),
            None => result,
        }
    }
    fn render_inner(
        &self,
        command: &QuickDrawGpuCommand,
        background: &[u32],
    ) -> Result<Vec<QuickDrawGpuPixel>, String> {
        let count = command.pixel_count().ok_or("invalid QuickDraw area")?;
        let width = (command.area[2] - command.area[0]) as u32;
        let height = (command.area[3] - command.area[1]) as u32;
        let limits = self.device.limits();
        let output_bytes = (count as u64)
            .checked_mul(8)
            .ok_or("QuickDraw size overflow")?;
        if background.len() != count
            || output_bytes > u64::from(limits.max_storage_buffer_binding_size)
            || output_bytes > limits.max_buffer_size
            || width.div_ceil(8) > limits.max_compute_workgroups_per_dimension
            || height.div_ceil(8) > limits.max_compute_workgroups_per_dimension
            || command
                .area
                .iter()
                .chain(&command.rect)
                .any(|v| !(-32768..=32767).contains(v))
            || command
                .border
                .is_some_and(|b| b.iter().any(|v| !(0..=65535).contains(v)))
        {
            return Err("unsupported QuickDraw GPU request".into());
        }
        let input: Vec<u8> = background.iter().flat_map(|p| p.to_le_bytes()).collect();
        let input = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("QuickDraw background"),
                contents: &input,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("QuickDraw result"),
            size: output_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let border = command.border.unwrap_or([0, 0]);
        let mut words = Vec::with_capacity(24);
        words.extend([
            command.area[0] as u32,
            command.area[1] as u32,
            width,
            height,
        ]);
        words.extend(command.rect.map(|v| v as u32));
        words.extend([
            border[0] as u32,
            border[1] as u32,
            u32::from(command.border.is_some()),
            0,
        ]);
        words.extend(command.transfer.into_iter().flatten());
        words.extend(command.pattern.map(u32::from));
        let uniform: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("QuickDraw geometry"),
                contents: &uniform,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("QuickDraw readback"),
            size: output_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output_bytes);
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| e.to_string())?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let bytes = readback
            .slice(..)
            .get_mapped_range()
            .map_err(|e| e.to_string())?;
        Ok(bytes
            .chunks_exact(8)
            .map(|p| QuickDrawGpuPixel {
                value: u32::from_le_bytes(p[..4].try_into().unwrap()),
                covered: u32::from_le_bytes(p[4..].try_into().unwrap()) != 0,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_quickdraw_rectangles_match_patterns_transfers_frames_and_negative_origins() {
        let state = match State::new() {
            Ok(state) => state,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        for origin in [[-11, -9], [-3, 2], [0, 0], [5, 7]] {
            for border in [None, Some([0, 0]), Some([1, 2]), Some([20, 1])] {
                for transfer in [
                    [[0, 0], [0, 255]],
                    [[0xff, 0], [0xff, 0xff]],
                    [[0x00ff_ffff, 0x0000_1234], [0x0000_ffff, 0x00ff_0000]],
                ] {
                    let command = QuickDrawGpuCommand {
                        area: [origin[0], origin[1], origin[0] + 17, origin[1] + 13],
                        rect: [origin[0] + 1, origin[1] + 1, origin[0] + 16, origin[1] + 12],
                        border,
                        pattern: [0x96, 0x69, 0xaa, 0x55, 0xcc, 0x33, 0xf0, 0x0f],
                        transfer,
                    };
                    let background: Vec<u32> = (0..17 * 13)
                        .map(|i| (i as u32).wrapping_mul(0x10203) ^ 0xa5c6e7)
                        .collect();
                    let actual = state.render(&command, &background).unwrap();
                    for y in 0..13 {
                        for x in 0..17 {
                            let i = y * 17 + x;
                            let px = origin[0] + x as i32;
                            let py = origin[1] + y as i32;
                            let inside = (1..16).contains(&x) && (1..12).contains(&y);
                            let covered = inside
                                && border.is_none_or(|b| {
                                    x < 1 + b[0] as usize
                                        || x >= 16usize.saturating_sub(b[0] as usize)
                                        || y < 1 + b[1] as usize
                                        || y >= 12usize.saturating_sub(b[1] as usize)
                                });
                            let black = command.pattern[py.rem_euclid(8) as usize]
                                & (0x80 >> px.rem_euclid(8))
                                != 0;
                            let [pass, xor] = transfer[usize::from(black)];
                            let expected = QuickDrawGpuPixel {
                                covered,
                                value: if covered {
                                    (background[i] & pass) ^ xor
                                } else {
                                    background[i]
                                },
                            };
                            assert_eq!(
                                actual[i], expected,
                                "origin={origin:?},border={border:?},point={x},{y}"
                            );
                        }
                    }
                }
            }
        }
        let command = QuickDrawGpuCommand {
            area: [0, 0, 1, 1],
            rect: [0, 0, 1, 1],
            border: None,
            pattern: [0; 8],
            transfer: [[0; 2]; 2],
        };
        assert!(state.render(&command, &[]).is_err());
    }
}
