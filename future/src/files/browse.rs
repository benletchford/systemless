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
    pub fn catalog_fork(&self, node: u32, kind: super::Fork) -> Option<&[u8]> {
        self.node_fork(node, kind)
    }
}
