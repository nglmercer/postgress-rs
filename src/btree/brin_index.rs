use crate::types::{Oid, PageId};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct BlockRangeSummary {
    pub block_start: u32,
    pub block_end: u32,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub null_count: u32,
    pub distinct_count: u32,
}

pub struct BrinIndex {
    pub index_oid: Oid,
    pub rel_oid: Oid,
    pub pages_per_range: u32,
    pub ranges: Vec<BlockRangeSummary>,
    pub values: HashMap<u32, Vec<(String, (PageId, u16))>>,
}

impl BrinIndex {
    pub fn new(index_oid: Oid, rel_oid: Oid) -> Self {
        Self {
            index_oid,
            rel_oid,
            pages_per_range: 128,
            ranges: Vec::new(),
            values: HashMap::new(),
        }
    }

    pub fn with_pages_per_range(mut self, pages: u32) -> Self {
        self.pages_per_range = pages;
        self
    }

    pub fn insert(&mut self, block_id: u32, value: &str, tid: (PageId, u16)) {
        let range_idx = (block_id / self.pages_per_range) as usize;

        while self.ranges.len() <= range_idx {
            self.ranges.push(BlockRangeSummary {
                block_start: (self.ranges.len() as u32) * self.pages_per_range,
                block_end: ((self.ranges.len() as u32) + 1) * self.pages_per_range - 1,
                min_value: None,
                max_value: None,
                null_count: 0,
                distinct_count: 0,
            });
        }

        let range = &mut self.ranges[range_idx];

        if value.is_empty() || value == "NULL" {
            range.null_count += 1;
        } else {
            match &range.min_value {
                None => {
                    range.min_value = Some(value.to_string());
                    range.max_value = Some(value.to_string());
                }
                Some(min) => {
                    if value < min.as_str() {
                        range.min_value = Some(value.to_string());
                    }
                }
            }
            match &range.max_value {
                None => {
                    range.max_value = Some(value.to_string());
                }
                Some(max) => {
                    if value > max.as_str() {
                        range.max_value = Some(value.to_string());
                    }
                }
            }
            range.distinct_count += 1;
        }

        self.values
            .entry(range_idx as u32)
            .or_default()
            .push((value.to_string(), tid));
    }

    pub fn lookup(&self, value: &str) -> Vec<(PageId, u16)> {
        let mut result = Vec::new();

        for (range_idx, range) in self.ranges.iter().enumerate() {
            let might_contain = match (&range.min_value, &range.max_value) {
                (Some(min), Some(max)) => value >= min.as_str() && value <= max.as_str(),
                (Some(min), None) => value >= min.as_str(),
                (None, Some(max)) => value <= max.as_str(),
                (None, None) => range.null_count > 0 && (value.is_empty() || value == "NULL"),
            };

            if might_contain {
                if let Some(entries) = self.values.get(&(range_idx as u32)) {
                    for (val, tid) in entries {
                        if val == value {
                            result.push(*tid);
                        }
                    }
                }
            }
        }

        result
    }

    pub fn lookup_range(&self, min: &str, max: &str) -> Vec<(PageId, u16)> {
        let mut result = Vec::new();

        for (range_idx, range) in self.ranges.iter().enumerate() {
            let might_overlap = match (&range.min_value, &range.max_value) {
                (Some(range_min), Some(range_max)) => {
                    range_min.as_str() <= max && range_max.as_str() >= min
                }
                _ => true,
            };

            if might_overlap {
                if let Some(entries) = self.values.get(&(range_idx as u32)) {
                    for (val, tid) in entries {
                        if val.as_str() >= min && val.as_str() <= max {
                            result.push(*tid);
                        }
                    }
                }
            }
        }

        result
    }

    pub fn delete(&mut self, value: &str, tid: &(PageId, u16)) -> bool {
        for entries in self.values.values_mut() {
            if let Some(pos) = entries.iter().position(|(v, t)| v == value && t == tid) {
                entries.remove(pos);
                return true;
            }
        }
        false
    }

    pub fn range_count(&self) -> usize {
        self.ranges.len()
    }

    pub fn entry_count(&self) -> usize {
        self.values.values().map(|v| v.len()).sum()
    }

    pub fn get_summary(&self, range_idx: usize) -> Option<&BlockRangeSummary> {
        self.ranges.get(range_idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brin_insert_and_lookup() {
        let mut index = BrinIndex::new(Oid(1), Oid(100));
        index.insert(0, "value1", (PageId(1), 0));
        index.insert(0, "value2", (PageId(1), 1));

        let results = index.lookup("value1");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], (PageId(1), 0));
    }

    #[test]
    fn test_brin_lookup_nonexistent() {
        let index = BrinIndex::new(Oid(1), Oid(100));
        let results = index.lookup("nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_brin_lookup_range() {
        let mut index = BrinIndex::new(Oid(1), Oid(100));
        index.insert(0, "apple", (PageId(1), 0));
        index.insert(0, "banana", (PageId(1), 1));
        index.insert(0, "cherry", (PageId(1), 2));

        let results = index.lookup_range("banana", "cherry");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_brin_delete() {
        let mut index = BrinIndex::new(Oid(1), Oid(100));
        let tid = (PageId(1), 0);
        index.insert(0, "value1", tid);

        assert!(index.delete("value1", &tid));
        assert!(index.lookup("value1").is_empty());
    }

    #[test]
    fn test_brin_delete_nonexistent() {
        let mut index = BrinIndex::new(Oid(1), Oid(100));
        assert!(!index.delete("key", &(PageId(1), 0)));
    }

    #[test]
    fn test_brin_range_summary() {
        let mut index = BrinIndex::new(Oid(1), Oid(100));
        index.insert(0, "apple", (PageId(1), 0));
        index.insert(0, "cherry", (PageId(1), 1));

        let summary = index.get_summary(0).unwrap();
        assert_eq!(summary.min_value.as_deref(), Some("apple"));
        assert_eq!(summary.max_value.as_deref(), Some("cherry"));
    }

    #[test]
    fn test_brin_multiple_ranges() {
        let mut index = BrinIndex::new(Oid(1), Oid(100)).with_pages_per_range(1);
        index.insert(0, "a", (PageId(1), 0));
        index.insert(1, "b", (PageId(1), 1));
        index.insert(2, "c", (PageId(1), 2));

        assert_eq!(index.range_count(), 3);
        assert_eq!(index.entry_count(), 3);
    }

    #[test]
    fn test_brin_with_custom_pages_per_range() {
        let index = BrinIndex::new(Oid(1), Oid(100)).with_pages_per_range(256);
        assert_eq!(index.pages_per_range, 256);
    }

    #[test]
    fn test_brin_entry_count() {
        let mut index = BrinIndex::new(Oid(1), Oid(100));
        index.insert(0, "value1", (PageId(1), 0));
        index.insert(0, "value2", (PageId(1), 1));
        index.insert(1, "value3", (PageId(1), 2));

        assert_eq!(index.entry_count(), 3);
    }

    #[test]
    fn test_brin_null_handling() {
        let mut index = BrinIndex::new(Oid(1), Oid(100));
        index.insert(0, "NULL", (PageId(1), 0));
        index.insert(0, "value1", (PageId(1), 1));

        let summary = index.get_summary(0).unwrap();
        assert_eq!(summary.null_count, 1);
    }
}
