//! Opt-in measurements of guest rendering, with identical simulated input.
//! This excludes host window presentation, vsync, and screenshot encoding.
use super::*;
use std::time::{Duration, Instant};

pub(super) fn presentation_fixture(text: bool) -> systemless::memory::CompactPresentation {
    let mut frame = systemless::memory::CompactPresentation {
        width: 640,
        height: 480,
        scale: 4,
        ..Default::default()
    };
    for y in 0..frame.height {
        for x in 0..frame.width {
            if text && y % 20 < 12 && x % 9 < 6 {
                frame.cells.push(0x80000000 | frame.detail.len() as u32);
                frame
                    .detail
                    .extend((0..16).map(|s| if (x + y + s) % 3 == 0 { 0xffffff } else { 0 }));
            } else {
                frame.cells.push(if y < 20 || x % 160 < 2 {
                    0xcccccc
                } else {
                    0xffffff
                });
            }
        }
    }
    frame
}

#[test]
#[ignore = "optimized offscreen rendering benchmark; run alone"]
fn benchmark_presenters() {
    // Both backends run on the same host thread and alternate order between
    // runs, avoiding fresh test-thread scheduling as a comparison variable.
    let reverse = std::env::var_os("SYSTEMLESS_BENCH_REVERSE").is_some();
    for text in [false, true] {
        for gpu in if reverse {
            [true, false]
        } else {
            [false, true]
        } {
            if gpu {
                wgpu_compact::tests::benchmark_wgpu_presentation(text);
            } else {
                #[cfg(target_os = "macos")]
                metal_present::tests::benchmark_default_presentation(text);
            }
        }
    }
}

fn frames(runner: &mut FixtureRunner, count: usize) -> usize {
    let mut exhausted = 0;
    for _ in 0..count {
        exhausted += usize::from(headless_time::frame(runner, 0).budget_exhausted);
        assert!(!runner.is_halted());
    }
    exhausted
}

fn page(runner: &mut FixtureRunner, number: i16) -> usize {
    runner.push_mouse_down(10, 64);
    let mut exhausted = frames(runner, 2);
    let y = 27 + (number - 1) * 16;
    runner.set_mouse_position(y, 150);
    exhausted += frames(runner, 2);
    runner.push_mouse_up(y, 150);
    exhausted + frames(runner, 20)
}

fn image(runner: &FixtureRunner) -> Vec<u32> {
    let dispatcher = runner.dispatcher();
    let mut pixels = Vec::new();
    systemless::display::render_screen_argb_with_gamma(
        runner.bus(),
        dispatcher.screen_mode,
        &dispatcher.device_clut,
        &dispatcher.device_gamma(),
        &mut pixels,
    );
    pixels
}

pub(super) fn report(name: &str, backend: &str, samples: &mut [Duration]) {
    // Keep chronological observations before sorting for the legacy log line.
    // Guest observations at the same index are paired across backend orders.
    eprintln!(
        "[RENDER-SAMPLES] {}",
        serde_json::json!({
            "case": name, "backend": backend,
            "samples_us": samples.iter().map(|sample| sample.as_secs_f64() * 1e6).collect::<Vec<_>>(),
        })
    );
    samples.sort_unstable();
    eprintln!(
        "[RENDER-BENCH] case={name} backend={backend} n={} p50_us={:.1} p95_us={:.1}",
        samples.len(),
        samples[samples.len() / 2].as_secs_f64() * 1e6,
        samples[(samples.len() * 95).div_ceil(100) - 1].as_secs_f64() * 1e6,
    );
}

#[test]
#[ignore = "optimized rendering benchmark; run alone with --nocapture --test-threads=1"]
fn benchmark_guest_renderers() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/toolbox-showcase/toolbox-showcase.sit");
    let count = std::env::var("SYSTEMLESS_BENCH_SAMPLES")
        .map(|value| value.parse::<usize>().unwrap())
        .unwrap_or(15);
    assert!(count >= 3);
    let selected = std::env::var("SYSTEMLESS_BENCH_GUEST_CASE").ok();
    assert!(
        selected
            .as_deref()
            .is_none_or(|name| { matches!(name, "ui" | "drawing" | "text" | "3d" | "sprites") }),
        "unknown guest benchmark case"
    );
    for (name, target) in [
        ("ui", 2),
        ("drawing", 1),
        ("text", 11),
        ("3d", 4),
        ("sprites", 14),
    ] {
        if selected.as_deref().is_some_and(|selected| selected != name) {
            continue;
        }
        let mut dispatch = None;
        let mut runners = [false, true].map(|gpu| {
            let mut runner = game::new_runner_with_configuration(true, 8);
            runner.set_prefer_powerpc_executables(true);
            if gpu {
                dispatch = Some(forced_gpu_guest::install_adaptive(&mut runner));
            }
            runner.set_app_start_time(0);
            let app = game::load_game_from_path(&mut runner, &fixture).unwrap();
            game::init_game(&mut runner, &app);
            assert!(
                runner.is_powerpc_app(),
                "benchmark requires the native fixture"
            );
            configure_realtime_execution_rate(&mut runner);
            frames(&mut runner, 40);
            runner
        });
        let dispatch = dispatch.unwrap();
        let mut dispatch_samples = Vec::new();
        let reverse = usize::from(std::env::var_os("SYSTEMLESS_BENCH_REVERSE").is_some());
        let mut samples: [Vec<Duration>; 2] = std::array::from_fn(|_| Vec::new());
        for iteration in 0..count + 3 {
            // Pair the same redraw in independent guests, alternating which
            // backend runs first. Reset and screenshot work stay outside timing.
            let order = if (iteration + reverse) % 2 == 0 {
                [0, 1]
            } else {
                [1, 0]
            };
            for index in order {
                let runner = &mut runners[index];
                page(runner, if target == 1 { 2 } else { 1 });
                let before: [usize; 4] = std::array::from_fn(|i| dispatch[i].get());
                let start = Instant::now();
                let exhausted = page(runner, target);
                let elapsed = start.elapsed();
                // Read counters after the timer; exclude reset and warmup calls.
                if iteration >= 3 && index == 1 {
                    dispatch_samples.push(std::array::from_fn::<_, 4, _>(|i| {
                        dispatch[i].get() - before[i]
                    }));
                }
                assert_eq!(
                    exhausted, 0,
                    "{name} redraw exhausted its guest work budget"
                );
                if iteration >= 3 {
                    samples[index].push(elapsed);
                }
            }
        }
        eprintln!(
            "[RENDER-ORDER] {}",
            serde_json::json!({
                "case": name, "pairs": count,
                "first_measured_backend": if (3 + reverse) % 2 == 0 { "default" } else { "gpu" },
            })
        );
        eprintln!(
            "[RENDER-DISPATCH] {}",
            serde_json::json!({ "case": name, "requests": dispatch_samples })
        );
        let mut reference: Option<Vec<u32>> = None;
        for (index, runner) in runners.iter().enumerate() {
            let gpu = index == 1;
            let pixels = image(runner);
            if let Some(root) = std::env::var_os("SYSTEMLESS_BENCH_OUTPUT") {
                let root = PathBuf::from(root);
                std::fs::create_dir_all(&root).unwrap();
                let mode = runner.dispatcher().screen_mode;
                let rgb = pixels
                    .iter()
                    .flat_map(|pixel| [(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8])
                    .collect();
                image::RgbImage::from_raw(mode.2.into(), mode.3.into(), rgb)
                    .unwrap()
                    .save(root.join(format!(
                        "{name}-{}.png",
                        if gpu { "gpu" } else { "default" }
                    )))
                    .unwrap();
            }
            if let Some(reference) = &reference {
                // Float rasterization may differ by one RGB555 component.
                let tolerance = if target == 4 { 8 } else { 0 };
                assert_eq!(pixels.len(), reference.len());
                assert!(
                    pixels.iter().zip(reference).all(|(&a, &b)| {
                        [0, 8, 16].into_iter().all(|shift| {
                            ((a >> shift) & 255).abs_diff((b >> shift) & 255) <= tolerance
                        })
                    }),
                    "{name} GPU output differs from the default renderer"
                );
            } else {
                reference = Some(pixels);
            }
            report(
                name,
                if gpu { "gpu" } else { "default" },
                &mut samples[index],
            );
        }
    }
}

// Keep real guest -> GPU -> routed guest writeback coverage even when the
// production work selector correctly keeps this small fixture on the CPU.
mod forced_gpu_guest {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    use systemless::loader::ppc::{PpcQ3GpuFrame, PpcQ3GpuRenderer, PpcQ3RasterFrame};
    use systemless::quickdraw::gpu::*;

    pub(super) type Counts = Rc<[Cell<usize>; 4]>;

    // Benchmark instrumentation preserves production advisory selection. Forced
    // writeback tests below keep their historical selection for small fixtures.
    pub(super) fn install_adaptive(runner: &mut FixtureRunner) -> Counts {
        let calls: Counts = Rc::new(std::array::from_fn(|_| Cell::new(0)));
        runner.set_quickdraw_gpu_renderer(Box::new(QuickDraw {
            renderer: Rc::new(RefCell::new(wgpu_quickdraw::QuickDrawRenderer::new())),
            calls: calls.clone(),
            adaptive: true,
        }));
        runner.set_q3_gpu_renderer(Box::new(Qd3d {
            renderer: Rc::new(RefCell::new(wgpu_qd3d::Qd3dRenderer::new())),
            calls: calls.clone(),
            adaptive: true,
        }));
        calls
    }
    fn checked<T>(result: Result<T, String>, calls: &Cell<usize>) -> Result<T, String> {
        assert!(
            result.is_ok(),
            "guest GPU request failed: {:?}",
            result.as_ref().err()
        );
        calls.set(calls.get() + 1);
        result
    }
    struct QuickDraw {
        renderer: Rc<RefCell<wgpu_quickdraw::QuickDrawRenderer>>,
        calls: Counts,
        adaptive: bool,
    }
    impl QuickDrawGpuRenderer for QuickDraw {
        fn prefers_precomputed_raster(&self) -> bool {
            !self.adaptive || self.renderer.borrow().prefers_precomputed_raster()
        }
        fn prefers_raster(&self, command: &QuickDrawGpuCommand) -> bool {
            !self.adaptive || self.renderer.borrow().prefers_raster(command)
        }
        fn prefers_blit(&self, pixels: usize, expensive_color: bool) -> bool {
            !self.adaptive || self.renderer.borrow().prefers_blit(pixels, expensive_color)
        }
        fn render(
            &mut self,
            command: &QuickDrawGpuCommand,
            background: &[u32],
        ) -> Result<Vec<QuickDrawGpuPixel>, String> {
            let result = checked(
                self.renderer.borrow_mut().render(command, background),
                &self.calls[0],
            );
            if command.mask.is_some() {
                self.calls[1].set(self.calls[1].get() + 1);
            }
            result
        }
        fn render_blit(&mut self, command: &QuickDrawGpuBlit) -> Result<Vec<u32>, String> {
            checked(
                self.renderer.borrow_mut().render_blit(command),
                &self.calls[0],
            )
        }
        fn render_blit_cells(
            &mut self,
            command: &QuickDrawGpuBlit,
        ) -> Result<Vec<QuickDrawGpuPixel>, String> {
            let result = checked(
                self.renderer.borrow_mut().render_blit_cells(command),
                &self.calls[0],
            );
            if command.deep_mask.is_some() {
                self.calls[2].set(self.calls[2].get() + 1);
            }
            result
        }
        fn render_packed_blit(
            &mut self,
            command: &QuickDrawGpuPackedCopy,
        ) -> Result<Vec<u8>, String> {
            checked(
                self.renderer.borrow_mut().render_packed_blit(command),
                &self.calls[0],
            )
        }
    }
    struct Qd3d {
        renderer: Rc<RefCell<wgpu_qd3d::Qd3dRenderer>>,
        calls: Counts,
        adaptive: bool,
    }
    impl PpcQ3GpuRenderer for Qd3d {
        fn prefers_native(&self, frame: &PpcQ3RasterFrame) -> bool {
            !self.adaptive || self.renderer.borrow().prefers_native(frame)
        }
        fn render(
            &mut self,
            frame: &PpcQ3GpuFrame,
            background: &[u16],
        ) -> Result<Vec<u16>, String> {
            checked(
                self.renderer.borrow_mut().render(frame, background),
                &self.calls[3],
            )
        }
        fn render_native(
            &mut self,
            frame: &PpcQ3RasterFrame,
            background: &[u16],
        ) -> Result<Vec<u16>, String> {
            checked(
                self.renderer.borrow_mut().render_native(frame, background),
                &self.calls[3],
            )
        }
        fn render_indexed(
            &mut self,
            frame: &PpcQ3RasterFrame,
            background: &[u16],
            clut: &[[u16; 3]; 256],
        ) -> Result<Vec<u16>, String> {
            checked(
                self.renderer
                    .borrow_mut()
                    .render_indexed(frame, background, clut),
                &self.calls[3],
            )
        }
    }

    #[test]
    fn wgpu_guest_raster_writeback_matches_default() {
        let instance = wgpu::Instance::default();
        let adapter = match pollster::block_on(instance.request_adapter(&Default::default())) {
            Ok(adapter) => adapter,
            Err(error) => {
                assert!(
                    std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                    "GPU required: {error}"
                );
                return;
            }
        };
        if let Err(error) = wgpu_present::request_device(&adapter) {
            assert!(
                std::env::var_os("SYSTEMLESS_REQUIRE_GPU").is_none(),
                "GPU required: {error}"
            );
            return;
        }
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/toolbox-showcase/toolbox-showcase.sit");
        let calls: Counts = Rc::new(std::array::from_fn(|_| Cell::new(0)));
        let mut reference = Vec::new();
        for gpu in [false, true] {
            let mut runner = game::new_runner_with_configuration(true, 8);
            runner.set_prefer_powerpc_executables(true);
            if gpu {
                runner.set_quickdraw_gpu_renderer(Box::new(QuickDraw {
                    renderer: Rc::new(RefCell::new(wgpu_quickdraw::QuickDrawRenderer::new())),
                    calls: calls.clone(),
                    adaptive: false,
                }));
                runner.set_q3_gpu_renderer(Box::new(Qd3d {
                    renderer: Rc::new(RefCell::new(wgpu_qd3d::Qd3dRenderer::new())),
                    calls: calls.clone(),
                    adaptive: false,
                }));
            }
            runner.set_app_start_time(0);
            let app = game::load_game_from_path(&mut runner, &fixture).unwrap();
            game::init_game(&mut runner, &app);
            assert!(runner.is_powerpc_app());
            configure_realtime_execution_rate(&mut runner);
            frames(&mut runner, 40);
            for (index, target) in [2, 1, 11, 14, 4].into_iter().enumerate() {
                page(&mut runner, target);
                let pixels = image(&runner);
                if gpu {
                    let expected: &Vec<u32> = &reference[index];
                    assert_eq!(pixels.len(), expected.len());
                    let tolerance = if target == 4 { 8 } else { 0 };
                    assert!(
                        pixels.iter().zip(expected).all(|(&a, &b)| {
                            [0, 8, 16].into_iter().all(|shift| {
                                ((a >> shift) & 255).abs_diff((b >> shift) & 255) <= tolerance
                            })
                        }),
                        "forced GPU guest page {target} differs from default rendering"
                    );
                } else {
                    reference.push(pixels);
                }
            }
        }
        for (index, kind) in ["QuickDraw", "glyph masks", "deep masks", "3D"]
            .into_iter()
            .enumerate()
        {
            assert!(calls[index].get() > 0, "no successful {kind} GPU writeback");
        }
    }
    #[cfg(feature = "test-support")]
    #[test]
    #[ignore = "explicit-duration active GPU qualification; run through soak-gpu-compute.py"]
    fn qualify_gpu_compute_resources() {
        use std::io::Write;
        let seconds: u64 = std::env::var("SYSTEMLESS_GPU_COMPUTE_SOAK_SECONDS")
            .expect("set an explicit compute soak duration")
            .parse()
            .expect("compute soak seconds");
        assert!((1..=21600).contains(&seconds));
        let report = std::env::var_os("SYSTEMLESS_GPU_COMPUTE_SOAK_REPORT")
            .expect("set a compute resource report path");
        let mut report = std::io::BufWriter::new(std::fs::File::create(report).unwrap());
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/toolbox-showcase/toolbox-showcase.sit");
        let calls: Counts = Rc::new(std::array::from_fn(|_| Cell::new(0)));
        let quickdraw = Rc::new(RefCell::new(wgpu_quickdraw::QuickDrawRenderer::new()));
        let qd3d = Rc::new(RefCell::new(wgpu_qd3d::Qd3dRenderer::new()));
        let mut runners = [false, true].map(|gpu| {
            let mut runner = game::new_runner_with_configuration(true, 8);
            runner.set_prefer_powerpc_executables(true);
            if gpu {
                // These wrappers deliberately force real GPU writeback for
                // small fixture work that production adaptive selection skips.
                runner.set_quickdraw_gpu_renderer(Box::new(QuickDraw {
                    renderer: quickdraw.clone(),
                    calls: calls.clone(),
                    adaptive: false,
                }));
                runner.set_q3_gpu_renderer(Box::new(Qd3d {
                    renderer: qd3d.clone(),
                    calls: calls.clone(),
                    adaptive: false,
                }));
            }
            runner.set_app_start_time(0);
            let app = game::load_game_from_path(&mut runner, &fixture).unwrap();
            game::init_game(&mut runner, &app);
            assert!(runner.is_powerpc_app());
            configure_realtime_execution_rate(&mut runner);
            frames(&mut runner, 40);
            runner
        });
        let sweep = |runners: &mut [FixtureRunner; 2]| {
            for target in [2, 1, 11, 14, 4] {
                for runner in runners.iter_mut() {
                    assert_eq!(
                        page(runner, target),
                        0,
                        "compute soak exhausted guest work budget"
                    );
                }
                let expected = image(&runners[0]);
                let actual = image(&runners[1]);
                let tolerance = if target == 4 { 8 } else { 0 };
                assert_eq!(expected.len(), actual.len());
                assert!(
                    actual.iter().zip(expected).all(|(&a, b)| {
                        [0, 8, 16].into_iter().all(|shift| {
                            ((a >> shift) & 255).abs_diff((b >> shift) & 255) <= tolerance
                        })
                    }),
                    "active GPU page {target} differs from CPU writeback"
                );
            }
        };
        sweep(&mut runners);
        assert!(
            calls.iter().all(|count| count.get() > 0),
            "missing GPU workload coverage"
        );
        let started = Instant::now();
        let mut cycles = 0u64;
        let mut sampled = Instant::now();
        loop {
            let finished = cycles > 0 && started.elapsed() >= Duration::from_secs(seconds);
            if cycles == 0 || finished || sampled.elapsed() >= Duration::from_secs(30) {
                let sample = serde_json::json!({
                    "seconds": started.elapsed().as_secs_f64(), "cycles": cycles,
                    "gpu_writebacks": calls.iter().map(Cell::get).collect::<Vec<_>>(),
                    "quickdraw": quickdraw.borrow().qualification_snapshot().expect("healthy QuickDraw GPU"),
                    "qd3d": qd3d.borrow().qualification_snapshot().expect("healthy QD3D GPU"),
                    "complete": finished,
                });
                writeln!(report, "{sample}").unwrap();
                report.flush().unwrap();
                sampled = Instant::now();
            }
            if finished {
                break;
            }
            let before = std::array::from_fn::<_, 4, _>(|i| calls[i].get());
            sweep(&mut runners);
            assert!(
                calls
                    .iter()
                    .zip(before)
                    .all(|(count, old)| count.get() > old),
                "a compute workload stopped reaching the GPU"
            );
            cycles += 1;
        }
        assert!(cycles > 0);
        // A complete workload sample does not prove that driver-owned queues
        // and devices can be destroyed. Keep ordinary destruction synchronous
        // here so qualification detects a stall rather than hiding it.
        fn teardown<T>(stage: &str, value: T) {
            eprintln!("[GPU-SOAK-TEARDOWN] {stage}:begin");
            drop(value);
            eprintln!("[GPU-SOAK-TEARDOWN] {stage}:end");
        }
        teardown("runners", runners);
        teardown("qd3d", qd3d);
        teardown("quickdraw", quickdraw);
        teardown("report", report);
    }
}
