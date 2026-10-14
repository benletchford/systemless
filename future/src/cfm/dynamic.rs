use super::{Connection, Connections, BAD_CONNECTION};
use crate::{
    abi::Abi,
    lifecycle::Routine,
    load::pef::{self, Placement},
    memory::{GuestPtr, Memory},
    Error, Result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadRequest {
    pub source: GuestPtr,
    pub length: u32,
    pub name: GuestPtr,
    pub options: u32,
    pub connection: GuestPtr,
    pub main: GuestPtr,
    pub error: GuestPtr,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Initializing,
    Ready,
    Closing,
}
pub(super) struct Fragment {
    slot: u32,
    source: GuestPtr,
    length: u32,
    name: Vec<u8>,
    references: u32,
    state: State,
    main: u32,
    termination: Option<Routine>,
    mappings: Vec<u32>,
    gates: Vec<u32>,
}
pub(crate) enum LoadAction {
    Complete(i16),
    Initialize {
        connection: u32,
        routine: Routine,
        block: u32,
    },
}
pub(crate) enum CloseAction {
    Complete(i16),
    Terminate { connection: u32, routine: Routine },
}
impl LoadRequest {
    fn validate(self, memory: &Memory, error_length: usize) -> Result<()> {
        if self.connection.0 != 0 {
            memory.validate_write(self.connection, 4)?;
        }
        if self.main.0 != 0 {
            memory.validate_write(self.main, 4)?;
        }
        if self.error.0 != 0 {
            memory.validate_write(self.error, error_length + 1)?;
        }
        Ok(())
    }
    fn success(self, memory: &mut Memory, connection: u32, main: u32) -> Result<()> {
        if self.connection.0 != 0 {
            memory.validate_write(self.connection, 4)?;
        }
        if self.main.0 != 0 {
            memory.validate_write(self.main, 4)?;
        }
        if self.connection.0 != 0 {
            memory.write(self.connection, &connection.to_be_bytes())?;
        }
        if self.main.0 != 0 {
            memory.write(self.main, &main.to_be_bytes())?;
        }
        Ok(())
    }
    fn failure(self, memory: &mut Memory, error: i16, name: &[u8]) -> Result<LoadAction> {
        if self.error.0 != 0 {
            let mut pascal = vec![name.len() as u8];
            pascal.extend_from_slice(name);
            memory.write(self.error, &pascal)?;
        }
        Ok(LoadAction::Complete(error))
    }
}
impl Connections {
    pub(crate) fn load_memory(
        &mut self,
        memory: &mut Memory,
        abi: &mut Abi,
        request: LoadRequest,
    ) -> Result<LoadAction> {
        let name = if request.name.0 == 0 {
            Vec::new()
        } else {
            let len = memory.read(request.name, 1)?[0] as usize;
            if len > 63 {
                return Err(Error::Cpu("CFM fragment name exceeds Str63".into()));
            }
            memory.read(request.name, len + 1)?[1..].to_vec()
        };
        // Reserve room for an imported library's name as well as the root name.
        request.validate(memory, 63)?;
        if !matches!(request.options, 1 | 2 | 5) {
            return Err(Error::Cpu(format!(
                "unsupported CFM load options {:#x}",
                request.options
            )));
        }
        if request.options != 5 {
            if let Some((&id, connection)) = self.entries.iter_mut().find(|(_, c)| {
                c.dynamic
                    .as_ref()
                    .is_some_and(|f| f.source == request.source && f.length == request.length)
            }) {
                let fragment = connection.dynamic.as_mut().unwrap();
                if fragment.state != State::Ready {
                    return request.failure(memory, -2812, &name);
                }
                let references = if request.options == 1 {
                    fragment
                        .references
                        .checked_add(1)
                        .ok_or_else(|| Error::Cpu("CFM reference count overflow".into()))?
                } else {
                    fragment.references
                };
                request.success(memory, id, fragment.main)?;
                fragment.references = references;
                return Ok(LoadAction::Complete(0));
            }
            if request.options == 2 {
                return request.failure(memory, -2804, &name);
            }
        }
        if request.length as usize > crate::load::IMAGE_LIMIT {
            return request.failure(memory, -2820, &name);
        }
        let Some(slot) = (0..14).find(|slot| {
            !self
                .entries
                .values()
                .any(|c| c.dynamic.as_ref().is_some_and(|f| f.slot == *slot))
                && Placement::dynamic(*slot).available(memory, abi)
        }) else {
            return request.failure(memory, -2810, &name);
        };
        let id = self.next_id.max(2);
        let next_id = id
            .checked_add(1)
            .ok_or_else(|| Error::Cpu("CFM connection IDs exhausted".into()))?;
        let input = memory.read(request.source, request.length as usize)?;
        let placement = Placement::dynamic(slot);
        let mut image =
            match pef::prepare_fragment(input, placement, id, &name, Some(request.source)) {
                Ok(image) => image,
                Err(Error::UnresolvedImport { library_bytes, .. }) => {
                    return request.failure(memory, -2807, &library_bytes)
                }
                Err(Error::Load(_) | Error::Memory { .. } | Error::Alignment(_)) => {
                    return request.failure(memory, -2820, &name)
                }
                Err(error) => return Err(error),
            };
        let mappings = image.memory.mapping_bases();
        let gates = image.abi.imports.keys().copied().collect();
        let install_stdlib = image.stdlib_data && !self.stdlib_installed;
        if install_stdlib {
            crate::stdlib::install(&mut image.memory)?;
        }
        if memory.adopt(image.memory).is_err() {
            return request.failure(memory, -2810, &name);
        }
        abi.imports.extend(image.abi.imports);
        self.stdlib_installed |= install_stdlib;
        self.next_id = next_id;
        let main = image.main_address.unwrap_or(0);
        self.entries.insert(
            id,
            Connection {
                exports: image.exports,
                dynamic: Some(Fragment {
                    slot,
                    source: request.source,
                    length: request.length,
                    name,
                    references: 1,
                    state: if image.init.is_some() {
                        State::Initializing
                    } else {
                        State::Ready
                    },
                    main,
                    termination: image.term,
                    mappings,
                    gates,
                }),
            },
        );
        if let Some(routine) = image.init {
            Ok(LoadAction::Initialize {
                connection: id,
                routine,
                block: placement.block(),
            })
        } else {
            request.success(memory, id, main)?;
            Ok(LoadAction::Complete(0))
        }
    }
    pub(crate) fn initialized(
        &mut self,
        memory: &mut Memory,
        abi: &mut Abi,
        id: u32,
        request: LoadRequest,
        result: i16,
    ) -> Result<i16> {
        let fragment = self
            .entries
            .get_mut(&id)
            .and_then(|c| c.dynamic.as_mut())
            .ok_or_else(|| Error::Cpu("lost initializing CFM connection".into()))?;
        if result != 0 {
            let name = fragment.name.clone();
            self.remove(memory, abi, id);
            request.failure(memory, -2821, &name)?;
            return Ok(-2821);
        }
        request.success(memory, id, fragment.main)?;
        fragment.state = State::Ready;
        Ok(0)
    }
    pub(crate) fn close(
        &mut self,
        memory: &mut Memory,
        abi: &mut Abi,
        destination: GuestPtr,
    ) -> Result<CloseAction> {
        let id = memory.u32(destination)?;
        memory.validate_write(destination, 4)?;
        let Some(connection) = self.entries.get_mut(&id) else {
            return Ok(CloseAction::Complete(BAD_CONNECTION));
        };
        let Some(fragment) = connection.dynamic.as_mut() else {
            return Ok(CloseAction::Complete(-2824));
        };
        if fragment.state != State::Ready {
            return Ok(CloseAction::Complete(-2812));
        }
        if fragment.references > 1 {
            memory.write(destination, &0u32.to_be_bytes())?;
            fragment.references -= 1;
            return Ok(CloseAction::Complete(0));
        }
        fragment.state = State::Closing;
        if let Some(routine) = fragment.termination {
            Ok(CloseAction::Terminate {
                connection: id,
                routine,
            })
        } else {
            self.closed(memory, abi, id, destination)?;
            Ok(CloseAction::Complete(0))
        }
    }
    pub(crate) fn closed(
        &mut self,
        memory: &mut Memory,
        abi: &mut Abi,
        id: u32,
        destination: GuestPtr,
    ) -> Result<()> {
        // A caller may keep the ID in storage owned by the closing fragment.
        if destination.0 != 0 {
            memory.write(destination, &0u32.to_be_bytes())?;
        }
        self.remove(memory, abi, id);
        Ok(())
    }
    fn remove(&mut self, memory: &mut Memory, abi: &mut Abi, id: u32) {
        if let Some(fragment) = self.entries.remove(&id).and_then(|c| c.dynamic) {
            memory.remove_mappings(&fragment.mappings);
            for gate in fragment.gates {
                abi.imports.remove(&gate);
            }
        }
    }
    pub(crate) fn shutdown(
        &mut self,
        memory: &mut Memory,
        abi: &mut Abi,
    ) -> Result<Option<(u32, Routine)>> {
        while let Some((&id, _)) = self.entries.iter().rev().find(|(_, c)| c.dynamic.is_some()) {
            let fragment = self.entries.get_mut(&id).unwrap().dynamic.as_mut().unwrap();
            if fragment.state != State::Ready {
                return Err(Error::Cpu("process exit during CFM init/term".into()));
            }
            fragment.state = State::Closing;
            if let Some(routine) = fragment.termination {
                return Ok(Some((id, routine)));
            }
            self.remove(memory, abi, id);
        }
        Ok(None)
    }
}
