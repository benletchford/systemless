//! Read-only snapshots for separating guest drawing, copies and display CLUTs.
use systemless_future::{
    cpu::Call,
    load::LoadedApplication,
    memory::{GuestPtr, Memory},
};
pub fn report(app: &LoadedApplication) {
    let m = &app.runtime.memory;
    let main = (|| {
        let handle = m.u32(systemless_future::devices::MAIN)?;
        let device = m.u32(GuestPtr(handle))?;
        let pixmap_handle = m.u32(plus(device, 22)?)?;
        let pixmap = m.u32(GuestPtr(pixmap_handle))?;
        Ok::<_, systemless_future::Error>((handle, device, pixmap))
    })();
    println!("display diagnostic main={main:08x?}");
    if let Ok((_, _, p)) = main {
        describe(app, "main PixMap", GuestPtr(p), true);
    }
    current_port(app);
    if let Some(Call::CopyBits(copy)) = app
        .runtime
        .services
        .calls
        .iter()
        .rev()
        .find(|v| matches!(v, Call::CopyBits(_)))
    {
        println!(
            "display diagnostic last CopyBits recorded pointers (live snapshot, not historical values)={copy:?} source rect={:?} destination rect={:?}",
            systemless_future::quickdraw::read_rect(m, copy.source_rect),
            systemless_future::quickdraw::read_rect(m, copy.destination_rect)
        );
        describe(app, "last copy source", copy.source, false);
        describe(app, "last copy destination", copy.destination, false);
    }
}
pub fn drawing(app: &LoadedApplication, call: &Call) {
    println!(
        "drawing diagnostic instruction={} after={call:?}",
        app.runtime.instructions
    );
    current_port(app);
}
fn current_port(app: &LoadedApplication) {
    let m = &app.runtime.memory;
    if let Some(ports) = app.runtime.process.ports {
        if let Ok(p) = ports.current(m) {
            println!(
                "display diagnostic current port={:08x} bytes={:02x?}",
                p.0,
                m.read(p, 108)
            );
            for (name, offset) in [("visible", 24), ("clip", 28)] {
                if let Ok(h) = plus(p.0, offset).and_then(|at| m.u32(at)) {
                    if let Ok(data) = m.u32(GuestPtr(h)) {
                        println!("display diagnostic {name} handle={h:08x} data={data:08x} prefix={:02x?}",m.read(GuestPtr(data),10));
                    }
                }
            }
            if let Ok(bitmap) = plus(p.0, 2) {
                describe(app, "current port bitmap", bitmap, false);
            }
        }
    }
}
fn describe(app: &LoadedApplication, label: &str, mut pointer: GuestPtr, known_pixmap: bool) {
    let m = &app.runtime.memory;
    let result = (|| -> systemless_future::Result<()> {
        let header = m.read(pointer, 14)?;
        let row = u16::from_be_bytes(header[4..6].try_into().unwrap());
        let mut color = known_pixmap || row & 0x8000 != 0;
        if row & 0xc000 == 0xc000 && !known_pixmap {
            let h = u32::from_be_bytes(header[..4].try_into().unwrap());
            pointer = GuestPtr(m.u32(GuestPtr(h))?);
            color = true;
        }
        let b = m.read(pointer, if color { 50 } else { 14 })?;
        println!(
            "display diagnostic {label} descriptor={:08x} bytes={b:02x?}",
            pointer.0
        );
        let base = GuestPtr(u32::from_be_bytes(b[..4].try_into().unwrap()));
        let stride = usize::from(u16::from_be_bytes(b[4..6].try_into().unwrap()) & 0x3fff);
        let word = |at| i16::from_be_bytes(b[at..at + 2].try_into().unwrap()) as i32;
        let width = word(12) - word(8);
        let height = word(10) - word(6);
        let depth = if color {
            u16::from_be_bytes(b[32..34].try_into().unwrap())
        } else if base == systemless_future::quickdraw::SCREEN {
            app.runtime.display().depth()
        } else {
            1
        };
        if !(1..=4096).contains(&width)
            || !(1..=4096).contains(&height)
            || ![1, 2, 4, 8].contains(&depth)
            || width as usize * usize::from(depth) > stride * 8
        {
            println!("display diagnostic {label}: unsupported histogram geometry/depth");
            return Ok(());
        }
        let data = m.read(base, stride * height as usize)?;
        let mut histogram = [0usize; 256];
        let mut content = [0usize; 256];
        for y in 0..height as usize {
            for x in 0..width as usize {
                let bit = x * usize::from(depth);
                let index = usize::from(
                    (data[y * stride + bit / 8] >> (8 - usize::from(depth) - bit % 8))
                        & ((1u16 << depth) - 1) as u8,
                );
                histogram[index] += 1;
                if y >= 20 {
                    content[index] += 1;
                }
            }
        }
        let used: Vec<_> = histogram
            .iter()
            .enumerate()
            .filter(|(_, n)| **n != 0)
            .map(|(i, n)| (i, *n, content[i]))
            .collect();
        println!("display diagnostic {label} base={:08x} stride={stride} depth={depth} size={width}x{height} histogram(index,total,rows20plus)={used:?}",base.0);
        if color {
            palette(m, label, b, &histogram)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        println!("display diagnostic {label} unavailable: {e}");
    }
}
fn palette(
    m: &Memory,
    label: &str,
    b: &[u8],
    histogram: &[usize; 256],
) -> systemless_future::Result<()> {
    let handle = u32::from_be_bytes(b[42..46].try_into().unwrap());
    let table = GuestPtr(m.u32(GuestPtr(handle))?);
    let size = usize::from(m.u16(plus(table.0, 6)?)?) + 1;
    if size > 256 {
        println!("display diagnostic {label} CLUT too large: {size}");
        return Ok(());
    }
    let bytes = m.read(table, 8 + size * 8)?;
    println!(
        "display diagnostic {label} CLUT handle={handle:08x} data={:08x} header={:02x?}",
        table.0,
        &bytes[..8]
    );
    for ordinal in 0..size {
        let entry = &bytes[8 + ordinal * 8..16 + ordinal * 8];
        let value = u16::from_be_bytes(entry[..2].try_into().unwrap());
        let device_table = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) & 0x8000 != 0;
        let used = if device_table {
            histogram[ordinal] != 0
        } else {
            usize::from(value) < 256 && histogram[usize::from(value)] != 0
        };
        if used {
            let rgb: Vec<_> = entry[2..]
                .chunks_exact(2)
                .map(|w| u16::from_be_bytes(w.try_into().unwrap()))
                .collect();
            println!("display diagnostic {label} CLUT ordinal={ordinal} value={value} rgb={rgb:?}");
        }
    }
    Ok(())
}

fn plus(base: u32, offset: u32) -> systemless_future::Result<GuestPtr> {
    base.checked_add(offset)
        .map(GuestPtr)
        .ok_or_else(|| systemless_future::Error::Cpu("display diagnostic pointer overflow".into()))
}
