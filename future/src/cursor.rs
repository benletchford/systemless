//! Process-owned monochrome cursor value, independent of CPU architecture.
mod resources;
use crate::{
    events::Input,
    graphics::Draw,
    memory::{GuestPtr, Memory},
    process::Process,
    Error, Result,
};
pub(crate) use resources::standard;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Init,
    Set(GuestPtr),
    Hide,
    Show,
    Obscure,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Image {
    pub data: [u16; 16],
    pub mask: [u16; 16],
    pub vertical: i16,
    pub horizontal: i16,
}
impl Image {
    pub fn read(memory: &Memory, pointer: GuestPtr) -> Result<Self> {
        if pointer.0 == 0 {
            return Err(Error::Graphics("nil cursor record".into()));
        }
        let bytes = memory.read(pointer, 68)?;
        let row = |offset| u16::from_be_bytes(bytes[offset..offset + 2].try_into().unwrap());
        Ok(Self {
            data: std::array::from_fn(|i| row(i * 2)),
            mask: std::array::from_fn(|i| row(32 + i * 2)),
            vertical: row(64) as i16,
            horizontal: row(66) as i16,
        })
    }
}
pub struct Cursor {
    image: Option<Image>,
    level: i16,
    obscure: bool,
    position: (i16, i16),
    pub inside: bool,
}
impl Default for Cursor {
    fn default() -> Self {
        Self {
            image: None,
            level: 0,
            obscure: false,
            position: (0, 0),
            inside: true,
        }
    }
}
impl Cursor {
    pub fn initialized(&self) -> bool {
        self.image.is_some()
    }
    pub fn visible(&self) -> bool {
        self.initialized() && self.level == 0 && !self.obscure && self.inside
    }
    pub fn observe(&mut self, input: Input) {
        let position = (input.horizontal, input.vertical);
        if position != self.position {
            self.obscure = false;
            self.position = position;
        }
    }
    pub fn apply(
        &mut self,
        memory: &Memory,
        ports: Option<crate::quickdraw::Ports>,
        input: Input,
        op: Op,
    ) -> Result<()> {
        match op {
            Op::Init => {
                let ports =
                    ports.ok_or_else(|| Error::Graphics("InitCursor requires InitGraf".into()))?;
                let address = ports
                    .current_slot
                    .0
                    .checked_sub(108)
                    .ok_or_else(|| Error::Graphics("invalid QuickDraw arrow address".into()))?;
                let image = Image::read(memory, GuestPtr(address))?;
                self.image = Some(image);
                self.level = 0;
                self.obscure = false;
                self.position = (input.horizontal, input.vertical);
            }
            Op::Set(pointer) => {
                let image = Image::read(memory, pointer)?;
                self.image = Some(image);
            }
            Op::Hide => self.level = self.level.saturating_sub(1),
            Op::Show => self.level = (self.level.saturating_add(1)).min(0),
            Op::Obscure => {
                self.position = (input.horizontal, input.vertical);
                self.obscure = true;
            }
        }
        Ok(())
    }
    pub fn draw(&self, input: Input) -> Draw<'static> {
        match self.image.filter(|_| self.visible()) {
            Some(image) => Draw::Cursor {
                x: i32::from(input.horizontal) - i32::from(image.horizontal),
                y: i32::from(input.vertical) - i32::from(image.vertical),
                data: image.data,
                mask: image.mask,
            },
            None => Draw::Present,
        }
    }
}
pub fn call(memory: &Memory, process: &mut Process, op: Op) -> Result<()> {
    process
        .cursor
        .apply(memory, process.ports, process.input, op)
}
