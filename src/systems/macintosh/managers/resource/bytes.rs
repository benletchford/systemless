//! Resource snapshots backed by a retained file fork or decoded bytes.

use std::ops::{Deref, DerefMut, Range};
use std::rc::Rc;

/// A snapshot of resource bytes. Cloning or slicing retains the backing buffer;
/// editing detaches the payload from other resources and the original fork.
#[derive(Clone)]
pub struct ResourceBytes {
    backing: Rc<Vec<u8>>,
    range: Range<usize>,
}

impl ResourceBytes {
    pub(crate) fn from_shared(backing: Rc<Vec<u8>>) -> Self {
        let len = backing.len();
        Self { backing, range: 0..len }
    }

    pub(crate) fn slice(&self, range: Range<usize>) -> Self {
        assert!(range.start <= range.end && range.end <= self.len());
        Self {
            backing: Rc::clone(&self.backing),
            range: self.range.start + range.start..self.range.start + range.end,
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        self
    }
}

impl Default for ResourceBytes {
    fn default() -> Self {
        Vec::new().into()
    }
}

impl From<Vec<u8>> for ResourceBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self::from_shared(Rc::new(bytes))
    }
}

impl Deref for ResourceBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.backing[self.range.clone()]
    }
}

impl AsRef<[u8]> for ResourceBytes {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl std::fmt::Debug for ResourceBytes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A resource's debug output must not dump the entire containing file.
        std::fmt::Debug::fmt(self.as_slice(), formatter)
    }
}

impl DerefMut for ResourceBytes {
    fn deref_mut(&mut self) -> &mut [u8] {
        // Copy only this resource, not the whole file that contains it.
        if self.range != (0..self.backing.len()) {
            *self = self.to_vec().into();
        }
        Rc::make_mut(&mut self.backing).as_mut_slice()
    }
}

impl<T: AsRef<[u8]>> PartialEq<T> for ResourceBytes {
    fn eq(&self, other: &T) -> bool {
        self.as_slice() == other.as_ref()
    }
}

impl Eq for ResourceBytes {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_resource_slice_detaches_only_its_payload() {
        let fork = ResourceBytes::from(vec![0x5a; 4096]);
        let mut resource = fork.slice(100..104);
        let snapshot = resource.clone();
        assert_eq!(format!("{resource:?}"), "[90, 90, 90, 90]");
        resource[1] = 0xff;
        assert_eq!(resource, [0x5a, 0xff, 0x5a, 0x5a]);
        assert_eq!(snapshot, [0x5a; 4]);
        assert_eq!(fork[101], 0x5a);
        assert_eq!(resource.backing.len(), 4);
        let address = resource.as_ptr();
        resource[2] = 0xfe;
        assert_eq!(resource.as_ptr(), address);
    }

    #[test]
    fn editing_whole_payload_preserves_cloned_snapshot() {
        let mut resource = ResourceBytes::from(b"original".to_vec());
        let snapshot = resource.clone();
        resource.copy_from_slice(b"modified");
        assert_eq!(snapshot, b"original");
        assert_eq!(resource, b"modified");
        let mut empty = snapshot.slice(3..3);
        empty.copy_from_slice(&[]);
        assert!(empty.is_empty());
    }
}
