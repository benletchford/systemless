//! Semantic resumption of one CFM load across guest initialization.
//! Inside Macintosh: PowerPC System Software (1994), pp. 3-15--3-18, 3-27.

/// A selected resource routine whose fragment must be prepared before it is callable.
/// PowerPC System Software (1994), pp. 2-27–2-28 and 2-36.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CfmResourcePreparation {
    pub(crate) descriptor: u32,
    pub(crate) record: u32,
    pub(crate) fragment_address: u32,
    pub(crate) proc_info: u32,
    pub(crate) routine_flags: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfmConnection {
    pub id: u32,
    pub library_name: String,
    pub main_addr: u32,
    pub init_addr: u32,
    pub term_addr: u32,
    pub exports: Vec<CfmExport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfmExport {
    pub name: String,
    pub class: u8,
    pub address: u32,
}

/// Load identities use the connection reserved for that load. The connection
/// allocator never reuses these IDs, including when initialization fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CfmLoadId(pub(crate) u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CfmLoadOutputs {
    pub(crate) connection: u32,
    pub(crate) main_address: u32,
    pub(crate) error_name: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CfmLoadOperation {
    pub(crate) id: CfmLoadId,
    pub(crate) main_address: u32,
    pub(crate) outputs: CfmLoadOutputs,
    pub(crate) created_connection: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CfmLoadCompletion {
    Ready(CfmLoadOperation),
    InitializationFailed(CfmLoadId),
}

impl CfmLoadOperation {
    pub(crate) fn resume(self, initializer_result: u32) -> CfmLoadCompletion {
        if initializer_result == 0 {
            CfmLoadCompletion::Ready(self)
        } else {
            CfmLoadCompletion::InitializationFailed(self.id)
        }
    }
}

/// Semantic failure; the caller ABI decides where to deliver its OSErr.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CfmLoadError {
    InitializationFailed,
    ConnectionNotFound,
    InvalidOutputs,
}

impl CfmLoadError {
    pub(crate) fn os_error(self) -> i16 {
        match self {
            Self::InitializationFailed => -2821,
            Self::ConnectionNotFound => -2801,
            Self::InvalidOutputs => -50,
        }
    }
}

impl CfmLoadOperation {
    /// Finish a load without an architectural return context. The memory
    /// service must commit all output ranges together or leave them unchanged.
    /// No guest execution occurs between validating and publishing the result.
    pub(crate) fn complete(
        self,
        initializer_result: u32,
        connections: &mut Vec<CfmConnection>,
        write_outputs: impl FnOnce(&[(u32, &[u8])]) -> bool,
    ) -> Result<(), CfmLoadError> {
        let result = match self.resume(initializer_result) {
            CfmLoadCompletion::InitializationFailed(_) => Err(CfmLoadError::InitializationFailed),
            CfmLoadCompletion::Ready(operation) => {
                if !connections
                    .iter()
                    .any(|connection| connection.id == operation.id.0)
                {
                    Err(CfmLoadError::ConnectionNotFound)
                } else {
                    let connection = operation.id.0.to_be_bytes();
                    let main_address = operation.main_address.to_be_bytes();
                    let error_name = [0];
                    let mut writes: Vec<(u32, &[u8])> = vec![
                        (operation.outputs.connection, &connection),
                        (operation.outputs.main_address, &main_address),
                    ];
                    if operation.outputs.error_name != 0 {
                        writes.push((operation.outputs.error_name, &error_name));
                    }
                    if writes.iter().any(|(address, bytes)| {
                        *address == 0 || u64::from(*address) + bytes.len() as u64 > (1u64 << 32)
                    }) || !write_outputs(&writes)
                    {
                        Err(CfmLoadError::InvalidOutputs)
                    } else {
                        Ok(())
                    }
                }
            }
        };
        if result.is_err() && self.created_connection {
            connections.retain(|connection| connection.id != self.id.0);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{GuestAddressSpace, MacMemoryBus, MemoryBus};

    const OUTPUT: u32 = 0x0100_0000;

    fn connection(id: u32) -> CfmConnection {
        CfmConnection {
            id,
            library_name: format!("library-{id}"),
            main_addr: 0x1234_5678,
            init_addr: 0,
            term_addr: 0,
            exports: vec![],
        }
    }

    fn operation(created_connection: bool) -> CfmLoadOperation {
        CfmLoadOperation {
            id: CfmLoadId(7),
            main_address: 0x1234_5678,
            outputs: CfmLoadOutputs {
                connection: OUTPUT,
                main_address: OUTPUT + 8,
                error_name: OUTPUT + 16,
            },
            created_connection,
        }
    }

    fn snapshot(memory: &mut GuestAddressSpace) -> Vec<u8> {
        let mut bytes = vec![0; 20];
        memory.read_bytes_into(OUTPUT, &mut bytes).unwrap();
        bytes
    }

    #[test]
    fn load_completion_publishes_and_cleans_up_identically_through_both_memory_views() {
        for classic in [false, true] {
            for created in [false, true] {
                // Each failure must preserve every output and the unrelated
                // connection. Reused ready connections survive failed output delivery.
                for failure in 0..8 {
                    let mut memory = GuestAddressSpace::new();
                    memory.add_region(OUTPUT, vec![0xAA; 20]);
                    let mut op = operation(created);
                    let mut connections = vec![connection(7), connection(9)];
                    let mut initializer_result = 0;
                    let expected = match failure {
                        0 => Ok(()),
                        1 => {
                            initializer_result = 1;
                            Err(CfmLoadError::InitializationFailed)
                        }
                        2 => {
                            connections.remove(0);
                            Err(CfmLoadError::ConnectionNotFound)
                        }
                        3..=5 => {
                            let (address, len) = match failure {
                                3 => (OUTPUT + 3, 1),
                                4 => (OUTPUT + 11, 1),
                                _ => (OUTPUT + 16, 1),
                            };
                            memory.add_readonly_region(address, vec![0xAA; len]);
                            Err(CfmLoadError::InvalidOutputs)
                        }
                        6 => {
                            op.outputs.main_address = OUTPUT + 18;
                            Err(CfmLoadError::InvalidOutputs)
                        }
                        _ => {
                            op.outputs.main_address = u32::MAX - 1;
                            Err(CfmLoadError::InvalidOutputs)
                        }
                    };
                    let mut bus = MacMemoryBus::new(0x10000);
                    bus.set_addressing_32_bit(true);
                    bus.attach_guest_address_space(memory.shared_view());
                    let result = op.complete(initializer_result, &mut connections, |writes| {
                        if classic {
                            bus.try_write_ranges_atomic(writes)
                        } else {
                            memory.try_write_ranges_atomic(writes)
                        }
                    });
                    assert_eq!(
                        result, expected,
                        "classic={classic}, created={created}, case={failure}"
                    );
                    let mut expected_bytes = vec![0xAA; 20];
                    if result.is_ok() {
                        expected_bytes[0..4].copy_from_slice(&7u32.to_be_bytes());
                        expected_bytes[8..12].copy_from_slice(&op.main_address.to_be_bytes());
                        expected_bytes[16] = 0;
                    }
                    assert_eq!(snapshot(&mut memory), expected_bytes);
                    assert_eq!(bus.read_bytes(OUTPUT, 20), expected_bytes);
                    assert_eq!(
                        connections.iter().any(|connection| connection.id == 7),
                        failure != 2 && (result.is_ok() || !created)
                    );
                    assert_eq!(connections.last(), Some(&connection(9)));
                }
            }
        }
    }

    #[test]
    fn load_output_contract_handles_optional_names_and_rejects_null_required_outputs() {
        for classic in [false, true] {
            for null_output in 0..3 {
                let mut memory = GuestAddressSpace::new();
                // Exercise output longs split across adjacent mappings.
                for i in 0..20 {
                    memory.add_region(OUTPUT + i, vec![0xAA]);
                }
                let mut bus = MacMemoryBus::new(0x10000);
                bus.set_addressing_32_bit(true);
                bus.attach_guest_address_space(memory.shared_view());
                let mut op = operation(true);
                op.outputs.error_name = 0;
                if null_output == 1 {
                    op.outputs.connection = 0;
                }
                if null_output == 2 {
                    op.outputs.main_address = 0;
                }
                let mut connections = vec![connection(7)];
                let mut called = false;
                let result = op.complete(0, &mut connections, |writes| {
                    called = true;
                    if classic {
                        bus.try_write_ranges_atomic(writes)
                    } else {
                        memory.try_write_ranges_atomic(writes)
                    }
                });
                assert_eq!(called, null_output == 0);
                assert_eq!(
                    result,
                    if null_output == 0 {
                        Ok(())
                    } else {
                        Err(CfmLoadError::InvalidOutputs)
                    }
                );
                let bytes = snapshot(&mut memory);
                assert_eq!(bytes[16], 0xAA);
                if null_output != 0 {
                    assert_eq!(bytes, vec![0xAA; 20]);
                }
            }
        }
    }
}
