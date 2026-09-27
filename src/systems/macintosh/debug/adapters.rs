use super::error::{DebugError, DebugResult};
use super::ids::{AddressSpaceId, ContextId};
use super::model::{
    AddressAccess, AddressMappingKind, AddressSpaceCapabilities, AddressSpaceDescriptor, ByteOrder,
    ContextCapabilities, ContextLifecycle, ContextSelector, DebugAddress, DisassemblyLine,
    ExecutionContextDescriptor, ExecutionLocation, MemoryReadResult, RegisterCategory,
    RegisterDescriptor, RegisterRole, RegisterSnapshot, RegisterValue, RegisterWidth, StackPreview,
};
use crate::memory::MemoryBus;
use crate::runner::FixtureRunner;
use ppc::PpcCpu;

/// Read-only access to one execution context. Return owned data, enforce transfer
/// limits, and route memory/disassembly by explicit address-space ID.
///
/// Register a factory with
/// [`FixtureRunner::debug_register_adapter`]. IDs must match the registration.
/// Control capabilities require scheduler integration: report retirement through
/// `debug_note_context_executed_units`, add `debug_context_breakpoint_addresses`
/// to batch watches, stop before execution with `debug_stop_context_at_breakpoint`,
/// and complete steps at a safe point with `debug_finish_context_step`.
/// Advertising a capability alone does not implement execution control.
pub trait ArchitectureAdapter {
    fn context(&self) -> ExecutionContextDescriptor;

    fn address_spaces(&self) -> Vec<AddressSpaceDescriptor>;
    fn address_space_capabilities(
        &self,
        space: AddressSpaceId,
    ) -> DebugResult<AddressSpaceCapabilities>;

    fn location(&self) -> ExecutionLocation;

    fn registers(&self, context: ContextId) -> DebugResult<Vec<RegisterSnapshot>>;

    fn disassemble(
        &self,
        context: ContextId,
        address: DebugAddress,
        count: u32,
    ) -> DebugResult<Vec<DisassemblyLine>>;

    fn stack_preview(&self, context: ContextId, words: u32) -> DebugResult<StackPreview>;

    fn read_memory(
        &self,
        space: AddressSpaceId,
        address: u64,
        length: u64,
    ) -> DebugResult<MemoryReadResult>;
}

pub type AdapterFactory =
    for<'a> fn(&'a FixtureRunner) -> Option<Box<dyn ArchitectureAdapter + 'a>>;

/// Runner-local adapter factory and active-context predicate. Context/space IDs
/// must be nonzero and unique across registrations. Multiple active predicates
/// make the `Active` selector unavailable; registration order is not priority.
#[derive(Clone, Copy, Debug)]
pub struct AdapterRegistration {
    pub context: ContextId,
    pub address_spaces: &'static [AddressSpaceId],
    pub factory: AdapterFactory,
    pub is_active: fn(&FixtureRunner) -> bool,
}

pub const M68K_CONTEXT: ContextId = ContextId(1);
pub const M68K_SPACE: AddressSpaceId = AddressSpaceId(1);
pub const PPC_CONTEXT: ContextId = ContextId(2);
pub const PPC_COMPANION_CONTEXT: ContextId = ContextId(3);
pub const PPC_COMPANION_SPACE: AddressSpaceId = AddressSpaceId(3);
pub const PPC_SPACE: AddressSpaceId = AddressSpaceId(2);

pub const MAX_MEMORY_TRANSFER: u64 = 16 * 1024 * 1024;
pub const MAX_DISASSEMBLY_COUNT: u32 = 4096;
pub const MAX_STACK_PREVIEW_BYTES: u64 = 64 * 1024;
pub const MAX_ARTIFACT_TRANSFER: u64 = 32 * 1024 * 1024;

const M68K_REG_D0: u32 = 0;
const M68K_REG_A0: u32 = 8;
const M68K_REG_PC: u32 = 16;
const M68K_REG_SR: u32 = 17;
const M68K_REG_CCR: u32 = 18;
const M68K_REG_FP0: u32 = 19;
const M68K_REG_FPCR: u32 = 27;
const M68K_REG_FPSR: u32 = 28;
const M68K_REG_FPIAR: u32 = 29;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveArchitecture {
    M68k,
    PowerPc,
    PowerPcCompanion,
    Blocked,
}

fn m68k_flag_descriptions() -> Vec<super::model::RegisterFlag> {
    use super::model::RegisterFlag;
    [
        (15, "T1", "Trace mode 1"),
        (13, "S", "Supervisor mode"),
        (10, "I2", "Interrupt mask bit 2"),
        (9, "I1", "Interrupt mask bit 1"),
        (8, "I0", "Interrupt mask bit 0"),
        (4, "X", "Extend"),
        (3, "N", "Negative"),
        (2, "Z", "Zero"),
        (1, "V", "Overflow"),
        (0, "C", "Carry"),
    ]
    .into_iter()
    .map(|(bit, name, description)| RegisterFlag {
        bit,
        name: name.to_string(),
        description: description.to_string(),
    })
    .collect()
}

fn m68k_context_capabilities() -> ContextCapabilities {
    ContextCapabilities {
        pause: true,
        resume: true,
        step: true,
        // CPU-slot stepping is implemented, but transitions through saved
        // cooperative tasks do not yet carry precise continuation identity.
        exact_step: false,
        set_breakpoint: true,
        read_registers: true,
        disassembly: true,
        exact_disassembly: false,
        stack_preview: true,
        write_registers: false,
        read_memory: true,
        write_memory: false,
        precise_breakpoints: true,
        watchpoints: false,
    }
}

fn m68k_context_descriptor() -> ExecutionContextDescriptor {
    ExecutionContextDescriptor {
        id: M68K_CONTEXT,
        architecture: "m68k".to_string(),
        task: None,
        lifecycle: ContextLifecycle::Active,
        address_spaces: vec![M68K_SPACE],
        capabilities: m68k_context_capabilities(),
    }
}

fn m68k_space_descriptor() -> AddressSpaceDescriptor {
    AddressSpaceDescriptor {
        id: M68K_SPACE,
        name: "68k-ram".to_string(),
        address_bits: 32,
        byte_order: ByteOrder::Big,
        mapping: AddressMappingKind::Ram,
        access: AddressAccess {
            read: true,
            write: false,
            execute: true,
        },
        supports_translation: false,
    }
}

fn ppc_context_capabilities() -> ContextCapabilities {
    ContextCapabilities {
        pause: true,
        resume: true,
        step: false,
        exact_step: false,
        set_breakpoint: false,
        read_registers: true,
        disassembly: false,
        exact_disassembly: false,
        stack_preview: false,
        write_registers: false,
        read_memory: false,
        write_memory: false,
        precise_breakpoints: false,
        watchpoints: false,
    }
}

fn ppc_context_descriptor() -> ExecutionContextDescriptor {
    ExecutionContextDescriptor {
        id: PPC_CONTEXT,
        architecture: "ppc".to_string(),
        task: Some("native-application".to_string()),
        lifecycle: ContextLifecycle::Active,
        address_spaces: vec![PPC_SPACE],
        capabilities: ppc_context_capabilities(),
    }
}

fn ppc_space_descriptor() -> AddressSpaceDescriptor {
    AddressSpaceDescriptor {
        id: PPC_SPACE,
        name: "ppc-sections".to_string(),
        address_bits: 32,
        byte_order: ByteOrder::Big,
        mapping: AddressMappingKind::Ram,
        access: AddressAccess {
            read: false,
            write: false,
            execute: true,
        },
        supports_translation: false,
    }
}

fn m68k_register_descriptors() -> Vec<RegisterDescriptor> {
    let mut descriptors = Vec::with_capacity(32);
    for index in 0..8u32 {
        descriptors.push(RegisterDescriptor {
            id: M68K_REG_D0 + index,
            name: format!("D{index}"),
            width: RegisterWidth::Bits32,
            category: RegisterCategory::General,
            role: None,
            readable: true,
            writable: false,
            flags: Vec::new(),
        });
    }
    for index in 0..8u32 {
        descriptors.push(RegisterDescriptor {
            id: M68K_REG_A0 + index,
            name: format!("A{index}"),
            width: RegisterWidth::Bits32,
            category: if index == 7 {
                RegisterCategory::StackPointer
            } else {
                RegisterCategory::Address
            },
            role: if index == 7 {
                Some(RegisterRole::StackPointer)
            } else if index == 6 {
                Some(RegisterRole::FramePointer)
            } else {
                None
            },
            readable: true,
            writable: false,
            flags: Vec::new(),
        });
    }
    descriptors.push(RegisterDescriptor {
        id: M68K_REG_PC,
        name: "PC".to_string(),
        width: RegisterWidth::Bits32,
        category: RegisterCategory::ProgramCounter,
        role: Some(RegisterRole::ProgramCounter),
        readable: true,
        writable: false,
        flags: Vec::new(),
    });
    descriptors.push(RegisterDescriptor {
        id: M68K_REG_SR,
        name: "SR".to_string(),
        width: RegisterWidth::Bits16,
        category: RegisterCategory::Status,
        role: Some(RegisterRole::ConditionCodes),
        readable: true,
        writable: false,
        flags: m68k_flag_descriptions(),
    });
    descriptors.push(RegisterDescriptor {
        id: M68K_REG_CCR,
        name: "CCR".to_string(),
        width: RegisterWidth::Bits8,
        category: RegisterCategory::Status,
        role: Some(RegisterRole::ConditionCodes),
        readable: true,
        writable: false,
        flags: m68k_flag_descriptions(),
    });
    for index in 0..8u32 {
        descriptors.push(RegisterDescriptor {
            id: M68K_REG_FP0 + index,
            name: format!("FP{index}"),
            width: RegisterWidth::Other(80),
            category: RegisterCategory::FloatingPoint,
            role: None,
            readable: true,
            writable: false,
            flags: Vec::new(),
        });
    }
    descriptors.push(RegisterDescriptor {
        id: M68K_REG_FPCR,
        name: "FPCR".to_string(),
        width: RegisterWidth::Bits32,
        category: RegisterCategory::FloatingPoint,
        role: None,
        readable: true,
        writable: false,
        flags: Vec::new(),
    });
    descriptors.push(RegisterDescriptor {
        id: M68K_REG_FPSR,
        name: "FPSR".to_string(),
        width: RegisterWidth::Bits32,
        category: RegisterCategory::FloatingPoint,
        role: None,
        readable: true,
        writable: false,
        flags: Vec::new(),
    });
    descriptors.push(RegisterDescriptor {
        id: M68K_REG_FPIAR,
        name: "FPIAR".to_string(),
        width: RegisterWidth::Bits32,
        category: RegisterCategory::FloatingPoint,
        role: None,
        readable: true,
        writable: false,
        flags: Vec::new(),
    });
    descriptors
}

fn float_x80_bytes(value: &m68k::fpu::FloatX80) -> Vec<u8> {
    // Preserve the explicit significand and NaN payload rather than converting
    // through a host float.
    let mut bytes = Vec::with_capacity(10);
    bytes.extend_from_slice(&value.sign_exp.to_be_bytes());
    bytes.extend_from_slice(&value.mantissa.to_be_bytes());
    bytes
}

fn m68k_register_snapshots(runner: &FixtureRunner) -> Vec<RegisterSnapshot> {
    let cpu = runner.cpu();
    let descriptors = m68k_register_descriptors();
    descriptors
        .into_iter()
        .map(|descriptor| {
            let value = match descriptor.id {
                M68K_REG_PC => RegisterValue::unsigned(u64::from(cpu.core.pc), 32),
                M68K_REG_SR => RegisterValue::unsigned(u64::from(cpu.core.get_sr()), 16),
                M68K_REG_CCR => RegisterValue::unsigned(u64::from(cpu.core.get_ccr()), 8),
                M68K_REG_FPCR => RegisterValue::unsigned(u64::from(cpu.core.fpcr), 32),
                M68K_REG_FPSR => RegisterValue::unsigned(u64::from(cpu.core.fpsr), 32),
                M68K_REG_FPIAR => RegisterValue::unsigned(u64::from(cpu.core.fpiar), 32),
                id if (M68K_REG_D0..M68K_REG_D0 + 8).contains(&id) => {
                    RegisterValue::unsigned(u64::from(cpu.core.d((id - M68K_REG_D0) as usize)), 32)
                }
                id if (M68K_REG_A0..M68K_REG_A0 + 8).contains(&id) => {
                    RegisterValue::unsigned(u64::from(cpu.core.a((id - M68K_REG_A0) as usize)), 32)
                }
                id if (M68K_REG_FP0..M68K_REG_FP0 + 8).contains(&id) => {
                    let index = (id - M68K_REG_FP0) as usize;
                    RegisterValue::bytes(float_x80_bytes(&cpu.core.fpr[index]), 80)
                }
                _ => RegisterValue::unsigned(0, 32),
            };
            RegisterSnapshot { descriptor, value }
        })
        .collect()
}

fn ppc_register_descriptors() -> Vec<RegisterDescriptor> {
    let mut descriptors = Vec::with_capacity(32 + 32 + 6);
    for index in 0..32u32 {
        descriptors.push(RegisterDescriptor {
            id: index,
            name: format!("r{index}"),
            width: RegisterWidth::Bits32,
            category: if index == 1 {
                RegisterCategory::StackPointer
            } else {
                RegisterCategory::General
            },
            role: match index {
                1 => Some(RegisterRole::StackPointer),
                2 => Some(RegisterRole::Other),
                _ => None,
            },
            readable: true,
            writable: false,
            flags: Vec::new(),
        });
    }
    for index in 0..32u32 {
        descriptors.push(RegisterDescriptor {
            id: 32 + index,
            name: format!("f{index}"),
            width: RegisterWidth::Bits64,
            category: RegisterCategory::FloatingPoint,
            role: None,
            readable: true,
            writable: false,
            flags: Vec::new(),
        });
    }
    let specials = [
        (
            "pc",
            RegisterCategory::ProgramCounter,
            Some(RegisterRole::ProgramCounter),
        ),
        (
            "lr",
            RegisterCategory::Control,
            Some(RegisterRole::LinkRegister),
        ),
        ("ctr", RegisterCategory::Control, None),
        (
            "cr",
            RegisterCategory::Status,
            Some(RegisterRole::ConditionCodes),
        ),
        ("xer", RegisterCategory::Status, None),
        ("fpscr", RegisterCategory::FloatingPoint, None),
        ("msr", RegisterCategory::Status, None),
    ];
    for (index, (name, category, role)) in specials.into_iter().enumerate() {
        descriptors.push(RegisterDescriptor {
            id: 64 + index as u32,
            name: name.to_string(),
            width: RegisterWidth::Bits32,
            category,
            role,
            readable: true,
            writable: false,
            flags: Vec::new(),
        });
    }
    descriptors
}

fn ppc_register_snapshots(cpu: &PpcCpu) -> Vec<RegisterSnapshot> {
    ppc_register_descriptors()
        .into_iter()
        .map(|descriptor| {
            let value = match descriptor.id {
                id if id < 32 => RegisterValue::unsigned(u64::from(cpu.gpr[id as usize]), 32),
                id if id < 64 => RegisterValue::unsigned(cpu.fpr[(id - 32) as usize], 64),
                id => match id - 64 {
                    0 => RegisterValue::unsigned(u64::from(cpu.pc), 32),
                    1 => RegisterValue::unsigned(u64::from(cpu.lr), 32),
                    2 => RegisterValue::unsigned(u64::from(cpu.ctr), 32),
                    3 => RegisterValue::unsigned(u64::from(cpu.cr), 32),
                    4 => RegisterValue::unsigned(u64::from(cpu.xer), 32),
                    5 => RegisterValue::unsigned(u64::from(cpu.fpscr()), 32),
                    6 => RegisterValue::unsigned(u64::from(cpu.msr), 32),
                    _ => RegisterValue::unsigned(0, 32),
                },
            };
            RegisterSnapshot { descriptor, value }
        })
        .collect()
}

pub struct M68kAdapter<'a> {
    runner: &'a FixtureRunner,
}

impl<'a> M68kAdapter<'a> {
    pub fn new(runner: &'a FixtureRunner) -> Self {
        Self { runner }
    }
}

impl ArchitectureAdapter for M68kAdapter<'_> {
    fn context(&self) -> ExecutionContextDescriptor {
        m68k_context_descriptor()
    }

    fn address_spaces(&self) -> Vec<AddressSpaceDescriptor> {
        vec![m68k_space_descriptor()]
    }

    fn address_space_capabilities(
        &self,
        space: AddressSpaceId,
    ) -> DebugResult<AddressSpaceCapabilities> {
        if space != M68K_SPACE {
            return Err(DebugError::Inaccessible {
                space,
                detail: "unknown m68k address space".into(),
            });
        }
        Ok(AddressSpaceCapabilities {
            read: true,
            write: false,
            observational_reads: true,
            code_cache_invalidation: false,
            max_transfer_bytes: MAX_MEMORY_TRANSFER,
        })
    }

    fn location(&self) -> ExecutionLocation {
        ExecutionLocation {
            context: M68K_CONTEXT,
            address: DebugAddress::new(M68K_SPACE, u64::from(self.runner.cpu().core.pc)),
        }
    }

    fn registers(&self, context: ContextId) -> DebugResult<Vec<RegisterSnapshot>> {
        if context != M68K_CONTEXT {
            return Err(DebugError::UnknownContext { id: context });
        }
        Ok(m68k_register_snapshots(self.runner))
    }

    fn disassemble(
        &self,
        context: ContextId,
        address: DebugAddress,
        count: u32,
    ) -> DebugResult<Vec<DisassemblyLine>> {
        if context != M68K_CONTEXT {
            return Err(DebugError::UnknownContext { id: context });
        }
        if address.space != M68K_SPACE {
            return Err(DebugError::Inaccessible {
                space: address.space,
                detail: "m68k adapter only disassembles 68k memory".to_string(),
            });
        }
        m68k_disassembly(self.runner, address.offset, count)
    }

    fn stack_preview(&self, context: ContextId, words: u32) -> DebugResult<StackPreview> {
        if context != M68K_CONTEXT {
            return Err(DebugError::UnknownContext { id: context });
        }
        m68k_stack_preview(self.runner, u64::from(self.runner.cpu().core.a(7)), words)
    }

    fn read_memory(
        &self,
        space: AddressSpaceId,
        address: u64,
        length: u64,
    ) -> DebugResult<MemoryReadResult> {
        if space != M68K_SPACE {
            return Err(DebugError::Inaccessible {
                space,
                detail: "m68k adapter only exposes 68k memory".to_string(),
            });
        }
        read_m68k_memory(self.runner, address, length)
    }
}

pub struct PpcAdapter<'a> {
    cpu: &'a PpcCpu,
    context: ContextId,
    space: AddressSpaceId,
    task: &'static str,
}

impl<'a> PpcAdapter<'a> {
    pub fn new(
        cpu: &'a PpcCpu,
        context: ContextId,
        space: AddressSpaceId,
        task: &'static str,
    ) -> Self {
        Self {
            cpu,
            context,
            space,
            task,
        }
    }

    fn context_descriptor(&self) -> ExecutionContextDescriptor {
        let mut descriptor = ppc_context_descriptor();
        descriptor.id = self.context;
        descriptor.task = Some(self.task.to_string());
        descriptor.address_spaces = vec![self.space];
        descriptor
    }

    fn space_descriptor(&self) -> AddressSpaceDescriptor {
        let mut descriptor = ppc_space_descriptor();
        descriptor.id = self.space;
        descriptor
    }
}

impl ArchitectureAdapter for PpcAdapter<'_> {
    fn context(&self) -> ExecutionContextDescriptor {
        self.context_descriptor()
    }

    fn address_spaces(&self) -> Vec<AddressSpaceDescriptor> {
        vec![self.space_descriptor()]
    }

    fn address_space_capabilities(
        &self,
        space: AddressSpaceId,
    ) -> DebugResult<AddressSpaceCapabilities> {
        if space != self.space {
            return Err(DebugError::Inaccessible {
                space,
                detail: "unknown PowerPC address space".into(),
            });
        }
        Ok(AddressSpaceCapabilities {
            read: false,
            write: false,
            observational_reads: true,
            code_cache_invalidation: false,
            max_transfer_bytes: 0,
        })
    }

    fn location(&self) -> ExecutionLocation {
        ExecutionLocation {
            context: self.context,
            address: DebugAddress::new(self.space, u64::from(self.cpu.pc)),
        }
    }

    fn registers(&self, context: ContextId) -> DebugResult<Vec<RegisterSnapshot>> {
        if context != self.context {
            return Err(DebugError::UnknownContext { id: context });
        }
        Ok(ppc_register_snapshots(self.cpu))
    }

    fn disassemble(
        &self,
        context: ContextId,
        address: DebugAddress,
        _count: u32,
    ) -> DebugResult<Vec<DisassemblyLine>> {
        if context != self.context {
            return Err(DebugError::UnknownContext { id: context });
        }
        if address.space != self.space {
            return Err(DebugError::Inaccessible {
                space: address.space,
                detail: "ppc adapter only exposes its registered address space".to_string(),
            });
        }
        Err(DebugError::unsupported("ppc disassembly"))
    }

    fn stack_preview(&self, _context: ContextId, _words: u32) -> DebugResult<StackPreview> {
        Err(DebugError::unsupported("ppc stack preview"))
    }

    fn read_memory(
        &self,
        space: AddressSpaceId,
        _address: u64,
        _length: u64,
    ) -> DebugResult<MemoryReadResult> {
        Err(DebugError::unsupported(format!(
            "ppc memory inspection in space {space}"
        )))
    }
}

pub struct Adapters<'a> {
    adapters: Vec<Box<dyn ArchitectureAdapter + 'a>>,
    active: Option<ContextId>,
    active_ambiguous: bool,
    contract_error: Option<DebugError>,
}

fn m68k_factory(runner: &FixtureRunner) -> Option<Box<dyn ArchitectureAdapter + '_>> {
    Some(Box::new(M68kAdapter::new(runner)))
}

fn ppc_factory(runner: &FixtureRunner) -> Option<Box<dyn ArchitectureAdapter + '_>> {
    runner.debug_ppc_cpu().map(|cpu| {
        Box::new(PpcAdapter::new(
            cpu,
            PPC_CONTEXT,
            PPC_SPACE,
            "native-application",
        )) as Box<dyn ArchitectureAdapter>
    })
}

fn companion_factory(runner: &FixtureRunner) -> Option<Box<dyn ArchitectureAdapter + '_>> {
    runner.debug_ppc_companion_cpu().map(|cpu| {
        Box::new(PpcAdapter::new(
            cpu,
            PPC_COMPANION_CONTEXT,
            PPC_COMPANION_SPACE,
            "native-companion",
        )) as Box<dyn ArchitectureAdapter>
    })
}

pub fn builtin_adapter_registrations() -> Vec<AdapterRegistration> {
    vec![
        AdapterRegistration {
            context: M68K_CONTEXT,
            address_spaces: &[M68K_SPACE],
            factory: m68k_factory,
            is_active: |runner| runner.debug_active_architecture() == ActiveArchitecture::M68k,
        },
        AdapterRegistration {
            context: PPC_CONTEXT,
            address_spaces: &[PPC_SPACE],
            factory: ppc_factory,
            is_active: |runner| runner.debug_active_architecture() == ActiveArchitecture::PowerPc,
        },
        AdapterRegistration {
            context: PPC_COMPANION_CONTEXT,
            address_spaces: &[PPC_COMPANION_SPACE],
            factory: companion_factory,
            is_active: |runner| {
                runner.debug_active_architecture() == ActiveArchitecture::PowerPcCompanion
            },
        },
    ]
}

impl<'a> Adapters<'a> {
    pub fn for_runner(runner: &'a FixtureRunner) -> Self {
        let registrations = runner.debug.adapter_registrations();
        let mut active = registrations
            .iter()
            .filter(|registration| (registration.is_active)(runner))
            .map(|registration| registration.context);
        let first_active = active.next();
        let active_ambiguous = active.next().is_some();
        let active = (!active_ambiguous).then_some(first_active).flatten();
        let mut contract_error = None;
        let adapters = registrations
            .iter()
            .filter_map(|registration| {
                let adapter = (registration.factory)(runner);
                if let Some(adapter) = adapter.as_ref() {
                    let context_descriptor = adapter.context();
                    let context = context_descriptor.id;
                    let spaces = adapter.address_spaces();
                    let duplicate_space = spaces
                        .iter()
                        .enumerate()
                        .any(|(index, space)| spaces[..index].iter().any(|prior| prior.id == space.id));
                    let invalid_id = context == ContextId::UNSPECIFIED
                        || spaces.iter().any(|space| space.id == AddressSpaceId::UNSPECIFIED);
                    let registered_spaces_match = registration.address_spaces.len() == spaces.len()
                        && registration
                            .address_spaces
                            .iter()
                            .all(|id| spaces.iter().any(|space| space.id == *id));
                    let context_spaces_match = context_descriptor.address_spaces.len() == spaces.len()
                        && spaces
                            .iter()
                            .all(|space| context_descriptor.address_spaces.contains(&space.id));
                    if invalid_id
                        || context != registration.context
                        || !registered_spaces_match
                        || !context_spaces_match
                        || duplicate_space
                    {
                        contract_error = Some(DebugError::InvalidValue {
                            detail: format!(
                                "adapter factory identity does not match registration for context {}",
                                registration.context
                            ),
                        });
                        return None;
                    }
                }
                adapter
            })
            .collect();
        Self {
            adapters,
            active,
            active_ambiguous,
            contract_error,
        }
    }

    pub fn validate(&self) -> DebugResult<()> {
        self.contract_error.clone().map_or(Ok(()), Err)
    }

    fn as_dyn(&self) -> impl Iterator<Item = &dyn ArchitectureAdapter> {
        self.adapters.iter().map(Box::as_ref)
    }

    pub fn find(&self, id: ContextId) -> Option<&dyn ArchitectureAdapter> {
        self.as_dyn().find(|adapter| adapter.context().id == id)
    }

    pub fn by_space(&self, space: AddressSpaceId) -> Option<&dyn ArchitectureAdapter> {
        self.as_dyn().find(|adapter| {
            adapter
                .address_spaces()
                .iter()
                .any(|candidate| candidate.id == space)
        })
    }

    pub fn active(&self) -> Option<&dyn ArchitectureAdapter> {
        self.active.and_then(|id| self.find(id))
    }

    pub fn resolve(&self, selector: ContextSelector) -> DebugResult<ContextId> {
        match selector {
            ContextSelector::Id { id } => {
                if self.find(id).is_some() {
                    Ok(id)
                } else {
                    Err(DebugError::UnknownContext { id })
                }
            }
            ContextSelector::Active => self
                .active()
                .map(|adapter| adapter.context().id)
                .ok_or_else(|| {
                    DebugError::invalid_state(if self.active_ambiguous {
                        "multiple execution contexts claim to be active"
                    } else {
                        "no inspectable active context"
                    })
                }),
        }
    }

    pub fn contexts(&self) -> Vec<ExecutionContextDescriptor> {
        let active = self.active().map(|adapter| adapter.context().id);
        self.as_dyn()
            .map(|adapter| {
                let mut descriptor = adapter.context();
                descriptor.lifecycle = if Some(descriptor.id) == active {
                    ContextLifecycle::Active
                } else {
                    ContextLifecycle::Suspended
                };
                descriptor
            })
            .collect()
    }
}

pub(crate) fn read_m68k_memory(
    runner: &FixtureRunner,
    address: u64,
    length: u64,
) -> DebugResult<MemoryReadResult> {
    if length > MAX_MEMORY_TRANSFER {
        return Err(DebugError::TooLarge {
            limit: MAX_MEMORY_TRANSFER,
            requested: length,
        });
    }
    if address > u64::from(u32::MAX) || length > u64::from(u32::MAX) {
        return Err(DebugError::InvalidRange {
            detail: "68k address space is 32-bit".to_string(),
        });
    }
    let bus = runner.bus();
    let ram_size = bus.ram_size();
    let mut truncated = false;
    let mut bytes = Vec::with_capacity(length as usize);
    for offset in 0..length {
        let current = address.wrapping_add(offset);
        if current > u64::from(u32::MAX) {
            truncated = true;
            break;
        }
        let translated = bus.translate_guest_address(current as u32);
        if translated >= ram_size {
            truncated = true;
            break;
        }
        bytes.push(bus.read_byte(current as u32));
    }
    Ok(MemoryReadResult {
        address: DebugAddress::new(M68K_SPACE, address),
        bytes,
        truncated,
    })
}

pub(crate) fn m68k_disassembly(
    runner: &FixtureRunner,
    address: u64,
    count: u32,
) -> DebugResult<Vec<DisassemblyLine>> {
    if count > MAX_DISASSEMBLY_COUNT {
        return Err(DebugError::TooLarge {
            limit: u64::from(MAX_DISASSEMBLY_COUNT),
            requested: u64::from(count),
        });
    }
    if address > u64::from(u32::MAX) {
        return Err(DebugError::InvalidRange {
            detail: "68k address space is 32-bit".to_string(),
        });
    }
    let bus = runner.bus();
    let ram_size = bus.ram_size();
    let mut lines = Vec::with_capacity(count as usize);
    let mut current = address as u32;
    for _ in 0..count {
        if bus.translate_guest_address(current) >= ram_size {
            lines.push(DisassemblyLine {
                address: DebugAddress::new(M68K_SPACE, u64::from(current)),
                approximate: true,
                text: "<unmapped>".to_string(),
                size: 2,
                bytes: Vec::new(),
            });
            let Some(next) = current.checked_add(2) else {
                break;
            };
            current = next;
            continue;
        }
        let opcode_bytes = read_m68k_memory(runner, u64::from(current), 2)?;
        if opcode_bytes.bytes.len() != 2 {
            break;
        }
        let opcode = u16::from_be_bytes([opcode_bytes.bytes[0], opcode_bytes.bytes[1]]);
        let (text, size) = m68k::dasm::disassemble(current, opcode, runner.cpu().core.cpu_type);
        let size = size.clamp(2, 10);
        let bytes = read_m68k_memory(runner, u64::from(current), u64::from(size))?.bytes;
        lines.push(DisassemblyLine {
            address: DebugAddress::new(M68K_SPACE, u64::from(current)),
            text,
            approximate: true,
            size: size as u8,
            bytes,
        });
        let Some(next) = current.checked_add(size) else {
            break;
        };
        current = next;
    }
    Ok(lines)
}

fn m68k_stack_preview(runner: &FixtureRunner, base: u64, words: u32) -> DebugResult<StackPreview> {
    let requested = u64::from(words) * 4;
    if requested > MAX_STACK_PREVIEW_BYTES {
        return Err(DebugError::TooLarge {
            limit: MAX_STACK_PREVIEW_BYTES,
            requested,
        });
    }
    let memory = read_m68k_memory(runner, base, requested)?;
    Ok(StackPreview {
        context: M68K_CONTEXT,
        address: DebugAddress::new(M68K_SPACE, base),
        bytes: memory.bytes,
        stride: 4,
        truncated: memory.truncated,
    })
}

pub(crate) fn active_context_and_location(
    runner: &FixtureRunner,
) -> (Option<ContextId>, Option<ExecutionLocation>) {
    let adapters = Adapters::for_runner(runner);
    match adapters.active() {
        Some(adapter) => (Some(adapter.context().id), Some(adapter.location())),
        None => (None, None),
    }
}

pub(crate) fn active_context_id(runner: &FixtureRunner) -> Option<ContextId> {
    Adapters::for_runner(runner)
        .active()
        .map(|adapter| adapter.context().id)
}
