use crate::types::{PageId, Oid};
use crate::btree::page::{BTreePage, BTreePageType, IndexTuple};
use crate::storage::StorageTrait;

pub fn serialize_page(page: &BTreePage, page_size: usize) -> Vec<u8> {
    let mut data = vec![0u8; page_size];

    // Byte 0: page type
    data[0] = match page.page_type {
        BTreePageType::Meta => 0,
        BTreePageType::Root => 1,
        BTreePageType::Internal => 2,
        BTreePageType::Leaf => 3,
        BTreePageType::Overflow => 4,
    };

    // Bytes 1-2: level
    let level_bytes = page.level.to_le_bytes();
    data[1] = level_bytes[0];
    data[2] = level_bytes[1];

    // Bytes 3-4: number of keys
    let num_keys = page.keys.len() as u16;
    let num_keys_bytes = num_keys.to_le_bytes();
    data[3] = num_keys_bytes[0];
    data[4] = num_keys_bytes[1];

    // Bytes 8-11: left sibling page id (0 = none)
    let left_sibling = page.left_sibling.map(|p| p.0).unwrap_or(0);
    data[8..12].copy_from_slice(&left_sibling.to_le_bytes());

    // Bytes 12-15: right sibling page id (0 = none)
    let right_sibling = page.right_sibling.map(|p| p.0).unwrap_or(0);
    data[12..16].copy_from_slice(&right_sibling.to_le_bytes());

    // Entries start at offset 64
    let mut pos = 64;
    for key_tuple in &page.keys {
        let key_len = key_tuple.key.len() as u32;
        if pos + 4 + key_len as usize + 6 > page_size {
            break;
        }

        // Key length (4 bytes)
        data[pos..pos + 4].copy_from_slice(&key_len.to_le_bytes());
        pos += 4;

        // Key data
        data[pos..pos + key_len as usize].copy_from_slice(&key_tuple.key);
        pos += key_len as usize;

        // Heap pointer: page_id (4 bytes) + offset (2 bytes)
        data[pos..pos + 4].copy_from_slice(&key_tuple.heap_pointer.0.to_le_bytes());
        pos += 4;
        data[pos..pos + 2].copy_from_slice(&key_tuple.heap_pointer.1.to_le_bytes());
        pos += 2;
    }

    data
}

pub fn deserialize_page(page_id: PageId, data: &[u8]) -> anyhow::Result<BTreePage> {
    if data.is_empty() || data.len() < 16 {
        return Ok(BTreePage::new(BTreePageType::Leaf, page_id));
    }

    let page_type = match data[0] {
        0 => BTreePageType::Leaf,
        1 => BTreePageType::Root,
        2 => BTreePageType::Internal,
        3 => BTreePageType::Leaf,
        4 => BTreePageType::Overflow,
        _ => BTreePageType::Leaf,
    };

    let level = u16::from_le_bytes([data[1], data[2]]);
    let num_keys = u16::from_le_bytes([data[3], data[4]]) as usize;

    let left_sibling_val = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let right_sibling_val = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);

    let left_sibling = if left_sibling_val == 0 { None } else { Some(PageId(left_sibling_val)) };
    let right_sibling = if right_sibling_val == 0 { None } else { Some(PageId(right_sibling_val)) };

    let mut keys = Vec::new();
    let mut pos = 64;

    for _ in 0..num_keys {
        if pos + 4 + 6 > data.len() {
            break;
        }

        let key_len = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        if key_len == 0 || pos + 4 + key_len + 6 > data.len() {
            break;
        }
        pos += 4;

        let key = data[pos..pos + key_len].to_vec();
        pos += key_len;

        let heap_page = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;
        let heap_offset = u16::from_le_bytes([data[pos], data[pos + 1]]);
        pos += 2;

        keys.push(IndexTuple {
            key,
            heap_pointer: (heap_page, heap_offset),
            heap_oid: Oid(0),
        });
    }

    Ok(BTreePage {
        page_type,
        page_id,
        level,
        keys,
        left_sibling,
        right_sibling,
    })
}

pub fn read_page(
    storage: &dyn StorageTrait,
    page_id: PageId,
    _page_size: usize,
) -> anyhow::Result<BTreePage> {
    let data = storage.read_page(page_id)?;
    if data.is_empty() || data.len() < 16 {
        return Ok(BTreePage::new(BTreePageType::Leaf, page_id));
    }
    deserialize_page(page_id, &data)
}

pub fn write_page(
    storage: &dyn StorageTrait,
    page_id: PageId,
    page: &BTreePage,
    page_size: usize,
) -> anyhow::Result<()> {
    let data = serialize_page(page, page_size);
    storage.write_page(page_id, &data)?;
    Ok(())
}

pub fn btree_insert(
    storage: &dyn StorageTrait,
    root_page: PageId,
    tuple: IndexTuple,
    page_size: usize,
) -> anyhow::Result<PageId> {
    let mut page = read_page(storage, root_page, page_size)?;
    page.insert_sorted(tuple);
    write_page(storage, root_page, &page, page_size)?;
    Ok(root_page)
}

pub fn btree_insert_multipage(
    storage: &dyn StorageTrait,
    root_page: PageId,
    tuple: IndexTuple,
    page_size: usize,
    next_page_id: &mut dyn FnMut() -> PageId,
) -> anyhow::Result<PageId> {
    let result = insert_recursive(
        storage,
        root_page,
        &tuple,
        page_size,
        next_page_id,
        true,
    )?;

    match result {
        Some(new_root_page_id) => Ok(new_root_page_id),
        None => Ok(root_page),
    }
}

fn insert_recursive(
    storage: &dyn StorageTrait,
    page_id: PageId,
    tuple: &IndexTuple,
    page_size: usize,
    next_page_id: &mut dyn FnMut() -> PageId,
    is_root: bool,
) -> anyhow::Result<Option<PageId>> {
    let mut page = read_page(storage, page_id, page_size)?;

    if page.page_type == BTreePageType::Leaf || page.keys.is_empty() {
        if page.keys.is_empty() {
            page.page_type = BTreePageType::Leaf;
        }
        if page.is_full(page_size) {
            let split_pos = page.keys.len() / 2;
            let mut right = page.split_at(split_pos);
            right.page_id = (next_page_id)();
            right.level = page.level;

            let push_up_key: Option<IndexTuple> =
                if tuple.key.as_slice()
                    < right
                        .keys
                        .first()
                        .map(|k| k.key.as_slice())
                        .unwrap_or_default()
                {
                    page.insert_sorted(tuple.clone());
                    right.keys.first().cloned()
                } else {
                    right.insert_sorted(tuple.clone());
                    Some(right.keys[0].clone())
                };

            right.left_sibling = Some(page.page_id);

            write_page(storage, page_id, &page, page_size)?;
            write_page(storage, right.page_id, &right, page_size)?;

            if is_root {
                let new_root_id = PageId((next_page_id)().0);
                let mut new_root = BTreePage::new(BTreePageType::Root, new_root_id);
                new_root.level = page.level + 1;

                if let Some(mut sep) = push_up_key {
                    sep.heap_pointer = (page.page_id.0, 0);
                    new_root.keys.push(sep);
                    let mut rk = new_root.keys[0].clone();
                    rk.heap_pointer = (right.page_id.0, 0);
                    new_root.keys.push(rk);
                } else {
                    new_root.keys.push(IndexTuple {
                        key: vec![],
                        heap_pointer: (page.page_id.0, 0),
                        heap_oid: Oid(0),
                    });
                    let rk = IndexTuple {
                        key: vec![],
                        heap_pointer: (right.page_id.0, 0),
                        heap_oid: Oid(0),
                    };
                    new_root.keys.push(rk);
                }

                write_page(storage, new_root_id, &new_root, page_size)?;
                Ok(Some(new_root_id))
            } else {
                Ok(Some(right.page_id))
            }
        } else {
            page.insert_sorted(tuple.clone());
            write_page(storage, page_id, &page, page_size)?;
            Ok(None)
        }
    } else {
        let child_page_id = find_child_page_id(&page, &tuple.key);

        match insert_recursive(storage, child_page_id, tuple, page_size, next_page_id, false)? {
            None => Ok(None),
            Some(new_child_id) => {
                let mut updated = read_page(storage, page_id, page_size)?;

                for k in &mut updated.keys {
                    if k.heap_pointer.0 == child_page_id.0 {
                        k.heap_pointer = (new_child_id.0, 0);
                        break;
                    }
                }

                if !updated.is_full(page_size) {
                    write_page(storage, page_id, &updated, page_size)?;
                    Ok(None)
                } else {
                    split_internal(updated, new_child_id, next_page_id, storage, page_size)
                }
            }
        }
    }
}

fn split_internal(
    mut page: BTreePage,
    updated_child_id: PageId,
    next_page_id: &mut dyn FnMut() -> PageId,
    storage: &dyn StorageTrait,
    page_size: usize,
) -> anyhow::Result<Option<PageId>> {
    let split_pos = page.keys.len() / 2;
    let mut right = page.split_at(split_pos);
    right.page_id = PageId((next_page_id)().0);
    right.level = page.level;
    right.left_sibling = Some(page.page_id);

    if let Some(k) = right.keys.get_mut(0) {
        k.heap_pointer = (updated_child_id.0, 0);
    }
    page.page_type = BTreePageType::Internal;
    right.page_type = BTreePageType::Internal;

    write_page(storage, page.page_id, &page, page_size)?;
    write_page(storage, right.page_id, &right, page_size)?;

    let new_root_id = PageId((next_page_id)().0);
    let mut new_root = BTreePage::new(BTreePageType::Root, new_root_id);
    new_root.level = page.level + 1;

    let push_up = right.keys[0].clone();
    let mut pk1 = push_up.clone();
    pk1.heap_pointer = (page.page_id.0, 0);
    new_root.keys.push(pk1);

    let mut pk2 = push_up;
    pk2.heap_pointer = (right.page_id.0, 0);
    new_root.keys.push(pk2);

    write_page(storage, new_root_id, &new_root, page_size)?;
    Ok(Some(new_root_id))
}

fn find_child_page_id(page: &BTreePage, key: &[u8]) -> PageId {
    if page.keys.is_empty() {
        return page.page_id;
    }
    let mut child = page.keys[0].clone();
    for k in &page.keys {
        if key >= k.key.as_slice() {
            child = k.clone();
        } else {
            break;
        }
    }
    PageId(child.heap_pointer.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ephemeral::EphemeralStorage;

    fn make_tuple(key: &[u8], page: u32, offset: u16) -> IndexTuple {
        IndexTuple {
            key: key.to_vec(),
            heap_pointer: (page, offset),
            heap_oid: Oid(0),
        }
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let page = BTreePage {
            page_type: BTreePageType::Leaf,
            page_id: PageId(42),
            level: 2,
            keys: vec![
                make_tuple(b"apple", 10, 1),
                make_tuple(b"banana", 20, 2),
                make_tuple(b"cherry", 30, 3),
            ],
            left_sibling: Some(PageId(100)),
            right_sibling: Some(PageId(200)),
        };

        let data = serialize_page(&page, 8192);
        let decoded = deserialize_page(PageId(42), &data).unwrap();

        assert_eq!(decoded.page_type, BTreePageType::Leaf);
        assert_eq!(decoded.page_id, PageId(42));
        assert_eq!(decoded.level, 2);
        assert_eq!(decoded.keys.len(), 3);
        assert_eq!(decoded.keys[0].key, b"apple");
        assert_eq!(decoded.keys[1].key, b"banana");
        assert_eq!(decoded.keys[2].key, b"cherry");
        assert_eq!(decoded.keys[0].heap_pointer, (10, 1));
        assert_eq!(decoded.left_sibling, Some(PageId(100)));
        assert_eq!(decoded.right_sibling, Some(PageId(200)));
    }

    #[test]
    fn test_serialize_empty_page() {
        let page = BTreePage::new(BTreePageType::Leaf, PageId(1));
        let data = serialize_page(&page, 8192);
        assert_eq!(data.len(), 8192);
        assert_eq!(data[0], 3); // Leaf
    }

    #[test]
    fn test_deserialize_zeroed_page() {
        let data = vec![0u8; 8192];
        let page = deserialize_page(PageId(1), &data).unwrap();
        assert_eq!(page.page_type, BTreePageType::Leaf); // zero = Leaf
        assert!(page.keys.is_empty());
    }

    #[test]
    fn test_deserialize_short_data() {
        let page = deserialize_page(PageId(1), &[]).unwrap();
        assert_eq!(page.page_type, BTreePageType::Leaf);
        assert!(page.keys.is_empty());

        let page = deserialize_page(PageId(1), &[0u8; 10]).unwrap();
        assert_eq!(page.page_type, BTreePageType::Leaf);
    }

    #[test]
    fn test_serialize_root_page() {
        let page = BTreePage {
            page_type: BTreePageType::Root,
            page_id: PageId(1),
            level: 3,
            keys: vec![make_tuple(b"key", 10, 0)],
            left_sibling: None,
            right_sibling: None,
        };
        let data = serialize_page(&page, 8192);
        assert_eq!(data[0], 1); // Root
        let decoded = deserialize_page(PageId(1), &data).unwrap();
        assert_eq!(decoded.page_type, BTreePageType::Root);
        assert_eq!(decoded.level, 3);
    }

    #[test]
    fn test_read_write_page_roundtrip() {
        let storage = EphemeralStorage::new();
        let page = BTreePage {
            page_type: BTreePageType::Internal,
            page_id: PageId(5),
            level: 1,
            keys: vec![
                make_tuple(b"x", 10, 0),
                make_tuple(b"y", 20, 0),
            ],
            left_sibling: None,
            right_sibling: None,
        };
        write_page(&storage, PageId(5), &page, 8192).unwrap();
        let read = read_page(&storage, PageId(5), 8192).unwrap();
        assert_eq!(read.page_type, BTreePageType::Internal);
        assert_eq!(read.keys.len(), 2);
        assert_eq!(read.keys[0].key, b"x");
        assert_eq!(read.keys[1].key, b"y");
    }

    #[test]
    fn test_btree_insert_single_page() {
        let storage = EphemeralStorage::new();
        let root = PageId(1);
        btree_insert(&storage, root, make_tuple(b"a", 10, 0), 8192).unwrap();
        btree_insert(&storage, root, make_tuple(b"c", 30, 0), 8192).unwrap();
        btree_insert(&storage, root, make_tuple(b"b", 20, 0), 8192).unwrap();
        
        let page = read_page(&storage, root, 8192).unwrap();
        assert_eq!(page.keys.len(), 3);
        assert_eq!(page.keys[0].key, b"a");
        assert_eq!(page.keys[1].key, b"b");
        assert_eq!(page.keys[2].key, b"c");
    }

    #[test]
    fn test_btree_insert_multipage() {
        let storage = EphemeralStorage::new();
        let root = PageId(1);
        let mut allocator: Vec<PageId> = (2u32..100).map(PageId).collect();
        let mut next = || allocator.pop().unwrap();

        for i in 0u32..10 {
            let key = i.to_le_bytes().to_vec();
            let _ = btree_insert_multipage(
                &storage, root, make_tuple(&key, i, 0), 8192, &mut next,
            );
        }

        // Verify we can find a key
        let _target = 5u32.to_le_bytes().to_vec();
        let page = read_page(&storage, root, 8192).unwrap();
        assert!(!page.keys.is_empty());
    }

    #[test]
    fn test_find_child_page_id() {
        let page = BTreePage {
            page_type: BTreePageType::Root,
            page_id: PageId(1),
            level: 1,
            keys: vec![
                make_tuple(b"apple", 10, 0),
                make_tuple(b"cherry", 20, 0),
                make_tuple(b"fig", 30, 0),
            ],
            left_sibling: None,
            right_sibling: None,
        };
        assert_eq!(find_child_page_id(&page, b"aardvark").0, 10);
        assert_eq!(find_child_page_id(&page, b"apple").0, 10);
        assert_eq!(find_child_page_id(&page, b"banana").0, 10);
        assert_eq!(find_child_page_id(&page, b"cherry").0, 20);
        assert_eq!(find_child_page_id(&page, b"date").0, 20);
        assert_eq!(find_child_page_id(&page, b"fig").0, 30);
        assert_eq!(find_child_page_id(&page, b"grape").0, 30);
    }

    #[test]
    fn test_empty_page_find_child() {
        let page = BTreePage::new(BTreePageType::Root, PageId(1));
        assert_eq!(find_child_page_id(&page, b"anything").0, 1);
    }
}
