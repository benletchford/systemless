//! CGrafPort storage and attached-handle ownership shared by both ABIs.
//! IM: Imaging With QuickDraw, Color QuickDraw, OpenCPort/InitCPort/CloseCPort.
use crate::{
    devices,
    memory::{GuestPtr, Memory},
    process::Process,
    Error, Result,
};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Open,
    Init,
    Close,
}
#[derive(Default)]
pub struct Manager {
    ports: BTreeMap<u32, [u32; 7]>,
}
const SIZES: [u32; 7] = [50, 10, 10, 28, 28, 28, 26];
const FIELDS: [u32; 7] = [2, 24, 28, 32, 58, 62, 8];
fn bad(message: &str) -> Error {
    Error::Graphics(message.into())
}
fn owned(memory: &Memory, process: &mut Process, handle: u32, size: u32) -> Result<GuestPtr> {
    let actual = process.handle_size(memory, handle)?;
    if process.mem_error != 0 || actual < size {
        return Err(bad("color port requires live owned attached handles"));
    }
    Ok(GuestPtr(memory.u32(GuestPtr(handle))?))
}
impl Manager {
    fn validate(&self, memory: &Memory, process: &mut Process, port: GuestPtr) -> Result<[u32; 7]> {
        let handles = *self
            .ports
            .get(&port.0)
            .ok_or_else(|| bad("unknown or closed color port"))?;
        if memory.u16(GuestPtr(port.0 + 6))? & 0xc000 != 0xc000 {
            return Err(bad("color port version changed"));
        }
        for (index, &handle) in handles.iter().enumerate() {
            if memory.u32(GuestPtr(port.0 + FIELDS[index]))? != handle {
                return Err(bad("color port attached handle ownership changed"));
            }
            let data = owned(memory, process, handle, SIZES[index])?;
            if (3..=5).contains(&index) && memory.read(data, 20)?.iter().any(|byte| *byte != 0) {
                return Err(bad(
                    "expanded and color pixel-pattern ownership is not implemented",
                ));
            }
        }
        Ok(handles)
    }
    pub(crate) fn attached(
        &self,
        memory: &Memory,
        process: &mut Process,
        port: GuestPtr,
    ) -> Result<[u32; 7]> {
        let handles = self.validate(memory, process, port)?;
        for handle in handles {
            memory.validate_write(GuestPtr(handle), 4)?;
        }
        Ok(handles)
    }
    fn reset(
        memory: &mut Memory,
        process: &mut Process,
        port: GuestPtr,
        handles: [u32; 7],
    ) -> Result<()> {
        let ports = process
            .ports
            .ok_or_else(|| bad("color ports require InitGraf"))?;
        memory.validate_write(port, 108)?;
        memory.validate_write(ports.current_slot, 4)?;
        let mut manager = std::mem::take(&mut process.devices);
        let device = manager.call(memory, process, devices::Op::Current);
        process.devices = manager;
        let gd = owned(memory, process, device?, 62)?;
        let pixels = memory.u32(GuestPtr(gd.0 + 22))?;
        let source = owned(memory, process, pixels, 50)?;
        let pm = memory.read(source, 50)?.to_vec();
        if u16::from_be_bytes(pm[4..6].try_into().unwrap()) & 0xc000 != 0x8000 {
            return Err(bad("current device requires a PixMap"));
        }
        let table = u32::from_be_bytes(pm[42..46].try_into().unwrap());
        let ct = owned(memory, process, table, 8)?;
        let count = u32::from(memory.u16(GuestPtr(ct.0 + 6))?) + 1;
        if count > 256 {
            return Err(bad("invalid current-device color table"));
        }
        owned(memory, process, table, 8 + count * 8)?;
        let (foreground, background) = crate::pixmap::default_colors(memory, source)?;
        let mut visibility = vec![0; 10];
        visibility[..2].copy_from_slice(&10u16.to_be_bytes());
        visibility[2..].copy_from_slice(&pm[6..14]);
        let clip = vec![0, 10, 0x80, 0, 0x80, 0, 0x7f, 0xff, 0x7f, 0xff];
        let pattern = |value| {
            let mut data = vec![0; 28];
            data[20..].fill(value);
            data
        };
        let mut vars = vec![0; 26];
        vars[6..12].copy_from_slice(memory.read(GuestPtr(0xda0), 6)?);
        let data = [
            pm.clone(),
            visibility,
            clip,
            pattern(0),
            pattern(255),
            pattern(255),
            vars,
        ];
        let mut destinations = Vec::new();
        for (index, &handle) in handles.iter().enumerate() {
            let target = owned(memory, process, handle, SIZES[index])?;
            memory.validate_write(target, SIZES[index] as usize)?;
            destinations.push(target);
        }
        let mut bytes = [0; 108];
        bytes[6..8].copy_from_slice(&0xc000u16.to_be_bytes());
        bytes[14..16].copy_from_slice(&0x8000u16.to_be_bytes());
        bytes[16..24].copy_from_slice(&pm[6..14]);
        bytes[42..48].fill(255);
        for (index, &handle) in handles.iter().enumerate() {
            let at = FIELDS[index] as usize;
            bytes[at..at + 4].copy_from_slice(&handle.to_be_bytes());
        }
        for (at, value) in [(52, 1u16), (54, 1), (56, 8), (72, 1)] {
            bytes[at..at + 2].copy_from_slice(&value.to_be_bytes());
        }
        bytes[80..84].copy_from_slice(&foreground.to_be_bytes());
        bytes[84..88].copy_from_slice(&background.to_be_bytes());
        for (destination, bytes) in destinations.into_iter().zip(data) {
            memory.write(destination, &bytes)?;
        }
        memory.write(port, &bytes)?;
        memory.write(ports.current_slot, &port.0.to_be_bytes())?;
        process.mem_error = 0;
        Ok(())
    }
    pub fn call(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        action: Action,
        port: GuestPtr,
    ) -> Result<()> {
        if port.0 == 0 {
            return Err(bad("nil color port"));
        }
        match action {
            Action::Open => {
                memory.validate_write(port, 108)?;
                if self.ports.contains_key(&port.0) {
                    return Err(bad("color port is already open"));
                }
                let ports = process
                    .ports
                    .ok_or_else(|| bad("OpenCPort requires InitGraf"))?;
                memory.validate_write(ports.current_slot, 4)?;
                // Establish the shared device before the port-owned allocation
                // transaction. The device persists independently of port failure.
                let mut devices = std::mem::take(&mut process.devices);
                let device = devices.call(memory, process, devices::Op::Current);
                process.devices = devices;
                device?;
                let mut allocated = Vec::new();
                for size in SIZES {
                    let handle = process.new_handle(memory, size, true)?;
                    if handle == 0 {
                        for handle in allocated.into_iter().rev() {
                            process.dispose_handle(memory, handle)?;
                        }
                        process.mem_error = -108;
                        return Ok(());
                    }
                    allocated.push(handle);
                }
                let handles: [u32; 7] = allocated.try_into().unwrap();
                if let Err(error) = Self::reset(memory, process, port, handles) {
                    for handle in handles.into_iter().rev() {
                        process.dispose_handle(memory, handle)?;
                    }
                    return Err(error);
                }
                self.ports.insert(port.0, handles);
                Ok(())
            }
            Action::Init => {
                memory.read(port, 108)?;
                if memory.u16(GuestPtr(port.0 + 6))? & 0xc000 != 0xc000 {
                    // Apple's InitCPort leaves a basic GrafPort untouched.
                    process.mem_error = memory.u16(GuestPtr(0x220))? as i16;
                    return Ok(());
                }
                let handles = self.validate(memory, process, port)?;
                Self::reset(memory, process, port, handles)
            }
            Action::Close => {
                memory.read(port, 108)?;
                let handles = self.validate(memory, process, port)?;
                for handle in handles {
                    memory.validate_write(GuestPtr(handle), 4)?;
                }
                for handle in handles.into_iter().rev() {
                    process.dispose_handle(memory, handle)?;
                }
                self.ports.remove(&port.0);
                process.mem_error = 0;
                Ok(())
            }
        }
    }
}
