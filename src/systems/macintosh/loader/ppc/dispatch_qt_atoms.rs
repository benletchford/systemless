//! QuickTime atom containers: a 12-byte container header and 20-byte atom
//! headers, as documented in Apple's QuickTime File Format specification.
//! Unlike movie-file atoms, these carry IDs and immediate-child counts.
use super::*;

const INVALID_CONTAINER: i16 = -2107;
const INVALID_ATOM: i16 = -2106;
const NOT_LEAF: i16 = -2102;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcQtAtomOperation {
    New,
    Dispose,
    FindId,
    FindIndex,
    TypeAndId,
    DataPtr,
    Lock,
    Unlock,
    Count,
    NextType,
    Insert,
    SetData,
    Remove,
    RemoveChildren,
}
impl PpcQtAtomOperation {
    pub(super) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "QTNewAtomContainer" => Self::New,
            "QTDisposeAtomContainer" => Self::Dispose,
            "QTFindChildByID" => Self::FindId,
            "QTFindChildByIndex" => Self::FindIndex,
            "QTGetAtomTypeAndID" => Self::TypeAndId,
            "QTGetAtomDataPtr" => Self::DataPtr,
            "QTLockContainer" => Self::Lock,
            "QTUnlockContainer" => Self::Unlock,
            "QTCountChildrenOfType" => Self::Count,
            "QTGetNextChildType" => Self::NextType,
            "QTInsertChild" => Self::Insert,
            "QTSetAtomData" => Self::SetData,
            "QTRemoveAtom" => Self::Remove,
            "QTRemoveChildren" => Self::RemoveChildren,
            _ => return None,
        })
    }
}

#[derive(Debug)]
struct Atom {
    offset: u32,
    end: u32,
    kind: u32,
    id: u32,
    parent: u32,
    children: u16,
}

// Validate the whole hierarchy before returning any interior pointer. An
// iterative walk avoids host stack exhaustion from deeply nested guest data.
fn parse(bytes: &[u8]) -> Option<Vec<Atom>> {
    let word = |p: usize| {
        Some(u32::from_be_bytes(
            bytes.get(p..p.checked_add(4)?)?.try_into().ok()?,
        ))
    };
    if bytes.len() < 32
        || word(12)? as usize != bytes.len() - 12
        || word(16)? != u32::from_be_bytes(*b"sean")
        || word(20)? != 1
    {
        return None;
    }
    let mut pending = vec![(12usize, bytes.len(), 0u32)];
    let mut atoms = Vec::new();
    while let Some((offset, limit, parent)) = pending.pop() {
        let end = offset.checked_add(word(offset)? as usize)?;
        if end > limit || end < offset.checked_add(20)? {
            return None;
        }
        let children = u16::from_be_bytes(bytes.get(offset + 14..offset + 16)?.try_into().ok()?);
        let atom = Atom {
            offset: u32::try_from(offset).ok()?,
            end: u32::try_from(end).ok()?,
            kind: word(offset + 4)?,
            id: word(offset + 8)?,
            parent,
            children,
        };
        if children != 0 {
            let mut child = offset + 20;
            let mut pending_children = Vec::new();
            for _ in 0..children {
                let next = child.checked_add(word(child)? as usize)?;
                if next > end || next < child.checked_add(20)? {
                    return None;
                }
                pending_children.push((child, next, atom.offset));
                child = next;
            }
            if child != end {
                return None;
            }
            pending.extend(pending_children.into_iter().rev());
        }
        atoms.push(atom);
    }
    Some(atoms)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch(
    operation: PpcQtAtomOperation,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> PpcImportAction {
    use PpcQtAtomOperation as Op;
    let result = |error| PpcImportAction::Return(ppc_i16_result(error));
    let g = cpu.gpr;
    if operation == Op::New {
        if g[3] == 0 || !ppc_memory_can_write_bytes(memory, g[3], 4) {
            return result(PPC_PARAM_ERR);
        }
        let mut bytes = vec![0; 32];
        bytes[12..16].copy_from_slice(&20u32.to_be_bytes());
        bytes[16..20].copy_from_slice(b"sean");
        bytes[20..24].copy_from_slice(&1u32.to_be_bytes());
        let handle = ppc_process_alloc_handle_with_bytes(
            manager,
            memory,
            heap_cursor,
            last_mem_error,
            handles,
            &bytes,
        );
        let _ = memory.write_u32_be(g[3], handle);
        return result(if handle == 0 {
            PPC_MEM_FULL_ERR
        } else {
            PPC_NO_ERR
        });
    }
    let returns_offset = matches!(
        operation,
        Op::FindId | Op::FindIndex | Op::Count | Op::NextType
    );
    let failure = |error| {
        if returns_offset {
            PpcImportAction::Return(0)
        } else {
            result(error)
        }
    };
    let handle = g[3];
    let Some(ptr) = memory.read_u32_be(handle).filter(|p| *p != 0) else {
        return failure(INVALID_CONTAINER);
    };
    let size = manager
        .process_handle_size_from_master_pointer(handle, ptr)
        .or_else(|| {
            handles
                .iter()
                .find(|h| h.handle == handle && h.ptr == ptr)
                .map(|h| h.size)
        });
    let Some(bytes) = size
        .filter(|size| ptr.checked_add(*size).is_some())
        .and_then(|size| ppc_memory_read_bytes(memory, ptr, size))
    else {
        return failure(INVALID_CONTAINER);
    };
    let Some(atoms) = parse(&bytes) else {
        return failure(INVALID_CONTAINER);
    };
    match operation {
        Op::Insert | Op::SetData | Op::Remove | Op::RemoveChildren => {
            let (edited, output) = match edit(operation, &g, bytes, &atoms, memory) {
                Ok(value) => value,
                Err(error) => return result(error),
            };
            let Ok(size) = u32::try_from(edited.len()) else {
                return result(PPC_MEM_FULL_ERR);
            };
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: manager,
            };
            let error =
                allocator.resize_handle(memory, heap_cursor, last_mem_error, handles, handle, size);
            if error != PPC_NO_ERR {
                return result(error);
            }
            let Some(ptr) = memory.read_u32_be(handle).filter(|ptr| *ptr != 0) else {
                return result(INVALID_CONTAINER);
            };
            for (index, byte) in edited.iter().enumerate() {
                let _ = memory.write_u8(ptr + index as u32, *byte);
            }
            if let Some((address, value)) = output {
                let _ = memory.write_u32_be(address, value);
            }
            result(PPC_NO_ERR)
        }
        Op::New => unreachable!(),
        Op::Dispose => {
            if !manager.dispose_process_handle_from_native_import(memory, handle) {
                return result(INVALID_CONTAINER);
            }
            handles.retain(|record| record.handle != handle);
            ppc_apply_process_native_allocator(manager, memory, heap_cursor, last_mem_error);
            result(PPC_NO_ERR)
        }
        Op::Lock | Op::Unlock => {
            let count = u16::from_be_bytes([bytes[10], bytes[11]]);
            let next = if operation == Op::Lock {
                count.checked_add(1)
            } else {
                count.checked_sub(1)
            };
            let Some(next) = next else {
                return result(PPC_PARAM_ERR);
            };
            if memory.write_u16_be(ptr + 10, next).is_none() {
                return result(PPC_PARAM_ERR);
            }
            if operation == Op::Lock {
                manager.lock_process_handle(handle, false);
            } else if next == 0 {
                manager.unlock_process_handle(handle);
            }
            result(PPC_NO_ERR)
        }
        Op::TypeAndId | Op::DataPtr => {
            let Some(atom) = atoms.iter().find(|atom| atom.offset == g[4]) else {
                return result(INVALID_ATOM);
            };
            if operation == Op::DataPtr && atom.children != 0 {
                return result(NOT_LEAF);
            }
            if [g[5], g[6]]
                .iter()
                .any(|p| *p != 0 && !ppc_memory_can_write_bytes(memory, *p, 4))
            {
                return result(PPC_PARAM_ERR);
            }
            let values = if operation == Op::TypeAndId {
                [atom.kind, atom.id]
            } else {
                [atom.end - atom.offset - 20, ptr + atom.offset + 20]
            };
            for (out, value) in [g[5], g[6]].into_iter().zip(values) {
                if out != 0 {
                    let _ = memory.write_u32_be(out, value);
                }
            }
            result(PPC_NO_ERR)
        }
        Op::FindId | Op::FindIndex | Op::Count | Op::NextType => {
            let parent = if g[4] == 0 { 12 } else { g[4] };
            if !atoms.iter().any(|atom| atom.offset == parent) {
                return PpcImportAction::Return(0);
            }
            let children: Vec<_> = atoms.iter().filter(|atom| atom.parent == parent).collect();
            if operation == Op::NextType {
                let mut types = Vec::new();
                for atom in &children {
                    if !types.contains(&atom.kind) {
                        types.push(atom.kind);
                    }
                }
                let next = if g[5] == 0 {
                    types.first()
                } else {
                    types
                        .iter()
                        .position(|kind| *kind == g[5])
                        .and_then(|i| types.get(i + 1))
                };
                return PpcImportAction::Return(next.copied().unwrap_or(0));
            }
            let matching: Vec<_> = children
                .into_iter()
                .filter(|atom| atom.kind == g[5] || (operation == Op::Count && g[5] == 0))
                .collect();
            if operation == Op::Count {
                return PpcImportAction::Return(ppc_i16_result(matching.len() as i16));
            }
            let found = matching.iter().enumerate().find(|(index, atom)| {
                if operation == Op::FindId {
                    atom.id == g[6]
                } else {
                    *index as i32 + 1 == g[6] as u16 as i16 as i32
                }
            });
            let Some((index, atom)) = found else {
                return PpcImportAction::Return(0);
            };
            if g[7] != 0 {
                let size = if operation == Op::FindId { 2 } else { 4 };
                if !ppc_memory_can_write_bytes(memory, g[7], size) {
                    return PpcImportAction::Return(0);
                }
                if operation == Op::FindId {
                    let _ = memory.write_u16_be(g[7], (index + 1) as u16);
                } else {
                    let _ = memory.write_u32_be(g[7], atom.id);
                }
            }
            PpcImportAction::Return(atom.offset)
        }
    }
}

// Build an edited copy first. All source offsets refer to the original tree;
// ancestor headers precede their descendants and remain at the same offsets.
fn edit(
    operation: PpcQtAtomOperation,
    g: &[u32; 32],
    mut bytes: Vec<u8>,
    atoms: &[Atom],
    memory: &mut PpcSectionMem,
) -> Result<(Vec<u8>, Option<(u32, u32)>), i16> {
    use PpcQtAtomOperation as Op;
    if u16::from_be_bytes([bytes[10], bytes[11]]) != 0 {
        return Err(PPC_PARAM_ERR);
    }
    let offset = if g[4] == 0 && matches!(operation, Op::Insert | Op::RemoveChildren) {
        12
    } else {
        g[4]
    };
    let node = atoms
        .iter()
        .find(|atom| atom.offset == offset)
        .ok_or(INVALID_ATOM)?;
    let (start, end, replacement, mut ancestor, output) = match operation {
        Op::Insert => {
            if g[5] == 0 {
                return Err(-2108);
            } // invalidAtomTypeErr
            if node.children == 0 && node.end != node.offset + 20 {
                return Err(NOT_LEAF);
            }
            let children: Vec<_> = atoms
                .iter()
                .filter(|atom| atom.parent == node.offset && atom.kind == g[5])
                .collect();
            let index = g[7] as u16 as i16;
            if index < 0 || index as usize > children.len() + 1 {
                return Err(-2104);
            }
            let id = if g[6] == 0 {
                (1..=u32::MAX)
                    .find(|id| !children.iter().any(|child| child.id == *id))
                    .ok_or(PPC_MEM_FULL_ERR)?
            } else {
                g[6]
            };
            if children.iter().any(|child| child.id == id) {
                return Err(-2105);
            }
            if (g[8] as i32) < 0 || (g[8] != 0 && g[9] == 0) {
                return Err(PPC_PARAM_ERR);
            }
            if g[10] != 0 && !ppc_memory_can_write_bytes(memory, g[10], 4) {
                return Err(PPC_PARAM_ERR);
            }
            if !ppc_memory_can_read_bytes(memory, g[9], g[8]) {
                return Err(PPC_PARAM_ERR);
            }
            let payload = ppc_memory_read_bytes(memory, g[9], g[8]).ok_or(PPC_PARAM_ERR)?;
            let size = g[8].checked_add(20).ok_or(PPC_MEM_FULL_ERR)?;
            let mut replacement = vec![0; 20];
            replacement[0..4].copy_from_slice(&size.to_be_bytes());
            replacement[4..8].copy_from_slice(&g[5].to_be_bytes());
            replacement[8..12].copy_from_slice(&id.to_be_bytes());
            replacement.extend(payload);
            let start = if index == 0 {
                node.end
            } else {
                children
                    .get(index as usize - 1)
                    .map_or(node.end, |child| child.offset)
            };
            let count = node.children.checked_add(1).ok_or(PPC_PARAM_ERR)?;
            bytes[node.offset as usize + 14..node.offset as usize + 16]
                .copy_from_slice(&count.to_be_bytes());
            (
                start,
                start,
                replacement,
                node.offset,
                (g[10] != 0).then_some((g[10], start)),
            )
        }
        Op::SetData => {
            if node.children != 0 || node.offset == 12 {
                return Err(NOT_LEAF);
            }
            if (g[5] as i32) < 0 || (g[5] != 0 && g[6] == 0) {
                return Err(PPC_PARAM_ERR);
            }
            if !ppc_memory_can_read_bytes(memory, g[6], g[5]) {
                return Err(PPC_PARAM_ERR);
            }
            let payload = ppc_memory_read_bytes(memory, g[6], g[5]).ok_or(PPC_PARAM_ERR)?;
            (node.offset + 20, node.end, payload, node.offset, None)
        }
        Op::Remove => {
            if node.offset == 12 {
                return Err(INVALID_ATOM);
            }
            let parent = atoms
                .iter()
                .find(|atom| atom.offset == node.parent)
                .ok_or(INVALID_ATOM)?;
            bytes[parent.offset as usize + 14..parent.offset as usize + 16]
                .copy_from_slice(&(parent.children - 1).to_be_bytes());
            (node.offset, node.end, Vec::new(), node.parent, None)
        }
        Op::RemoveChildren => {
            if node.children == 0 {
                return Ok((bytes, None));
            }
            bytes[node.offset as usize + 14..node.offset as usize + 16]
                .copy_from_slice(&0u16.to_be_bytes());
            (node.offset + 20, node.end, Vec::new(), node.offset, None)
        }
        _ => unreachable!(),
    };
    let delta = replacement.len() as i64 - i64::from(end - start);
    let total = bytes.len() as i64 + delta;
    if total < 32 || total > i64::from(i32::MAX) {
        return Err(PPC_MEM_FULL_ERR);
    }
    while ancestor != 0 {
        let atom = atoms
            .iter()
            .find(|atom| atom.offset == ancestor)
            .ok_or(INVALID_ATOM)?;
        let size = u32::try_from(i64::from(atom.end - atom.offset) + delta)
            .map_err(|_| PPC_MEM_FULL_ERR)?;
        bytes[ancestor as usize..ancestor as usize + 4].copy_from_slice(&size.to_be_bytes());
        ancestor = atom.parent;
    }
    bytes.splice(start as usize..end as usize, replacement);
    Ok((bytes, output))
}
