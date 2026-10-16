//! Transactional application preparation: all parsing/relocation completes in
//! owned staging state before a new Runtime can execute any guest instruction.
mod exports;
mod partition;
pub use exports::Export;
pub use partition::ApplicationSize;
pub(crate) mod pef;
mod reloc;
pub mod resource;
use crate::{
    abi::Abi,
    cpu::{Architecture, Cpu},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Runtime,
    Error, Result,
};
use resource::ResourceFork;
use std::collections::BTreeMap;
pub const IMAGE_LIMIT: usize = 64 * 1024 * 1024;
pub const SECTION_BASE: u32 = 0x0100_0000;
pub const STACK_BASE: u32 = 0x0080_0000;
pub const STACK_SIZE: u32 = 0x10000;
pub const APPLICATION_RETURN: u32 = 0x8200;
pub const A5_BASE: u32 = 0x0040_0000;
#[derive(Debug)]
pub struct LoadInfo {
    pub architecture: Architecture,
    pub entry: u32,
    pub toc_or_a5: u32,
    pub sections: Vec<(u32, u32)>,
    pub stack: (u32, u32),
    pub application_size: Option<ApplicationSize>,
    /// PEF export table order, retained for CFM indexed symbol enumeration.
    pub exports: std::sync::Arc<[Export]>,
}
pub struct LoadedApplication {
    pub runtime: Runtime,
    pub info: LoadInfo,
}
impl LoadedApplication {
    /// Select a physical main display before any guest instruction or graphics
    /// state exists. Consuming preparation keeps failed configuration private.
    pub fn with_display(mut self, display: crate::display::Display) -> Result<Self> {
        if self.runtime.instructions != 0
            || self.runtime.process.ports.is_some()
            || self.runtime.graphics.is_some()
            || self.runtime.memory.u32(crate::devices::MAIN)? != 0
        {
            return Err(invalid(
                "display must be selected before application execution",
            ));
        }
        let screen = display.bitmap();
        let mut prepared = Memory::default();
        prepared.map(screen.base, vec![0; screen.byte_len()?], Permissions::RW)?;
        self.runtime.memory.remove_mappings(&[screen.base.0]);
        self.runtime.memory.adopt(prepared)?;
        self.runtime.process.display = display;
        if display.depth() != 1 {
            let mut devices = std::mem::take(&mut self.runtime.process.devices);
            let result = devices.call(
                &mut self.runtime.memory,
                &mut self.runtime.process,
                crate::devices::Op::Main,
            );
            self.runtime.process.devices = devices;
            result?;
        }
        Ok(self)
    }

    pub fn code(resource_fork: &[u8]) -> Result<Self> {
        let mut app = code(ResourceFork::parse(resource_fork)?)?;
        app.runtime.process.install_files(&[], Some(resource_fork));
        app.runtime.memory.write(
            GuestPtr(0xa5a),
            &app.runtime.process.current_resource_file().to_be_bytes(),
        )?;
        Ok(app)
    }
    pub fn pef(data_fork: &[u8], resource_fork: Option<&[u8]>) -> Result<Self> {
        let mut app = pef::load(
            data_fork,
            resource_fork
                .map(ResourceFork::parse)
                .transpose()?
                .unwrap_or_default(),
        )?;
        app.runtime.process.install_files(data_fork, resource_fork);
        app.runtime.memory.write(
            GuestPtr(0xa5a),
            &app.runtime.process.current_resource_file().to_be_bytes(),
        )?;
        Ok(app)
    }
}
pub(super) fn invalid(message: impl Into<String>) -> Error {
    Error::Load(message.into())
}
pub(super) fn limit(size: usize) -> Result<()> {
    if size > IMAGE_LIMIT {
        Err(invalid("64 MiB image limit exceeded"))
    } else {
        Ok(())
    }
}
pub(super) fn bytes(input: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    input
        .get(
            offset
                ..offset
                    .checked_add(len)
                    .ok_or_else(|| invalid("file range overflow"))?,
        )
        .ok_or_else(|| invalid(format!("truncated file range {offset:#x}+{len:#x}")))
}
pub(super) fn u16_at(b: &[u8], o: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(bytes(b, o, 2)?.try_into().unwrap()))
}
pub(super) fn u32_at(b: &[u8], o: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(bytes(b, o, 4)?.try_into().unwrap()))
}
pub(super) fn finish(
    memory: Memory,
    arch: Architecture,
    entry: u32,
    context: u32,
    resources: ResourceFork,
    sections: Vec<(u32, u32)>,
    abi: Abi,
) -> Result<LoadedApplication> {
    finish_at(
        memory, arch, entry, context, resources, sections, abi, STACK_BASE,
    )
}
#[allow(clippy::too_many_arguments)]
fn finish_at(
    mut memory: Memory,
    arch: Architecture,
    entry: u32,
    context: u32,
    resources: ResourceFork,
    sections: Vec<(u32, u32)>,
    abi: Abi,
    stack_base: u32,
) -> Result<LoadedApplication> {
    let application_size = ApplicationSize::read(&resources)?;
    crate::traps::install(&mut memory)?;
    memory.map(GuestPtr(0), vec![0; 4096], Permissions::RW)?;
    crate::addressing::install(&mut memory)?;
    crate::retrace::install(&mut memory)?;
    memory.write(
        crate::calendar::TIME,
        &crate::calendar::DEFAULT_TIME.to_be_bytes(),
    )?;
    memory.write(GuestPtr(0x2f4), &32u32.to_be_bytes())?;
    crate::events::install(&mut memory)?;
    crate::quickdraw::install(&mut memory)?;
    memory.map(
        GuestPtr(stack_base),
        vec![0; STACK_SIZE as usize],
        Permissions::RW,
    )?;
    let sp = stack_base
        .checked_add(STACK_SIZE)
        .and_then(|end| end.checked_sub(0x100))
        .ok_or_else(|| invalid("application stack overflow"))?;
    let mut cpu = Cpu::new(arch, entry, sp);
    match &mut cpu {
        Cpu::M68k(c) => {
            c.dar[13] = context;
            // MPW startup inspects the caller's instruction bytes. Give loaded
            // applications an actual ExitToShell return target, not a private gate.
            memory.map(
                GuestPtr(APPLICATION_RETURN),
                vec![0xa9, 0xf4, 0x4a, 0xfc],
                Permissions::RX,
            )?;
            memory.write(GuestPtr(sp), &APPLICATION_RETURN.to_be_bytes())?;
        }
        Cpu::PowerPc(c) => {
            c.gpr[2] = context;
            memory.write(GuestPtr(sp), &0u32.to_be_bytes())?;
        }
    }
    memory.fetch(
        GuestPtr(entry),
        if arch == Architecture::M68k { 2 } else { 4 },
        if arch == Architecture::M68k { 2 } else { 4 },
    )?;
    let mut runtime = Runtime::new(memory, cpu, None);
    runtime.abi = abi;
    if arch == Architecture::PowerPc {
        if let Some(size) = application_size {
            runtime
                .process
                .initialize_sized_heap(&mut runtime.memory, size.aligned_size()? - STACK_SIZE)?;
        } else {
            runtime.process.initialize_heap(&mut runtime.memory)?;
        }
    } else {
        runtime.process.initialize_heap(&mut runtime.memory)?;
    }
    runtime.process.install_application_resources(resources);
    Ok(LoadedApplication {
        runtime,
        info: LoadInfo {
            architecture: arch,
            entry,
            toc_or_a5: context,
            sections,
            stack: (stack_base, STACK_SIZE),
            application_size,
            exports: std::sync::Arc::from([]),
        },
    })
}
/// Near-model CODE resources: Mac OS Runtime Architectures (1997), chapter 10,
/// "The Jump Table". Eager loading is explicit; far models remain unsupported.
fn code(resources: ResourceFork) -> Result<LoadedApplication> {
    let zero = resources
        .get(*b"CODE", 0)
        .ok_or_else(|| invalid("missing CODE 0"))?;
    if zero.attributes & 1 != 0 {
        return Err(invalid("compressed CODE resources unsupported"));
    }
    let c = &zero.data;
    let above = u32_at(c, 0)?;
    let below = u32_at(c, 4)?;
    let jt_len = u32_at(c, 8)?;
    let jt_offset = u32_at(c, 12)?;
    if jt_len == 0
        || jt_len % 8 != 0
        || jt_offset < 32
        || jt_offset > u16::MAX as u32
        || jt_offset.checked_add(jt_len).is_none_or(|end| end > above)
    {
        return Err(invalid("invalid CODE 0 A5/jump-table layout"));
    }
    let world_len = above
        .checked_add(below)
        .ok_or_else(|| invalid("A5 size overflow"))?;
    limit(world_len as usize)?;
    let size = ApplicationSize::read(&resources)?;
    let stack_base = if let Some(size) = size {
        0x0001_0000u32
            .checked_add(size.aligned_size()?)
            .and_then(|end| end.checked_sub(STACK_SIZE))
            .ok_or_else(|| invalid("CODE partition overflow"))?
    } else {
        STACK_BASE
    };
    let a5 = if size.is_some() {
        stack_base
            .checked_sub(above)
            .map(|v| v & !3)
            .ok_or_else(|| invalid("SIZE partition cannot fit A5 globals"))?
    } else {
        A5_BASE
    };
    let world_base = a5
        .checked_sub(below)
        .filter(|v| *v >= 0x0001_0100)
        .ok_or_else(|| invalid("A5 globals exceed reserved region"))?;
    if a5.checked_add(above).is_none_or(|v| v > stack_base) {
        return Err(invalid("A5 world overlaps stack reservation"));
    }
    let table = bytes(c, 16, jt_len as usize)?;
    let mut world = vec![0; world_len as usize];
    let mut memory = Memory::default();
    let mut segments = BTreeMap::new();
    let mut section_info = Vec::new();
    let mut cursor = SECTION_BASE.max(
        stack_base
            .checked_add(STACK_SIZE)
            .ok_or_else(|| invalid("CODE section base overflow"))?,
    );
    let mut total = world_len as usize;
    for (i, entry) in table.as_chunks::<8>().0.iter().enumerate() {
        if u16_at(entry, 2)? != 0x3f3c || u16_at(entry, 6)? != 0xa9f0 {
            return Err(invalid("unsupported CODE jump-table format"));
        }
        let id = u16_at(entry, 4)? as i16;
        if id <= 0 {
            return Err(invalid("invalid CODE segment ID"));
        }
        if let std::collections::btree_map::Entry::Vacant(slot) = segments.entry(id) {
            let resource = resources
                .get(*b"CODE", id)
                .ok_or_else(|| invalid(format!("missing CODE {id}")))?;
            if resource.attributes & 1 != 0 {
                return Err(invalid("compressed CODE resources unsupported"));
            }
            let seg = &resource.data;
            let first = u16_at(seg, 0)? as usize;
            let count = u16_at(seg, 2)? as usize;
            if first == 0xffff || count & 0xc000 != 0 {
                return Err(invalid("far-model CODE unsupported"));
            }
            if !first.is_multiple_of(8) || first + count * 8 > table.len() || count == 0 {
                return Err(invalid("invalid CODE segment jump-table range"));
            }
            for index in first / 8..first / 8 + count {
                if u16_at(table, index * 8 + 4)? as i16 != id {
                    return Err(invalid("CODE segment/table ownership mismatch"));
                }
            }
            let payload = bytes(seg, 4, seg.len() - 4)?;
            if payload.is_empty() {
                return Err(invalid("empty CODE segment"));
            }
            total = total
                .checked_add(payload.len())
                .ok_or_else(|| invalid("image size overflow"))?;
            limit(total)?;
            let base = cursor;
            cursor = cursor
                .checked_add(payload.len() as u32)
                .and_then(|v| v.checked_add(15))
                .map(|v| v & !15)
                .ok_or_else(|| invalid("CODE address overflow"))?;
            // Classic RAM CODE can contain startup-relocated operands and
            // self-modifying instructions. Fetches share these canonical bytes.
            memory.map(GuestPtr(base), payload.to_vec(), Permissions::RWX)?;
            section_info.push((base, payload.len() as u32));
            slot.insert((base, payload.len(), first / 8, count));
        }
        let &(base, len, first, count) = segments.get(&id).unwrap();
        let offset = u16_at(entry, 0)? as usize;
        if i < first || i >= first + count || !offset.is_multiple_of(2) || offset + 2 > len {
            return Err(invalid("CODE routine offset/ownership invalid"));
        }
        let p = below as usize + jt_offset as usize + i * 8;
        world[p..p + 2].copy_from_slice(&(id as u16).to_be_bytes());
        world[p + 2..p + 4].copy_from_slice(&0x4ef9u16.to_be_bytes());
        world[p + 4..p + 8].copy_from_slice(&(base + offset as u32).to_be_bytes());
    }
    // Classic jump tables are writable executable guest structures.
    memory.map(GuestPtr(world_base), world, Permissions::RWX)?;
    let mut app = finish_at(
        memory,
        Architecture::M68k,
        a5 + jt_offset + 2,
        a5,
        resources,
        section_info,
        Abi::macintosh(),
        stack_base,
    )?;
    app.runtime
        .process
        .initialize_code_heap(&mut app.runtime.memory, world_base)?;
    // Inside Macintosh: Memory, Memory Manager summary: CurStackBase is
    // the start of application globals. It is not our separate stack mapping.
    let memory = &mut app.runtime.memory;
    memory.write(GuestPtr(0x0904), &a5.to_be_bytes())?; // CurrentA5
    memory.write(GuestPtr(0x0908), &world_base.to_be_bytes())?; // CurStackBase
    memory.write(GuestPtr(0x0934), &(jt_offset as u16).to_be_bytes())?; // CurJTOffset
    Ok(app)
}
