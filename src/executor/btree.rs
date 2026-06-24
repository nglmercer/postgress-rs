use crate::types::{PageId, Oid};
use crate::storage::StorageTrait;
use crate::btree::{IndexTuple, btree_insert as btree_insert_page, btree_search, BTreeIndex, IndexColumn, SortDirection};

pub struct BTreeScan {
    pub index_oid: Oid,
    pub scan_from: Vec<u8>,
}

pub struct BTreeInsert {
    pub index_oid: Oid,
    pub root_page: PageId,
    pub key: Vec<u8>,
    pub heap_page: u32,
    pub heap_offset: u16,
}

pub fn btree_insert(
    storage: &dyn StorageTrait,
    op: &BTreeInsert,
    page_size: usize,
) -> anyhow::Result<()> {
    let tuple = IndexTuple {
        key: op.key.clone(),
        heap_pointer: (op.heap_page, op.heap_offset),
        heap_oid: Oid(0),
    };

    let mut allocator_counter: u32 = 10000;
    let _next_page_id = || {
        let id = PageId(allocator_counter);
        allocator_counter += 1;
        id
    };

    btree_insert_page(storage, op.root_page, tuple, page_size)?;
    Ok(())
}

pub fn btree_scan(
    storage: &dyn StorageTrait,
    op: &BTreeScan,
    root_page: PageId,
    page_size: usize,
) -> anyhow::Result<Vec<(Vec<u8>, (PageId, u16))>> {
    let results = btree_search(storage, root_page, &op.scan_from, page_size)?;
    Ok(results
        .into_iter()
        .map(|t| {
            let page_id = PageId(t.heap_pointer.0);
            let offset = t.heap_pointer.1;
            (t.key, (page_id, offset))
        })
        .collect())
}

pub fn index_only_scan(
    storage: &dyn StorageTrait,
    op: &BTreeScan,
    root_page: PageId,
    page_size: usize,
    index: &BTreeIndex,
    select_columns: &[String],
) -> anyhow::Result<Vec<Vec<String>>> {
    // Check if this is an index-only scan
    if !can_satisfy_from_index(index, select_columns) {
        return Err(anyhow::anyhow!("Cannot satisfy query from index only"));
    }

    let results = btree_search(storage, root_page, &op.scan_from, page_size)?;

    let mut rows = Vec::new();
    for tuple in results {
        // Decode the composite key to get column values
        let values = index.decode_composite_key(&tuple.key);
        rows.push(values);
    }

    Ok(rows)
}

fn can_satisfy_from_index(index: &BTreeIndex, select_columns: &[String]) -> bool {
    // For index-only scan, all selected columns must be in the index
    let index_columns: Vec<String> = index.columns.iter().map(|c| c.name.clone()).collect();

    for col in select_columns {
        if !index_columns.contains(col) {
            return false;
        }
    }
    true
}

pub fn encode_index_key(
    values: &[String],
    columns: &[IndexColumn],
) -> Vec<u8> {
    let mut key = Vec::new();
    for (i, val) in values.iter().enumerate() {
        let direction = columns.get(i).map(|c| c.direction).unwrap_or(SortDirection::Asc);
        let nulls_first = columns.get(i).map(|c| c.nulls_first).unwrap_or(false);

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

pub fn decode_index_key(
    key: &[u8],
    _columns: &[IndexColumn],
) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ephemeral::EphemeralStorage;

    #[test]
    fn test_btree_insert_and_scan() {
        let storage = EphemeralStorage::new();
        let root = PageId(1);

        let insert_op = BTreeInsert {
            index_oid: Oid(1),
            root_page: root,
            key: b"hello".to_vec(),
            heap_page: 10,
            heap_offset: 0,
        };
        btree_insert(&storage, &insert_op, 8192).unwrap();

        let insert_op2 = BTreeInsert {
            index_oid: Oid(1),
            root_page: root,
            key: b"world".to_vec(),
            heap_page: 10,
            heap_offset: 1,
        };
        btree_insert(&storage, &insert_op2, 8192).unwrap();

        let scan_op = BTreeScan {
            index_oid: Oid(1),
            scan_from: b"hello".to_vec(),
        };
        let results = btree_scan(&storage, &scan_op, root, 8192).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, b"hello");
        assert_eq!(results[0].1 .0, PageId(10));
        assert_eq!(results[0].1 .1, 0);
    }

    #[test]
    fn test_btree_scan_empty() {
        let storage = EphemeralStorage::new();
        let root = PageId(1);
        let scan_op = BTreeScan {
            index_oid: Oid(1),
            scan_from: b"nonexistent".to_vec(),
        };
        let results = btree_scan(&storage, &scan_op, root, 8192).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_encode_decode_index_key() {
        let columns = vec![
            IndexColumn { name: "id".to_string(), direction: SortDirection::Asc, nulls_first: false },
            IndexColumn { name: "name".to_string(), direction: SortDirection::Desc, nulls_first: true },
        ];

        let values = vec!["42".to_string(), "hello".to_string()];
        let key = encode_index_key(&values, &columns);
        let decoded = decode_index_key(&key, &columns);

        assert_eq!(decoded, values);
    }

    #[test]
    fn test_encode_decode_with_null() {
        let columns = vec![
            IndexColumn { name: "id".to_string(), direction: SortDirection::Asc, nulls_first: false },
        ];

        let values = vec!["NULL".to_string()];
        let key = encode_index_key(&values, &columns);
        let decoded = decode_index_key(&key, &columns);

        assert_eq!(decoded, values);
    }
}
