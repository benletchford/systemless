//! Checked classic resource-fork serialization, shared by resource-file writes.
use super::{invalid, limit, ResourceFork};
use crate::Result;
impl ResourceFork {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut groups = std::collections::BTreeMap::<[u8; 4], Vec<_>>::new();
        for r in self.entries.values() {
            groups.entry(r.kind).or_default().push(r);
        }
        if groups.len() > 65535 {
            return Err(invalid("resource type count exceeds on-disk limit"));
        }
        let types = 28usize;
        let refs = types + 2 + groups.len() * 8;
        let mut map = vec![0u8; refs];
        map[22..24].copy_from_slice(&self.attributes.to_be_bytes());
        map[24..26].copy_from_slice(&(types as u16).to_be_bytes());
        map[types..types + 2].copy_from_slice(&(groups.len() as u16).wrapping_sub(1).to_be_bytes());
        let mut data = Vec::new();
        let mut names = Vec::new();
        for (i, (kind, entries)) in groups.into_iter().enumerate() {
            if entries.len() > 65535 {
                return Err(invalid("resource reference count exceeds on-disk limit"));
            }
            let t = types + 2 + i * 8;
            map[t..t + 4].copy_from_slice(&kind);
            map[t + 4..t + 6].copy_from_slice(&(entries.len() as u16 - 1).to_be_bytes());
            let relative = u16::try_from(map.len() - types)
                .map_err(|_| invalid("resource reference offset overflow"))?;
            map[t + 6..t + 8].copy_from_slice(&relative.to_be_bytes());
            for r in entries {
                if data.len() > 0xffffff {
                    return Err(invalid("resource data offset exceeds 24 bits"));
                }
                let name = if let Some(name) = &r.name {
                    if name.len() > 255 {
                        return Err(invalid("resource name exceeds Pascal string"));
                    }
                    let at = u16::try_from(names.len())
                        .map_err(|_| invalid("resource name offset overflow"))?;
                    if at == 0xffff {
                        return Err(invalid("resource name offset uses unnamed sentinel"));
                    }
                    names.push(name.len() as u8);
                    names.extend(name);
                    at
                } else {
                    0xffff
                };
                let length = u32::try_from(r.data.len())
                    .map_err(|_| invalid("resource data length overflow"))?;
                let offset = ((r.attributes as u32) << 24) | data.len() as u32;
                map.extend(r.id.to_be_bytes());
                map.extend(name.to_be_bytes());
                map.extend(offset.to_be_bytes());
                map.extend([0; 4]);
                limit(data.len() + 4 + r.data.len())?;
                data.extend(length.to_be_bytes());
                data.extend_from_slice(&r.data);
                if data.len() % 2 != 0 {
                    data.push(0);
                }
            }
        }
        let names_at =
            u16::try_from(map.len()).map_err(|_| invalid("resource name-list offset overflow"))?;
        map[26..28].copy_from_slice(&names_at.to_be_bytes());
        map.extend(names);
        let mut output = vec![0; 256];
        let header = [
            256u32,
            256 + data.len() as u32,
            data.len() as u32,
            map.len() as u32,
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect::<Vec<_>>();
        output[..16].copy_from_slice(&header);
        map[..16].copy_from_slice(&header);
        limit(256 + data.len() + map.len())?;
        output.extend(data);
        output.extend(map);
        Ok(output)
    }
}
