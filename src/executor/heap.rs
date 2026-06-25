use crate::buffer_cache::SharedBufferCache;
use crate::storage::StorageTrait;
use crate::transaction::Snapshot;
use crate::types::{Oid, PageId, Tuple, TupleDesc};

pub struct TupleInsert {
    pub rel_oid: Oid,
    pub values: Vec<Vec<u8>>,
}

pub struct TupleInsertBulk {
    pub rel_oid: Oid,
    pub tuples: Vec<Vec<Vec<u8>>>,
}

pub struct HeapScan {
    pub rel_oid: Oid,
}

pub struct SlowScan {
    pub rel_oid: Oid,
    pub filter: Option<Filter>,
}

pub struct FilterScan {
    pub rel_oid: Oid,
    pub filter: String,
}

pub struct Filter {
    pub column: usize,
    pub value: Vec<u8>,
}

pub fn is_visible(tup: &Tuple, snapshot: &Snapshot) -> bool {
    if tup.xmin == 0 {
        return false;
    }

    let xmin = crate::transaction::TransactionId(tup.xmin as u32);
    let xmax = if tup.xmax != 0 {
        Some(crate::transaction::TransactionId(tup.xmax as u32))
    } else {
        None
    };

    let xmin_visible = !snapshot.active_xids.contains(&xmin) && xmin.0 < snapshot.xid.0;
    if !xmin_visible {
        return false;
    }

    if let Some(xmax) = xmax {
        if !snapshot.active_xids.contains(&xmax) && xmax.0 < snapshot.xid.0 {
            return false;
        }
    }

    true
}

pub async fn tuple_insert(
    cache: &SharedBufferCache,
    wal: &crate::wal::WAL,
    op: &TupleInsert,
) -> anyhow::Result<()> {
    let (page_id, _encoded, xid) = {
        let state = cache
            .get_relation_state(op.rel_oid)
            .ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
        let mut rel_state = state.lock();
        let mut rel = rel_state.relation.clone();

        let page_id = if rel.pages.is_empty() {
            let new_page = PageId(1);
            rel.pages.push(new_page);
            let new_heap_page = crate::storage::heap_page::HeapPage::new();
            cache
                .storage
                .write_page(new_page, &new_heap_page.serialize())?;
            new_page
        } else {
            *rel.pages.last().unwrap()
        };

        let page_data = cache.storage.read_page(page_id)?;
        let mut heap_page = crate::storage::heap_page::HeapPage::deserialize(&page_data);

        let mut data = Vec::new();
        for (i, val) in op.values.iter().enumerate() {
            if i > 0 {
                data.push(0);
            }
            data.extend_from_slice(val);
        }
        let xid = wal.allocate_xid();
        let tup = Tuple {
            slots: (0..op.values.len())
                .map(|i| crate::types::SlotId(i as u16))
                .collect(),
            data,
            xmin: xid,
            xmax: 0,
            cmin: 0,
            cmax: 0,
            xvac: 0,
        };
        let encoded: Vec<u8> = bincode::serialize(&tup)?;

        // Add tuple to heap page
        if heap_page.add_tuple(&encoded).is_none() {
            // Page is full, create a new page
            let new_page_id = PageId(page_id.0 + 1);
            rel.pages.push(new_page_id);
            let mut new_heap_page = crate::storage::heap_page::HeapPage::new();
            new_heap_page.add_tuple(&encoded);
            cache
                .storage
                .write_page(new_page_id, &new_heap_page.serialize())?;
            cache.storage.write_page(page_id, &heap_page.serialize())?;
            rel_state.dirty_buffers.push(new_page_id);
            rel_state.relation = rel;
            (new_page_id, encoded, tup)
        } else {
            cache.storage.write_page(page_id, &heap_page.serialize())?;
            rel_state.dirty_buffers.push(page_id);
            rel_state.relation = rel;
            (page_id, encoded, tup)
        }
    };

    wal.append(&crate::wal::WALRecord::Insert {
        rel_oid: op.rel_oid,
        page_id,
        tuple: xid,
    })
    .await?;
    Ok(())
}

pub async fn tuple_insert_bulk(
    cache: &SharedBufferCache,
    wal: &crate::wal::WAL,
    op: &TupleInsertBulk,
) -> anyhow::Result<()> {
    for values in &op.tuples {
        tuple_insert(
            cache,
            wal,
            &TupleInsert {
                rel_oid: op.rel_oid,
                values: values.clone(),
            },
        )
        .await?;
    }
    Ok(())
}

pub async fn heap_scan(
    cache: &SharedBufferCache,
    rel_oid: u32,
) -> anyhow::Result<Vec<(crate::types::ItemPointerData, Vec<String>)>> {
    heap_scan_with_optional_snapshot(cache, rel_oid, None).await
}

pub async fn heap_scan_with_optional_snapshot(
    cache: &SharedBufferCache,
    rel_oid: u32,
    snapshot: Option<&crate::transaction::Snapshot>,
) -> anyhow::Result<Vec<(crate::types::ItemPointerData, Vec<String>)>> {
    let state = cache
        .get_relation_state(Oid(rel_oid))
        .ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
    let rel_state = state.lock();
    let rel = &rel_state.relation;

    let default_snapshot = Snapshot {
        xid: crate::transaction::TransactionId(u32::MAX),
        active_xids: vec![],
    };
    let snap = snapshot.unwrap_or(&default_snapshot);

    let mut rows = Vec::new();
    for &page_id in rel.pages.iter() {
        let page_data = cache.fetch_page(page_id)?;
        let page = page_data.lock();
        let heap_page = crate::storage::heap_page::HeapPage::deserialize(&page.data);

        for (slot_idx, tuple_data) in heap_page.tuples.iter().enumerate() {
            if let Ok(tup) = bincode::deserialize::<Tuple>(tuple_data) {
                if !is_visible(&tup, snap) {
                    continue;
                }
                let tid = crate::types::ItemPointerData {
                    page_id,
                    offset: slot_idx as u16,
                };
                let values = decode_tuple_values(&tup, &rel.tuple_desc);
                rows.push((tid, values));
            }
        }
    }
    Ok(rows)
}

pub async fn heap_scan_with_snapshot(
    cache: &SharedBufferCache,
    rel_oid: u32,
    snapshot: &Snapshot,
) -> anyhow::Result<Vec<(crate::types::ItemPointerData, Vec<String>)>> {
    let state = cache
        .get_relation_state(Oid(rel_oid))
        .ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
    let rel_state = state.lock();
    let rel = &rel_state.relation;

    let mut rows = Vec::new();
    for &page_id in rel.pages.iter() {
        let page_data = cache.fetch_page(page_id)?;
        let page = page_data.lock();
        let heap_page = crate::storage::heap_page::HeapPage::deserialize(&page.data);

        for (slot_idx, tuple_data) in heap_page.tuples.iter().enumerate() {
            if let Ok(tup) = bincode::deserialize::<Tuple>(tuple_data) {
                if !is_visible(&tup, snapshot) {
                    continue;
                }
                let tid = crate::types::ItemPointerData {
                    page_id,
                    offset: slot_idx as u16,
                };
                let values = decode_tuple_values(&tup, &rel.tuple_desc);
                rows.push((tid, values));
            }
        }
    }
    Ok(rows)
}

pub async fn slow_scan(
    cache: &SharedBufferCache,
    op: &SlowScan,
) -> anyhow::Result<Vec<(crate::types::ItemPointerData, Vec<String>)>> {
    let state = cache
        .get_relation_state(op.rel_oid)
        .ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
    let rel_state = state.lock();
    let rel = &rel_state.relation;

    let snapshot = Snapshot {
        xid: crate::transaction::TransactionId(u32::MAX),
        active_xids: vec![],
    };

    let mut rows = Vec::new();
    for &page_id in rel.pages.iter() {
        let page_data = cache.storage.read_page(page_id)?;
        let heap_page = crate::storage::heap_page::HeapPage::deserialize(&page_data);

        for (slot_idx, tuple_data) in heap_page.tuples.iter().enumerate() {
            if let Ok(tup) = bincode::deserialize::<Tuple>(tuple_data) {
                if !is_visible(&tup, &snapshot) {
                    continue;
                }
                let row = decode_tuple_values(&tup, &rel.tuple_desc);

                if let Some(filter) = &op.filter {
                    let filter_col = filter.column;
                    if filter_col < row.len() {
                        let expected = String::from_utf8_lossy(&filter.value);
                        if !row[filter_col].contains(&*expected)
                            && row[filter_col] != expected
                        {
                            continue;
                        }
                    }
                }
                let tid = crate::types::ItemPointerData {
                    page_id,
                    offset: slot_idx as u16,
                };
                rows.push((tid, row));
            }
        }
    }
    Ok(rows)
}

pub(crate) fn decode_tuple_values(tup: &Tuple, desc: &TupleDesc) -> Vec<String> {
    if tup.data.is_empty() {
        return vec![String::new(); desc.fields.len()];
    }

    let mut values = Vec::new();
    let mut pos = 0;
    for (i, _field) in desc.fields.iter().enumerate() {
        if i < tup.slots.len() && pos < tup.data.len() {
            let start = pos;
            while pos < tup.data.len() && tup.data[pos] != 0 {
                pos += 1;
            }
            let val = std::str::from_utf8(&tup.data[start..pos])
                .unwrap_or_default()
                .to_string();
            values.push(val);
            // Safety check: reached end without null byte and not last field
            if pos == tup.data.len() {
                if i < desc.fields.len() - 1 {
                    values.resize(desc.fields.len(), String::new());
                    break;
                }
            } else {
                pos += 1;
            }
        } else {
            values.push(String::new());
        }
    }
    values
}

pub async fn index_scan(
    cache: &SharedBufferCache,
    _index_oid: u32,
    scan_key: Vec<u8>,
) -> anyhow::Result<Vec<(u32, u16)>> {
    let storage = &cache.storage;
    let page_size: usize = 8192;

    let root_page = (1u32..=4096u32).find_map(|pid| {
        storage.read_page(PageId(pid)).ok().and_then(|data| {
            if !data.is_empty() && data[0] == 1 {
                Some(PageId(pid))
            } else {
                None
            }
        })
    });

    let Some(root) = root_page else {
        return Ok(vec![]);
    };

    let search_key: &[u8] = if scan_key.is_empty() { &[] } else { &scan_key };
    let mut results: Vec<(u32, u16)> = Vec::new();

    let _ = walk_btree(
        cache.storage.as_ref(),
        root,
        search_key,
        page_size,
        &mut results,
    );

    Ok(results)
}

#[allow(clippy::only_used_in_recursion)]
fn walk_btree(
    storage: &dyn StorageTrait,
    page_id: PageId,
    search_key: &[u8],
    page_size: usize,
    results: &mut Vec<(u32, u16)>,
) -> anyhow::Result<()> {
    let data = match storage.read_page(page_id) {
        Ok(d) if !d.is_empty() && d.len() >= 64 => d,
        _ => return Ok(()),
    };

    let page_type = match data[0] {
        0 => crate::btree::page::BTreePageType::Meta,
        1 => crate::btree::page::BTreePageType::Root,
        2 => crate::btree::page::BTreePageType::Internal,
        3 => crate::btree::page::BTreePageType::Leaf,
        _ => return Ok(()),
    };

    let mut entries: Vec<(Vec<u8>, u32, u16)> = Vec::new();
    let mut pos: usize = 64;

    while pos + 4 + 6 <= data.len() {
        let key_len =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        if key_len == 0 || pos + 4 + key_len + 6 > data.len() {
            break;
        }
        let key_start = pos + 4;
        let key = data[key_start..key_start + key_len].to_vec();
        let ptr = key_start + key_len;
        let heap_page =
            u32::from_le_bytes([data[ptr], data[ptr + 1], data[ptr + 2], data[ptr + 3]]);
        let heap_offset = u16::from_le_bytes([data[ptr + 4], data[ptr + 5]]);
        entries.push((key, heap_page, heap_offset));
        pos = ptr + 6;
    }

    match page_type {
        crate::btree::page::BTreePageType::Leaf => {
            for (key, page, offset) in entries {
                if search_key.is_empty() || key == search_key {
                    results.push((page, offset));
                }
            }
        }
        crate::btree::page::BTreePageType::Root | crate::btree::page::BTreePageType::Internal => {
            let mut descended = false;
            for (key, child_page, _) in &entries {
                if search_key.is_empty() || search_key <= key.as_slice() {
                    let _ =
                        walk_btree(storage, PageId(*child_page), search_key, page_size, results);
                    descended = true;
                    break;
                }
            }
            if !descended {
                if let Some((_, child_page, _)) = entries.last() {
                    let _ =
                        walk_btree(storage, PageId(*child_page), search_key, page_size, results);
                }
            }
        }
        _ => {}
    }

    Ok(())
}

pub async fn tuple_update(
    cache: &SharedBufferCache,
    wal: &crate::wal::WAL,
    rel_oid: Oid,
    column_idx: usize,
    new_value: &[u8],
    filter: Option<Filter>,
) -> anyhow::Result<u64> {
    let mut updated = 0u64;
    let state = cache
        .get_relation_state(rel_oid)
        .ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
    let pages;
    let tuple_desc;
    {
        let rel_state = state.lock();
        pages = rel_state.relation.pages.clone();
        tuple_desc = rel_state.relation.tuple_desc.clone();
    }

    for &page_id in &pages {
        let page_data = cache.storage.read_page(page_id)?;
        let mut heap_page = crate::storage::heap_page::HeapPage::deserialize(&page_data);

        let mut tuples_to_update = Vec::new();
        for (slot_idx, tuple_data) in heap_page.tuples.iter().enumerate() {
            if let Ok(tup) = bincode::deserialize::<Tuple>(tuple_data) {
                let row = decode_tuple_values(&tup, &tuple_desc);
                let should_update = filter.as_ref().is_none_or(|f| {
                    let filter_col = f.column;
                    filter_col < row.len()
                        && row[filter_col] == String::from_utf8_lossy(&f.value)
                });

                if should_update {
                    if let Some(new_data) = build_updated_data(&tup.data, column_idx, new_value) {
                        tuples_to_update.push((slot_idx, tup.clone(), new_data));
                    }
                }
            }
        }

        for (slot_idx, mut tup, new_data) in tuples_to_update {
            let xid = wal.allocate_xid();

            // Try HOT update first: if new tuple fits on same page, use LP_REDIRECT chain
            if let Some(new_slot) = heap_page.hot_update(
                slot_idx as u16,
                &bincode::serialize(&{
                    let mut new_tup = tup.clone();
                    new_tup.xmin = xid;
                    new_tup.xmax = 0;
                    new_tup.data = new_data.clone();
                    new_tup
                })?,
            ) {
                tup.xmax = xid;
                let old_encoded = bincode::serialize(&tup)?;
                heap_page.tuples[slot_idx] = old_encoded;

                cache.storage.write_page(page_id, &heap_page.serialize())?;
                {
                    let mut rel_state = state.lock();
                    rel_state.dirty_buffers.push(page_id);
                }
                wal.append(&crate::wal::WALRecord::Update {
                    rel_oid,
                    page_id,
                    old_tuple: tup,
                    new_tuple: bincode::deserialize(&heap_page.tuples[new_slot as usize])?,
                })
                .await?;
                updated += 1;
                continue;
            }

            // Fallback: mark old tuple as deleted, append new version
            tup.xmax = xid;
            let old_encoded = bincode::serialize(&tup)?;
            heap_page.tuples[slot_idx] = old_encoded;

            // Create new tuple version
            let mut new_tup = tup.clone();
            new_tup.xmin = xid;
            new_tup.xmax = 0;
            new_tup.data = new_data;
            let new_encoded = bincode::serialize(&new_tup)?;

            // Add new tuple to page
            heap_page.add_tuple(&new_encoded);

            cache.storage.write_page(page_id, &heap_page.serialize())?;
            {
                let mut rel_state = state.lock();
                rel_state.dirty_buffers.push(page_id);
            }

            wal.append(&crate::wal::WALRecord::Update {
                rel_oid,
                page_id,
                old_tuple: tup,
                new_tuple: new_tup,
            })
            .await?;
            updated += 1;
        }
    }
    Ok(updated)
}

pub async fn tuple_delete(
    cache: &SharedBufferCache,
    wal: &crate::wal::WAL,
    rel_oid: Oid,
    filter: Option<Filter>,
) -> anyhow::Result<u64> {
    let mut deleted = 0u64;
    let state = cache
        .get_relation_state(rel_oid)
        .ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
    let pages;
    let tuple_desc;
    {
        let rel_state = state.lock();
        pages = rel_state.relation.pages.clone();
        tuple_desc = rel_state.relation.tuple_desc.clone();
    }

    for &page_id in &pages {
        let page_data = cache.storage.read_page(page_id)?;
        let mut heap_page = crate::storage::heap_page::HeapPage::deserialize(&page_data);

        let mut tuples_to_delete = Vec::new();
        for (slot_idx, tuple_data) in heap_page.tuples.iter().enumerate() {
            if let Ok(tup) = bincode::deserialize::<Tuple>(tuple_data) {
                let row = decode_tuple_values(&tup, &tuple_desc);
                let should_delete = filter.as_ref().is_none_or(|f| {
                    let filter_col = f.column;
                    filter_col < row.len()
                        && row[filter_col] == String::from_utf8_lossy(&f.value)
                });

                if should_delete {
                    tuples_to_delete.push((slot_idx, tup.clone()));
                }
            }
        }

        for (slot_idx, mut tup) in tuples_to_delete {
            let xid = wal.allocate_xid();
            tup.xmax = xid;
            let encoded = bincode::serialize(&tup)?;
            heap_page.tuples[slot_idx] = encoded;

            cache.storage.write_page(page_id, &heap_page.serialize())?;
            {
                let mut rel_state = state.lock();
                rel_state.dirty_buffers.push(page_id);
            }

            wal.append(&crate::wal::WALRecord::Delete {
                rel_oid,
                page_id,
                tuple: tup,
            })
            .await?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

fn build_updated_data(old_data: &[u8], column_idx: usize, new_value: &[u8]) -> Option<Vec<u8>> {
    // Split old data by null bytes
    let mut parts: Vec<&[u8]> = old_data.split(|&b| b == 0).collect();
    if column_idx < parts.len() {
        parts[column_idx] = new_value;
        let mut new_data = Vec::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                new_data.push(0);
            }
            new_data.extend_from_slice(part);
        }
        Some(new_data)
    } else {
        None
    }
}

pub async fn vacuum_relation(
    cache: &SharedBufferCache,
    rel_oid: Oid,
    oldest_xmin: u32,
) -> anyhow::Result<u64> {
    let mut reclaimed_tuples = 0u64;
    let state = cache
        .get_relation_state(rel_oid)
        .ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
    let pages;
    {
        let rel_state = state.lock();
        pages = rel_state.relation.pages.clone();
    }

    for &page_id in &pages {
        let page_data = cache.storage.read_page(page_id)?;
        let mut heap_page = crate::storage::heap_page::HeapPage::deserialize(&page_data);
        let mut modified = false;

        for slot_idx in 0..heap_page.line_pointers.len() {
            if heap_page.line_pointers[slot_idx].lp_flags == crate::storage::heap_page::LP_NORMAL
                && slot_idx < heap_page.tuples.len() {
                    let tuple_data = &heap_page.tuples[slot_idx];
                    if let Ok(tup) = bincode::deserialize::<Tuple>(tuple_data) {
                        if tup.xmax != 0 && tup.xmax < oldest_xmin as u64 {
                            heap_page.line_pointers[slot_idx].lp_flags =
                                crate::storage::heap_page::LP_DEAD;
                            modified = true;
                            reclaimed_tuples += 1;
                        }
                    }
                }
        }

        if modified {
            heap_page.compact();
            cache.storage.write_page(page_id, &heap_page.serialize())?;
            // Invalidate the cached (stale) buffer so subsequent scans re-read from storage
            cache.invalidate_page(page_id);
            {
                let mut rel_state = state.lock();
                rel_state.dirty_buffers.push(page_id);
            }
        }
    }
    Ok(reclaimed_tuples)
}
