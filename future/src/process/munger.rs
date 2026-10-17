//! Byte-oriented relocatable-handle editing shared by the Macintosh ABIs.
use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    pub handle: u32,
    pub offset: i32,
    pub needle: GuestPtr,
    pub needle_len: i32,
    pub replacement: GuestPtr,
    pub replacement_len: i32,
}
impl Process {
    pub fn munger(&mut self, memory: &mut Memory, request: Request) -> Result<i32> {
        let Request {
            handle,
            offset,
            needle,
            needle_len,
            replacement,
            replacement_len,
        } = request;
        let size = self.handle_size(memory, handle)?;
        if self.mem_error != 0 {
            return Ok(-1);
        }
        let pointer = GuestPtr(memory.u32(GuestPtr(handle))?);
        if pointer.0 == 0 {
            self.mem_error = NIL_HANDLE;
            return Ok(-1);
        }
        if offset < 0 || offset as u32 > size {
            return Ok(-1);
        }
        if (needle.0 != 0 && needle_len < 0) || (replacement.0 != 0 && replacement_len < 0) {
            return Err(Error::Load(
                "unsupported negative Munger string length".into(),
            ));
        }
        let original = memory.read(pointer, size as usize)?.to_vec();
        let mut start = offset as usize;
        let mut removed = if needle_len < 0 {
            original.len() - start
        } else {
            needle_len as usize
        };
        if needle.0 != 0 && needle_len > 0 {
            let target = memory.read(needle, needle_len as usize)?.to_vec();
            // Classic reference policy: only complete matches count. The Apple
            // documentation's partial-tail replacement differs from preserved
            // Systemless; no new native-ROM evidence is claimed here.
            let Some(at) = original[start..]
                .windows(target.len())
                .position(|w| w == target)
            else {
                return Ok(-1);
            };
            start += at;
        }
        removed = removed.min(original.len() - start);
        // Preserve the classic reference's direct-deletion form when both
        // pointers are NIL; a NIL second pointer with a nonnil first is search-only.
        if replacement.0 == 0 && needle.0 != 0 {
            return Ok(start as i32);
        }
        let added = if replacement.0 == 0 {
            0
        } else {
            replacement_len as usize
        };
        let Some(length) = (original.len() - removed).checked_add(added) else {
            self.mem_error = MEM_FULL;
            return Ok(-1);
        };
        if length > self.heap_capacity() as usize || length > i32::MAX as usize {
            self.mem_error = MEM_FULL;
            return Ok(-1);
        }
        if !self.can_resize_handles(memory, &[(handle, length as u32)])? {
            self.mem_error = MEM_FULL;
            return Ok(-1);
        }
        // Snapshot replacement before any resizing; both inputs may alias this
        // handle, its master, or another block affected by future compaction.
        let replacement = if added == 0 {
            Vec::new()
        } else {
            memory.read(replacement, added)?.to_vec()
        };
        let mut edited = Vec::with_capacity(length);
        edited.extend_from_slice(&original[..start]);
        edited.extend_from_slice(&replacement);
        edited.extend_from_slice(&original[start + removed..]);
        self.replace_handles(memory, vec![(handle, edited)])?;
        if self.mem_error != 0 {
            return Ok(-1);
        }
        Ok((start + added) as i32)
    }
}
