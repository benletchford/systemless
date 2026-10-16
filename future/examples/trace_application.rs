//! Bounded instruction/service trace for diagnosing a file application's startup.
use systemless_future::{cpu::Cpu, load::LoadedApplication, runtime::Stop};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("usage: trace_application code|pef PATH INSTRUCTION_LIMIT [--graphics] [--display WIDTHxHEIGHTx8] [--volume IMAGE] [--services-only] [--elapse-waits]".into());
    }
    let mut graphics = false;
    let mut services_only = false;
    let mut elapse_waits = false;
    let mut volume = None;
    let mut display = None;
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--graphics" if !graphics => graphics = true,
            "--services-only" if !services_only => services_only = true,
            "--elapse-waits" if !elapse_waits => elapse_waits = true,
            "--display" if display.is_none() => {
                index += 1;
                display = Some(
                    args.get(index)
                        .ok_or("missing display")?
                        .parse::<systemless_future::display::Display>()?,
                );
            }
            "--volume" if volume.is_none() => {
                index += 1;
                volume = Some(args.get(index).ok_or("missing volume image")?);
            }
            _ => return Err("invalid trace option".into()),
        }
        index += 1;
    }
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
    let budget: u64 = args[2].parse()?;
    for _ in 0..budget {
        if !services_only {
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
        let calls = app.runtime.services.calls.len();
        let stop = match app.runtime.advance(1) {
            Ok(stop) => stop,
            Err(error) => {
                println!(
                    "latest service before fault: {:?}",
                    app.runtime.services.calls.last()
                );
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
                return Err(error.into());
            }
        };
        for call in &app.runtime.services.calls[calls..] {
            println!("service {call:?}");
            use systemless_future::{cpu::Call, dialogs::Op, memory::GuestPtr};
            if let Call::Dialog(Op::Text {
                pointer, set: true, ..
            }) = call
            {
                let m = &app.runtime.memory;
                if let Ok(n) = m.read(*pointer, 1) {
                    if let Some(start) = pointer.0.checked_add(1) {
                        if let Ok(bytes) = m.read(GuestPtr(start), usize::from(n[0])) {
                            println!("dialog item text: {:?}", String::from_utf8_lossy(bytes));
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
        if let Stop::Waiting { ticks } = stop {
            if elapse_waits {
                app.runtime.elapse_ticks(ticks)?;
                println!("elapse {ticks} virtual ticks");
                continue;
            }
        }
        if stop != Stop::Budget {
            println!("{stop:?}");
            return Ok(());
        }
    }
    Err("trace instruction budget exhausted".into())
}
