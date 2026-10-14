use std::sync::Arc;
use systemless_future::{
    cpu::Architecture,
    fixtures,
    graphics::{Draw, Graphics},
    memory::GuestPtr,
    runtime::{Runtime, Stop},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--code" || a == "--pef") {
        return run_file(&args);
    }
    if args
        .iter()
        .any(|a| !matches!(a.as_str(), "--ppc" | "--window" | "--smoke"))
    {
        return Err("unknown option; use --code RESOURCE_FORK or --pef DATA_FORK [--resources RESOURCE_FORK] [--budget N]".into());
    }
    let arch = if args.iter().any(|a| a == "--ppc") {
        Architecture::PowerPc
    } else {
        Architecture::M68k
    };
    if args.iter().any(|a| a == "--window") {
        let event_loop = EventLoop::new()?;
        let mut app = App {
            arch,
            window: None,
            state: None,
            error: None,
            frames: 0,
            attempts: 0,
            redraw_at: None,
            smoke: args.iter().any(|a| a == "--smoke"),
        };
        event_loop.run_app(&mut app)?;
        if let Some(e) = app.error {
            return Err(e.into());
        }
    } else {
        let mut rt = fixtures::runtime(arch, true)?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        rt.graphics = Some(pollster::block_on(Graphics::new(
            &instance,
            None,
            8,
            8,
            GuestPtr(fixtures::PIXELS),
        ))?);
        let stop = rt.advance(100)?;
        if stop != Stop::Exit(0) {
            return Err(format!("unexpected {stop:?}").into());
        }
        let gpu = rt.graphics.as_ref().unwrap();
        let rgba = gpu.read_rgba()?;
        println!(
            "{arch:?}: {stop:?}, sum={}, instructions={}, first_pixel={:?}, adapter={:?}",
            rt.services.accumulator,
            rt.instructions,
            &rgba[..4],
            gpu.adapter.get_info()
        );
    }
    Ok(())
}
struct State {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    rt: Runtime,
    pipeline: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
}
impl State {
    fn new(window: Arc<Window>, arch: Architecture) -> Result<Self, Box<dyn std::error::Error>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window.clone())?;
        let mut gpu = pollster::block_on(Graphics::new(
            &instance,
            Some(&surface),
            8,
            8,
            GuestPtr(fixtures::PIXELS),
        ))?;
        gpu.palette[0] = [245, 245, 245, 255];
        gpu.palette[3] = [30, 90, 180, 255];
        gpu.palette[6] = [240, 100, 20, 255];
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&gpu.adapter, size.width.max(1), size.height.max(1))
            .ok_or("no surface configuration")?;
        // Guest RGB values are already encoded bytes; avoid an extra sRGB encode.
        config.format = surface
            .get_capabilities(&gpu.adapter)
            .formats
            .into_iter()
            .find(|f| !f.is_srgb())
            .ok_or("no unorm presentation format")?;
        surface.configure(&gpu.device, &config);
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("nearest guest display"),
                source: wgpu::ShaderSource::Wgsl(include_str!("present.wgsl").into()),
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &gpu.texture.create_view(&Default::default()),
                ),
            }],
        });
        let mut rt = fixtures::runtime(arch, true)?;
        rt.graphics = Some(gpu);
        if rt.advance(100)? != Stop::Exit(0) {
            return Err("fixture failed to exit".into());
        }
        Ok(Self {
            instance,
            surface,
            config,
            rt,
            pipeline,
            bind,
        })
    }
    fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        if width > 0 && height > 0 {
            self.surface
                .configure(&self.rt.graphics.as_ref().unwrap().device, &self.config);
        }
    }
    fn draw(&mut self, window: Arc<Window>) -> Result<bool, Box<dyn std::error::Error>> {
        if self.config.width == 0 || self.config.height == 0 {
            return Ok(false);
        }
        let gpu = self.rt.graphics.as_ref().unwrap();
        gpu.check_error()?;
        let acquired = self.surface.get_current_texture();
        let (frame, reconfigure) = match acquired {
            wgpu::CurrentSurfaceTexture::Success(f) => (f, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => (f, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&gpu.device, &self.config);
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self.instance.create_surface(window)?;
                self.surface.configure(&gpu.device, &self.config);
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("surface validation failed".into())
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &attachments,
                ..Default::default()
            });
            let scale = (self.config.width as f32 / gpu.width as f32)
                .min(self.config.height as f32 / gpu.height as f32);
            let w = scale * gpu.width as f32;
            let h = scale * gpu.height as f32;
            pass.set_viewport(
                (self.config.width as f32 - w) / 2.0,
                (self.config.height as f32 - h) / 2.0,
                w,
                h,
                0.0,
                1.0,
            );
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([encoder.finish()]);
        window.pre_present_notify();
        gpu.queue.present(frame);
        if reconfigure {
            self.surface.configure(&gpu.device, &self.config);
        }
        gpu.check_error()?;
        Ok(true)
    }
}
struct App {
    arch: Architecture,
    window: Option<Arc<Window>>,
    state: Option<State>,
    error: Option<String>,
    frames: u32,
    attempts: u32,
    redraw_at: Option<std::time::Instant>,
    smoke: bool,
}
impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl ToString) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }
}
impl ApplicationHandler for App {
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(at) = self.redraw_at {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(at));
        } else {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
        }
    }
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, _cause: winit::event::StartCause) {
        if self
            .redraw_at
            .is_some_and(|at| std::time::Instant::now() >= at)
        {
            self.redraw_at = None;
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| {
            let window = Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title("Systemless future — dual CPU graphics fixture")
                        .with_inner_size(winit::dpi::LogicalSize::new(512, 512)),
                )?,
            );
            let state = State::new(window.clone(), self.arch)?;
            Ok::<_, Box<dyn std::error::Error>>((window, state))
        })();
        match result {
            Ok((w, s)) => {
                w.focus_window();
                w.request_redraw();
                self.window = Some(w);
                self.state = Some(s);
            }
            Err(e) => self.fail(event_loop, e),
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let (Some(window), Some(state)) = (&self.window, &mut self.state) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Occluded(false) => window.request_redraw(),
            WindowEvent::Resized(size) => {
                state.resize(size.width, size.height);
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.attempts += 1;
                if self.smoke && self.attempts > 600 {
                    self.fail(
                        event_loop,
                        "window smoke could not acquire/present/resize within 600 paced attempts",
                    );
                    return;
                }
                match state.draw(window.clone()) {
                    Ok(true) => self.frames += 1,
                    Ok(false) => {
                        self.redraw_at =
                            Some(std::time::Instant::now() + std::time::Duration::from_millis(16));
                    }
                    Err(e) => {
                        self.fail(event_loop, e);
                        return;
                    }
                }
                if self.smoke {
                    if self.frames == 1 {
                        if let Some(size) =
                            window.request_inner_size(winit::dpi::PhysicalSize::new(640, 360))
                        {
                            state.resize(size.width, size.height);
                        }
                    }
                    if self.frames > 120 {
                        let message = format!(
                            "window smoke resize did not complete: {}x{}",
                            state.config.width, state.config.height
                        );
                        self.fail(event_loop, message);
                        return;
                    }
                    if self.frames >= 3 && state.config.width == 640 && state.config.height == 360 {
                        state.resize(0, 0);
                        assert!(!state.draw(window.clone()).unwrap());
                        state.resize(640, 360);
                        assert_eq!(state.rt.graphics.as_ref().unwrap().width, 8);
                        println!("window smoke: presented, resized to 640x360, zero-size suspended; guest remains 8x8");
                        event_loop.exit();
                    } else {
                        self.redraw_at =
                            Some(std::time::Instant::now() + std::time::Duration::from_millis(16));
                    }
                }
            }
            // Host demo interaction; not claimed to be Macintosh Event Manager support.
            WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed,
                ..
            } => {
                let gpu = state.rt.graphics.as_mut().unwrap();
                gpu.palette.swap(3, 6);
                if let Err(e) = gpu.execute(&mut state.rt.memory, Draw::Present) {
                    self.fail(event_loop, e);
                    return;
                }
                window.request_redraw();
            }
            _ => {}
        }
    }
}

fn run_file(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    use systemless_future::load::LoadedApplication;
    let mut executable = None;
    let mut resources = None;
    let mut budget = 1_000_000u64;
    let mut i = 0;
    while i < args.len() {
        let option = &args[i];
        let value = args.get(i + 1).ok_or("option needs a value")?;
        match option.as_str() {
            "--code" | "--pef" => {
                if executable
                    .replace((option.as_str(), value.as_str()))
                    .is_some()
                {
                    return Err("choose one application format".into());
                }
            }
            "--resources" => {
                if resources.replace(value.as_str()).is_some() {
                    return Err("duplicate resource fork".into());
                }
            }
            "--budget" => {
                budget = value.parse()?;
                if budget == 0 {
                    return Err("budget must be positive".into());
                }
            }
            _ => return Err(format!("unknown application option {option}").into()),
        }
        i += 2;
    }
    let (format, path) = executable.ok_or("missing application")?;
    let data = read_bounded(path)?;
    let fork = resources.map(read_bounded).transpose()?;
    let mut app = match format {
        "--code" => {
            if fork.is_some() {
                return Err("--code already selects the resource fork".into());
            }
            LoadedApplication::code(&data)?
        }
        _ => LoadedApplication::pef(&data, fork.as_deref())?,
    };
    // Deterministic headless policy: no external input; advance virtual time
    // to each requested wakeup while retaining one total instruction budget.
    let started = app.runtime.instructions;
    let stop = loop {
        let remaining = budget.saturating_sub(app.runtime.instructions - started);
        match app.runtime.advance(remaining)? {
            Stop::Waiting { ticks } => app.runtime.elapse_ticks(ticks)?,
            stop => break stop,
        }
    };
    println!(
        "{:?}: {stop:?}, instructions={}, service_calls={}, entry={:#x}",
        app.info.architecture,
        app.runtime.instructions,
        app.runtime.services.calls.len(),
        app.info.entry
    );
    if stop == Stop::Budget {
        return Err("guest execution budget exhausted".into());
    }
    Ok(())
}
fn read_bounded(path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(systemless_future::load::IMAGE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > systemless_future::load::IMAGE_LIMIT {
        return Err("input exceeds 64 MiB limit".into());
    }
    Ok(bytes)
}
