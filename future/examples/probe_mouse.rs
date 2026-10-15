//! Bounded original 68K mouse probe. Writes PPM screenshots without a host window.
use std::{io::Read, path::Path};
use systemless_future::{
    cpu::Cpu,
    graphics::Graphics,
    host_input::Pointer,
    load::{LoadedApplication, IMAGE_LIMIT},
    memory::GuestPtr,
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
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: probe_mouse CODE_RESOURCE_FORK OUTPUT_PREFIX X Y".into());
    }
    let mut fork = Vec::new();
    std::fs::File::open(&args[0])?
        .take(IMAGE_LIMIT as u64 + 1)
        .read_to_end(&mut fork)?;
    let mut app = LoadedApplication::code(&fork)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut gpu = pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000)))?;
    gpu.palette[0] = [255; 4];
    gpu.palette[1] = [0, 0, 0, 255];
    app.runtime.graphics = Some(gpu);
    println!("startup {:?}", app.runtime.advance(30_000)?);
    screenshot(&mut app, Path::new(&format!("{}-before.ppm", args[1])))?;
    let mut pointer = Pointer::default();
    pointer.motion(
        &mut app.runtime.process.input,
        &mut app.runtime.memory,
        (args[2].parse()?, args[3].parse()?),
        (512, 342),
        (512, 342),
    )?;
    pointer.button(
        &mut app.runtime.memory,
        &mut app.runtime.process.input,
        true,
    )?;
    let first = app.runtime.services.calls.len();
    let mut result = app.runtime.advance(30_000);
    println!("mouse down {result:?}");
    if result.is_ok() {
        pointer.button(
            &mut app.runtime.memory,
            &mut app.runtime.process.input,
            false,
        )?;
        result = app.runtime.advance(30_000);
        println!("mouse up {result:?}");
    }
    for call in &app.runtime.services.calls[first..] {
        println!("service {call:?}");
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
    if result.is_err() {
        if let Cpu::M68k(c) = &app.runtime.cpu {
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
