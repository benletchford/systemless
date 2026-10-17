//! Guest keyboard driver state: sixteen bytes at low-memory KeyMap ($0174).
//! Preserved Systemless uses byte=virtual_code/8, mask=1<<(virtual_code%8).
use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
pub const KEY_MAP: GuestPtr = GuestPtr(0x174);
pub fn get_keys(memory: &mut Memory, destination: GuestPtr) -> Result<()> {
    if destination.0 == 0 {
        return Err(Error::Cpu("nil GetKeys destination".into()));
    }
    memory.validate_write(destination, 16)?;
    let bytes: [u8; 16] = memory.read(KEY_MAP, 16)?.try_into().unwrap();
    memory.write(destination, &bytes)
}
