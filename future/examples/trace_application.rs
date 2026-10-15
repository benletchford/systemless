//! Bounded instruction/service trace for diagnosing a file application's startup.
use systemless_future::{cpu::Cpu, load::LoadedApplication, runtime::Stop};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("usage: trace_application code|pef PATH INSTRUCTION_LIMIT [--graphics] [--volume IMAGE] [--services-only]".into());
    }
    let mut graphics = false;
    let mut services_only = false;
    let mut volume = None;
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--graphics" if !graphics => graphics = true,
            "--services-only" if !services_only => services_only = true,
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
                512,
                342,
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
                return Err(error.into());
            }
        };
        for call in &app.runtime.services.calls[calls..] {
            println!("service {call:?}");
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
        if stop != Stop::Budget {
            println!("{stop:?}");
            return Ok(());
        }
    }
    Err("trace instruction budget exhausted".into())
}
