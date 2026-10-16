//! Staged relocation of unlocked handle payloads; fixed allocations are barriers.
//! This is the compaction component, not the purging contract of MaxMem.
use super::*;
impl Process {
    pub fn max_memory(&mut self, memory: &mut Memory) -> Result<u32> {
        self.application_limit(memory)?;
        if memory.u32(GuestPtr(0x114))? != self.heap_base() + self.heap_capacity() {
            return Err(Error::Cpu(
                "MaxMem requires the owned eagerly grown application zone".into(),
            ));
        }
        self.purge_and_compact_memory(memory)
    }
    pub fn compact_memory(&mut self, memory: &mut Memory) -> Result<u32> {
        self.compact_heap(memory, false)
    }
    pub fn purge_and_compact_memory(&mut self, memory: &mut Memory) -> Result<u32> {
        self.compact_heap(memory, true)
    }
    fn compact_heap(&mut self, memory: &mut Memory, purge: bool) -> Result<u32> {
        self.ensure_heap(memory)?;
        memory.validate_write(GuestPtr(self.heap_base()), self.heap_capacity() as usize)?;
        let mut movable = Vec::new();
        let mut purged = Vec::new();
        for (&handle, block) in &self.handles {
            if memory.u32(GuestPtr(handle))? != block.data {
                return Err(Error::Cpu(
                    "compaction found a guest-modified master pointer".into(),
                ));
            }
            if block.data != 0 && !block.locked && !self.pixel_pinned(handle) {
                memory.validate_write(GuestPtr(handle), 4)?;
                if purge && block.attributes & 0x40 != 0 && !self.pixel_locks.contains_key(&handle)
                {
                    purged.push((handle, block.data));
                    continue;
                }
                let bytes = memory
                    .read(GuestPtr(block.data), block.size as usize)?
                    .to_vec();
                movable.push((block.data, handle, block.size, bytes));
            }
        }
        movable.sort_by_key(|entry| entry.0);
        let mut staged = self.heap.as_ref().unwrap().clone();
        for &(data, _, _, _) in &movable {
            staged.release(data);
        }
        for &(_, data) in &purged {
            staged.release(data);
        }
        // Slide in original address order within fixed-block intervals. A global
        // first-fit pass could consume a large interval with a smaller block and
        // strand a later allocation, even though the original heap was valid.
        let fixed = staged.allocated.clone();
        let mut cursor = self.heap_base();
        let mut placements = Vec::new();
        for (old, handle, size, bytes) in movable {
            let lower = fixed
                .range(..old)
                .next_back()
                .map_or(self.heap_base(), |(&base, &extent)| base + extent);
            let data = cursor.max(lower);
            if !staged.allocate_at(data, size) {
                return Err(Error::Cpu(
                    "compaction could not preserve a live allocation".into(),
                ));
            }
            cursor = data + staged.allocated[&data];
            memory.validate_write(GuestPtr(data), bytes.len())?;
            placements.push((handle, data, bytes));
        }
        let largest = staged.free.values().copied().max().unwrap_or(0);
        for (handle, data, bytes) in placements {
            memory.write(GuestPtr(data), &bytes)?;
            memory.write(GuestPtr(handle), &data.to_be_bytes())?;
            self.handles.get_mut(&handle).unwrap().data = data;
        }
        for (handle, _) in purged {
            memory.write(GuestPtr(handle), &0u32.to_be_bytes())?;
            let block = self.handles.get_mut(&handle).unwrap();
            block.data = 0;
            block.size = 0;
        }
        self.heap = Some(staged);
        self.mem_error = 0;
        Ok(largest)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compaction_preserves_fixed_storage_and_locked_payloads_and_merges_holes() {
        let mut p = Process::default();
        let mut m = Memory::default();
        let hole = p.new_pointer(&mut m, 64, false).unwrap();
        let a = p.new_handle(&mut m, 32, false).unwrap();
        let ad = m.u32(GuestPtr(a)).unwrap();
        m.write(GuestPtr(ad), &[0x31; 32]).unwrap();
        let fixed = p.new_pointer(&mut m, 16, false).unwrap();
        m.write(GuestPtr(fixed), &[0x7a; 16]).unwrap();
        let b = p.new_handle(&mut m, 64, false).unwrap();
        let bd = m.u32(GuestPtr(b)).unwrap();
        m.write(GuestPtr(bd), &[0x42; 64]).unwrap();
        p.set_handle_lock(&m, b, true).unwrap();
        p.dispose_pointer(hole);
        let total = p.free_memory();
        let largest = p.compact_memory(&mut m).unwrap();
        assert_eq!(p.free_memory(), total);
        assert!(largest <= total);
        assert_eq!(m.u32(GuestPtr(a)).unwrap(), hole);
        assert_eq!(m.read(GuestPtr(hole), 32).unwrap(), [0x31; 32]);
        assert_eq!(m.u32(GuestPtr(b)).unwrap(), bd);
        assert_eq!(m.read(GuestPtr(bd), 64).unwrap(), [0x42; 64]);
        assert_eq!(m.read(GuestPtr(fixed), 16).unwrap(), [0x7a; 16]);
        assert_eq!(
            p.new_pointer(&mut m, largest, false).unwrap(),
            *p.pointers.keys().max().unwrap()
        );
        assert_eq!(p.mem_error, 0);
    }
    #[test]
    fn purging_preserves_master_identity_and_reloads_resource_bytes() {
        let mut p = Process::default();
        let mut m = Memory::default();
        p.resources
            .insert_unique(crate::load::resource::Resource {
                kind: *b"DATA",
                id: 128,
                attributes: 0x20,
                name: None,
                data: vec![1, 2, 3],
            })
            .unwrap();
        let h = p.get_resource(&mut m, *b"DATA", 128).unwrap();
        let data = m.u32(GuestPtr(h)).unwrap();
        m.write(GuestPtr(data), &[9; 3]).unwrap();
        let locked = p.new_handle(&mut m, 32, false).unwrap();
        let locked_data = m.u32(GuestPtr(locked)).unwrap();
        p.set_handle_state(&m, locked, 0xc0).unwrap();
        let free = p.free_memory();
        p.purge_and_compact_memory(&mut m).unwrap();
        assert_eq!(m.u32(GuestPtr(h)).unwrap(), 0);
        assert_eq!(p.free_memory(), free + 4);
        assert_eq!(m.u32(GuestPtr(locked)).unwrap(), locked_data);
        let mut fillers = Vec::new();
        while let Some(size) = p.heap.as_ref().unwrap().free.values().next().copied() {
            let pointer = p.new_pointer(&mut m, size, false).unwrap();
            assert_ne!(pointer, 0);
            fillers.push(pointer);
        }
        assert_eq!(p.get_resource(&mut m, *b"DATA", 128).unwrap(), 0);
        assert_eq!((p.mem_error, p.res_error), (-108, -108));
        assert_eq!(m.u32(GuestPtr(h)).unwrap(), 0);
        assert_eq!(p.free_memory(), 0);
        for pointer in fillers {
            p.dispose_pointer(pointer);
        }
        assert_eq!(p.get_resource(&mut m, *b"DATA", 128).unwrap(), h);
        let restored = m.u32(GuestPtr(h)).unwrap();
        assert_ne!(restored, 0);
        assert_eq!(m.read(GuestPtr(restored), 3).unwrap(), [1, 2, 3]);
        assert_eq!(p.handle_state(&m, h).unwrap(), 0x60);
    }
    #[test]
    fn corrupt_master_faults_before_any_relocation_or_allocator_change() {
        let mut p = Process::default();
        let mut m = Memory::default();
        let a = p.new_handle(&mut m, 32, false).unwrap();
        let b = p.new_handle(&mut m, 32, false).unwrap();
        let ad = m.u32(GuestPtr(a)).unwrap();
        m.write(GuestPtr(ad), &[0x5a; 32]).unwrap();
        m.write(GuestPtr(b), &0x12345678u32.to_be_bytes()).unwrap();
        let free = p.free_memory();
        p.set_handle_attribute(&m, a, false, true).unwrap();
        assert!(p.purge_and_compact_memory(&mut m).is_err());
        assert_eq!(m.u32(GuestPtr(a)).unwrap(), ad);
        assert_eq!(m.read(GuestPtr(ad), 32).unwrap(), [0x5a; 32]);
        assert_eq!(p.free_memory(), free);
    }
}
