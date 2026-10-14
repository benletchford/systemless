use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GuestPtr(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
    Execute,
}
#[derive(Debug, Clone, Copy)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}
impl Permissions {
    pub const RW: Self = Self {
        read: true,
        write: true,
        execute: false,
    };
    pub const RWX: Self = Self {
        read: true,
        write: true,
        execute: true,
    };
    pub const RX: Self = Self {
        read: true,
        write: false,
        execute: true,
    };
}
struct Mapping {
    base: u32,
    bytes: Vec<u8>,
    permissions: Permissions,
}
#[derive(Default)]
pub struct Memory {
    mappings: Vec<Mapping>,
}
impl Memory {
    pub fn map(&mut self, base: GuestPtr, bytes: Vec<u8>, permissions: Permissions) -> Result<()> {
        let end = u64::from(base.0) + bytes.len() as u64;
        if bytes.is_empty()
            || end > 1u64 << 32
            || self.mappings.iter().any(|m| {
                u64::from(base.0) < u64::from(m.base) + m.bytes.len() as u64
                    && u64::from(m.base) < end
            })
        {
            return Err(Error::Mapping(base.0));
        }
        self.mappings.push(Mapping {
            base: base.0,
            bytes,
            permissions,
        });
        Ok(())
    }
    fn locate(&self, p: GuestPtr, len: usize, access: Access) -> Result<(usize, usize)> {
        let end = u64::from(p.0)
            .checked_add(len as u64)
            .ok_or(Error::Memory {
                address: p.0,
                access,
            })?;
        if end <= 1u64 << 32 {
            for (i, m) in self.mappings.iter().enumerate() {
                let allowed = match access {
                    Access::Read => m.permissions.read,
                    Access::Write => m.permissions.write,
                    Access::Execute => m.permissions.execute,
                };
                if allowed && p.0 >= m.base && end <= u64::from(m.base) + m.bytes.len() as u64 {
                    return Ok((i, (p.0 - m.base) as usize));
                }
            }
        }
        Err(Error::Memory {
            address: p.0,
            access,
        })
    }
    pub fn read(&self, p: GuestPtr, len: usize) -> Result<&[u8]> {
        let (i, o) = self.locate(p, len, Access::Read)?;
        Ok(&self.mappings[i].bytes[o..o + len])
    }
    pub fn validate_write(&self, p: GuestPtr, len: usize) -> Result<()> {
        self.locate(p, len, Access::Write).map(|_| ())
    }
    pub fn write(&mut self, p: GuestPtr, bytes: &[u8]) -> Result<()> {
        let (i, o) = self.locate(p, bytes.len(), Access::Write)?;
        self.mappings[i].bytes[o..o + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }
    pub fn fetch(&self, p: GuestPtr, len: usize, alignment: u32) -> Result<&[u8]> {
        if alignment == 0 || !p.0.is_multiple_of(alignment) {
            return Err(Error::Alignment(p.0));
        }
        let (i, o) = self.locate(p, len, Access::Execute)?;
        Ok(&self.mappings[i].bytes[o..o + len])
    }
    pub fn u16(&self, p: GuestPtr) -> Result<u16> {
        Ok(u16::from_be_bytes(self.read(p, 2)?.try_into().unwrap()))
    }
    pub fn u32(&self, p: GuestPtr) -> Result<u32> {
        Ok(u32::from_be_bytes(self.read(p, 4)?.try_into().unwrap()))
    }
}
