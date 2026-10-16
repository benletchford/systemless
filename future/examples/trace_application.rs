//! Bounded instruction/service trace for diagnosing a file application's startup.
#[path = "trace_application/display_diagnostics.rs"]
mod display_diagnostics;
use systemless_future::{cpu::Cpu, load::LoadedApplication, runtime::Stop};
struct KeyAction {
    instruction: u64,
    code: u8,
    down: bool,
    text: Option<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("usage: trace_application code|pef PATH INSTRUCTION_LIMIT [--graphics] [--display-diagnostics] [--draw-diagnostics START:END] [--display WIDTHxHEIGHTx8] [--volume IMAGE] [--services-only] [--numeric-operands] [--watch-address HEX] [--elapse-waits] [--save-volume NEW_IMAGE] [--save-screen NEW_PPM] [--key-at INSTRUCTION:CODE:down|up[:TEXT]] [--trace-range START:END] [--key-at-poll POLL:CODE:down|up[:TEXT]]".into());
    }
    let mut graphics = false;
    let mut display_diagnostics = false;
    let mut services_only = false;
    let mut elapse_waits = false;
    let mut numeric_operands = false;
    let mut watch_address = None;
    let mut volume = None;
    let mut save_volume = None;
    let mut save_screen = None;
    let mut keys = Vec::new();
    let mut poll_keys = Vec::new();
    let mut trace_range = None;
    let mut draw_diagnostics = None;
    let budget: u64 = args[2].parse()?;
    let mut display = None;
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--graphics" if !graphics => graphics = true,
            "--display-diagnostics" if !display_diagnostics => display_diagnostics = true,
            "--services-only" if !services_only => services_only = true,
            "--elapse-waits" if !elapse_waits => elapse_waits = true,
            "--numeric-operands" if !numeric_operands => numeric_operands = true,
            "--watch-address" if watch_address.is_none() => {
                index += 1;
                let value = args.get(index).ok_or("missing watch address")?;
                watch_address = Some(u32::from_str_radix(value.trim_start_matches("0x"), 16)?);
            }
            "--display" if display.is_none() => {
                index += 1;
                display = Some(
                    args.get(index)
                        .ok_or("missing display")?
                        .parse::<systemless_future::display::Display>()?,
                );
            }
            "--save-volume" if save_volume.is_none() => {
                index += 1;
                save_volume = Some(args.get(index).ok_or("missing save volume image")?);
            }
            "--save-screen" if save_screen.is_none() => {
                index += 1;
                save_screen = Some(args.get(index).ok_or("missing screen output")?);
            }
            "--trace-range" if trace_range.is_none() => {
                index += 1;
                let (start, end) = args
                    .get(index)
                    .ok_or("missing trace range")?
                    .split_once(':')
                    .ok_or("trace range requires START:END")?;
                let start = start.parse::<u64>()?;
                let end = end.parse::<u64>()?;
                if start >= end || end > budget {
                    return Err("trace range must be nonempty and within the budget".into());
                }
                trace_range = Some(start..end);
            }
            "--draw-diagnostics" if draw_diagnostics.is_none() => {
                index += 1;
                let (start, end) = args
                    .get(index)
                    .ok_or("missing trace range")?
                    .split_once(':')
                    .ok_or("trace range requires START:END")?;
                let start = start.parse::<u64>()?;
                let end = end.parse::<u64>()?;
                if start >= end || end > budget {
                    return Err("trace range must be nonempty and within the budget".into());
                }
                draw_diagnostics = Some(start..end);
            }
            "--key-at" | "--key-at-poll" if keys.len() + poll_keys.len() < 64 => {
                let poll = args[index] == "--key-at-poll";
                index += 1;
                let fields: Vec<_> = args
                    .get(index)
                    .ok_or("missing key action")?
                    .splitn(4, ':')
                    .collect();
                if fields.len() < 3 {
                    return Err("key action requires INSTRUCTION:CODE:down|up[:TEXT]".into());
                }
                let instruction = fields[0].parse()?;
                let code = fields[1].parse::<u8>()?;
                if instruction >= budget || code >= 128 {
                    return Err(
                        "key action exceeds instruction budget or virtual code range".into(),
                    );
                }
                let down = match fields[2] {
                    "down" => true,
                    "up" => false,
                    _ => return Err("key action state must be down or up".into()),
                };
                let actions = if poll { &mut poll_keys } else { &mut keys };
                actions.push(KeyAction {
                    instruction,
                    code,
                    down,
                    text: fields.get(3).map(|s| s.to_string()),
                });
            }
            "--volume" if volume.is_none() => {
                index += 1;
                volume = Some(args.get(index).ok_or("missing volume image")?);
            }
            _ => return Err("invalid trace option".into()),
        }
        index += 1;
    }
    if (save_screen.is_some() || display_diagnostics || draw_diagnostics.is_some()) && !graphics {
        return Err("screen capture/diagnostics require --graphics".into());
    }
    keys.sort_by_key(|action| action.instruction);
    poll_keys.sort_by_key(|action| action.instruction);
    let file = std::fs::File::open(&args[1])?;
    use std::io::Read;
    let mut data = Vec::new();
    file.take(systemless_future::load::IMAGE_LIMIT as u64 + 1)
        .read_to_end(&mut data)?;
    let mut app = match args[0].as_str() {
        "code" => LoadedApplication::code(&data)?,
        "pef" => LoadedApplication::pef(&data, None)?,
        _ => return Err("expected code or pef".into()),
    };
    if let Some(display) = display {
        app = app.with_display(display)?;
    }
    if let Some(path) = volume {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(systemless_future::files::VOLUME_IMAGE_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)?;
        // Diagnostic snapshot only: never checkpoint changes to the input image.
        app.runtime.process.files.restore_volume(&bytes)?;
    }
    if graphics {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        app.runtime.graphics = Some(pollster::block_on(
            systemless_future::graphics::Graphics::new(
                &instance,
                None,
                app.runtime.display().width(),
                app.runtime.display().height(),
                systemless_future::memory::GuestPtr(0x4000),
            ),
        )?);
    }
    let mut keyboard = systemless_future::host_input::Keyboard::default();
    let mut next_key = 0;
    let mut next_poll_key = 0;
    let mut watched = None;
    let mut previous_pc = None;
    let mut draw_diagnostic_count = 0u16;
    for poll in 0..budget {
        for (actions, next, clock) in [
            (&keys, &mut next_key, app.runtime.instructions),
            (&poll_keys, &mut next_poll_key, poll),
        ] {
            while let Some(action) = actions.get(*next).filter(|a| a.instruction <= clock) {
                keyboard.key(
                    &mut app.runtime.memory,
                    &mut app.runtime.process.input,
                    action.code,
                    action.text.as_deref(),
                    action.down,
                    false,
                )?;
                println!(
                    "delivered host key instruction={} poll={} code={} down={}",
                    app.runtime.instructions, poll, action.code, action.down
                );
                *next += 1;
            }
        }
        if let Some(address) = watch_address {
            let current = app
                .runtime
                .memory
                .read(systemless_future::memory::GuestPtr(address), 16)
                .ok()
                .map(|bytes| bytes.to_vec());
            if current != watched {
                println!(
                    "watch {address:08x} after pc={previous_pc:08x?} service={:?}: {current:02x?}",
                    app.runtime.services.calls.last()
                );
                watched = current;
            }
        }
        previous_pc = Some(match &app.runtime.cpu {
            Cpu::M68k(c) => c.pc,
            Cpu::PowerPc(c) => c.pc,
        });

        if !services_only
            || trace_range
                .as_ref()
                .is_some_and(|r| r.contains(&app.runtime.instructions))
        {
            match &app.runtime.cpu {
                Cpu::M68k(c) => println!(
                "{} pc={:08x} sp={:08x} a5={:08x} a0={:08x} d0={:08x} regs={:08x?} opcode={:04x?}",
                app.runtime.instructions,
                c.pc,
                c.dar[15],
                c.dar[13],
                c.dar[8],
                c.dar[0],
                c.dar,
                app.runtime
                    .memory
                    .u16(systemless_future::memory::GuestPtr(c.pc))
            ),
                Cpu::PowerPc(c) => println!(
                    "{} pc={:08x} sp={:08x} toc={:08x} lr={:08x} r3={:08x}",
                    app.runtime.instructions, c.pc, c.gpr[1], c.gpr[2], c.lr, c.gpr[3]
                ),
            }
        }
        if numeric_operands {
            if let Cpu::M68k(c) = &app.runtime.cpu {
                use systemless_future::memory::GuestPtr;
                if matches!(app.runtime.memory.u16(GuestPtr(c.pc)), Ok(0xa9eb)) {
                    let sp = GuestPtr(c.dar[15]);
                    println!(
                        "numeric registers={:08x?} code-before={:02x?}",
                        c.dar,
                        app.runtime
                            .memory
                            .read(GuestPtr(c.pc.saturating_sub(32)), 34)
                    );
                    println!(
                        "numeric before pc={:08x} sp={:08x} environment={:04x} frame={:02x?}",
                        c.pc,
                        sp.0,
                        app.runtime.process.numerics.environment_word(),
                        app.runtime.memory.read(sp, 10)
                    );
                    for (name, offset) in [("destination", 2), ("source", 6)] {
                        if let Some(address) = sp.0.checked_add(offset) {
                            if let Ok(pointer) = app.runtime.memory.u32(GuestPtr(address)) {
                                println!(
                                    "numeric {name} at {pointer:08x}: {:02x?}",
                                    app.runtime.memory.read(GuestPtr(pointer), 10)
                                );
                            }
                        }
                    }
                }
            }
        }
        let calls = app.runtime.services.calls.len();
        let stop = match app.runtime.advance(1) {
            Ok(stop) => stop,
            Err(error) => {
                println!(
                    "latest service before fault: {:?}",
                    app.runtime.services.calls.last()
                );
                if let Some(systemless_future::cpu::Call::CopyBits(request)) =
                    app.runtime.services.calls.last()
                {
                    for (name, pointer) in [
                        ("source", request.source),
                        ("destination", request.destination),
                    ] {
                        println!(
                            "CopyBits {name} descriptor at {:08x}: {:02x?}",
                            pointer.0,
                            app.runtime.memory.read(pointer, 14)
                        );
                        if let Ok(handle) = app.runtime.memory.u32(pointer) {
                            if let Ok(data) = app
                                .runtime
                                .memory
                                .u32(systemless_future::memory::GuestPtr(handle))
                            {
                                println!("CopyBits {name} possible handle={handle:08x} data={data:08x}: {:02x?}",app.runtime.memory.read(systemless_future::memory::GuestPtr(data),50));
                            }
                        }
                    }
                }
                if let Some(systemless_future::cpu::Call::Port {
                    op: systemless_future::quickdraw::PortOp::PaintRect,
                    pointer,
                }) = app.runtime.services.calls.last()
                {
                    println!(
                        "paint rectangle at fault: {:?}",
                        systemless_future::quickdraw::read_rect(&app.runtime.memory, *pointer)
                    );
                }
                if let Some(ports) = app.runtime.process.ports {
                    if let Ok(port) = ports.current(&app.runtime.memory) {
                        println!(
                            "fault current port={:08x} bytes={:02x?}",
                            port.0,
                            app.runtime.memory.read(port, 108)
                        );
                        if let Ok(handle) = app
                            .runtime
                            .memory
                            .u32(systemless_future::memory::GuestPtr(port.0 + 2))
                        {
                            if let Ok(data) = app
                                .runtime
                                .memory
                                .u32(systemless_future::memory::GuestPtr(handle))
                            {
                                println!("fault port possible PixMap handle={handle:08x} data={data:08x} bytes={:02x?}", app.runtime.memory.read(systemless_future::memory::GuestPtr(data), 50));
                            }
                        }
                    }
                    println!(
                        "fault screen bitmap={:08x} bytes={:02x?}",
                        ports.screen_bitmap.0,
                        app.runtime.memory.read(ports.screen_bitmap, 14)
                    );
                }
                if let Cpu::M68k(c) = &app.runtime.cpu {
                    println!(
                        "fault pc={:08x} d0={:08x} sp={:08x} regs={:08x?}",
                        c.pc, c.dar[0], c.dar[15], c.dar
                    );
                    println!(
                        "fault stack memory={:02x?}",
                        app.runtime
                            .memory
                            .read(systemless_future::memory::GuestPtr(c.dar[15]), 64)
                    );
                    if let Ok(argument) = app
                        .runtime
                        .memory
                        .u32(systemless_future::memory::GuestPtr(c.dar[15]))
                    {
                        let pointer = systemless_future::memory::GuestPtr(argument);
                        println!(
                            "fault first argument={argument:08x} bytes={:02x?}",
                            app.runtime.memory.read(pointer, 64)
                        );
                        if let Ok(data) = app.runtime.memory.u32(pointer) {
                            println!(
                                "fault first argument indirect={data:08x} bytes={:02x?}",
                                app.runtime
                                    .memory
                                    .read(systemless_future::memory::GuestPtr(data), 64)
                            );
                        }
                    }
                    println!(
                        "fault A0 memory={:02x?}",
                        app.runtime
                            .memory
                            .read(systemless_future::memory::GuestPtr(c.dar[8]), 64)
                    );
                }
                if let Some(systemless_future::cpu::Call::Retrace(op)) = app.runtime.services.calls.iter().rev()
                    .find(|call| matches!(call, systemless_future::cpu::Call::Retrace(op) if !op.remove))
                {
                    println!("last installed VBL record at {:08x}: {:02x?}", op.task.0,
                        app.runtime.memory.read(op.task,14));
                }
                println!(
                    "completed callbacks: sound={} timer={} vbl={}",
                    app.runtime.services.completed_sound_callbacks,
                    app.runtime.services.completed_timer_callbacks,
                    app.runtime.services.completed_vbl_callbacks
                );
                save_snapshot(&mut app, save_volume, save_screen, display_diagnostics)?;
                return Err(error.into());
            }
        };
        for call in &app.runtime.services.calls[calls..] {
            println!("service {call:?}");
            if draw_diagnostics
                .as_ref()
                .is_some_and(|r| r.contains(&app.runtime.instructions))
                && matches!(
                    call,
                    systemless_future::cpu::Call::DrawPicture { .. }
                        | systemless_future::cpu::Call::TextDraw(_)
                        | systemless_future::cpu::Call::CopyBits(_)
                        | systemless_future::cpu::Call::Port {
                            op: systemless_future::quickdraw::PortOp::PaintRect,
                            ..
                        }
                )
            {
                if draw_diagnostic_count < 512 {
                    display_diagnostics::drawing(&app, call);
                } else if draw_diagnostic_count == 512 {
                    println!("drawing diagnostic limit reached: further samples omitted");
                }
                draw_diagnostic_count = draw_diagnostic_count.saturating_add(1);
            }

            use systemless_future::{cpu::Call, dialogs::Op, memory::GuestPtr};
            if let Call::Dialog(Op::Text { pointer, set, .. }) = call {
                let m = &app.runtime.memory;
                if let Ok(n) = m.read(*pointer, 1) {
                    if let Some(start) = pointer.0.checked_add(1) {
                        if let Ok(bytes) = m.read(GuestPtr(start), usize::from(n[0])) {
                            println!(
                                "dialog item text {}: {:?}",
                                if *set { "set" } else { "get" },
                                String::from_utf8_lossy(bytes)
                            );
                        }
                    }
                }
            }
            if let Call::GetCurrentResource { kind, id } = call {
                let entry = app.runtime.process.resources.get(*kind, *id);
                println!("current resource metadata: kind={kind:?} id={id} bytes={:?} ResErr={} MemErr={}",entry.map(|r|r.data.len()),app.runtime.process.res_error,app.runtime.process.mem_error);
                let m = &app.runtime.memory;
                let depth = (|| -> systemless_future::Result<u16> {
                    let h = m.u32(systemless_future::devices::CURRENT)?;
                    if h == 0 {
                        return Err(systemless_future::Error::Graphics(
                            "no current graphics device".into(),
                        ));
                    }
                    let d = m.u32(GuestPtr(h))?;
                    let map_h = m.u32(GuestPtr(d.checked_add(22).ok_or_else(|| {
                        systemless_future::Error::Graphics("device address overflow".into())
                    })?))?;
                    let map = m.u32(GuestPtr(map_h))?;
                    m.u16(GuestPtr(map.checked_add(32).ok_or_else(|| {
                        systemless_future::Error::Graphics("PixMap address overflow".into())
                    })?))
                })();
                println!("current display PixMap depth: {depth:?}");
            }
            if let systemless_future::cpu::Call::Dialog(systemless_future::dialogs::Op::Param(
                strings,
            )) = call
            {
                for (index, &pointer) in strings.iter().enumerate() {
                    if let Ok(length) = app.runtime.memory.read(pointer, 1) {
                        if let Some(start) = pointer.0.checked_add(1) {
                            if let Ok(bytes) = app.runtime.memory.read(
                                systemless_future::memory::GuestPtr(start),
                                usize::from(length[0]),
                            ) {
                                println!("dialog parameter {index}: {bytes:02x?}");
                            }
                        }
                    }
                }
            }
        }
        if stop == Stop::FlushVolume {
            systemless_future::volume::checkpoint_runtime(None, &mut app.runtime)?;
            println!("acknowledged memory-only volume flush (no host backing)");
            continue;
        }
        if let Stop::Waiting { ticks } = stop {
            if elapse_waits {
                app.runtime.elapse_ticks(ticks)?;
                println!("elapse {ticks} virtual ticks");
                continue;
            }
        }
        if stop != Stop::Budget {
            println!("{stop:?}");
            save_snapshot(&mut app, save_volume, save_screen, display_diagnostics)?;
            return Ok(());
        }
    }
    save_snapshot(&mut app, save_volume, save_screen, display_diagnostics)?;
    Err("trace instruction budget exhausted".into())
}

// Explicit diagnostic output: serialize only committed catalog/fork contents.
// Never overwrite input or existing output images, including on guest faults.
fn save_snapshot(
    app: &mut LoadedApplication,
    output: Option<&String>,
    screen: Option<&String>,
    diagnostics: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if diagnostics {
        display_diagnostics::report(app);
    }
    if let Some(path) = output {
        use std::io::Write;
        let bytes = app.runtime.process.files.export_volume()?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        println!("saved committed volume: {} bytes", bytes.len());
    }
    if let Some(path) = screen {
        use std::io::Write;
        app.runtime.refresh_screen()?;
        let gpu = app
            .runtime
            .graphics
            .as_ref()
            .ok_or("screen capture requires GPU")?;
        let rgba = gpu.read_rgba()?;
        let mut bytes = format!("P6\n{} {}\n255\n", gpu.width, gpu.height).into_bytes();
        for pixel in rgba.as_chunks::<4>().0 {
            bytes.extend_from_slice(&pixel[..3]);
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        println!("saved canonical screen: {}x{}", gpu.width, gpu.height);
    }
    println!(
        "snapshot instructions={} pc={:08x}",
        app.runtime.instructions,
        app.runtime.cpu.pc()
    );
    Ok(())
}
