use crate::types::{PageId, Oid};
use crate::storage::StorageTrait;
use crate::btree::{IndexTuple, btree_insert as btree_insert_page, btree_search};

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
}
