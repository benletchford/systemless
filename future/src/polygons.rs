//! Live polygon handles and recording. Destination coverage executes through wgpu.
use crate::{
    graphics::Graphics,
    memory::{GuestPtr, Memory},
    process::Process,
    Error, Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Frame,
    Paint,
    Erase,
    Invert,
    Fill(GuestPtr),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Open,
    Close,
    Kill(u32),
    Offset { handle: u32, dh: i16, dv: i16 },
    Draw { handle: u32, verb: Verb },
}
#[derive(Default)]
pub struct Manager {
    active: Option<(u32, GuestPtr)>,
}
fn bad(s: &str) -> Error {
    Error::Graphics(s.into())
}
fn encode(points: &[(i16, i16)]) -> Result<Vec<u8>> {
    if points.len() > 16381 {
        return Err(bad("polygon size exceeds 64 KB"));
    }
    let mut b = vec![0; 10];
    b[..2].copy_from_slice(&((10 + points.len() * 4) as u16).to_be_bytes());
    if !points.is_empty() {
        let values = [
            points.iter().map(|p| p.1).min().unwrap(),
            points.iter().map(|p| p.0).min().unwrap(),
            points.iter().map(|p| p.1).max().unwrap(),
            points.iter().map(|p| p.0).max().unwrap(),
        ];
        for (i, v) in values.into_iter().enumerate() {
            b[2 + i * 2..4 + i * 2].copy_from_slice(&v.to_be_bytes());
        }
    }
    for &(h, v) in points {
        b.extend(v.to_be_bytes());
        b.extend(h.to_be_bytes());
    }
    Ok(b)
}
type Record = (Vec<u8>, Vec<(i16, i16)>);
fn read(memory: &Memory, process: &mut Process, h: u32) -> Result<Record> {
    let old_error = process.mem_error;
    let result = process.handle_size(memory, h);
    let error = process.mem_error;
    process.mem_error = old_error;
    let n = result? as usize;
    if error != 0 || !(10..=65534).contains(&n) {
        return Err(bad("invalid polygon handle storage"));
    }
    let b = memory.read(GuestPtr(memory.u32(GuestPtr(h))?), n)?.to_vec();
    let size = usize::from(u16::from_be_bytes(b[..2].try_into().unwrap()));
    if size < 10 || size > n || !(size - 10).is_multiple_of(4) {
        return Err(bad("invalid polygon size"));
    }
    let points = b[10..size]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            (
                i16::from_be_bytes(p[2..4].try_into().unwrap()),
                i16::from_be_bytes(p[..2].try_into().unwrap()),
            )
        })
        .collect();
    Ok((b, points))
}
impl Manager {
    pub fn call(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: Option<&Graphics>,
        op: Op,
    ) -> Result<u32> {
        match op {
            Op::Open => {
                if self.active.is_some() {
                    return Err(bad("nested polygon recording is unsupported"));
                }
                let ports = process
                    .ports
                    .ok_or_else(|| bad("OpenPoly requires InitGraf"))?;
                let port = ports.current(memory)?;
                for at in [92, 96, 100, 104] {
                    if memory.u32(GuestPtr(port.0 + at))? != 0 {
                        return Err(bad(
                            "foreign recording or custom procedures are unsupported",
                        ));
                    }
                }
                memory.validate_write(GuestPtr(port.0 + 66), 2)?;
                memory.validate_write(GuestPtr(port.0 + 100), 4)?;
                let vis = (memory.u16(GuestPtr(port.0 + 66))? as i16).wrapping_sub(1);
                let h = process.new_handle(memory, 10, false)?;
                if h == 0 {
                    return Ok(0);
                }
                memory.write(GuestPtr(memory.u32(GuestPtr(h))?), &encode(&[])?)?;
                memory.write(GuestPtr(port.0 + 66), &vis.to_be_bytes())?;
                memory.write(GuestPtr(port.0 + 100), &h.to_be_bytes())?;
                self.active = Some((h, port));
                Ok(h)
            }
            Op::Close => {
                let (h, port) = self
                    .active
                    .ok_or_else(|| bad("ClosePoly without OpenPoly"))?;
                if process
                    .ports
                    .ok_or_else(|| bad("ClosePoly requires InitGraf"))?
                    .current(memory)?
                    != port
                    || memory.u32(GuestPtr(port.0 + 100))? != h
                {
                    return Err(bad("polygon recording port or handle changed"));
                }
                read(memory, process, h)?;
                memory.validate_write(GuestPtr(port.0 + 66), 2)?;
                memory.validate_write(GuestPtr(port.0 + 100), 4)?;
                let vis = (memory.u16(GuestPtr(port.0 + 66))? as i16).wrapping_add(1);
                memory.write(GuestPtr(port.0 + 66), &vis.to_be_bytes())?;
                memory.write(GuestPtr(port.0 + 100), &0u32.to_be_bytes())?;
                self.active = None;
                Ok(0)
            }
            Op::Kill(h) => {
                if h == 0 {
                    return Ok(0);
                }
                if self.active.is_some_and(|a| a.0 == h) {
                    return Err(bad("cannot dispose an open polygon"));
                }
                process.dispose_handle(memory, h)?;
                Ok(0)
            }
            Op::Offset { handle, dh, dv } => {
                if handle == 0 {
                    return Ok(0);
                }
                if self.active.is_some_and(|a| a.0 == handle) {
                    return Err(bad("cannot offset an open polygon"));
                }
                let (mut bytes, _) = read(memory, process, handle)?;
                let size = usize::from(u16::from_be_bytes(bytes[..2].try_into().unwrap()));
                for at in (2..size).step_by(4) {
                    let v =
                        i16::from_be_bytes(bytes[at..at + 2].try_into().unwrap()).wrapping_add(dv);
                    let h = i16::from_be_bytes(bytes[at + 2..at + 4].try_into().unwrap())
                        .wrapping_add(dh);
                    bytes[at..at + 2].copy_from_slice(&v.to_be_bytes());
                    bytes[at + 2..at + 4].copy_from_slice(&h.to_be_bytes());
                }
                memory.write(GuestPtr(memory.u32(GuestPtr(handle))?), &bytes)?;
                Ok(0)
            }
            Op::Draw { handle, verb } => {
                if handle == 0 {
                    return Ok(0);
                }
                if self.active.is_some_and(|a| a.0 == handle) {
                    return Err(bad("cannot draw an open polygon"));
                }
                let (bytes, points) = read(memory, process, handle)?;
                if points.is_empty() {
                    return Ok(0);
                }
                let canonical = encode(&points)?;
                if bytes[2..10] != canonical[2..10] {
                    return Err(bad("polygon bounds do not match live vertices"));
                }
                let ports = process
                    .ports
                    .ok_or_else(|| bad("polygon drawing requires InitGraf"))?;
                ports.draw_polygon(memory, gpu, &points, verb)?;
                Ok(0)
            }
        }
    }
    pub fn line(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: Option<&Graphics>,
        h: i16,
        v: i16,
        relative: bool,
    ) -> Result<bool> {
        let ports = process
            .ports
            .ok_or_else(|| bad("line drawing requires InitGraf"))?;
        let port = ports.current(memory)?;
        let save = memory.u32(GuestPtr(port.0 + 100))?;
        if save == 0 {
            return Ok(false);
        }
        if self.active != Some((save, port)) {
            return Err(bad("foreign polygon recording state"));
        }
        for at in [92, 96, 104] {
            if memory.u32(GuestPtr(port.0 + at))? != 0 {
                return Err(bad(
                    "combined recording or custom procedures are unsupported",
                ));
            }
        }
        let from = (
            memory.u16(GuestPtr(port.0 + 50))? as i16,
            memory.u16(GuestPtr(port.0 + 48))? as i16,
        );
        let to = if relative {
            (from.0.saturating_add(h), from.1.saturating_add(v))
        } else {
            (h, v)
        };
        let (old, mut points) = read(memory, process, save)?;
        if points.is_empty() {
            points.push(from);
        } else if points.last() != Some(&from) {
            return Err(bad("disconnected polygon lines are unsupported"));
        }
        points.push(to);
        let bytes = encode(&points)?;
        memory.validate_write(GuestPtr(port.0 + 48), 4)?;
        process.replace_handles(memory, vec![(save, bytes)])?;
        if process.mem_error != 0 {
            return Ok(true);
        }
        let result = if memory.u16(GuestPtr(port.0 + 66))? as i16 >= 0 {
            ports.line_recorded(memory, gpu, h, v, relative)
        } else {
            let mut p = Vec::new();
            p.extend(to.1.to_be_bytes());
            p.extend(to.0.to_be_bytes());
            memory.write(GuestPtr(port.0 + 48), &p)
        };
        if result.is_err() {
            process.replace_handles(memory, vec![(save, old)])?;
        }
        result?;
        Ok(true)
    }
}
