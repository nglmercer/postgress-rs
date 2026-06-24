use crate::types::PageId;
use crate::btree::page::{BTreePage, BTreePageType, IndexTuple};
use crate::btree::insert::read_page as btree_read_page;
use crate::storage::StorageTrait;
use anyhow::Context;

pub fn btree_search(
    storage: &dyn StorageTrait,
    root_page: PageId,
    search_key: &[u8],
    page_size: usize,
) -> anyhow::Result<Vec<IndexTuple>> {
    let mut results = Vec::new();
    search_page(storage, root_page, search_key, &mut results, page_size)?;
    Ok(results)
}

fn search_page(
    storage: &dyn StorageTrait,
    page_id: PageId,
    search_key: &[u8],
    results: &mut Vec<IndexTuple>,
    page_size: usize,
) -> anyhow::Result<()> {
    let page = read_page(storage, page_id, page_size)
        .with_context(|| format!("reading page {:?} during search", page_id))?;

    match page.page_type {
        BTreePageType::Leaf | BTreePageType::Overflow => {
            for key in &page.keys {
                if key.key.as_slice() == search_key {
                    results.push(key.clone());
                }
            }
        }
        BTreePageType::Root | BTreePageType::Internal => {
            let child_page_id = find_descend_child(&page, search_key);
            search_page(storage, child_page_id, search_key, results, page_size)?;
        }
        BTreePageType::Meta => {}
    }

    Ok(())
}

fn find_descend_child(page: &BTreePage, search_key: &[u8]) -> PageId {
    if page.keys.is_empty() {
        return page.page_id;
    }

    let mut child = PageId(page.keys[0].heap_pointer.0);
    for k in &page.keys {
        if search_key >= k.key.as_slice() {
            child = PageId(k.heap_pointer.0);
        } else {
            break;
        }
    }
    child
}

fn read_page(
    storage: &dyn StorageTrait,
    page_id: PageId,
    page_size: usize,
) -> anyhow::Result<BTreePage> {
    btree_read_page(storage, page_id, page_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btree::page::BTreePage;
    use crate::storage::ephemeral::EphemeralStorage;
    use crate::types::Oid;

    #[test]
    fn test_leaf_search_basic() {
        let storage = EphemeralStorage::new();
        let page_id = PageId(1);
        let page_size = 4096;

        let mut page = BTreePage::new(BTreePageType::Leaf, page_id);
        page.level = 0;
        page.keys = vec![
            IndexTuple { key: b"apple".to_vec(),  heap_pointer: (10, 1), heap_oid: Oid(1) },
            IndexTuple { key: b"banana".to_vec(), heap_pointer: (10, 2), heap_oid: Oid(2) },
            IndexTuple { key: b"cherry".to_vec(), heap_pointer: (11, 1), heap_oid: Oid(3) },
            IndexTuple { key: b"date".to_vec(),   heap_pointer: (12, 1), heap_oid: Oid(4) },
            IndexTuple { key: b"fig".to_vec(),    heap_pointer: (13, 1), heap_oid: Oid(5) },
        ];

        crate::btree::insert::write_page(&storage, page_id, &page, page_size).unwrap();

        let hits = btree_search(&storage, page_id, b"cherry", page_size).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].heap_pointer, (11, 1));

        let none = btree_search(&storage, page_id, b"grape", page_size).unwrap();
        assert!(none.is_empty());

        let all = btree_search(&storage, page_id, b"fig", page_size).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].key, b"fig");
    }

    #[test]
    fn test_search_descends_two_level_tree() {
        let storage = EphemeralStorage::new();
        let page_size: usize = 8192;
        let _capacity = page_size / 64;

        // Build a multi-level tree by inserting enough tuples to cause splits
        let capacity = page_size / 64;
        let mut allocator: Vec<PageId> =
            (2u32..((capacity * 3) as u32)).map(PageId).collect();
        let mut next = move || allocator.pop().unwrap();

        let mut root = PageId(1);
        for i in 0u32..=(capacity as u32) {
            root = crate::btree::insert::btree_insert_multipage(
                &storage,
                root,
                IndexTuple {
                    key: i.to_le_bytes().to_vec(),
                    heap_pointer: (i * 2, 0),
                    heap_oid: Oid(i),
                },
                page_size,
                &mut next,
            ).unwrap();
        }

        // Search for a key that definitely exists
        let target_key = (capacity as u32 / 2).to_le_bytes();
        let hits = btree_search(&storage, root, &target_key, page_size).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, target_key);

        // Search for a key that does not exist
        let missing_key = u32::MAX.to_le_bytes();
        let none = btree_search(&storage, root, &missing_key, page_size).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn test_search_empty_root() {
        let storage = EphemeralStorage::new();
        let empty_root = PageId(99);
        let result = btree_search(&storage, empty_root, b"x", 4096).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_search_multiple_matches_at_leaf() {
        let storage = EphemeralStorage::new();
        let page_id = PageId(10);

        let mut page = BTreePage::new(BTreePageType::Leaf, page_id);
        page.level = 0;
        page.keys = vec![
            IndexTuple { key: b"a".to_vec(), heap_pointer: (1, 0), heap_oid: Oid(1) },
            IndexTuple { key: b"b".to_vec(), heap_pointer: (2, 0), heap_oid: Oid(2) },
            IndexTuple { key: b"b".to_vec(), heap_pointer: (3, 0), heap_oid: Oid(3) },
            IndexTuple { key: b"c".to_vec(), heap_pointer: (4, 0), heap_oid: Oid(4) },
        ];
        crate::btree::insert::write_page(&storage, page_id, &page, 4096).unwrap();

        let hits = btree_search(&storage, page_id, b"b", 4096).unwrap();
        assert_eq!(hits.len(), 2);
    }
}
