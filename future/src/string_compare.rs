//! Roman EqualString: IM Text (1993), 5-51–52; conversion tables IM IV, IV-235.
use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bytes {
    pub pointer: GuestPtr,
    pub length: u16,
}
impl Bytes {
    pub fn pascal(m: &Memory, pointer: GuestPtr) -> Result<Self> {
        let length = u16::from(m.read(pointer, 1)?[0]);
        let start = if length == 0 {
            pointer
        } else {
            GuestPtr(
                pointer
                    .0
                    .checked_add(1)
                    .ok_or_else(|| Error::Cpu("Pascal string address overflow".into()))?,
            )
        };
        Ok(Self {
            pointer: start,
            length,
        })
    }
    fn read(self, m: &Memory) -> Result<&[u8]> {
        if self.length == 0 {
            Ok(&[])
        } else {
            m.read(self.pointer, usize::from(self.length))
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Equal {
    pub first: Bytes,
    pub second: Bytes,
    pub case_sensitive: bool,
    pub marks_sensitive: bool,
}
fn strip(byte: u8) -> u8 {
    match byte {
        0x80 | 0x81 | 0xcb | 0xcc => b'A',
        0x82 => b'C',
        0x83 => b'E',
        0x84 => b'N',
        0x85 | 0xcd | 0xaf => b'O',
        0x86 => b'U',
        0x87..=0x8c | 0xbb => b'a',
        0x8d => b'c',
        0x8e..=0x91 => b'e',
        0x92..=0x95 => b'i',
        0x96 => b'n',
        0x97..=0x9b | 0xbf | 0xbc => b'o',
        0x9c..=0x9f => b'u',
        0xd8 => b'y',
        _ => byte,
    }
}
fn upper(byte: u8) -> u8 {
    match byte {
        b'a'..=b'z' => byte - 32,
        0x88 => 0xcb,
        0x8b => 0xcc,
        0x8a => 0x80,
        0x8c => 0x81,
        0xbe => 0xae,
        0x8d => 0x82,
        0x8e => 0x83,
        0x96 => 0x84,
        0x9a => 0x85,
        0x9b => 0xcd,
        0xbf => 0xaf,
        0xcf => 0xce,
        0x9f => 0x86,
        _ => byte,
    }
}
impl Equal {
    pub fn evaluate(self, m: &Memory) -> Result<bool> {
        // Validate both complete nonempty spans, even when lengths differ.
        let a = self.first.read(m)?;
        let b = self.second.read(m)?;
        let key = |byte| {
            let byte = if self.marks_sensitive {
                byte
            } else {
                strip(byte)
            };
            if self.case_sensitive {
                byte
            } else {
                upper(byte)
            }
        };
        Ok(a.len() == b.len() && a.iter().copied().map(key).eq(b.iter().copied().map(key)))
    }
}
