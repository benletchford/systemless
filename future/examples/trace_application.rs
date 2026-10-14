//! Bounded instruction/service trace for diagnosing a file application's startup.
use systemless_future::{cpu::Cpu, load::LoadedApplication, runtime::Stop};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(args.len() == 3 || (args.len() == 4 && args[3] == "--graphics")) {
        return Err("usage: trace_application code|pef PATH INSTRUCTION_LIMIT [--graphics]".into());
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
    if args.len() == 4 {
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
        let calls = app.runtime.services.calls.len();
        let stop = match app.runtime.advance(1) {
            Ok(stop) => stop,
            Err(error) => {
                if let Cpu::M68k(c) = &app.runtime.cpu {
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
        }
        if stop != Stop::Budget {
            println!("{stop:?}");
            return Ok(());
        }
    }
    Err("trace instruction budget exhausted".into())
}
