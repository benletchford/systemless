//! Portable writable catalog image. No host I/O, handles, WDs or CPU state.
use super::{Directory, File, Files};
use crate::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};
pub const IMAGE_LIMIT: usize = super::io::LIMIT + 128 * 1024;
const MAGIC: &[u8; 8] = b"SLVOLUME";
fn bad(message: &str) -> Error {
    Error::Volume(message.into())
}
const fn crc_table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut v = i as u32;
        let mut bit = 0;
        while bit < 8 {
            v = (v >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(v & 1));
            bit += 1;
        }
        table[i] = v;
        i += 1;
    }
    table
}
const CRC: [u32; 256] = crc_table();
fn crc(bytes: &[u8]) -> u32 {
    !bytes
        .iter()
        .fold(u32::MAX, |v, b| CRC[((v as u8) ^ b) as usize] ^ (v >> 8))
}
fn word(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_be_bytes());
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| bad("range overflow"))?;
        let bytes = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| bad("truncated catalog image"))?;
        self.at = end;
        Ok(bytes)
    }
    fn word(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn name(&mut self) -> Result<Vec<u8>> {
        let n = self.take(1)?[0] as usize;
        let name = self.take(n)?;
        if n == 0 || n > 31 || name.contains(&0) || name.contains(&b':') {
            return Err(bad("invalid HFS leaf name"));
        }
        Ok(name.to_vec())
    }
}
impl Files {
    /// Synchronous PBFlushVol selection/preflight, IM Files p. 2-143.
    /// The host must checkpoint before the runtime publishes a successful result.
    pub(crate) fn prepare_flush(
        &self,
        memory: &crate::memory::Memory,
        pointer: crate::memory::GuestPtr,
    ) -> Result<i16> {
        use crate::memory::GuestPtr;
        if pointer.0 == 0 {
            return Err(Error::Cpu("nil volume flush parameter block".into()));
        }
        let pb = memory.read(pointer, 24)?;
        memory.validate_write(GuestPtr(pointer.0 + 12), 6)?;
        let name = Self::name_input(
            memory,
            GuestPtr(u32::from_be_bytes(pb[18..22].try_into().unwrap())),
        )?;
        let reference = i16::from_be_bytes(pb[22..24].try_into().unwrap());
        if self.name.is_empty() {
            return Ok(if reference == 0 { -50 } else { -35 });
        }
        if name.contains(&0) {
            return Ok(-37);
        }
        if !name.is_empty() && !name.starts_with(b":") {
            let volume = name.split(|&b| b == b':').next().unwrap();
            if volume.len() > 27 {
                return Ok(-37);
            }
            return Ok(if volume.eq_ignore_ascii_case(b"Application Volume") {
                0
            } else {
                -35
            });
        }
        Ok(
            if matches!(reference, -1..=1) || self.working.contains_key(&reference) {
                0
            } else if reference > 1 {
                -56
            } else {
                -35
            },
        )
    }
    /// Stable, big-endian writable volume image; the loaded application is excluded.
    pub fn export_volume(&self) -> Result<Vec<u8>> {
        if self.name.is_empty() {
            return Err(bad("no application volume"));
        }
        let version = if self.system_folder.is_some() {
            3
        } else if self.files.values().any(|f| f.extended_finder != [0; 16]) {
            2
        } else {
            1
        };
        let mut out = MAGIC.to_vec();
        for v in [
            version,
            self.next_node,
            self.directories.len() as u32,
            self.files.len() as u32,
            0,
            0,
        ] {
            word(&mut out, v);
        }
        if version == 3 {
            word(&mut out, self.system_folder.unwrap());
        }
        for (&id, d) in &self.directories {
            for v in [id, d.parent, d.created] {
                word(&mut out, v);
            }
            out.push(d.name.len() as u8);
            out.extend(&d.name);
        }
        for (&id, f) in &self.files {
            for v in [id, f.parent, f.created, f.modified] {
                word(&mut out, v);
            }
            out.extend(f.finder);
            if version >= 2 {
                out.extend(f.extended_finder);
            }
            word(&mut out, f.data.len() as u32);
            word(&mut out, f.resource.len() as u32);
            out.push(f.name.len() as u8);
            out.extend(&f.name);
            out.extend(&f.data);
            out.extend(&f.resource);
        }
        if out.len() + 4 > IMAGE_LIMIT {
            return Err(bad("catalog image exceeds size limit"));
        }
        let payload = (out.len() - 32) as u32;
        out[24..28].copy_from_slice(&payload.to_be_bytes());
        let check = crc(&out);
        word(&mut out, check);
        Ok(out)
    }
    /// Restore before guest execution into a fresh writable catalog. Application
    /// forks and their loader-owned open reference are preserved, never imported.
    pub fn restore_volume(&mut self, image: &[u8]) -> Result<()> {
        if self.name.is_empty()
            || !self.files.is_empty()
            || !self.directories.is_empty()
            || !self.working.is_empty()
            || self.open.values().any(|f| f.node != 16)
        {
            return Err(bad("restore requires a fresh writable catalog"));
        }
        if image.len() < 36 || image.len() > IMAGE_LIMIT {
            return Err(bad("invalid catalog image size"));
        }
        let end = image.len() - 4;
        let check = u32::from_be_bytes(image[end..].try_into().unwrap());
        if crc(&image[..end]) != check {
            return Err(bad("catalog checksum mismatch"));
        }
        let mut r = Reader {
            bytes: &image[..end],
            at: 0,
        };
        if r.take(8)? != MAGIC {
            return Err(bad("unsupported catalog image format"));
        }
        let version = r.word()?;
        if !matches!(version, 1..=3) {
            return Err(bad("unsupported catalog image format"));
        }
        let next = r.word()?;
        let nd = r.word()? as usize;
        let nf = r.word()? as usize;
        if nd > 1024 || nf > 1024 || nd + nf > 1024 || next < 17 {
            return Err(bad("invalid catalog counts or next ID"));
        }
        if r.word()? as usize != end - 32 || r.word()? != 0 {
            return Err(bad("invalid catalog header"));
        }
        let system_folder = if version == 3 { Some(r.word()?) } else { None };
        let mut dirs = BTreeMap::new();
        let mut files = BTreeMap::new();
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::from([(
            2,
            self.name
                .iter()
                .map(u8::to_ascii_lowercase)
                .collect::<Vec<_>>(),
        )]);
        let mut register = |id: u32, parent: u32, name: &[u8]| -> Result<()> {
            if id < 17
                || id >= next
                || !ids.insert(id)
                || !names.insert((
                    parent,
                    name.iter().map(u8::to_ascii_lowercase).collect::<Vec<_>>(),
                ))
            {
                return Err(bad("duplicate or invalid catalog node"));
            }
            Ok(())
        };
        for _ in 0..nd {
            let id = r.word()?;
            let parent = r.word()?;
            let created = r.word()?;
            let name = r.name()?;
            register(id, parent, &name)?;
            dirs.insert(
                id,
                Directory {
                    name,
                    parent,
                    created,
                },
            );
        }
        let mut used = 0usize;
        for _ in 0..nf {
            let id = r.word()?;
            let parent = r.word()?;
            let created = r.word()?;
            let modified = r.word()?;
            let finder = r.take(16)?.try_into().unwrap();
            let extended_finder = if version >= 2 {
                r.take(16)?.try_into().unwrap()
            } else {
                [0; 16]
            };
            let data_len = r.word()? as usize;
            let resource_len = r.word()? as usize;
            used = used
                .checked_add(data_len)
                .and_then(|v| v.checked_add(resource_len))
                .filter(|v| *v <= super::io::LIMIT)
                .ok_or_else(|| bad("writable forks exceed 64 MiB limit"))?;
            let name = r.name()?;
            register(id, parent, &name)?;
            let data = r.take(data_len)?.to_vec();
            let resource = r.take(resource_len)?.to_vec();
            files.insert(
                id,
                File {
                    name,
                    parent,
                    data,
                    resource,
                    created,
                    modified,
                    finder,
                    extended_finder,
                },
            );
        }
        if r.at != end {
            return Err(bad("trailing catalog data"));
        }
        for parent in dirs
            .values()
            .map(|d| d.parent)
            .chain(files.values().map(|f| f.parent))
        {
            let mut node = parent;
            let mut depth = 0;
            while node != 2 {
                node = dirs
                    .get(&node)
                    .ok_or_else(|| bad("missing catalog parent"))?
                    .parent;
                depth += 1;
                if depth > nd {
                    return Err(bad("cyclic catalog directories"));
                }
            }
        }
        if system_folder.is_some_and(|id| !dirs.contains_key(&id)) {
            return Err(bad("invalid System Folder identity"));
        }
        self.system_folder = system_folder;
        self.directories = dirs;
        self.files = files;
        self.next_node = next;
        self.changed();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flush_selection_validates_names_references_and_guest_outputs_without_catalog_changes() {
        use crate::memory::{GuestPtr, Memory, Permissions};
        let mut files = catalog();
        let wd = files.open_working_directory(-1, 17, 42).unwrap();
        let mut memory = Memory::default();
        memory
            .map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
            .unwrap();
        let pb = GuestPtr(0x1000);
        let before = files.export_volume().unwrap();
        let revision = files.revision();
        for (reference, name, expected) in [
            (0, b"".as_slice(), 0),
            (-1, b"".as_slice(), 0),
            (1, b"".as_slice(), 0),
            (wd, b"".as_slice(), 0),
            (-10, b"".as_slice(), -35),
            (2, b"".as_slice(), -56),
            (2, b"Application Volume:File".as_slice(), 0),
            (0, b"Missing".as_slice(), -35),
            (0, b"Bad\0Name".as_slice(), -37),
            (0, b"abcdefghijklmnopqrstuvwxyz12".as_slice(), -37),
        ] {
            memory
                .write(GuestPtr(0x1012), &0x1100u32.to_be_bytes())
                .unwrap();
            memory
                .write(GuestPtr(0x1016), &i16::to_be_bytes(reference))
                .unwrap();
            let mut string = vec![name.len() as u8];
            string.extend(name);
            memory.write(GuestPtr(0x1100), &string).unwrap();
            let bytes = memory.read(pb, 24).unwrap().to_vec();
            assert_eq!(files.prepare_flush(&memory, pb).unwrap(), expected);
            assert_eq!(memory.read(pb, 24).unwrap(), bytes);
            assert_eq!(files.revision(), revision);
            assert_eq!(files.export_volume().unwrap(), before);
        }
        memory
            .map(GuestPtr(0x2000), vec![0; 24], Permissions::RX)
            .unwrap();
        assert!(files.prepare_flush(&memory, GuestPtr(0x2000)).is_err());
        assert!(files.prepare_flush(&memory, GuestPtr(0x11f0)).is_err());
        assert!(files.prepare_flush(&memory, GuestPtr(0)).is_err());
    }
    fn catalog() -> Files {
        let mut f = Files::application(b"original data", Some(b"original resources"));
        f.directories.insert(
            17,
            Directory {
                name: b"Saves".to_vec(),
                parent: 2,
                created: 123,
            },
        );
        f.files.insert(
            18,
            File {
                name: vec![0x8e, b'G'],
                parent: 17,
                data: b"game".to_vec(),
                resource: b"RSRC".to_vec(),
                created: 124,
                modified: 125,
                finder: *b"GAMEFUTR12345678",
                extended_finder: [0; 16],
            },
        );
        f.next_node = 20;
        f
    }
    fn repair(b: &mut [u8]) {
        let n = b.len() - 4;
        let c = crc(&b[..n]);
        b[n..].copy_from_slice(&c.to_be_bytes());
    }
    #[test]
    fn round_trip_preserves_catalog_ids_forks_dates_finder_bytes_but_not_application_or_access_paths(
    ) {
        assert_eq!(crc(b"123456789"), 0xcbf43926);
        let mut f = catalog();
        let appref = f.open(super::super::Fork::Resource).unwrap();
        f.open_working_directory(-1, 17, 44).unwrap();
        let image = f.export_volume().unwrap();
        assert!(!image.windows(8).any(|b| b == b"original"));
        let mut restored = Files::application(b"different app", None);
        let r = restored.open(super::super::Fork::Data).unwrap();
        restored.restore_volume(&image).unwrap();
        assert_eq!(restored.export_volume().unwrap(), image);
        assert_eq!(
            restored.fork_bytes(super::super::Fork::Data).unwrap(),
            b"different app"
        );
        assert_eq!(restored.open.len(), 1);
        assert!(restored.open.contains_key(&r));
        assert!(restored.working.is_empty());
        assert_eq!(f.open[&appref].node, 16);
        assert_eq!(restored.files[&18].finder, *b"GAMEFUTR12345678");
        assert_eq!(
            (restored.files[&18].created, restored.files[&18].modified),
            (124, 125)
        );
        assert_eq!(restored.directories[&17].created, 123);
    }
    #[test]
    fn malformed_images_fail_transactionally_including_valid_checksum_graph_corruption() {
        let valid = catalog().export_volume().unwrap();
        let empty = Files::application(&[], None).export_volume().unwrap();
        for n in 0..valid.len() {
            let mut f = Files::application(&[], None);
            assert!(f.restore_volume(&valid[..n]).is_err());
            assert_eq!(f.export_volume().unwrap(), empty);
        }
        for (offset, value) in [
            (8, 3u32),
            (12, 18),
            (16, 1025),
            (20, 1025),
            (24, 0),
            (28, 1),
            (32, 16),
            (36, 17),
            (50, 17),
            (54, 99),
            (82, u32::MAX),
            (86, u32::MAX),
        ] {
            let mut image = valid.clone();
            image[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
            repair(&mut image);
            let mut f = Files::application(&[], None);
            assert!(f.restore_volume(&image).is_err(), "offset {offset}");
            assert_eq!(f.export_volume().unwrap(), empty);
        }
        let mut corrupt = valid.clone();
        corrupt[45] ^= 1;
        assert!(Files::application(&[], None)
            .restore_volume(&corrupt)
            .is_err());
        let mut f = catalog();
        let before = f.export_volume().unwrap();
        assert!(f.restore_volume(&valid).is_err());
        assert_eq!(f.export_volume().unwrap(), before);
    }
    #[test]
    fn extended_finder_v2_round_trips_and_v1_defaults_without_partial_imports() {
        let old = catalog().export_volume().unwrap();
        assert_eq!(&old[8..12], &1u32.to_be_bytes());
        let mut f = Files::application(&[], None);
        f.restore_volume(&old).unwrap();
        assert_eq!(f.files[&18].extended_finder, [0; 16]);
        f.files.get_mut(&18).unwrap().extended_finder = *b"12345678\x81abcdefg";
        let image = f.export_volume().unwrap();
        assert_eq!(&image[8..12], &2u32.to_be_bytes());
        assert_eq!(image.len(), old.len() + 16);
        let empty = Files::application(&[], None).export_volume().unwrap();
        for n in 0..image.len() {
            let mut fresh = Files::application(&[], None);
            assert!(fresh.restore_volume(&image[..n]).is_err());
            assert_eq!(fresh.export_volume().unwrap(), empty);
        }
        let mut fresh = Files::application(&[], None);
        fresh.restore_volume(&image).unwrap();
        assert_eq!(fresh.export_volume().unwrap(), image);
        assert_eq!(
            fresh.files[&18].extended_finder,
            f.files[&18].extended_finder
        );
        assert_eq!(fresh.files[&18].finder, f.files[&18].finder);
        let mut invalid = image.clone();
        invalid[8..12].copy_from_slice(&3u32.to_be_bytes());
        repair(&mut invalid);
        let mut fresh = Files::application(&[], None);
        assert!(fresh.restore_volume(&invalid).is_err());
        assert_eq!(fresh.export_volume().unwrap(), empty);
    }
    #[test]
    fn exhausted_imported_ids_report_file_errors_without_wrapping_or_changing_catalog() {
        use crate::memory::{GuestPtr, Memory, Permissions};
        let mut source = Files::application(&[], None);
        source.next_node = u32::MAX;
        let image = source.export_volume().unwrap();
        let mut f = Files::application(&[], None);
        f.restore_volume(&image).unwrap();
        assert_eq!(f.create_file(b"new", -1, 2, 0, [0; 16]), Err(-33));
        let mut m = Memory::default();
        m.map(GuestPtr(0), vec![0; 4096], Permissions::RW).unwrap();
        m.map(GuestPtr(0x2000), vec![0; 128], Permissions::RW)
            .unwrap();
        m.write(GuestPtr(0x2050), b"\x03dir").unwrap();
        m.write(GuestPtr(0x2012), &0x2050u32.to_be_bytes()).unwrap();
        m.write(GuestPtr(0x2016), &(-1i16).to_be_bytes()).unwrap();
        m.write(GuestPtr(0x2030), &2u32.to_be_bytes()).unwrap();
        assert_eq!(f.create_directory(&mut m, GuestPtr(0x2000)).unwrap(), -33);
        assert_eq!(m.u16(GuestPtr(0x2010)).unwrap() as i16, -33);
        assert_eq!(f.export_volume().unwrap(), image);
    }
}

#[cfg(test)]
mod folder_identity_tests {
    use super::*;
    use crate::memory::{Memory, Permissions};
    fn clock() -> Memory {
        let mut m = Memory::default();
        m.map(
            crate::calendar::TIME,
            crate::calendar::DEFAULT_TIME.to_be_bytes().to_vec(),
            Permissions::RW,
        )
        .unwrap();
        m
    }
    #[test]
    fn v2_upgrade_preserves_extended_finder_data_and_protects_blessed_folder() {
        let mut f = Files::application(b"app", None);
        let node = f.create_file(b"Data", -1, 2, 0, [0; 16]).unwrap();
        f.files.get_mut(&node).unwrap().extended_finder = [0x7a; 16];
        let old = f.export_volume().unwrap();
        assert_eq!(u32::from_be_bytes(old[8..12].try_into().unwrap()), 2);
        let mut loaded = Files::application(b"app", None);
        loaded.restore_volume(&old).unwrap();
        let m = clock();
        assert_eq!(loaded.provision_system_folder(&m).unwrap(), 0);
        let system = loaded.system_folder.unwrap();
        assert_eq!(loaded.delete_node(system), -47);
        let image = loaded.export_volume().unwrap();
        let mut again = Files::application(b"app", None);
        again.restore_volume(&image).unwrap();
        assert_eq!(again.files[&node].extended_finder, [0x7a; 16]);
        assert_eq!(again.system_folder, Some(system));
        assert_eq!(again.export_volume().unwrap(), image);
    }
    #[test]
    fn corrupt_blessed_identity_is_rejected_before_restore_publication() {
        let mut f = Files::application(b"app", None);
        f.provision_system_folder(&clock()).unwrap();
        let image = f.export_volume().unwrap();
        for id in [0u32, 2, 16, 999] {
            let mut bad = image.clone();
            bad[32..36].copy_from_slice(&id.to_be_bytes());
            let end = bad.len() - 4;
            let checksum = crc(&bad[..end]);
            bad[end..].copy_from_slice(&checksum.to_be_bytes());
            let mut target = Files::application(b"app", None);
            let before = target.export_volume().unwrap();
            assert!(target.restore_volume(&bad).is_err());
            assert_eq!(target.export_volume().unwrap(), before);
            assert_eq!(target.system_folder, None);
        }
    }
}
