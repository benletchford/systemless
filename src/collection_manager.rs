//! Process-owned Collection Manager state shared by the 68K and PowerPC ABIs.
//!
//! A Collection is opaque to applications.  The guest-visible value is only a
//! stable token; all ownership, ordering, attributes, and item bytes live in
//! this process service.

use std::collections::BTreeMap;

pub(crate) const COLLECTION_ITEM_LOCKED_ERR: i16 = -5750;
pub(crate) const COLLECTION_ITEM_NOT_FOUND_ERR: i16 = -5751;
pub(crate) const COLLECTION_INDEX_RANGE_ERR: i16 = -5752;
pub(crate) const COLLECTION_VERSION_ERR: i16 = -5753;

pub(crate) const COLLECTION_PERSISTENCE_MASK: u32 = 1 << 30;
pub(crate) const COLLECTION_LOCK_MASK: u32 = 1 << 31;
pub(crate) const DEFAULT_COLLECTION_ATTRIBUTES: u32 = COLLECTION_PERSISTENCE_MASK;

const COLLECTION_TOKEN_BASE: u32 = 0xC011_0000;
const FLATTEN_MAGIC: u32 = u32::from_be_bytes(*b"cltn");
const FLATTEN_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CollectionItem {
    pub(crate) tag: u32,
    pub(crate) id: i32,
    pub(crate) attributes: u32,
    pub(crate) data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CollectionRecord {
    pub(crate) owners: u32,
    pub(crate) default_attributes: u32,
    pub(crate) exception_proc: u32,
    /// Items are kept in Collection Manager key order: tag, then signed ID.
    /// This gives stable one-based tag positions and zero-based collection
    /// indexes while preserving the documented possibility that either can
    /// change when a new key is inserted.
    pub(crate) items: Vec<CollectionItem>,
}

impl Default for CollectionRecord {
    fn default() -> Self {
        Self {
            owners: 1,
            default_attributes: DEFAULT_COLLECTION_ATTRIBUTES,
            exception_proc: 0,
            items: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProcessCollectionManagerState {
    next_token: u32,
    collections: BTreeMap<u32, CollectionRecord>,
}

impl Default for ProcessCollectionManagerState {
    fn default() -> Self {
        Self {
            next_token: COLLECTION_TOKEN_BASE,
            collections: BTreeMap::new(),
        }
    }
}

impl ProcessCollectionManagerState {
    pub(crate) fn is_pristine(&self) -> bool {
        self.collections.is_empty() && self.next_token == COLLECTION_TOKEN_BASE
    }

    pub(crate) fn new_collection(&mut self) -> u32 {
        let token = self.next_token;
        self.next_token = self.next_token.wrapping_add(4).max(COLLECTION_TOKEN_BASE);
        self.collections.insert(token, CollectionRecord::default());
        token
    }

    pub(crate) fn dispose(&mut self, collection: u32) {
        let remove = self.collections.get_mut(&collection).is_some_and(|record| {
            record.owners = record.owners.saturating_sub(1);
            record.owners == 0
        });
        if remove {
            self.collections.remove(&collection);
        }
    }

    pub(crate) fn clone_reference(&mut self, collection: u32) -> u32 {
        let Some(record) = self.collections.get_mut(&collection) else {
            return 0;
        };
        record.owners = record.owners.saturating_add(1);
        collection
    }

    pub(crate) fn owner_count(&self, collection: u32) -> i32 {
        self.collections
            .get(&collection)
            .map_or(0, |record| record.owners.min(i32::MAX as u32) as i32)
    }

    pub(crate) fn copy_collection(&mut self, source: u32, target: u32) -> u32 {
        let Some(source_record) = self.collections.get(&source).cloned() else {
            return 0;
        };
        let copy = CollectionRecord {
            owners: 1,
            ..source_record
        };
        if target == 0 {
            let token = self.new_collection();
            self.collections.insert(token, copy);
            token
        } else if let Some(target_record) = self.collections.get_mut(&target) {
            *target_record = CollectionRecord {
                owners: target_record.owners,
                ..copy
            };
            target
        } else {
            0
        }
    }

    pub(crate) fn default_attributes(&self, collection: u32) -> u32 {
        self.collections
            .get(&collection)
            .map_or(0, |record| record.default_attributes)
    }

    pub(crate) fn set_default_attributes(&mut self, collection: u32, which: u32, values: u32) {
        if let Some(record) = self.collections.get_mut(&collection) {
            record.default_attributes = (record.default_attributes & !which) | (values & which);
        }
    }

    pub(crate) fn item_count(&self, collection: u32) -> i32 {
        self.collections
            .get(&collection)
            .map_or(0, |record| record.items.len().min(i32::MAX as usize) as i32)
    }

    fn key_index(items: &[CollectionItem], tag: u32, id: i32) -> Result<usize, usize> {
        items.binary_search_by(|item| (item.tag, item.id).cmp(&(tag, id)))
    }

    pub(crate) fn add_item(&mut self, collection: u32, tag: u32, id: i32, data: Vec<u8>) -> i16 {
        let Some(record) = self.collections.get_mut(&collection) else {
            return COLLECTION_ITEM_NOT_FOUND_ERR;
        };
        match Self::key_index(&record.items, tag, id) {
            Ok(index) if record.items[index].attributes & COLLECTION_LOCK_MASK != 0 => {
                COLLECTION_ITEM_LOCKED_ERR
            }
            Ok(index) => {
                record.items[index] = CollectionItem {
                    tag,
                    id,
                    attributes: record.default_attributes,
                    data,
                };
                0
            }
            Err(index) => {
                record.items.insert(
                    index,
                    CollectionItem {
                        tag,
                        id,
                        attributes: record.default_attributes,
                        data,
                    },
                );
                0
            }
        }
    }

    pub(crate) fn item_by_key(
        &self,
        collection: u32,
        tag: u32,
        id: i32,
    ) -> Option<(usize, &CollectionItem)> {
        let record = self.collections.get(&collection)?;
        let index = Self::key_index(&record.items, tag, id).ok()?;
        Some((index, &record.items[index]))
    }

    pub(crate) fn item_by_index(&self, collection: u32, index: i32) -> Option<&CollectionItem> {
        let index = usize::try_from(index).ok()?;
        self.collections.get(&collection)?.items.get(index)
    }

    pub(crate) fn tagged_item(
        &self,
        collection: u32,
        tag: u32,
        position: i32,
    ) -> Option<(usize, &CollectionItem)> {
        let wanted = usize::try_from(position.checked_sub(1)?).ok()?;
        self.collections
            .get(&collection)?
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.tag == tag)
            .nth(wanted)
    }

    pub(crate) fn replace_indexed(&mut self, collection: u32, index: i32, data: Vec<u8>) -> i16 {
        let Some(item) = usize::try_from(index)
            .ok()
            .and_then(|index| self.collections.get_mut(&collection)?.items.get_mut(index))
        else {
            return COLLECTION_INDEX_RANGE_ERR;
        };
        if item.attributes & COLLECTION_LOCK_MASK != 0 {
            COLLECTION_ITEM_LOCKED_ERR
        } else {
            item.data = data;
            0
        }
    }

    pub(crate) fn remove_key(&mut self, collection: u32, tag: u32, id: i32) -> i16 {
        let Some(record) = self.collections.get_mut(&collection) else {
            return COLLECTION_ITEM_NOT_FOUND_ERR;
        };
        let Ok(index) = Self::key_index(&record.items, tag, id) else {
            return COLLECTION_ITEM_NOT_FOUND_ERR;
        };
        if record.items[index].attributes & COLLECTION_LOCK_MASK != 0 {
            COLLECTION_ITEM_LOCKED_ERR
        } else {
            record.items.remove(index);
            0
        }
    }

    pub(crate) fn remove_indexed(&mut self, collection: u32, index: i32) -> i16 {
        let Ok(index) = usize::try_from(index) else {
            return COLLECTION_INDEX_RANGE_ERR;
        };
        let Some(record) = self.collections.get_mut(&collection) else {
            return COLLECTION_INDEX_RANGE_ERR;
        };
        let Some(item) = record.items.get(index) else {
            return COLLECTION_INDEX_RANGE_ERR;
        };
        if item.attributes & COLLECTION_LOCK_MASK != 0 {
            COLLECTION_ITEM_LOCKED_ERR
        } else {
            record.items.remove(index);
            0
        }
    }

    pub(crate) fn set_key_attributes(
        &mut self,
        collection: u32,
        tag: u32,
        id: i32,
        which: u32,
        values: u32,
    ) -> i16 {
        let Some(record) = self.collections.get_mut(&collection) else {
            return COLLECTION_ITEM_NOT_FOUND_ERR;
        };
        let Ok(index) = Self::key_index(&record.items, tag, id) else {
            return COLLECTION_ITEM_NOT_FOUND_ERR;
        };
        let item = &mut record.items[index];
        item.attributes = (item.attributes & !which) | (values & which);
        0
    }

    pub(crate) fn set_indexed_attributes(
        &mut self,
        collection: u32,
        index: i32,
        which: u32,
        values: u32,
    ) -> i16 {
        let Some(item) = usize::try_from(index)
            .ok()
            .and_then(|index| self.collections.get_mut(&collection)?.items.get_mut(index))
        else {
            return COLLECTION_INDEX_RANGE_ERR;
        };
        item.attributes = (item.attributes & !which) | (values & which);
        0
    }

    pub(crate) fn tag_exists(&self, collection: u32, tag: u32) -> bool {
        self.collections
            .get(&collection)
            .is_some_and(|record| record.items.iter().any(|item| item.tag == tag))
    }

    pub(crate) fn tags(&self, collection: u32) -> Vec<u32> {
        let Some(record) = self.collections.get(&collection) else {
            return Vec::new();
        };
        let mut tags = Vec::new();
        for item in &record.items {
            if tags.last().copied() != Some(item.tag) {
                tags.push(item.tag);
            }
        }
        tags
    }

    pub(crate) fn tagged_count(&self, collection: u32, tag: u32) -> i32 {
        self.collections.get(&collection).map_or(0, |record| {
            record
                .items
                .iter()
                .filter(|item| item.tag == tag)
                .count()
                .min(i32::MAX as usize) as i32
        })
    }

    pub(crate) fn purge_matching(&mut self, collection: u32, which: u32, matching: u32) {
        if let Some(record) = self.collections.get_mut(&collection) {
            record.items.retain(|item| {
                item.attributes & COLLECTION_LOCK_MASK != 0
                    || (item.attributes & which) != (matching & which)
            });
        }
    }

    pub(crate) fn purge_tag(&mut self, collection: u32, tag: u32) {
        if let Some(record) = self.collections.get_mut(&collection) {
            record
                .items
                .retain(|item| item.tag != tag || item.attributes & COLLECTION_LOCK_MASK != 0);
        }
    }

    pub(crate) fn empty(&mut self, collection: u32) {
        if let Some(record) = self.collections.get_mut(&collection) {
            record
                .items
                .retain(|item| item.attributes & COLLECTION_LOCK_MASK != 0);
        }
    }

    pub(crate) fn exception_proc(&self, collection: u32) -> u32 {
        self.collections
            .get(&collection)
            .map_or(0, |record| record.exception_proc)
    }

    pub(crate) fn set_exception_proc(&mut self, collection: u32, proc: u32) {
        if let Some(record) = self.collections.get_mut(&collection) {
            record.exception_proc = proc;
        }
    }

    /// Encode a portable, big-endian Collection Manager stream. The stream is
    /// self-describing and deliberately distinct from a `'cltn'` resource.
    pub(crate) fn flatten(&self, collection: u32, filter: Option<(u32, u32)>) -> Option<Vec<u8>> {
        let record = self.collections.get(&collection)?;
        let items: Vec<_> = record
            .items
            .iter()
            .filter(|item| item.attributes & COLLECTION_PERSISTENCE_MASK != 0)
            .filter(|item| {
                filter.is_none_or(|(which, matching)| item.attributes & which == matching & which)
            })
            .collect();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&FLATTEN_MAGIC.to_be_bytes());
        bytes.extend_from_slice(&FLATTEN_VERSION.to_be_bytes());
        bytes.extend_from_slice(&(items.len() as u32).to_be_bytes());
        for item in items {
            bytes.extend_from_slice(&item.tag.to_be_bytes());
            bytes.extend_from_slice(&item.id.to_be_bytes());
            bytes.extend_from_slice(&item.attributes.to_be_bytes());
            bytes.extend_from_slice(&(item.data.len() as u32).to_be_bytes());
            bytes.extend_from_slice(&item.data);
        }
        Some(bytes)
    }

    pub(crate) fn unflatten(&mut self, collection: u32, bytes: &[u8]) -> i16 {
        let Some(record) = self.collections.get_mut(&collection) else {
            return COLLECTION_VERSION_ERR;
        };
        let Some((magic, rest)) = read_u32(bytes) else {
            return COLLECTION_VERSION_ERR;
        };
        let Some((version, rest)) = read_u32(rest) else {
            return COLLECTION_VERSION_ERR;
        };
        let Some((count, mut rest)) = read_u32(rest) else {
            return COLLECTION_VERSION_ERR;
        };
        if magic != FLATTEN_MAGIC || version != FLATTEN_VERSION {
            return COLLECTION_VERSION_ERR;
        }
        let mut additions = Vec::new();
        for _ in 0..count {
            let Some((tag, next)) = read_u32(rest) else {
                return COLLECTION_VERSION_ERR;
            };
            let Some((id, next)) = read_u32(next) else {
                return COLLECTION_VERSION_ERR;
            };
            let Some((attributes, next)) = read_u32(next) else {
                return COLLECTION_VERSION_ERR;
            };
            let Some((size, next)) = read_u32(next) else {
                return COLLECTION_VERSION_ERR;
            };
            let Ok(size) = usize::try_from(size) else {
                return COLLECTION_VERSION_ERR;
            };
            let Some((data, next)) = next.split_at_checked(size) else {
                return COLLECTION_VERSION_ERR;
            };
            additions.push(CollectionItem {
                tag,
                id: id as i32,
                attributes,
                data: data.to_vec(),
            });
            rest = next;
        }
        if !rest.is_empty() {
            return COLLECTION_VERSION_ERR;
        }
        for item in additions {
            match Self::key_index(&record.items, item.tag, item.id) {
                Ok(index) if record.items[index].attributes & COLLECTION_LOCK_MASK != 0 => {
                    return COLLECTION_ITEM_LOCKED_ERR;
                }
                Ok(index) => record.items[index] = item,
                Err(index) => record.items.insert(index, item),
            }
        }
        0
    }

    /// Decode the documented `'cltn'` resource format: count followed by
    /// tag, ID, attributes, a 16-bit byte count, data, and word alignment.
    pub(crate) fn from_resource(&mut self, bytes: &[u8]) -> Option<u32> {
        let (count, mut rest) = read_u32(bytes)?;
        let mut items = Vec::new();
        for _ in 0..count {
            let (tag, next) = read_u32(rest)?;
            let (id, next) = read_u32(next)?;
            let (attributes, next) = read_u32(next)?;
            if next.len() < 2 {
                return None;
            }
            let size = usize::from(u16::from_be_bytes([next[0], next[1]]));
            let (data, next) = next.get(2..)?.split_at_checked(size)?;
            rest = if size & 1 != 0 { next.get(1..)? } else { next };
            items.push(CollectionItem {
                tag,
                id: id as i32,
                attributes,
                data: data.to_vec(),
            });
        }
        items.sort_by_key(|item| (item.tag, item.id));
        let token = self.new_collection();
        self.collections.get_mut(&token)?.items = items;
        Some(token)
    }
}

fn read_u32(bytes: &[u8]) -> Option<(u32, &[u8])> {
    let raw: &[u8; 4] = bytes.get(..4)?.try_into().ok()?;
    Some((u32::from_be_bytes(*raw), &bytes[4..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_key_order_locking_and_attribute_masks() {
        let mut manager = ProcessCollectionManagerState::default();
        let collection = manager.new_collection();
        assert_eq!(manager.clone_reference(collection), collection);
        assert_eq!(manager.owner_count(collection), 2);
        assert_eq!(manager.add_item(collection, 2, 5, vec![5]), 0);
        assert_eq!(manager.add_item(collection, 1, 8, vec![8]), 0);
        assert_eq!(manager.add_item(collection, 1, 3, vec![3]), 0);
        assert_eq!(manager.item_by_index(collection, 0).unwrap().id, 3);
        assert_eq!(manager.tagged_item(collection, 1, 2).unwrap().1.id, 8);
        assert_eq!(
            manager.set_key_attributes(
                collection,
                1,
                3,
                COLLECTION_LOCK_MASK,
                COLLECTION_LOCK_MASK
            ),
            0
        );
        assert_eq!(
            manager.remove_key(collection, 1, 3),
            COLLECTION_ITEM_LOCKED_ERR
        );
        manager.dispose(collection);
        assert_eq!(manager.owner_count(collection), 1);
        manager.dispose(collection);
        assert_eq!(manager.owner_count(collection), 0);
    }

    #[test]
    fn flatten_round_trip_filters_nonpersistent_items() {
        let mut manager = ProcessCollectionManagerState::default();
        let source = manager.new_collection();
        manager.add_item(source, 1, 1, vec![1, 2, 3]);
        manager.add_item(source, 2, 2, vec![4]);
        manager.set_key_attributes(source, 2, 2, COLLECTION_PERSISTENCE_MASK, 0);
        let bytes = manager.flatten(source, None).unwrap();
        let target = manager.new_collection();
        assert_eq!(manager.unflatten(target, &bytes), 0);
        assert_eq!(manager.item_count(target), 1);
        assert_eq!(manager.item_by_key(target, 1, 1).unwrap().1.data, [1, 2, 3]);
    }

    #[test]
    fn invalid_indexes_purge_locking_and_resource_decoding_follow_contract() {
        let mut manager = ProcessCollectionManagerState::default();
        let collection = manager.new_collection();
        assert_eq!(
            manager.remove_indexed(collection, -1),
            COLLECTION_INDEX_RANGE_ERR
        );
        assert_eq!(
            manager.remove_key(collection, 1, 1),
            COLLECTION_ITEM_NOT_FOUND_ERR
        );

        assert_eq!(manager.add_item(collection, 1, 1, vec![1]), 0);
        assert_eq!(manager.add_item(collection, 2, 2, vec![2]), 0);
        assert_eq!(
            manager.set_key_attributes(
                collection,
                2,
                2,
                COLLECTION_LOCK_MASK,
                COLLECTION_LOCK_MASK,
            ),
            0
        );
        manager.purge_matching(
            collection,
            COLLECTION_PERSISTENCE_MASK,
            COLLECTION_PERSISTENCE_MASK,
        );
        assert_eq!(manager.item_count(collection), 1);
        assert!(manager.item_by_key(collection, 2, 2).is_some());
        manager.empty(collection);
        assert_eq!(manager.item_count(collection), 1);

        let mut resource = Vec::new();
        resource.extend_from_slice(&1u32.to_be_bytes());
        resource.extend_from_slice(&u32::from_be_bytes(*b"name").to_be_bytes());
        resource.extend_from_slice(&(-4i32).to_be_bytes());
        resource.extend_from_slice(&DEFAULT_COLLECTION_ATTRIBUTES.to_be_bytes());
        resource.extend_from_slice(&3u16.to_be_bytes());
        resource.extend_from_slice(b"abc");
        resource.push(0);
        let decoded = manager.from_resource(&resource).unwrap();
        let item = manager
            .item_by_key(decoded, u32::from_be_bytes(*b"name"), -4)
            .unwrap()
            .1;
        assert_eq!(item.attributes, DEFAULT_COLLECTION_ATTRIBUTES);
        assert_eq!(item.data, b"abc");
    }

    #[test]
    fn malformed_flattened_stream_is_rejected_without_partial_mutation() {
        let mut manager = ProcessCollectionManagerState::default();
        let source = manager.new_collection();
        manager.add_item(source, 1, 1, vec![1]);
        manager.add_item(source, 2, 2, vec![2]);
        let mut bytes = manager.flatten(source, None).unwrap();
        bytes.pop();

        let target = manager.new_collection();
        assert_eq!(manager.unflatten(target, &bytes), COLLECTION_VERSION_ERR);
        assert_eq!(manager.item_count(target), 0);
    }
}
