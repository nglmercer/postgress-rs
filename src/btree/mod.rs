pub mod page;
pub mod scan;
pub mod insert;
pub mod search;
pub mod hash_index;
pub mod gin_index;

use crate::types::PageId;
pub use page::{BTreePage, BTreePageType, IndexTuple, BTreeMetaPage};
pub use scan::{BTreeScan, ScanDirection};
pub use insert::btree_insert;
pub use search::btree_search;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexType {
    BTree,
    Hash,
    GiST,
    GIN,
    BRIN,
}

#[derive(Debug, Clone)]
pub struct IndexColumn {
    pub name: String,
    pub direction: SortDirection,
    pub nulls_first: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

pub struct BTreeIndex {
    pub index_oid: u32,
    pub rel_oid: u32,
    pub root_page: PageId,
    pub page_size: usize,
    pub unique: bool,
    pub columns: Vec<IndexColumn>,
    pub index_type: IndexType,
}

impl BTreeIndex {
    pub fn new(index_oid: u32, rel_oid: u32, root_page: PageId, page_size: usize) -> Self {
        Self {
            index_oid,
            rel_oid,
            root_page,
            page_size,
            unique: false,
            columns: Vec::new(),
            index_type: IndexType::BTree,
        }
    }

    pub fn with_unique(mut self, unique: bool) -> Self {
        self.unique = unique;
        self
    }

    pub fn with_columns(mut self, columns: Vec<IndexColumn>) -> Self {
        self.columns = columns;
        self
    }

    pub fn with_index_type(mut self, index_type: IndexType) -> Self {
        self.index_type = index_type;
        self
    }

    pub fn is_unique(&self) -> bool {
        self.unique
    }

    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    pub fn encode_composite_key(&self, values: &[String]) -> Vec<u8> {
        let mut key = Vec::new();
        for (i, val) in values.iter().enumerate() {
            let direction = self.columns.get(i).map(|c| c.direction).unwrap_or(SortDirection::Asc);
            let nulls_first = self.columns.get(i).map(|c| c.nulls_first).unwrap_or(false);

            // Encode null indicator
            if val.is_empty() || val == "NULL" {
                key.push(0u8); // null
                key.push(if nulls_first { 1u8 } else { 0u8 });
            } else {
                key.push(1u8); // not null
                key.push(if direction == SortDirection::Desc { 1u8 } else { 0u8 });

                // Encode value length and data
                let val_bytes = val.as_bytes();
                let len = val_bytes.len() as u32;
                key.extend_from_slice(&len.to_le_bytes());
                key.extend_from_slice(val_bytes);
            }

            // Column separator
            key.push(0xFF);
        }
        key
    }

    pub fn decode_composite_key(&self, key: &[u8]) -> Vec<String> {
        let mut values = Vec::new();
        let mut pos = 0;

        while pos < key.len() {
            if pos >= key.len() {
                break;
            }

            let null_indicator = key[pos];
            pos += 1;

            if null_indicator == 0 {
                // Null value
                pos += 1; // skip direction byte
                values.push("NULL".to_string());
            } else {
                let _direction = key[pos];
                pos += 1;

                if pos + 4 > key.len() {
                    break;
                }

                let len = u32::from_le_bytes([
                    key[pos], key[pos + 1], key[pos + 2], key[pos + 3],
                ]) as usize;
                pos += 4;

                if pos + len > key.len() {
                    break;
                }

                let val = String::from_utf8_lossy(&key[pos..pos + len]).to_string();
                values.push(val);
                pos += len;
            }

            // Skip separator
            if pos < key.len() && key[pos] == 0xFF {
                pos += 1;
            }
        }

        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composite_key_roundtrip() {
        let index = BTreeIndex::new(1, 100, PageId(1), 8192)
            .with_columns(vec![
                IndexColumn { name: "id".to_string(), direction: SortDirection::Asc, nulls_first: false },
                IndexColumn { name: "name".to_string(), direction: SortDirection::Desc, nulls_first: true },
            ]);

        let values = vec!["42".to_string(), "hello".to_string()];
        let key = index.encode_composite_key(&values);
        let decoded = index.decode_composite_key(&key);

        assert_eq!(decoded, values);
    }

    #[test]
    fn test_composite_key_with_null() {
        let index = BTreeIndex::new(1, 100, PageId(1), 8192)
            .with_columns(vec![
                IndexColumn { name: "id".to_string(), direction: SortDirection::Asc, nulls_first: false },
                IndexColumn { name: "name".to_string(), direction: SortDirection::Asc, nulls_first: true },
            ]);

        let values = vec!["42".to_string(), "NULL".to_string()];
        let key = index.encode_composite_key(&values);
        let decoded = index.decode_composite_key(&key);

        assert_eq!(decoded, values);
    }

    #[test]
    fn test_unique_index() {
        let index = BTreeIndex::new(1, 100, PageId(1), 8192)
            .with_unique(true);

        assert!(index.is_unique());
        assert_eq!(index.column_count(), 0);
    }
}
