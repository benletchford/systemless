//! Read-only catalog views used by the shared Standard File UI and host diagnostics.
use super::Files;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: u32,
    pub name: Vec<u8>,
    pub directory: bool,
    pub locked: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryView {
    pub id: u32,
    pub parent: u32,
    pub name: Vec<u8>,
    pub entries: Vec<CatalogEntry>,
}
impl Files {
    pub fn directory_view(&self, directory: u32) -> std::result::Result<DirectoryView, i16> {
        if self.name.is_empty() {
            return Err(-43);
        }
        let (parent, name) = if directory == 2 {
            (1, b"Application Volume".to_vec())
        } else {
            let d = self.directories.get(&directory).ok_or(-120i16)?;
            (d.parent, d.name.clone())
        };
        let mut entries: Vec<_> = self
            .children(directory, false)
            .into_iter()
            .map(|id| {
                if let Some(d) = self.directories.get(&id) {
                    CatalogEntry {
                        id,
                        name: d.name.clone(),
                        directory: true,
                        locked: false,
                    }
                } else {
                    let f = self.file(id).unwrap();
                    CatalogEntry {
                        id,
                        name: f.name.to_vec(),
                        directory: false,
                        locked: f.locked,
                    }
                }
            })
            .collect();
        entries.sort_by(|a, b| {
            a.name
                .iter()
                .map(u8::to_ascii_lowercase)
                .cmp(b.name.iter().map(u8::to_ascii_lowercase))
                .then(a.id.cmp(&b.id))
        });
        Ok(DirectoryView {
            id: directory,
            parent,
            name,
            entries,
        })
    }
    pub(crate) fn file_type(&self, node: u32) -> Option<[u8; 4]> {
        Some(self.file(node)?.finder[..4].try_into().unwrap())
    }
    /// Catalog parameter block (classic FileParam prefix), using the same metadata projection as PBGetFInfo.
    pub(crate) fn filter_info(
        &self,
        m: &mut crate::memory::Memory,
        node: u32,
        pb: crate::memory::GuestPtr,
        name: crate::memory::GuestPtr,
    ) -> crate::Result<()> {
        let file = self
            .file(node)
            .ok_or_else(|| crate::Error::Cpu("Standard File filter target disappeared".into()))?;
        m.validate_write(pb, 108)?;
        m.validate_write(name, file.name.len() + 1)?;
        let mut bytes = [0; 108];
        bytes[18..22].copy_from_slice(&name.0.to_be_bytes());
        bytes[22..24].copy_from_slice(&(-1i16).to_be_bytes());
        self.fill_file_info(&mut bytes, node);
        bytes[100..104].copy_from_slice(&file.parent.to_be_bytes());
        bytes[104..108].copy_from_slice(&4096u32.to_be_bytes());
        let mut pascal = vec![file.name.len() as u8];
        pascal.extend(file.name);
        m.write(name, &pascal)?;
        m.write(pb, &bytes)
    }
    pub fn catalog_fork(&self, node: u32, kind: super::Fork) -> Option<&[u8]> {
        self.node_fork(node, kind)
    }
}
