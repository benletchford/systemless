//! WriteResource stages one payload; UpdateResFile publishes the live map.
//! Inside Macintosh: More Macintosh Toolbox, Resource Manager, WriteResource.
use super::*;
impl Process {
    pub fn write_resource(&mut self, memory: &Memory, handle: u32) -> Result<()> {
        memory.validate_write(GuestPtr(0xa60), 2)?;
        self.res_error = 0;
        let Some((&(owner, kind, id), _)) =
            self.resource_handles.iter().find(|(_, h)| **h == handle)
        else {
            self.res_error = -192;
            return Ok(());
        };
        let resource = self
            .resource_metadata(owner, kind, id)
            .ok_or_else(|| Error::Load("resource handle has no map entry".into()))?;
        if resource.attributes & 2 == 0 || resource.attributes & 8 != 0 {
            return Ok(());
        }
        let Some(map) = self.resource_files.files.get(&owner) else {
            self.res_error = -54;
            return Ok(());
        };
        if !map.writable || map.fork.attributes & 0x80 != 0 {
            self.res_error = -54;
            return Ok(());
        }
        let block = self
            .handles
            .get(&handle)
            .ok_or_else(|| Error::Load("resource handle was disposed".into()))?;
        if memory.u32(GuestPtr(handle))? != block.data {
            return Err(Error::Cpu("guest-modified resource master pointer".into()));
        }
        let data = if block.data == 0 || block.size == 0 {
            Vec::new()
        } else {
            memory
                .read(GuestPtr(block.data), block.size as usize)?
                .to_vec()
        };
        let mut candidate = map.fork.clone();
        let target = candidate
            .entries_mut()
            .find(|r| r.kind == kind && r.id == id)
            .unwrap();
        target.data = data.clone();
        target.attributes &= !2;
        // Resource information remains dirty until UpdateResFile/CloseResFile.
        candidate.attributes |= 0x20;
        let live_size = candidate.encode()?.len();
        let bytes = stage_payload(
            self.files
                .resource_bytes(map.node)
                .ok_or_else(|| Error::Load("resource fork disappeared".into()))?,
            &data,
        )?;
        let size = live_size.max(bytes.len());
        if self
            .resource_files
            .files
            .values()
            .map(|m| m.bytes)
            .sum::<usize>()
            - map.bytes
            + size
            > crate::load::IMAGE_LIMIT
        {
            self.res_error = -108;
            return Ok(());
        }
        let reference = self.owner_reference(owner);
        let modified = memory.u32(crate::calendar::TIME)?;
        let error = self
            .files
            .replace_resource_file_reserved(reference, bytes, modified, live_size)?;
        if error == 0 {
            let map = self.resource_files.files.get_mut(&owner).unwrap();
            map.fork = candidate;
            map.bytes = size;
        }
        self.res_error = error;
        Ok(())
    }
}
/// Append a length-prefixed payload inside the data area while retaining every
/// on-disk resource reference. Only the structural header/map location changes;
/// the new reference is published later by the normal map update. Repeated writes
/// leave reclaimable data, as normal non-compacting Resource Manager writes do.
fn stage_payload(old: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    ResourceFork::parse(old)?;
    let number = |at| u32::from_be_bytes(old[at..at + 4].try_into().unwrap()) as usize;
    let start = number(0);
    let map = number(4);
    let length = number(8);
    let end = start + length;
    let mut payload = (data.len() as u32).to_be_bytes().to_vec();
    payload.extend(data);
    if !payload.len().is_multiple_of(2) {
        payload.push(0);
    }
    let total = old
        .len()
        .checked_add(payload.len())
        .filter(|&n| n <= crate::load::IMAGE_LIMIT)
        .ok_or_else(|| Error::Load("resource staging exceeds image limit".into()))?;
    let mut bytes = Vec::with_capacity(total);
    bytes.extend(&old[..end]);
    bytes.extend(&payload);
    bytes.extend(&old[end..]);
    let map = if map >= end { map + payload.len() } else { map };
    bytes[4..8].copy_from_slice(&(map as u32).to_be_bytes());
    bytes[8..12].copy_from_slice(&((length + payload.len()) as u32).to_be_bytes());
    let header: [u8; 16] = bytes[..16].try_into().unwrap();
    bytes[map..map + 16].copy_from_slice(&header);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staged_bytes_keep_disk_references_and_metadata_until_map_update() {
        let mut fork = ResourceFork::default();
        fork.insert_unique(Resource {
            kind: *b"DATA",
            id: 1,
            attributes: 0,
            name: Some(b"original".to_vec()),
            data: vec![1, 2],
        })
        .unwrap();
        let old = fork.encode().unwrap();
        let staged = stage_payload(&old, &[9, 8, 7]).unwrap();
        let read = ResourceFork::parse(&staged).unwrap();
        assert_eq!(read.get(*b"DATA", 1), fork.get(*b"DATA", 1));
        let offset = u32::from_be_bytes(old[0..4].try_into().unwrap()) as usize
            + u32::from_be_bytes(old[8..12].try_into().unwrap()) as usize;
        assert_eq!(&staged[offset..offset + 8], &[0, 0, 0, 3, 9, 8, 7, 0]);
        let old_map = u32::from_be_bytes(old[4..8].try_into().unwrap()) as usize;
        let new_map = u32::from_be_bytes(staged[4..8].try_into().unwrap()) as usize;
        assert_eq!(&staged[new_map + 16..], &old[old_map + 16..]);
        let again = stage_payload(&staged, &[]).unwrap();
        assert_eq!(again.len(), staged.len() + 4);
        assert_eq!(
            ResourceFork::parse(&again).unwrap().get(*b"DATA", 1),
            fork.get(*b"DATA", 1)
        );
        assert!(stage_payload(&[0; 16], &[1]).is_err());
    }
}
