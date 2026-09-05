//! Semantic resumption of one CFM load across guest initialization.
//! Inside Macintosh: PowerPC System Software (1994), pp. 3-15--3-18, 3-27.

pub(crate) mod fragment;

use crate::execution_kernel::GuestProcedureInvocation;
use crate::guest_procedure::{
    GuestIsa, GuestProcedure, GuestProcedureMemory, GuestProcedureRepresentation,
};

/// Mapping-aware reads and atomic publication for CFM records.
pub(crate) trait CfmMemory: GuestProcedureMemory {
    fn publish_cfm_outputs(&mut self, writes: &[(u32, &[u8])]) -> bool;
}

fn cfm_bytes<const N: usize>(
    memory: &mut impl GuestProcedureMemory,
    address: u32,
) -> Option<[u8; N]> {
    let mut bytes = [0; N];
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = memory.procedure_read_u8(address.checked_add(u32::try_from(offset).ok()?)?)?;
    }
    Some(bytes)
}

pub(crate) const CFM_INIT_BLOCK_SIZE: u32 = 48;

/// Call-owned in-memory fragment metadata, independent of its caller's ABI.
/// PowerPC System Software (1994), pp. 3-15–3-16: InitBlock uses 680x0
/// alignment and points to a Pascal fragment name; kInMem has selector zero.
pub(crate) struct CfmInitBlock {
    bytes: Vec<u8>,
}

impl CfmInitBlock {
    pub(crate) fn in_memory(connection: CfmLoadId, address: u32, length: u32, name: &str) -> Self {
        let name = crate::mac_roman::encode_mac_roman_lossy(name);
        let name = &name[..name.len().min(255)];
        let mut bytes = vec![0; CFM_INIT_BLOCK_SIZE as usize + 1 + name.len()];
        bytes[8..12].copy_from_slice(&connection.0.to_be_bytes());
        bytes[16..20].copy_from_slice(&address.to_be_bytes());
        bytes[20..24].copy_from_slice(&length.to_be_bytes());
        bytes[CFM_INIT_BLOCK_SIZE as usize] = name.len() as u8;
        bytes[CFM_INIT_BLOCK_SIZE as usize + 1..].copy_from_slice(name);
        Self { bytes }
    }

    pub(crate) fn size(&self) -> u32 {
        self.bytes.len() as u32
    }

    pub(crate) fn publish(
        mut self,
        memory: &mut impl CfmMemory,
        address: u32,
    ) -> Result<(), CfmLoadError> {
        if address == 0 || address.checked_add(self.size() - 1).is_none() {
            return Err(CfmLoadError::InvalidOutputs);
        }
        let name = address
            .checked_add(CFM_INIT_BLOCK_SIZE)
            .ok_or(CfmLoadError::InvalidOutputs)?;
        self.bytes[28..32].copy_from_slice(&name.to_be_bytes());
        if !memory.publish_cfm_outputs(&[(address, &self.bytes)]) {
            return Err(CfmLoadError::InvalidOutputs);
        }
        Ok(())
    }
}

fn fragment_procedure(
    memory: &mut impl GuestProcedureMemory,
    vector: u32,
    proc_info: u32,
    routine_flags: u16,
) -> Result<GuestProcedure, CfmLoadError> {
    if vector == 0 {
        return Err(CfmLoadError::CorruptFragment);
    }
    let bytes = cfm_bytes::<8>(memory, vector).ok_or(CfmLoadError::CorruptFragment)?;
    let entry = u32::from_be_bytes(bytes[..4].try_into().unwrap());
    let rtoc = u32::from_be_bytes(bytes[4..].try_into().unwrap());
    if entry == 0 || cfm_bytes::<4>(memory, entry).is_none() {
        return Err(CfmLoadError::CorruptFragment);
    }
    Ok(GuestProcedure {
        original_pointer: vector,
        representation: GuestProcedureRepresentation::PowerPcTransitionVector { address: vector },
        isa: GuestIsa::PowerPc,
        entry,
        rtoc,
        proc_info,
        routine_flags,
    })
}

/// The initializer receives one InitBlockPtr and returns OSErr. Its caller's
/// register/stack return context is deliberately absent from this request.
/// PowerPC System Software (1994), p. 3-27; ProcInfo encoding, pp. 2-18–2-21.
pub(crate) fn initialization_invocation(
    memory: &mut impl GuestProcedureMemory,
    task: crate::execution_kernel::ExecutionTaskId,
    vector: u32,
    block: u32,
) -> Result<GuestProcedureInvocation, CfmLoadError> {
    const PROC_INFO: u32 = 0xE1; // C stack, two-byte result, one four-byte pointer.
    let procedure = fragment_procedure(memory, vector, PROC_INFO, 0)?;
    if block == 0 || cfm_bytes::<48>(memory, block).is_none() {
        return Err(CfmLoadError::InvalidOutputs);
    }
    Ok(GuestProcedureInvocation {
        task,
        procedure,
        arguments: crate::execution_kernel::GuestArgumentValues::from_slice(&[block]).unwrap(),
        caller_proc_info: PROC_INFO,
    })
}

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CfmResourceCall {
    pub(crate) task: crate::execution_kernel::ExecutionTaskId,
    pub(crate) id: CfmLoadId,
    pub(crate) preparation: CfmResourcePreparation,
    pub(crate) descriptor_header: [u8; 12],
    pub(crate) original_record: [u8; 20],
    pub(crate) main_address: u32,
    pub(crate) arguments: crate::execution_kernel::GuestArgumentValues,
    pub(crate) caller_proc_info: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CfmOperation {
    Load(CfmLoadOperation),
    Resource(CfmResourceCall),
}

impl CfmOperation {
    pub(crate) fn id(self) -> CfmLoadId {
        match self {
            Self::Load(load) => load.id,
            Self::Resource(call) => call.id,
        }
    }
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

/// One owned CFM registry. A standalone loader carries this as a seed;
/// process installation moves it, rather than cloning an adapter projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfmState {
    pub connections: Vec<CfmConnection>,
    pub library_fragments: Vec<CfmLibraryFragment>,
    pub next_connection_id: u32,
}

impl Default for CfmState {
    fn default() -> Self {
        Self {
            connections: Vec::new(),
            library_fragments: Vec::new(),
            next_connection_id: 1,
        }
    }
}

impl CfmState {
    pub(crate) fn is_pristine(&self) -> bool {
        self.connections.is_empty()
            && self.library_fragments.is_empty()
            && self.next_connection_id == 1
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfmLibraryFragment {
    pub name: String,
    pub bytes: Vec<u8>,
}

impl CfmResourceCall {
    pub(crate) fn complete(
        self,
        initializer_result: u32,
        connections: &mut Vec<CfmConnection>,
        memory: &mut impl CfmMemory,
    ) -> Result<GuestProcedureInvocation, CfmLoadError> {
        let request = self.preparation;
        let result = (|| {
            if initializer_result != 0 {
                return Err(CfmLoadError::InitializationFailed);
            }
            // Resource code becomes callable only after successful preparation.
            // PowerPC System Software (1994), pp. 2-27–2-28 and 2-36.
            let procedure = fragment_procedure(
                memory,
                self.main_address,
                request.proc_info,
                request.routine_flags & !3,
            )?;
            if !connections
                .iter()
                .any(|connection| connection.id == self.id.0)
            {
                return Err(CfmLoadError::ConnectionNotFound);
            }
            let header =
                cfm_bytes::<12>(memory, request.descriptor).ok_or(CfmLoadError::InvalidOutputs)?;
            if header != self.descriptor_header {
                return Err(CfmLoadError::DescriptorChanged);
            }
            let record =
                cfm_bytes::<20>(memory, request.record).ok_or(CfmLoadError::InvalidOutputs)?;
            if record != self.original_record
                || record[5] != 1
                || u32::from_be_bytes(record[0..4].try_into().unwrap()) != request.proc_info
                || u16::from_be_bytes(record[6..8].try_into().unwrap()) != request.routine_flags
                || request
                    .descriptor
                    .checked_add(u32::from_be_bytes(record[8..12].try_into().unwrap()))
                    != Some(request.fragment_address)
            {
                return Err(CfmLoadError::DescriptorChanged);
            }
            let flags = (request.routine_flags & !3).to_be_bytes();
            let target = self.main_address.to_be_bytes();
            let flags_address = request
                .record
                .checked_add(6)
                .ok_or(CfmLoadError::InvalidOutputs)?;
            let target_address = request
                .record
                .checked_add(8)
                .ok_or(CfmLoadError::InvalidOutputs)?;
            if !memory.publish_cfm_outputs(&[(flags_address, &flags), (target_address, &target)]) {
                return Err(CfmLoadError::InvalidOutputs);
            }
            Ok(GuestProcedureInvocation {
                task: self.task,
                procedure,
                arguments: self.arguments,
                caller_proc_info: self.caller_proc_info,
            })
        })();
        if result.is_err() {
            connections.retain(|connection| connection.id != self.id.0);
        }
        result
    }
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
    DescriptorChanged,
    CorruptFragment,
    NoAddressSpace,
}

impl CfmLoadError {
    pub(crate) fn os_error(self) -> i16 {
        match self {
            Self::InitializationFailed => -2821,
            Self::ConnectionNotFound => -2801,
            Self::InvalidOutputs => -50,
            Self::NoAddressSpace => -2810,
            Self::DescriptorChanged | Self::CorruptFragment => -2820,
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
    fn initialization_records_publish_atomically_in_both_memory_views() {
        for name in [String::new(), "Café™".to_owned(), "é".repeat(300)] {
            for fault in 0..5 {
                let mut outcomes = Vec::new();
                for classic in [false, true] {
                    let block = CfmInitBlock::in_memory(CfmLoadId(7), 0x4000, 123, &name);
                    let size = block.size();
                    let address = match fault {
                        3 => u32::MAX - 31,
                        4 => 0,
                        _ => OUTPUT,
                    };
                    let mapped_size = if fault == 2 { size - 1 } else { size };
                    let mut memory = GuestAddressSpace::new();
                    memory.add_region(OUTPUT, vec![0xa5; mapped_size as usize]);
                    if fault == 1 {
                        memory.add_readonly_region(OUTPUT + 28, vec![0xa5; 4]);
                    }
                    let mut bus = MacMemoryBus::new(0x10000);
                    bus.set_addressing_32_bit(true);
                    bus.attach_guest_address_space(memory.shared_view());
                    let result = if classic {
                        block.publish(&mut bus, address)
                    } else {
                        block.publish(&mut memory, address)
                    };
                    let bytes: Vec<_> = (0..size)
                        .map(|offset| memory.procedure_read_u8(OUTPUT + offset))
                        .collect();
                    if fault == 0 {
                        assert_eq!(result, Ok(()));
                        let bytes: Vec<_> = bytes.iter().map(|byte| byte.unwrap()).collect();
                        assert_eq!(&bytes[..8], &[0; 8]);
                        assert_eq!(&bytes[8..12], &7u32.to_be_bytes());
                        assert_eq!(&bytes[12..16], &[0; 4]);
                        assert_eq!(&bytes[16..20], &0x4000u32.to_be_bytes());
                        assert_eq!(&bytes[20..24], &123u32.to_be_bytes());
                        assert_eq!(&bytes[24..28], &[0; 4]);
                        assert_eq!(&bytes[28..32], &(OUTPUT + 48).to_be_bytes());
                        assert_eq!(&bytes[32..48], &[0; 16]);
                        let expected = if name.is_empty() {
                            vec![0]
                        } else if name == "Café™" {
                            vec![5, b'C', b'a', b'f', 0x8e, 0xaa]
                        } else {
                            let mut name = vec![0x8e; 256];
                            name[0] = 255;
                            name
                        };
                        assert_eq!(&bytes[48..], expected.as_slice());
                    } else {
                        assert_eq!(result, Err(CfmLoadError::InvalidOutputs));
                        assert!(bytes[..mapped_size as usize]
                            .iter()
                            .all(|b| *b == Some(0xa5)));
                        if fault == 2 {
                            assert_eq!(bytes.last(), Some(&None));
                        }
                    }
                    outcomes.push((result, bytes));
                }
                assert_eq!(outcomes[0], outcomes[1], "fault {fault}");
            }
        }
    }

    #[test]
    fn initializer_invocation_validates_target_and_retains_task_in_both_views() {
        const VECTOR: u32 = OUTPUT + 0x200;
        const ENTRY: u32 = OUTPUT + 0x300;
        let task = crate::execution_kernel::ExecutionTaskId::from_thread_id(42);
        for fault in 0..11 {
            let mut outcomes = Vec::new();
            for classic in [false, true] {
                let mut memory = GuestAddressSpace::new();
                memory.add_region(OUTPUT, vec![0; if fault == 6 { 47 } else { 48 }]);
                let entry = if fault == 1 {
                    0
                } else if fault == 8 {
                    u32::MAX - 1
                } else {
                    ENTRY
                };
                let mut vector = entry.to_be_bytes().to_vec();
                vector.extend(0x0100_4000u32.to_be_bytes());
                if fault == 2 {
                    vector.pop();
                }
                if fault == 9 {
                    memory.add_region(0, vector.clone());
                }
                memory.add_region(VECTOR, vector);
                if fault != 3 {
                    memory.add_region(ENTRY, vec![0x60; if fault == 4 { 3 } else { 4 }]);
                }
                let vector = match fault {
                    5 => u32::MAX - 3,
                    9 => 0,
                    _ => VECTOR,
                };
                let block = match fault {
                    7 => 0,
                    10 => u32::MAX - 31,
                    _ => OUTPUT,
                };
                let mut bus = MacMemoryBus::new(0x10000);
                bus.set_addressing_32_bit(true);
                bus.attach_guest_address_space(memory.shared_view());
                let result = if classic {
                    initialization_invocation(&mut bus, task, vector, block)
                } else {
                    initialization_invocation(&mut memory, task, vector, block)
                };
                if fault == 0 {
                    let invocation = result.unwrap();
                    assert_eq!(invocation.task, task);
                    assert_eq!(invocation.arguments.as_slice(), &[OUTPUT]);
                    assert_eq!(invocation.caller_proc_info, 0xe1);
                    assert_eq!(
                        invocation.procedure,
                        GuestProcedure {
                            original_pointer: VECTOR,
                            representation: GuestProcedureRepresentation::PowerPcTransitionVector {
                                address: VECTOR
                            },
                            isa: GuestIsa::PowerPc,
                            entry: ENTRY,
                            rtoc: 0x0100_4000,
                            proc_info: 0xe1,
                            routine_flags: 0,
                        }
                    );
                } else {
                    assert_eq!(
                        result,
                        Err(if matches!(fault, 6 | 7 | 10) {
                            CfmLoadError::InvalidOutputs
                        } else {
                            CfmLoadError::CorruptFragment
                        })
                    );
                }
                assert_eq!(cfm_bytes::<47>(&mut memory, OUTPUT), Some([0; 47]));
                outcomes.push(result);
            }
            assert_eq!(outcomes[0], outcomes[1], "fault {fault}");
        }
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
    fn prepared_resource_publication_is_atomic_and_rejects_changed_records_in_both_views() {
        use crate::execution_kernel::{ExecutionTaskId, GuestArgumentValues};

        const VECTOR: u32 = OUTPUT + 0x200;
        const ENTRY: u32 = OUTPUT + 0x300;
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum Fault {
            None,
            ProtectedOutput,
            ChangedRecord,
            InitializerFailed,
            ChangedHeader,
            ShortHeader,
            ShortRecord,
            ShortVector,
            ZeroEntry,
            MissingEntry,
            WrappingVector,
            ShortEntry,
            MissingConnection,
            WrappingDescriptor,
            WrappingRecord,
        }
        for fault in [
            Fault::None,
            Fault::ProtectedOutput,
            Fault::ChangedRecord,
            Fault::InitializerFailed,
            Fault::ChangedHeader,
            Fault::ShortHeader,
            Fault::ShortRecord,
            Fault::ShortVector,
            Fault::ZeroEntry,
            Fault::MissingEntry,
            Fault::WrappingVector,
            Fault::ShortEntry,
            Fault::MissingConnection,
            Fault::WrappingDescriptor,
            Fault::WrappingRecord,
        ] {
            let mut outcomes = Vec::new();
            for classic in [false, true] {
                let mut header = [0; 12];
                header[..3].copy_from_slice(&[0xaa, 0xfe, 7]);
                let mut record = [0u8; 20];
                record[..4].copy_from_slice(&0x3f0u32.to_be_bytes());
                record[5] = 1;
                record[6..8].copy_from_slice(&0xbu16.to_be_bytes());
                record[8..12].copy_from_slice(&0x100u32.to_be_bytes());
                let mut operation = CfmResourceCall {
                    task: ExecutionTaskId::from_thread_id(42),
                    id: CfmLoadId(7),
                    preparation: CfmResourcePreparation {
                        descriptor: OUTPUT - 12,
                        record: OUTPUT,
                        fragment_address: OUTPUT - 12 + 0x100,
                        proc_info: 0x3f0,
                        routine_flags: 0xb,
                    },
                    descriptor_header: header,
                    original_record: record,
                    main_address: VECTOR,
                    arguments: GuestArgumentValues::from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9])
                        .unwrap(),
                    caller_proc_info: 0x2f0,
                };
                if fault == Fault::ChangedRecord {
                    record[19] = 1;
                }
                if matches!(fault, Fault::ChangedHeader | Fault::InitializerFailed) {
                    header[11] = 1;
                }
                let mut memory = GuestAddressSpace::new();
                let header_size = if fault == Fault::ShortHeader { 11 } else { 12 };
                let record_size = if fault == Fault::ShortRecord { 19 } else { 20 };
                memory.add_region(OUTPUT - 12, header[..header_size].to_vec());
                memory.add_region(OUTPUT, record[..record_size].to_vec());
                if fault == Fault::ProtectedOutput {
                    memory.add_readonly_region(OUTPUT + 11, vec![record[11]]);
                }
                let entry = if matches!(fault, Fault::ZeroEntry | Fault::InitializerFailed) {
                    0
                } else {
                    ENTRY
                };
                let mut vector = entry.to_be_bytes().to_vec();
                vector.extend(0x0100_4000u32.to_be_bytes());
                if fault == Fault::ShortVector {
                    vector.pop();
                }
                memory.add_region(VECTOR, vector);
                if fault != Fault::MissingEntry {
                    let size = if fault == Fault::ShortEntry { 3 } else { 4 };
                    memory.add_region(ENTRY, [0x4e, 0x80, 0, 0x20][..size].to_vec());
                }
                match fault {
                    Fault::WrappingVector => operation.main_address = u32::MAX - 3,
                    Fault::WrappingDescriptor => operation.preparation.descriptor = u32::MAX - 4,
                    Fault::WrappingRecord => operation.preparation.record = u32::MAX - 10,
                    _ => {}
                }
                let before: Vec<_> = (0..20)
                    .map(|offset| memory.procedure_read_u8(OUTPUT + offset))
                    .collect();
                let mut bus = MacMemoryBus::new(0x10000);
                bus.set_addressing_32_bit(true);
                bus.attach_guest_address_space(memory.shared_view());
                let mut own_connection = connection(7);
                own_connection.main_addr = VECTOR;
                let mut connections = vec![own_connection, connection(9)];
                if fault == Fault::MissingConnection {
                    connections.remove(0);
                }
                let initializer_result = u32::from(fault == Fault::InitializerFailed);
                let result = if classic {
                    operation.complete(initializer_result, &mut connections, &mut bus)
                } else {
                    operation.complete(initializer_result, &mut connections, &mut memory)
                };
                let expected_error = match fault {
                    Fault::None => None,
                    Fault::InitializerFailed => Some(CfmLoadError::InitializationFailed),
                    Fault::ChangedHeader | Fault::ChangedRecord => {
                        Some(CfmLoadError::DescriptorChanged)
                    }
                    Fault::MissingConnection => Some(CfmLoadError::ConnectionNotFound),
                    Fault::ShortVector
                    | Fault::ZeroEntry
                    | Fault::MissingEntry
                    | Fault::WrappingVector
                    | Fault::ShortEntry => Some(CfmLoadError::CorruptFragment),
                    _ => Some(CfmLoadError::InvalidOutputs),
                };
                assert_eq!(result.as_ref().err().copied(), expected_error, "{fault:?}");
                let expected_bytes = if fault == Fault::None {
                    let invocation = result.unwrap();
                    assert_eq!(invocation.task.thread_id(), 42);
                    assert_eq!(
                        invocation.arguments.as_slice(),
                        &[1, 2, 3, 4, 5, 6, 7, 8, 9]
                    );
                    assert_eq!(invocation.caller_proc_info, 0x2f0);
                    assert_eq!(invocation.procedure.entry, ENTRY);
                    assert_eq!(invocation.procedure.rtoc, 0x0100_4000);
                    assert_eq!(invocation.procedure.proc_info, 0x3f0);
                    assert_eq!(invocation.procedure.routine_flags, 8);
                    assert_eq!(
                        invocation.procedure.representation,
                        GuestProcedureRepresentation::PowerPcTransitionVector { address: VECTOR }
                    );
                    record[6..8].copy_from_slice(&8u16.to_be_bytes());
                    record[8..12].copy_from_slice(&VECTOR.to_be_bytes());
                    record.into_iter().map(Some).collect::<Vec<_>>()
                } else {
                    before
                };
                for (offset, expected) in expected_bytes.into_iter().enumerate() {
                    assert_eq!(memory.procedure_read_u8(OUTPUT + offset as u32), expected);
                    assert_eq!(bus.procedure_read_u8(OUTPUT + offset as u32), expected);
                }
                assert_eq!(
                    connections.iter().any(|connection| connection.id == 7),
                    fault == Fault::None
                );
                assert_eq!(connections.last(), Some(&connection(9)));
                outcomes.push(result);
            }
            assert_eq!(
                outcomes[0], outcomes[1],
                "memory views disagree for {fault:?}"
            );
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
