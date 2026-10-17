//! Host pointer conversion and delivery into the shared Macintosh OS queue.
mod keyboard;
use crate::{
    events::{self, Input},
    memory::{GuestPtr, Memory},
    Result,
};
pub use keyboard::{virtual_key, Keyboard};
#[derive(Default)]
pub struct Pointer {
    inside: bool,
    down: bool,
}
impl Pointer {
    /// Physical window pixels use the same centered aspect fit as presentation.
    pub fn motion(
        &mut self,
        input: &mut Input,
        memory: &mut Memory,
        position: (f64, f64),
        window: (u32, u32),
        screen: (u32, u32),
    ) -> Result<()> {
        let (x, y) = position;
        self.inside = false;
        if window.0 == 0
            || window.1 == 0
            || screen.0 == 0
            || screen.1 == 0
            || !x.is_finite()
            || !y.is_finite()
        {
            return Ok(());
        }
        let scale = (f64::from(window.0) / f64::from(screen.0))
            .min(f64::from(window.1) / f64::from(screen.1));
        let gx = (x - (f64::from(window.0) - f64::from(screen.0) * scale) / 2.0) / scale;
        let gy = (y - (f64::from(window.1) - f64::from(screen.1) * scale) / 2.0) / scale;
        self.inside =
            gx >= 0.0 && gy >= 0.0 && gx < f64::from(screen.0) && gy < f64::from(screen.1);
        let horizontal =
            gx.floor()
                .clamp(0.0, f64::from(screen.0 - 1).min(f64::from(i16::MAX))) as i16;
        let vertical =
            gy.floor()
                .clamp(0.0, f64::from(screen.1 - 1).min(f64::from(i16::MAX))) as i16;
        let mut point = vertical.to_be_bytes().to_vec();
        point.extend(horizontal.to_be_bytes());
        memory.write(events::MOUSE, &point)?;
        input.horizontal = horizontal;
        input.vertical = vertical;
        Ok(())
    }
    pub fn inside(&self) -> bool {
        self.inside
    }
    pub fn leave(&mut self) {
        self.inside = false;
    }
    pub fn button(&mut self, memory: &mut Memory, input: &mut Input, down: bool) -> Result<()> {
        if self.down == down || (down && !self.inside) {
            return Ok(());
        }
        let mut next = *input;
        next.modifiers = (next.modifiers & !0x80) | if down { 0 } else { 0x80 };
        let ticks = memory.u32(GuestPtr(0x16a))?;
        memory.validate_write(events::MB_STATE, 1)?;
        let state = memory.read(events::MB_STATE, 1)?[0];
        events::post(memory, if down { 1 } else { 2 }, 0, ticks, next)?;
        memory.write(
            events::MB_STATE,
            &[if down { state & !0x80 } else { state | 0x80 }],
        )?;
        // A masked event still changes the physical button state.
        *input = next;
        self.down = down;
        Ok(())
    }
    pub fn unfocus(&mut self, memory: &mut Memory, input: &mut Input) -> Result<()> {
        self.leave();
        self.button(memory, input, false)?;
        input.modifiers &= !0x1f00;
        Ok(())
    }
}
