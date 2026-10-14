//! Process-owned CFM connections and symbol queries. This module sees guest
//! pointers and typed requests, never CPU registers or architecture state.
use crate::{
    load::Export,
    memory::{GuestPtr, Memory},
    Error, Result,
};
use std::{collections::BTreeMap, sync::Arc};

pub(crate) const APPLICATION_CONNECTION: u32 = 1;
const BAD_CONNECTION: i16 = -2801;
const MISSING_SYMBOL: i16 = -2802;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Query {
    Count {
        connection: u32,
        count: GuestPtr,
    },
    Find {
        connection: u32,
        name: GuestPtr,
        address: GuestPtr,
        class: GuestPtr,
    },
    Indexed {
        connection: u32,
        index: u32,
        name: GuestPtr,
        address: GuestPtr,
        class: GuestPtr,
    },
}
#[derive(Default)]
pub(crate) struct Connections {
    entries: BTreeMap<u32, Arc<[Export]>>,
}
impl Connections {
    pub(crate) fn install_application(&mut self, exports: Arc<[Export]>) -> Result<()> {
        if !self.entries.is_empty() {
            return Err(Error::Load(
                "CFM application connection already installed".into(),
            ));
        }
        self.entries.insert(APPLICATION_CONNECTION, exports);
        Ok(())
    }
    pub(crate) fn query(&self, memory: &mut Memory, query: Query) -> Result<i16> {
        let connection = match query {
            Query::Count { connection, .. }
            | Query::Find { connection, .. }
            | Query::Indexed { connection, .. } => connection,
        };
        let Some(exports) = self.entries.get(&connection) else {
            return Ok(BAD_CONNECTION);
        };
        let (symbol, name_out, address, class) = match query {
            Query::Count { count, .. } => {
                memory.write(count, &(exports.len() as u32).to_be_bytes())?;
                return Ok(0);
            }
            Query::Find {
                name,
                address,
                class,
                ..
            } => {
                let length = memory.read(name, 1)?[0] as usize;
                let name = &memory.read(name, length + 1)?[1..];
                let Some(symbol) = exports.iter().find(|e| e.name == name) else {
                    return Ok(MISSING_SYMBOL);
                };
                (symbol, None, address, class)
            }
            Query::Indexed {
                index,
                name,
                address,
                class,
                ..
            } => {
                // The later Code Fragment Manager API reference corrects the
                // original Inside Macintosh example: this index is zero-based.
                let Some(symbol) = exports.get(index as usize) else {
                    return Ok(MISSING_SYMBOL);
                };
                (symbol, (name.0 != 0).then_some(name), address, class)
            }
        };
        let mut pascal = Vec::new();
        if let Some(destination) = name_out {
            // PEF names can exceed Str255; report this unsupported conversion
            // rather than silently truncate a symbol or corrupt guest memory.
            let length = u8::try_from(symbol.name.len())
                .map_err(|_| Error::Cpu("CFM export name exceeds Str255".into()))?;
            pascal.push(length);
            pascal.extend_from_slice(&symbol.name);
            memory.validate_write(destination, pascal.len())?;
        }
        if address.0 != 0 {
            memory.validate_write(address, 4)?;
        }
        if class.0 != 0 {
            memory.validate_write(class, 1)?;
        }
        // Resolve all inputs and validate every output before the first write.
        // Optional output pointers are documented in Apple's CodeFragments.h.
        if let Some(destination) = name_out {
            memory.write(destination, &pascal)?;
        }
        if address.0 != 0 {
            memory.write(address, &symbol.address.0.to_be_bytes())?;
        }
        if class.0 != 0 {
            memory.write(class, &[symbol.class])?;
        }
        Ok(0)
    }
}
