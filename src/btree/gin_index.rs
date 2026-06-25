use crate::types::{Oid, PageId};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct GinIndexEntry {
    pub key: String,
    pub tids: Vec<(PageId, u16)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GinIndexType {
    Array,
    FullText,
    Jsonb,
}

pub struct GinIndex {
    pub index_oid: Oid,
    pub rel_oid: Oid,
    pub index_type: GinIndexType,
    pub entries: HashMap<String, Vec<(PageId, u16)>>,
}

impl GinIndex {
    pub fn new(index_oid: Oid, rel_oid: Oid, index_type: GinIndexType) -> Self {
        Self {
            index_oid,
            rel_oid,
            index_type,
            entries: HashMap::new(),
        }
    }

    pub fn insert_array_values(&mut self, values: &[String], tid: (PageId, u16)) {
        for value in values {
            self.entries.entry(value.clone()).or_default().push(tid);
        }
    }

    pub fn insert_full_text(&mut self, terms: &[String], tid: (PageId, u16)) {
        for term in terms {
            let normalized = term.to_lowercase();
            self.entries.entry(normalized).or_default().push(tid);
        }
    }

    pub fn insert_jsonb_keys(&mut self, keys: &[String], tid: (PageId, u16)) {
        for key in keys {
            self.entries.entry(key.clone()).or_default().push(tid);
        }
    }

    pub fn lookup(&self, key: &str) -> Vec<(PageId, u16)> {
        match self.index_type {
            GinIndexType::FullText => {
                let normalized = key.to_lowercase();
                self.entries.get(&normalized).cloned().unwrap_or_default()
            }
            _ => self.entries.get(key).cloned().unwrap_or_default(),
        }
    }

    pub fn contains(&self, values: &[String]) -> Vec<(PageId, u16)> {
        if values.is_empty() {
            return Vec::new();
        }

        let mut result: Vec<(PageId, u16)> = Vec::new();

        if let Some(first_tids) = self.entries.get(&values[0]) {
            'outer: for tid in first_tids {
                for value in &values[1..] {
                    if let Some(tids) = self.entries.get(value) {
                        if !tids.contains(tid) {
                            continue 'outer;
                        }
                    } else {
                        continue 'outer;
                    }
                }
                result.push(tid.clone());
            }
        }

        result
    }

    pub fn any_contains(&self, values: &[String]) -> Vec<(PageId, u16)> {
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for value in values {
            if let Some(tids) = self.entries.get(value) {
                for tid in tids {
                    if seen.insert(tid.clone()) {
                        result.push(tid.clone());
                    }
                }
            }
        }

        result
    }

    pub fn delete(&mut self, key: &str, tid: &(PageId, u16)) -> bool {
        if let Some(tids) = self.entries.get_mut(key) {
            if let Some(pos) = tids.iter().position(|t| t == tid) {
                tids.remove(pos);
                if tids.is_empty() {
                    self.entries.remove(key);
                }
                return true;
            }
        }
        false
    }

    pub fn scan(&self) -> Vec<GinIndexEntry> {
        self.entries
            .iter()
            .map(|(key, tids)| GinIndexEntry {
                key: key.clone(),
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
    fn test_insert_array_values() {
        let mut index = GinIndex::new(Oid(1), Oid(100), GinIndexType::Array);
        let values = vec!["tag1".to_string(), "tag2".to_string(), "tag3".to_string()];
        let tid = (PageId(1), 0);

        index.insert_array_values(&values, tid);
        assert_eq!(index.entry_count(), 3);
        assert_eq!(index.tid_count(), 3);
    }

    #[test]
    fn test_lookup_array() {
        let mut index = GinIndex::new(Oid(1), Oid(100), GinIndexType::Array);
        let values = vec!["tag1".to_string(), "tag2".to_string()];
        index.insert_array_values(&values, (PageId(1), 0));

        let results = index.lookup("tag1");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_contains() {
        let mut index = GinIndex::new(Oid(1), Oid(100), GinIndexType::Array);
        let values = vec!["tag1".to_string(), "tag2".to_string(), "tag3".to_string()];
        index.insert_array_values(&values, (PageId(1), 0));
        index.insert_array_values(
            &vec!["tag1".to_string(), "tag2".to_string()],
            (PageId(1), 1),
        );

        let query = vec!["tag1".to_string(), "tag2".to_string()];
        let results = index.contains(&query);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_any_contains() {
        let mut index = GinIndex::new(Oid(1), Oid(100), GinIndexType::Array);
        index.insert_array_values(&vec!["tag1".to_string()], (PageId(1), 0));
        index.insert_array_values(&vec!["tag2".to_string()], (PageId(1), 1));
        index.insert_array_values(&vec!["tag3".to_string()], (PageId(1), 2));

        let query = vec!["tag1".to_string(), "tag3".to_string()];
        let results = index.any_contains(&query);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_insert_full_text() {
        let mut index = GinIndex::new(Oid(1), Oid(100), GinIndexType::FullText);
        let terms = vec!["hello".to_string(), "world".to_string()];
        index.insert_full_text(&terms, (PageId(1), 0));

        assert_eq!(index.entry_count(), 2);
    }

    #[test]
    fn test_lookup_full_text_case_insensitive() {
        let mut index = GinIndex::new(Oid(1), Oid(100), GinIndexType::FullText);
        index.insert_full_text(&vec!["Hello".to_string()], (PageId(1), 0));

        let results = index.lookup("hello");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_insert_jsonb_keys() {
        let mut index = GinIndex::new(Oid(1), Oid(100), GinIndexType::Jsonb);
        let keys = vec!["name".to_string(), "age".to_string()];
        index.insert_jsonb_keys(&keys, (PageId(1), 0));

        assert_eq!(index.entry_count(), 2);
    }

    #[test]
    fn test_delete() {
        let mut index = GinIndex::new(Oid(1), Oid(100), GinIndexType::Array);
        let tid = (PageId(1), 0);
        index.insert_array_values(&vec!["tag1".to_string()], tid);

        assert!(index.delete("tag1", &tid));
        assert!(index.lookup("tag1").is_empty());
    }

    #[test]
    fn test_delete_nonexistent() {
        let mut index = GinIndex::new(Oid(1), Oid(100), GinIndexType::Array);
        assert!(!index.delete("key", &(PageId(1), 0)));
    }

    #[test]
    fn test_scan() {
        let mut index = GinIndex::new(Oid(1), Oid(100), GinIndexType::Array);
        index.insert_array_values(&vec!["tag1".to_string()], (PageId(1), 0));
        index.insert_array_values(&vec!["tag2".to_string()], (PageId(1), 1));

        let entries = index.scan();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_contains_empty_query() {
        let index = GinIndex::new(Oid(1), Oid(100), GinIndexType::Array);
        let results = index.contains(&[]);
        assert!(results.is_empty());
    }
}
