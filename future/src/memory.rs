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
    /// Adopt prepared mappings without cloning existing guest storage. On any
    /// overlap the destination is unchanged, including otherwise valid mappings
    /// preceding the conflict in the incoming image.
    pub(crate) fn adopt(&mut self, mut image: Self) -> Result<()> {
        for incoming in &image.mappings {
            let end = u64::from(incoming.base) + incoming.bytes.len() as u64;
            if self.mappings.iter().any(|existing| {
                u64::from(incoming.base) < u64::from(existing.base) + existing.bytes.len() as u64
                    && u64::from(existing.base) < end
            }) {
                return Err(Error::Mapping(incoming.base));
            }
        }
        self.mappings.append(&mut image.mappings);
        Ok(())
    }
    pub(crate) fn range_available(&self, base: GuestPtr, size: usize) -> bool {
        let end = u64::from(base.0) + size as u64;
        size != 0
            && end <= 1u64 << 32
            && !self.mappings.iter().any(|m| {
                u64::from(base.0) < u64::from(m.base) + m.bytes.len() as u64
                    && u64::from(m.base) < end
            })
    }
    pub(crate) fn mapping_bases(&self) -> Vec<u32> {
        self.mappings.iter().map(|m| m.base).collect()
    }
    pub(crate) fn remove_mappings(&mut self, bases: &[u32]) {
        self.mappings.retain(|m| !bases.contains(&m.base));
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_adoption_is_atomic_and_preserves_permissions_and_existing_bytes() {
        let mut memory = Memory::default();
        memory
            .map(GuestPtr(0x1000), vec![9; 16], Permissions::RW)
            .unwrap();
        let mut conflict = Memory::default();
        conflict
            .map(GuestPtr(0x2000), vec![1; 16], Permissions::RX)
            .unwrap();
        conflict
            .map(GuestPtr(0x100f), vec![2; 16], Permissions::RW)
            .unwrap();
        assert!(memory.adopt(conflict).is_err());
        assert!(memory.read(GuestPtr(0x2000), 1).is_err());
        assert_eq!(memory.read(GuestPtr(0x1000), 16).unwrap(), &[9; 16]);
        let mut image = Memory::default();
        image
            .map(GuestPtr(0x1010), vec![3; 16], Permissions::RX)
            .unwrap();
        image
            .map(GuestPtr(0xffff_fffc), vec![4; 4], Permissions::RW)
            .unwrap();
        memory.adopt(image).unwrap();
        assert_eq!(memory.fetch(GuestPtr(0x1010), 4, 4).unwrap(), &[3; 4]);
        assert!(memory.write(GuestPtr(0x1010), &[0]).is_err());
        assert!(memory.fetch(GuestPtr(0xffff_fffc), 4, 4).is_err());
        memory.write(GuestPtr(0xffff_ffff), &[5]).unwrap();
        memory.write(GuestPtr(0x1000), &[8]).unwrap();
        assert_eq!(memory.read(GuestPtr(0x1000), 2).unwrap(), &[8, 9]);
        assert_eq!(
            memory.read(GuestPtr(0xffff_fffc), 4).unwrap(),
            &[4, 4, 4, 5]
        );
    }
}
