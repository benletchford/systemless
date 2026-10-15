//! Exercise an external sound fork with synthetic 68K/PPC callers. This does not
//! execute the original application or establish gameplay/native audio acceptance.
use systemless_future::{
    abi::Service,
    cpu::{Architecture, Cpu},
    load::{resource::ResourceFork, LoadedApplication, IMAGE_LIMIT, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
fn read(path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(IMAGE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > IMAGE_LIMIT {
        return Err("input exceeds loader bound".into());
    }
    Ok(bytes)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "usage: probe_resource_sound TEMPLATE_CODE_RESOURCE_FORK SOUND_RESOURCE_FORK".into(),
        );
    }
    let template = read(&args[0])?;
    let raw = read(&args[1])?;
    let resources = ResourceFork::parse(&raw)?;
    let ids: Vec<_> = resources
        .entries()
        .filter(|r| r.kind == *b"snd ")
        .map(|r| r.id)
        .collect();
    if ids.is_empty() {
        return Err("no snd resources".into());
    }
    let mut prefixes = std::collections::BTreeMap::new();
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = LoadedApplication::code(&template)?;
        a.runtime.process.resources = resources.clone();
        a.runtime.memory.write(GuestPtr(0x260), &[7])?;
        a.runtime.memory.map(
            GuestPtr(0x51000000),
            vec![0xa8, 5, 0x60, 0xfe],
            Permissions::RX,
        )?;
        // lis r12,$5400; mtctr r12; bctrl; b self. The import gate is the
        // normal ABI boundary, reached through actual PowerPC instructions.
        a.runtime.memory.map(
            GuestPtr(0x53000000),
            [0x3d805400u32, 0x7d8903a6, 0x4e800421, 0x48000000]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
            Permissions::RX,
        )?;
        a.runtime.abi.imports.insert(0x54000000, Service::SndPlay);
        for &id in &ids {
            let h = a
                .runtime
                .process
                .get_resource(&mut a.runtime.memory, *b"snd ", id)?;
            if h == 0 {
                return Err(format!("resource {id} was not loaded").into());
            }
            let free = a.runtime.process.free_memory();
            let stack = STACK_BASE + 0x8000;
            if arch == Architecture::M68k {
                let mut frame = vec![1, 0x55];
                frame.extend(h.to_be_bytes());
                frame.extend(0u32.to_be_bytes());
                frame.extend([0x5a; 2]);
                a.runtime.memory.write(GuestPtr(stack), &frame)?;
                a.runtime.cpu = Cpu::new(arch, 0x51000000, stack);
            } else {
                a.runtime.cpu = Cpu::new(arch, 0x53000000, stack);
                let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
                    unreachable!()
                };
                c.gpr[3] = 0;
                c.gpr[4] = h;
                c.gpr[5] = 1;
            }
            if a.runtime.advance(8)? != (Stop::Waiting { ticks: 1 }) {
                return Err(format!("{arch:?} snd {id} did not enter synchronous play").into());
            }
            let pcm = a.runtime.render_sound(32, 48000)?;
            let hash = pcm
                .iter()
                .flatten()
                .flat_map(|v| v.to_le_bytes())
                .fold(0xcbf29ce484222325u64, |h, b| {
                    (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
                });
            if arch == Architecture::M68k {
                prefixes.insert(id, hash);
            } else if prefixes.get(&id) != Some(&hash) {
                return Err(format!("PCM prefix differs between CPU adapters for snd {id}").into());
            }
            a.runtime.elapse_sound_silently(u32::MAX)?;
            if a.runtime.advance(1)? != Stop::Budget {
                return Err("sound caller did not resume".into());
            }
            let result = match &a.runtime.cpu {
                Cpu::M68k(_) => a.runtime.memory.u16(GuestPtr(stack + 10))? as i16,
                Cpu::PowerPc(c) => c.gpr[3] as i16,
            };
            if result != 0
                || a.runtime.process.free_memory() != free
                || a.runtime.process.sound.playing()
            {
                return Err(format!(
                    "{arch:?} snd {id} failed completion or internal cleanup ({result})"
                )
                .into());
            }
            a.runtime
                .process
                .release_resource(&mut a.runtime.memory, h)?;
            if a.runtime.process.res_error != 0 {
                return Err("resource release failed".into());
            }
            println!("{arch:?} snd {id} prefix_hash={hash:016x} completed=true internal_storage_released=true");
        }
        println!(
            "{arch:?}: {} resources accepted through synthetic guest calls",
            ids.len()
        );
    }
    Ok(())
}
