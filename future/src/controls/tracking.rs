//! Cooperative tracking of the standard button family. The CPU reply stays pending.
use super::{fail, Controls, Op};
use crate::{
    graphics::Graphics,
    memory::{GuestPtr, Memory},
    process::Process,
    Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackControl {
    pub handle: u32,
    pub point: [i16; 2],
    pub action: u32,
}
pub(crate) struct Tracking {
    pub handle: u32,
    owner: u32,
    part: u16,
}
impl Controls {
    pub(crate) fn begin_tracking(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        request: TrackControl,
    ) -> Result<Option<Tracking>> {
        let record = self.record(memory, process, request.handle)?;
        let owner = self.records[&request.handle].0;
        if !process.windows.contains(memory, owner)?
            || !self.list(memory, process, owner)?.contains(&request.handle)
        {
            return Err(fail("tracking control is not in its live owner list"));
        }
        let action = if request.action == u32::MAX {
            memory.u32(GuestPtr(record.0 + 32))?
        } else {
            request.action
        };
        if action != 0 {
            return Err(fail("control action callbacks are not implemented"));
        }
        let part = self.call(
            memory,
            process,
            None,
            Op::Test(request.handle, request.point[1], request.point[0]),
        )? as u16;
        Ok((part != 0).then_some(Tracking {
            handle: request.handle,
            owner,
            part,
        }))
    }
    pub(crate) fn poll_tracking(
        &mut self,
        memory: &mut Memory,
        process: &mut Process,
        gpu: Option<&Graphics>,
        tracking: &Tracking,
        global_point: [u8; 4],
        released: bool,
    ) -> Result<Option<u16>> {
        let record = self.record(memory, process, tracking.handle)?;
        if self.records[&tracking.handle].0 != tracking.owner
            || !process.windows.contains(memory, tracking.owner)?
        {
            return Err(fail("tracked control owner changed"));
        }
        self.list(memory, process, tracking.owner)?;
        let ports = process
            .ports
            .ok_or_else(|| fail("control tracking requires InitGraf"))?;
        let point = ports.point_coordinates_for_port(
            memory,
            GuestPtr(tracking.owner),
            global_point,
            false,
        )?;
        let v = i16::from_be_bytes(point[..2].try_into().unwrap());
        let h = i16::from_be_bytes(point[2..].try_into().unwrap());
        let part = self.call(memory, process, None, Op::Test(tracking.handle, h, v))? as u16;
        let current = memory.read(GuestPtr(record.0 + 17), 1)?[0];
        let desired = if current == 255 {
            255
        } else if !released && part == tracking.part {
            tracking.part as u8
        } else {
            0
        };
        if desired != current {
            memory.write(GuestPtr(record.0 + 17), &[desired])?;
            let old_error = process.mem_error;
            let result = self.draw(memory, process, gpu, tracking.handle, false);
            process.mem_error = old_error;
            if let Err(error) = result {
                memory.write(GuestPtr(record.0 + 17), &[current])?;
                return Err(error);
            }
        }
        Ok(released.then_some(if part == tracking.part { part } else { 0 }))
    }
}
