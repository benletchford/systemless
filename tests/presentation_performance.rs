//! Opt-in optimized measurement of a real application's first modal dialog.
//! Set SYSTEMLESS_PROFILE_ARCHIVE to a locally obtained game archive.
use std::time::{Duration, Instant};
use systemless::game::{init_game, load_game, new_runner_with_screen_depth};

#[test]
#[ignore = "requires a local game archive; prints rendering timing measurements"]
fn profile_modal_presentation() {
    let archive = std::env::var("SYSTEMLESS_PROFILE_ARCHIVE").expect("archive path");
    let mut runner = new_runner_with_screen_depth(8);
    runner.set_instructions_per_tick(10_000);
    let app = load_game(&mut runner, &std::fs::read(archive).unwrap()).unwrap();
    init_game(&mut runner, &app);
    runner.prepare_text_presentation();
    let boot_slices = std::env::var("SYSTEMLESS_PROFILE_BOOT_SLICES")
        .ok()
        .map(|n| n.parse::<usize>().unwrap());
    for i in 0..boot_slices.unwrap_or(12_000) {
        runner.prepare_text_presentation();
        let tick = runner.guest_tick().wrapping_add(1);
        assert!(runner.run_steps(50_000, Some(tick)).1);
        runner.composite_frame();
        if boot_slices.is_none() && runner.dispatcher().is_dialog_tracking() {
            break;
        }
        if i % 1000 == 0 {
            eprintln!("boot slice={i}");
        }
    }
    assert!(
        boot_slices.is_some() || runner.dispatcher().is_dialog_tracking(),
        "no modal dialog reached"
    );
    for _ in 0..120 {
        runner.prepare_text_presentation();
        let tick = runner.guest_tick().wrapping_add(1);
        assert!(runner.run_steps(50_000, Some(tick)).1);
        runner.composite_frame();
    }
    runner.prepare_text_presentation();
    if let Ok(path) = std::env::var("SYSTEMLESS_PROFILE_IMAGE") {
        if let Some((w, h, pixels, _)) = runner.bus().outline_presentation_rgb() {
            image::RgbImage::from_raw(w, h, pixels)
                .unwrap()
                .save(path)
                .unwrap();
        }
    }
    let mut execute = Vec::new();
    let mut compose = Vec::new();
    let mut present = Vec::new();
    let screen = runner.dispatcher().screen_mode;
    let guest = vec![0; screen.2 as usize * screen.3 as usize];
    let mut output = Vec::new();
    let output_scale = std::env::var("SYSTEMLESS_PROFILE_OUTPUT_SCALE")
        .unwrap_or("2".into())
        .parse::<u32>()
        .unwrap();
    let mut outline_frames = 0;
    for _ in 0..std::env::var("SYSTEMLESS_PROFILE_FRAMES")
        .unwrap_or("120".into())
        .parse::<usize>()
        .unwrap()
    {
        let start = Instant::now();
        runner.prepare_text_presentation();
        let tick = runner.guest_tick().wrapping_add(1);
        assert!(runner.run_steps_with_audio(50_000, Some(tick), 735).1);
        execute.push(start.elapsed());
        let start = Instant::now();
        runner.composite_frame();
        compose.push(start.elapsed());
        let start = Instant::now();
        if runner.bus().has_visible_outline_detail() {
            outline_frames += 1;
            std::hint::black_box(runner.bus().presented_argb_scaled(
                &guest,
                &guest,
                output_scale,
                &mut output,
            ));
        } else {
            let d = runner.dispatcher();
            systemless::display::render_screen_argb_with_gamma(
                runner.bus(),
                d.screen_mode,
                &d.device_clut,
                &d.device_gamma,
                &mut output,
            );
        }
        present.push(start.elapsed());
    }
    eprintln!(
        "output_scale={output_scale} outline_frames={outline_frames}/{}",
        execute.len()
    );
    for (name, mut samples) in [
        ("execute", execute),
        ("compose", compose),
        ("present", present),
    ] {
        samples.sort();
        let total: Duration = samples.iter().copied().sum();
        eprintln!(
            "retained_scale=4 {name}: mean={:?} p95={:?} max={:?}",
            total / samples.len() as u32,
            samples[samples.len() * 95 / 100],
            samples.last().unwrap()
        );
    }
}
