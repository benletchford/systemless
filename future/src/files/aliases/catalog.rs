//! Alias identities use the same single mounted catalog as ordinary FSSpecs.
use super::*;
const VOLUME: &[u8] = b"Application Volume";
struct Identity {
    kind: u16,
    parent: u32,
    name: Vec<u8>,
    created: u32,
    finder: [u8; 8],
}
impl Files {
    fn alias_spec_target(
        &self,
        m: &Memory,
        spec: GuestPtr,
    ) -> Result<std::result::Result<u32, i16>> {
        if spec.0 == 0 {
            return Err(Error::Cpu("nil alias FSSpec".into()));
        }
        let bytes = m.read(spec, 70)?;
        if bytes[6] != 0 {
            return self.spec_target(m, spec);
        }
        if self.name.is_empty() {
            return Ok(Err(-35));
        }
        let volume = i16::from_be_bytes(bytes[..2].try_into().unwrap());
        let directory = u32::from_be_bytes(bytes[2..6].try_into().unwrap());
        Ok(self.parent(volume, directory).and_then(|n| {
            if self.directory_exists(n) {
                Ok(n)
            } else {
                Err(-120)
            }
        }))
    }
    fn alias_identity(&self, node: u32) -> Option<Identity> {
        if node == 2 {
            Some(Identity {
                kind: 1,
                parent: 1,
                name: VOLUME.to_vec(),
                created: 0,
                finder: [0; 8],
            })
        } else if let Some(d) = self.directories.get(&node) {
            Some(Identity {
                kind: 1,
                parent: d.parent,
                name: d.name.clone(),
                created: d.created,
                finder: [0; 8],
            })
        } else {
            let f = self.file(node)?;
            Some(Identity {
                kind: 0,
                parent: f.parent,
                name: f.name.to_vec(),
                created: f.created,
                finder: f.finder[..8].try_into().unwrap(),
            })
        }
    }
    fn alias_lineage(&self, mut node: u32) -> std::result::Result<Vec<u32>, i16> {
        let mut out = Vec::new();
        loop {
            if out.len() >= 1024 || out.contains(&node) {
                return Err(-50);
            }
            out.push(node);
            if node == 2 {
                return Ok(out);
            }
            node = self.alias_identity(node).ok_or(-43i16)?.parent;
        }
    }
    pub(super) fn alias_spec(&self, node: u32) -> [u8; 70] {
        let (parent, name) = self.spec_identity(node);
        let mut out = [0; 70];
        out[..2].copy_from_slice(&(-1i16).to_be_bytes());
        out[2..6].copy_from_slice(&parent.to_be_bytes());
        out[6] = name.len() as u8;
        out[7..7 + name.len()].copy_from_slice(&name);
        out
    }
    pub(super) fn alias_record(
        &self,
        m: &Memory,
        from: GuestPtr,
        target: GuestPtr,
    ) -> Result<std::result::Result<Record, i16>> {
        let node = match self.alias_spec_target(m, target)? {
            Ok(n) => n,
            Err(e) => return Ok(Err(e)),
        };
        self.alias_record_for_node(m, from, node)
    }
    pub(super) fn alias_record_for_node(
        &self,
        m: &Memory,
        from: GuestPtr,
        node: u32,
    ) -> Result<std::result::Result<Record, i16>> {
        let make = || -> std::result::Result<Record, i16> {
            let target = self.alias_identity(node).ok_or(-43i16)?;
            let lineage = self.alias_lineage(node)?;
            let mut r = Record::fresh();
            r.header[8..10].copy_from_slice(&target.kind.to_be_bytes());
            r.header[10] = VOLUME.len() as u8;
            r.header[11..11 + VOLUME.len()].copy_from_slice(VOLUME);
            r.header[46..50].copy_from_slice(&target.parent.to_be_bytes());
            r.header[50] = target.name.len() as u8;
            r.header[51..51 + target.name.len()].copy_from_slice(&target.name);
            r.header[114..118].copy_from_slice(&node.to_be_bytes());
            r.header[118..122].copy_from_slice(&target.created.to_be_bytes());
            r.header[122..130].copy_from_slice(&target.finder);
            r.parent_name = Some(if target.parent == 1 {
                VOLUME.to_vec()
            } else {
                self.alias_identity(target.parent).ok_or(-120i16)?.name
            });
            let mut path = VOLUME.to_vec();
            path.push(b':');
            for (i, &part) in lineage.iter().rev().skip(1).enumerate() {
                if i != 0 {
                    path.push(b':');
                }
                path.extend(self.alias_identity(part).ok_or(-43i16)?.name);
            }
            if target.kind == 1 && node != 2 {
                path.push(b':');
            }
            r.path = Some(path);
            Ok(r)
        };
        let mut r = match make() {
            Ok(r) => r,
            Err(e) => return Ok(Err(e)),
        };
        if from.0 != 0 {
            let start = match self.alias_spec_target(m, from)? {
                Ok(n) => n,
                Err(e) => return Ok(Err(e)),
            };
            let from_chain = match self.alias_lineage(start) {
                Ok(v) => v,
                Err(e) => return Ok(Err(e)),
            };
            let to_chain = match self.alias_lineage(node) {
                Ok(v) => v,
                Err(e) => return Ok(Err(e)),
            };
            let (a, b) = from_chain
                .iter()
                .enumerate()
                .find_map(|(i, n)| to_chain.iter().position(|v| v == n).map(|j| (i, j)))
                .ok_or_else(|| Error::Cpu("catalog nodes have no shared volume root".into()))?;
            r.header[130..132].copy_from_slice(&(a as i16).to_be_bytes());
            r.header[132..134].copy_from_slice(&(b as i16).to_be_bytes());
        }
        Ok(Ok(r))
    }
    fn alias_matches(&self, node: u32, r: &Record) -> bool {
        self.alias_identity(node)
            .is_some_and(|v| v.kind == r.word(8) && v.created == r.long(118))
    }
    pub(super) fn alias_target(
        &self,
        m: &Memory,
        r: &Record,
        from: GuestPtr,
    ) -> Result<std::result::Result<u32, i16>> {
        let start = if from.0 != 0 && (r.word(130) as i16) >= 0 && (r.word(132) as i16) >= 0 {
            match self.alias_spec_target(m, from)? {
                Ok(n) => Some(n),
                Err(e) => return Ok(Err(e)),
            }
        } else {
            None
        };
        Ok(self.alias_target_node(r, start))
    }
    pub(super) fn alias_target_node(
        &self,
        r: &Record,
        start: Option<u32>,
    ) -> std::result::Result<u32, i16> {
        if let Some(start) =
            start.filter(|_| (r.word(130) as i16) >= 0 && (r.word(132) as i16) >= 0)
        {
            let chain = self.alias_lineage(start)?;
            let up = usize::from(r.word(130));
            let down = usize::from(r.word(132));
            if let Some(&base) = chain.get(up) {
                if down == 0 && self.alias_matches(base, r) {
                    return Ok(base);
                }
                if let Some(path) = &r.path {
                    let parts: Vec<_> = path
                        .split(|&v| v == b':')
                        .filter(|v| !v.is_empty())
                        .collect();
                    if down < parts.len() && down != 0 {
                        let mut relative = vec![b':'];
                        for (i, part) in parts[parts.len() - down..].iter().enumerate() {
                            if i != 0 {
                                relative.push(b':');
                            }
                            relative.extend(*part);
                        }
                        if let Ok(node) = self.path(&relative, base) {
                            if self.alias_matches(node, r) {
                                return Ok(node);
                            }
                        }
                    }
                }
            }
        }
        if !r.string(10).eq_ignore_ascii_case(VOLUME) || r.long(38) != 0 || r.word(42) != 0x4244 {
            return Err(-35);
        }
        let node = r.long(114);
        if self.alias_matches(node, r) {
            return Ok(node);
        }
        if let Ok(node) = self.path(r.string(50), r.long(46)) {
            if self.alias_matches(node, r) {
                return Ok(node);
            }
        }
        if let Some(path) = &r.path {
            if let Ok(node) = self.path(path, 2) {
                if self.alias_matches(node, r) {
                    return Ok(node);
                }
            }
        }
        Err(-43)
    }
    pub(super) fn alias_missing_spec(&self, r: &Record) -> std::result::Result<[u8; 70], i16> {
        let mut parent = r.long(46);
        if !self.directory_exists(parent) {
            let path = r.path.as_ref().ok_or(-120i16)?;
            let path = path.strip_suffix(b":").unwrap_or(path);
            let split = path.iter().rposition(|&v| v == b':').ok_or(-120i16)?;
            parent = self.path(&path[..=split], 2).map_err(|_| -120i16)?;
            if !self.directory_exists(parent) {
                return Err(-120);
            }
        }
        let name = r.string(50);
        if name.is_empty() || name.len() > 31 || name.contains(&0) || name.contains(&b':') {
            return Err(-50);
        }
        let mut spec = [0; 70];
        spec[..2].copy_from_slice(&(-1i16).to_be_bytes());
        spec[2..6].copy_from_slice(&parent.to_be_bytes());
        spec[6] = name.len() as u8;
        spec[7..7 + name.len()].copy_from_slice(name);
        Ok(spec)
    }
}
