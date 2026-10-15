//! Checked standard MENU records. Resource and live record layouts share this prefix.
use crate::{Error, Result};
#[derive(Debug)]
pub struct Item<'a> {
    pub text: &'a [u8],
    pub icon: u8,
    pub command: u8,
    pub mark: u8,
    pub style: u8,
}
#[derive(Debug)]
pub struct Record<'a> {
    pub id: i16,
    pub enabled: u32,
    pub title: &'a [u8],
    pub items: Vec<Item<'a>>,
}
impl<'a> Record<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        let bad = || Error::Load("invalid or unsupported standard MENU record".into());
        if bytes.len() < 16 || bytes.len() > 1024 * 1024 || bytes[6..10] != [0; 4] {
            return Err(bad());
        }
        let end = 15 + usize::from(bytes[14]);
        let title = bytes.get(15..end).ok_or_else(bad)?;
        let mut at = end;
        let mut items = Vec::new();
        loop {
            let n = usize::from(*bytes.get(at).ok_or_else(bad)?);
            at += 1;
            if n == 0 {
                break;
            }
            if items.len() >= 1024 {
                return Err(bad());
            }
            let text = bytes.get(at..at + n).ok_or_else(bad)?;
            at += n;
            let fields = bytes.get(at..at + 4).ok_or_else(bad)?;
            at += 4;
            items.push(Item {
                text,
                icon: fields[0],
                command: fields[1],
                mark: fields[2],
                style: fields[3],
            });
        }
        Ok(Self {
            id: i16::from_be_bytes(bytes[..2].try_into().unwrap()),
            enabled: u32::from_be_bytes(bytes[10..14].try_into().unwrap()),
            title,
            items,
        })
    }
}
pub fn bar_ids(bytes: &[u8]) -> Result<Vec<i16>> {
    if bytes.len() < 2 {
        return Err(Error::Load("truncated MBAR resource".into()));
    }
    let n = i16::from_be_bytes(bytes[..2].try_into().unwrap());
    if !(0..=1024).contains(&n) || bytes.len() != 2 + n as usize * 2 {
        return Err(Error::Load("invalid MBAR menu count/storage".into()));
    }
    Ok(bytes[2..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_be_bytes(*b))
        .collect())
}
