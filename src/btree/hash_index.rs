use crate::types::{Oid, PageId};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct HashIndexEntry {
    pub hash_value: u32,
    pub tids: Vec<(PageId, u16)>,
}

pub struct HashIndex {
    pub index_oid: Oid,
    pub rel_oid: Oid,
    pub bucket_count: u32,
    pub entries: HashMap<u32, Vec<(PageId, u16)>>,
}

impl HashIndex {
    pub fn new(index_oid: Oid, rel_oid: Oid) -> Self {
        Self {
            index_oid,
            rel_oid,
            bucket_count: 256,
            entries: HashMap::new(),
        }
    }

    pub fn with_bucket_count(mut self, count: u32) -> Self {
        self.bucket_count = count;
        self
    }

    pub fn hash_value(key: &[u8]) -> u32 {
        let mut hash: u32 = 0;
        for byte in key {
            hash = hash.wrapping_mul(31).wrapping_add(*byte as u32);
        }
        hash
    }

    pub fn insert(&mut self, key: &[u8], tid: (PageId, u16)) {
        let hash = Self::hash_value(key);
        self.entries.entry(hash).or_default().push(tid);
    }

    pub fn lookup(&self, key: &[u8]) -> Vec<(PageId, u16)> {
        let hash = Self::hash_value(key);
        self.entries.get(&hash).cloned().unwrap_or_default()
    }

    pub fn delete(&mut self, key: &[u8], tid: &(PageId, u16)) -> bool {
        let hash = Self::hash_value(key);
        if let Some(tids) = self.entries.get_mut(&hash) {
            if let Some(pos) = tids.iter().position(|t| t == tid) {
                tids.remove(pos);
                if tids.is_empty() {
                    self.entries.remove(&hash);
                }
                return true;
            }
        }
        false
    }

    pub fn scan(&self) -> Vec<HashIndexEntry> {
        self.entries
            .iter()
            .map(|(&hash, tids)| HashIndexEntry {
                hash_value: hash,
                tids: tids.clone(),
            })
            .collect()
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn tid_count(&self) -> usize {
        self.entries.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_value_deterministic() {
        let key = b"hello";
        let h1 = HashIndex::hash_value(key);
        let h2 = HashIndex::hash_value(key);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_value_different_keys() {
        let h1 = HashIndex::hash_value(b"hello");
        let h2 = HashIndex::hash_value(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_insert_and_lookup() {
        let mut index = HashIndex::new(Oid(1), Oid(100));
        let key = b"test_key";
        let tid = (PageId(1), 0);

        index.insert(key, tid);
        let results = index.lookup(key);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], tid);
    }

    #[test]
    fn test_lookup_nonexistent() {
        let index = HashIndex::new(Oid(1), Oid(100));
        let results = index.lookup(b"nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_insert_multiple_same_key() {
        let mut index = HashIndex::new(Oid(1), Oid(100));
        let key = b"test_key";

        index.insert(key, (PageId(1), 0));
        index.insert(key, (PageId(1), 1));
        index.insert(key, (PageId(2), 0));

        let results = index.lookup(key);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_delete() {
        let mut index = HashIndex::new(Oid(1), Oid(100));
        let key = b"test_key";
        let tid = (PageId(1), 0);

        index.insert(key, tid);
        assert!(index.delete(key, &tid));
        assert!(index.lookup(key).is_empty());
    }

    #[test]
    fn test_delete_nonexistent() {
        let mut index = HashIndex::new(Oid(1), Oid(100));
        assert!(!index.delete(b"key", &(PageId(1), 0)));
    }

    #[test]
    fn test_scan() {
        let mut index = HashIndex::new(Oid(1), Oid(100));
        index.insert(b"key1", (PageId(1), 0));
        index.insert(b"key2", (PageId(1), 1));

        let entries = index.scan();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_entry_and_tid_count() {
        let mut index = HashIndex::new(Oid(1), Oid(100));
        index.insert(b"key1", (PageId(1), 0));
        index.insert(b"key1", (PageId(1), 1));
        index.insert(b"key2", (PageId(1), 2));

        assert_eq!(index.entry_count(), 2);
        assert_eq!(index.tid_count(), 3);
    }

    #[test]
    fn test_with_bucket_count() {
        let index = HashIndex::new(Oid(1), Oid(100)).with_bucket_count(1024);
        assert_eq!(index.bucket_count, 1024);
    }
}
