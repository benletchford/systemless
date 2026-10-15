//! Bounded original 68K mouse probe. Writes PPM screenshots without a host window.
use std::{io::Read, path::Path};
use systemless_future::{
    cpu::Cpu,
    graphics::Graphics,
    host_input::Pointer,
    load::{LoadedApplication, IMAGE_LIMIT},
    memory::GuestPtr,
    runtime::Stop,
};
fn screenshot(app: &mut LoadedApplication, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    app.runtime.refresh_screen()?;
    let gpu = app.runtime.graphics.as_ref().unwrap();
    let rgba = gpu.read_rgba()?;
    let mut ppm = format!("P6\n{} {}\n255\n", gpu.width, gpu.height).into_bytes();
    for pixel in rgba.chunks_exact(4) {
        ppm.extend(&pixel[..3]);
    }
    std::fs::write(path, ppm)?;
    Ok(())
}
struct ProbeTrace {
    enabled: bool,
    changes: u32,
    random: u32,
    pixels: Vec<(u32, u32)>,
    pixel_changes: u32,
}
fn seed(app: &LoadedApplication) -> Option<(u32, u32)> {
    let address = app.runtime.process.ports?.current_slot.0.checked_sub(126)?;
    Some((address, app.runtime.memory.u32(GuestPtr(address)).ok()?))
}
// Read canonical guest pixels, excluding the presentation-only cursor.
fn pixels(app: &LoadedApplication, points: &[(u32, u32)]) -> Option<Vec<bool>> {
    if points.is_empty() {
        return None;
    }
    let bitmap = systemless_future::bitmap::Bitmap::read(
        &app.runtime.memory,
        app.runtime.process.ports?.screen_bitmap,
    )
    .ok()?;
    points
        .iter()
        .map(|&(x, y)| {
            let offset = y
                .checked_mul(u32::try_from(bitmap.row_bytes).ok()?)?
                .checked_add(x / 8)?;
            let byte = app
                .runtime
                .memory
                .read(GuestPtr(bitmap.base.0.checked_add(offset)?), 1)
                .ok()?[0];
            Some(byte & (0x80 >> (x % 8)) != 0)
        })
        .collect()
}
fn elapse(app: &mut LoadedApplication, ticks: u32) -> systemless_future::Result<()> {
    app.runtime.elapse_sound_silently(ticks)?;
    app.runtime.elapse_ticks(ticks)
}
fn advance_guest(
    app: &mut LoadedApplication,
    budget: u64,
    trace: &mut ProbeTrace,
) -> systemless_future::Result<Stop> {
    if !trace.enabled && trace.pixels.is_empty() {
        return app.runtime.advance(budget);
    }
    for _ in 0..budget {
        let pixels_before = pixels(app, &trace.pixels);
        let before = seed(app);
        let pc = match &app.runtime.cpu {
            Cpu::M68k(c) => c.pc,
            Cpu::PowerPc(c) => c.pc,
        };
        let calls = app.runtime.services.calls.len();
        let result = app.runtime.advance(1);
        let after = seed(app);
        let pixels_after = pixels(app, &trace.pixels);
        if pixels_before != pixels_after {
            if trace.pixel_changes < 256 {
                println!("pixel trace pc={pc:08x} points={:?} before={pixels_before:?} after={pixels_after:?} calls={:?}", trace.pixels, &app.runtime.services.calls[calls..]);
            } else if trace.pixel_changes == 256 {
                println!("pixel trace change limit reached (256); execution remains bounded by the probe budget");
            }
            trace.pixel_changes = trace.pixel_changes.saturating_add(1);
        }
        let random = app.runtime.services.calls[calls..]
            .iter()
            .any(|c| matches!(c, systemless_future::cpu::Call::Random));
        if trace.enabled
            && (before != after
                && (trace.changes < 64 || after.is_some_and(|(_, value)| value == 0))
                || random && trace.random < 4)
        {
            println!("seed trace pc={pc:08x} before={before:?} after={after:?} random={random} code={:02x?}",app.runtime.memory.read(GuestPtr(pc.saturating_sub(24)),64));
            if let Cpu::M68k(c) = &app.runtime.cpu {
                println!(
                    "seed trace registers={:08x?} stack={:02x?}",
                    c.dar,
                    app.runtime.memory.read(GuestPtr(c.dar[15]), 32)
                );
            }
        }
        if before != after {
            trace.changes += 1;
        }
        if random {
            trace.random += 1;
        }
        match result {
            Ok(Stop::Budget) => {}
            other => return other,
        }
    }
    Ok(Stop::Budget)
}
fn advance(
    app: &mut LoadedApplication,
    budget: u64,
    trace: &mut ProbeTrace,
    volume: &mut Option<systemless_future::volume::Volume>,
) -> systemless_future::Result<Stop> {
    let result = advance_guest(app, budget, trace);
    if let Some(volume) = volume {
        volume
            .checkpoint(&app.runtime.process.files)
            .map_err(|e| systemless_future::Error::Volume(e.to_string()))?;
    }
    result
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 4 || !args.len().is_multiple_of(2) {
        return Err("usage: probe_mouse CODE_RESOURCE_FORK OUTPUT_PREFIX X Y [X Y ...]".into());
    }
    let budget: u64 = match std::env::var("SYSTEMLESS_PROBE_BUDGET") {
        Ok(value) => value.parse()?,
        Err(std::env::VarError::NotPresent) => 30_000,
        Err(error) => return Err(error.into()),
    };
    if !(1..=10_000_000).contains(&budget) {
        return Err("SYSTEMLESS_PROBE_BUDGET must be 1..=10000000".into());
    }
    let tail: u64 = match std::env::var("SYSTEMLESS_PROBE_TAIL_BUDGET") {
        Ok(value) => value.parse()?,
        Err(std::env::VarError::NotPresent) => 0,
        Err(error) => return Err(error.into()),
    };
    if tail > 10_000_000 {
        return Err("SYSTEMLESS_PROBE_TAIL_BUDGET must be 0..=10000000".into());
    }
    let settle_after: Vec<usize> = std::env::var("SYSTEMLESS_PROBE_SETTLE_AFTER_CLICK")
        .ok()
        .map(|v| v.split(',').map(str::parse).collect())
        .transpose()?
        .unwrap_or_default();
    if settle_after
        .iter()
        .any(|&n| n == 0 || n > (args.len() - 2) / 2)
        || !settle_after.is_empty() && tail == 0
    {
        return Err(
            "settle-after clicks must select input steps and require a nonzero tail budget".into(),
        );
    }
    let drags: Vec<(usize, f64, f64)> = std::env::var("SYSTEMLESS_PROBE_DRAG")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(|v| -> Result<_, Box<dyn std::error::Error>> {
                    let fields: Vec<_> = v.split(':').collect();
                    if fields.len() != 3 {
                        return Err("drag must be CLICK:X:Y".into());
                    }
                    let step = fields[0].parse::<usize>()?;
                    let x = fields[1].parse::<f64>()?;
                    let y = fields[2].parse::<f64>()?;
                    if step == 0 || step > (args.len() - 2) / 2 || !x.is_finite() || !y.is_finite()
                    {
                        return Err("invalid drag input step or coordinate".into());
                    }
                    Ok((step, x, y))
                })
                .collect()
        })
        .transpose()?
        .unwrap_or_default();
    if drags
        .iter()
        .enumerate()
        .any(|(i, d)| drags[..i].iter().any(|other| other.0 == d.0))
    {
        return Err("only one held drag is allowed per click".into());
    }
    let ticks: u32 = match std::env::var("SYSTEMLESS_PROBE_TICKS_PER_STEP") {
        Ok(value) => value.parse()?,
        Err(std::env::VarError::NotPresent) => 0,
        Err(error) => return Err(error.into()),
    };
    println!("virtual ticks per step {ticks}");
    println!("per-step budget {budget}");
    let mut trace = ProbeTrace {
        enabled: std::env::var("SYSTEMLESS_PROBE_TRACE_SEED").is_ok_and(|v| v == "1"),
        pixels: std::env::var("SYSTEMLESS_PROBE_TRACE_PIXELS")
            .ok()
            .map(
                |value| -> Result<Vec<(u32, u32)>, Box<dyn std::error::Error>> {
                    let points: Vec<_> = value
                        .split(',')
                        .map(|point| -> Result<_, Box<dyn std::error::Error>> {
                            let (x, y) = point
                                .split_once(':')
                                .ok_or("trace pixels must be X:Y pairs")?;
                            let point = (x.parse::<u32>()?, y.parse::<u32>()?);
                            if point.0 >= 512 || point.1 >= 342 {
                                return Err("trace pixels must fit the 512x342 display".into());
                            }
                            Ok(point)
                        })
                        .collect::<Result<_, _>>()?;
                    if points.is_empty() || points.len() > 16 {
                        return Err("trace pixels requires 1..=16 points".into());
                    }
                    Ok(points)
                },
            )
            .transpose()?
            .unwrap_or_default(),
        pixel_changes: 0,
        changes: 0,
        random: 0,
    };
    let mut fork = Vec::new();
    std::fs::File::open(&args[0])?
        .take(IMAGE_LIMIT as u64 + 1)
        .read_to_end(&mut fork)?;
    let mut app = LoadedApplication::code(&fork)?;
    if let Ok(level) = std::env::var("SYSTEMLESS_PROBE_SOUND_VOLUME") {
        systemless_future::sound::set_volume(&mut app.runtime.memory, level.parse()?)?;
    }
    let mut volume = std::env::var_os("SYSTEMLESS_PROBE_VOLUME")
        .map(|path| systemless_future::volume::Volume::open(path, &mut app.runtime.process.files))
        .transpose()?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut gpu = pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000)))?;
    gpu.palette[0] = [255; 4];
    gpu.palette[1] = [0, 0, 0, 255];
    app.runtime.graphics = Some(gpu);
    if ticks != 0 {
        elapse(&mut app, ticks)?;
    }
    println!(
        "startup {:?}",
        advance(&mut app, budget, &mut trace, &mut volume)?
    );
    screenshot(&mut app, Path::new(&format!("{}-before.ppm", args[1])))?;
    let mut pointer = Pointer::default();
    let first = app.runtime.services.calls.len();
    let mut result = Ok(systemless_future::runtime::Stop::Budget);
    for (index, coordinates) in args[2..].chunks_exact(2).enumerate() {
        if ticks != 0 {
            elapse(&mut app, ticks)?;
        }
        pointer.motion(
            &mut app.runtime.process.input,
            &mut app.runtime.memory,
            (coordinates[0].parse()?, coordinates[1].parse()?),
            (512, 342),
            (512, 342),
        )?;
        pointer.button(
            &mut app.runtime.memory,
            &mut app.runtime.process.input,
            true,
        )?;
        result = advance(&mut app, budget, &mut trace, &mut volume);
        println!("click {} mouse down {result:?}", index + 1);
        if matches!(result, Ok(Stop::Budget | Stop::Waiting { .. })) {
            if ticks != 0 {
                elapse(&mut app, ticks)?;
            }
            if let Some((step, x, y)) = drags
                .iter()
                .copied()
                .find(|(step, _, _)| *step == index + 1)
            {
                pointer.motion(
                    &mut app.runtime.process.input,
                    &mut app.runtime.memory,
                    (x, y),
                    (512, 342),
                    (512, 342),
                )?;
                result = advance(&mut app, budget, &mut trace, &mut volume);
                println!("click {step} drag ({x},{y}) {result:?}");
                screenshot(
                    &mut app,
                    Path::new(&format!("{}-drag-{}.ppm", args[1], step)),
                )?;
                if !matches!(result, Ok(Stop::Budget | Stop::Waiting { .. })) {
                    break;
                }
            }
            pointer.button(
                &mut app.runtime.memory,
                &mut app.runtime.process.input,
                false,
            )?;
            result = advance(&mut app, budget, &mut trace, &mut volume);
            println!("click {} mouse up {result:?}", index + 1);
        }
        if settle_after.contains(&(index + 1))
            && matches!(result, Ok(Stop::Budget | Stop::Waiting { .. }))
        {
            if ticks != 0 {
                elapse(&mut app, ticks)?;
            }
            result = advance(&mut app, tail, &mut trace, &mut volume);
            println!("settle after click {} budget {tail}: {result:?}", index + 1);
        }
        if args.len() > 4 {
            screenshot(
                &mut app,
                Path::new(&format!("{}-step-{}.ppm", args[1], index + 1)),
            )?;
        }
        if !matches!(result, Ok(Stop::Budget | Stop::Waiting { .. })) {
            break;
        }
    }
    if matches!(result, Ok(Stop::Budget | Stop::Waiting { .. })) && tail != 0 {
        if ticks != 0 {
            elapse(&mut app, ticks)?;
        }
        result = advance(&mut app, tail, &mut trace, &mut volume);
        println!("tail budget {tail}: {result:?}");
    }
    if let Some(ports) = app.runtime.process.ports {
        if let Some(seed) = ports.current_slot.0.checked_sub(126) {
            println!(
                "clock={:?} QuickDraw seed={:?}",
                app.runtime.memory.u32(GuestPtr(0x16a)),
                app.runtime.memory.u32(GuestPtr(seed))
            );
        }
    }
    for call in &app.runtime.services.calls[first..] {
        println!("service {call:?}");
    }
    if let Some(ports) = app.runtime.process.ports {
        if let Ok(owner) = app.runtime.memory.u32(ports.current_slot) {
            if app
                .runtime
                .process
                .windows
                .contains(&app.runtime.memory, owner)
                .unwrap_or(false)
            {
                let mut control = app.runtime.memory.u32(GuestPtr(owner + 140)).unwrap_or(0);
                for _ in 0..256 {
                    if control == 0 {
                        break;
                    }
                    let Ok(record) = app.runtime.memory.u32(GuestPtr(control)) else {
                        break;
                    };
                    let Some(status) = record.checked_add(8) else {
                        break;
                    };
                    println!(
                        "control {control:08x} bounds/status={:02x?}",
                        app.runtime.memory.read(GuestPtr(status), 10)
                    );
                    control = app.runtime.memory.u32(GuestPtr(record)).unwrap_or(0);
                }
            }
        }
    }
    if let Cpu::M68k(c) = &app.runtime.cpu {
        println!(
            "pc={:08x} d0={:08x} d1={:08x} opcode={:04x?}",
            c.pc,
            c.dar[0],
            c.dar[1],
            app.runtime.memory.u16(GuestPtr(c.pc))
        );
    }
    println!(
        "completed sound callbacks={}",
        app.runtime.services.completed_sound_callbacks
    );
    if std::env::var("SYSTEMLESS_PROBE_CATALOG").is_ok_and(|v| v == "1") {
        let mut folders = vec![2];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(folder) = folders.pop() {
            if !visited.insert(folder) || visited.len() > 1024 {
                return Err("catalog walk exceeds the process-local bound".into());
            }
            let view = app
                .runtime
                .process
                .files
                .directory_view(folder)
                .map_err(|e| format!("catalog error {e}"))?;
            for entry in view.entries {
                if entry.directory {
                    folders.push(entry.id);
                } else {
                    let data = app
                        .runtime
                        .process
                        .files
                        .catalog_fork(entry.id, systemless_future::files::Fork::Data)
                        .unwrap_or(&[]);
                    let resource = app
                        .runtime
                        .process
                        .files
                        .catalog_fork(entry.id, systemless_future::files::Fork::Resource)
                        .unwrap_or(&[]);
                    println!("catalog folder={folder} node={} name={:?} data={} resource={} prefix={:02x?}",entry.id,String::from_utf8_lossy(&entry.name),data.len(),resource.len(),&data[..data.len().min(16)]);
                }
            }
        }
    }
    if let Err(error) = &result {
        eprintln!("execution error: {error}");
        if let Cpu::M68k(c) = &app.runtime.cpu {
            println!(
                "fault SP={:08x} stack={:02x?}",
                c.dar[15],
                app.runtime.memory.read(GuestPtr(c.dar[15]), 32)
            );
            let pb = GuestPtr(c.dar[8]);
            println!(
                "fault A0={:08x} bytes={:02x?}",
                pb.0,
                app.runtime.memory.read(pb, 80)
            );
            if let Some(name) =
                pb.0.checked_add(18)
                    .and_then(|address| app.runtime.memory.u32(GuestPtr(address)).ok())
            {
                if let Ok(length) = app.runtime.memory.read(GuestPtr(name), 1) {
                    println!(
                        "parameter name={:02x?}",
                        app.runtime
                            .memory
                            .read(GuestPtr(name), usize::from(length[0]) + 1)
                    );
                }
            }
        }
    }
    screenshot(&mut app, Path::new(&format!("{}-after.ppm", args[1])))?;
    result?;
    Ok(())
}
