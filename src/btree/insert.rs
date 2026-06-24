use crate::types::{PageId, Oid};
use crate::btree::page::{BTreePage, BTreePageType, IndexTuple};
use crate::storage::StorageTrait;
use anyhow::{bail, Context};

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
    )?;

    match result {
        Some(new_root_page_id) => Ok(new_root_page_id),
        None => Ok(root_page),
    }
}

/// Recursively descends and inserts.
///
/// Returns `Some(new_root_id)` if a new root was created (root split),
/// `Some(new_page_id)` if a child was split (caller must update its child pointer),
/// or `None` if the page was simply modified in place.
fn insert_recursive(
    storage: &dyn StorageTrait,
    page_id: PageId,
    tuple: &IndexTuple,
    page_size: usize,
    next_page_id: &mut dyn FnMut() -> PageId,
) -> anyhow::Result<Option<PageId>> {
    let mut page = read_page(storage, page_id, page_size)?;

    if page.page_type == BTreePageType::Leaf || page.keys.is_empty() {
        // ── Leaf (or single-page fallback) ──────────────────────────────
        if page.is_full(page_size) {
            let split_pos = page.keys.len() / 2;
            let mut right = page.split_at(split_pos);
            right.page_id = (next_page_id)();
            right.level = page.level;

            // Decide which half takes the new tuple
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
            page.page_type = BTreePageType::Internal;
            right.page_type = BTreePageType::Internal;

            write_page(storage, page_id, &page, page_size)?;
            write_page(storage, right.page_id, &right, page_size)?;

            // Build new root keyed on the promoted separator
            let new_root_id = PageId((next_page_id)());
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
                let mut rk = IndexTuple {
                    key: vec![],
                    heap_pointer: (right.page_id.0, 0),
                    heap_oid: Oid(0),
                };
                new_root.keys.push(rk);
            }

            write_page(storage, new_root_id, &new_root, page_size)?;
            Ok(Some(new_root_id))
        } else {
            page.insert_sorted(tuple.clone());
            write_page(storage, page_id, &page, page_size)?;
            Ok(None)
        }
    } else {
        // ── Internal node ────────────────────────────────────────────────
        let child_page_id = find_child_page_id(&page, &tuple.key);

        match insert_recursive(storage, child_page_id, tuple, page_size, next_page_id)? {
            None => Ok(None),
            Some(new_child_id) => {
                // Child was split; update or split this internal node
                let mut updated = read_page(storage, page_id, page_size)?;

                // Update the pointer for the split child
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
    right.page_id = PageId((next_page_id)());
    right.level = page.level;
    right.left_sibling = Some(page.page_id);

    // right.keys[0] held the child-pointer just below the promoted separator.
    // That pointer was to the now-split child; update it to the new right sibling.
    if let Some(k) = right.keys.get_mut(0) {
        k.heap_pointer = (updated_child_id.0, 0);
    }
    page.page_type = BTreePageType::Internal;
    right.page_type = BTreePageType::Internal;

    write_page(storage, page.page_id, &page, page_size)?;
    write_page(storage, right.page_id, &right, page_size)?;

    let new_root_id = PageId((next_page_id)());
    let mut new_root = BTreePage::new(BTreePageType::Root, new_root_id);
    new_root.level = page.level + 1;

    let push_up = right.keys[0].clone(); // this is the separator key
    let mut pk1 = push_up.clone();
    pk1.heap_pointer = (page.page_id.0, 0);
    new_root.keys.push(pk1);

    // pk2 is the same separator key, but the left-pointer (its heap_pointer)
    // should point to right (which in turn has the updated child pointer in its key[0]).
    // So pk2.heap_pointer points to the right page itself.
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
    // Every non-leaf key at index i stores its left child in heap_pointer.
    // Navigate to the rightmost key that is <= search_key; its heap_pointer
    // is the child pointer to follow.
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
