use crate::types::{PageId, Tuple, TupleDesc, Oid};
use crate::buffer_cache::SharedBufferCache;
use crate::storage::StorageTrait;

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

fn is_visible(tup: &Tuple) -> bool {
    tup.xmin != 0 && tup.xmax == 0
}

pub async fn tuple_insert(
    cache: &SharedBufferCache,
    wal: &crate::wal::WAL,
    op: &TupleInsert,
) -> anyhow::Result<()> {
    let (page_id, _encoded, xid) = {
        let state = cache.get_relation_state(op.rel_oid).ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
        let mut rel_state = state.lock();
        let mut rel = rel_state.relation.clone();

        let page_id = if rel.pages.is_empty() {
            let new_page = PageId(1);
            rel.pages.push(new_page);
            cache.storage.write_page(new_page, &vec![0u8; 8192])?;
            new_page
        } else {
            *rel.pages.last().unwrap()
        };

        let mut page = cache.storage.read_page(page_id)?;
        let mut data = Vec::new();
        for (i, val) in op.values.iter().enumerate() {
            if i > 0 {
                data.push(0);
            }
            data.extend_from_slice(val);
        }
        let xid = wal.allocate_xid();
        let tup = Tuple { 
            slots: (0..op.values.len()).map(|i| crate::types::SlotId(i as u16)).collect(), 
            data,
            xmin: xid,
            xmax: 0,
            cmin: 0,
            cmax: 0,
            xvac: 0,
        };
        let encoded: Vec<u8> = bincode::serialize(&tup)?;
        
        // Find end of existing data by scanning forward
        let mut end_offset = 0;
        while end_offset + 4 <= page.len() {
            let len = u32::from_le_bytes([page[end_offset], page[end_offset+1], page[end_offset+2], page[end_offset+3]]) as usize;
            if len == 0 || end_offset + 4 + len > page.len() {
                break;
            }
            end_offset += 4 + len;
        }
        
        // Write length prefix + tuple data
        let len_bytes = (encoded.len() as u32).to_le_bytes();
        page[end_offset..end_offset + 4].copy_from_slice(&len_bytes);
        page[end_offset + 4..end_offset + 4 + encoded.len()].copy_from_slice(&encoded);
        
        cache.storage.write_page(page_id, &page)?;
        rel_state.dirty_buffers.push(page_id);
        rel_state.relation = rel;
        
        (page_id, encoded, tup)
    };

    wal.append(&crate::wal::WALRecord::Insert { rel_oid: op.rel_oid, page_id, tuple: xid }).await?;
    Ok(())
}

pub async fn tuple_insert_bulk(
    cache: &SharedBufferCache,
    wal: &crate::wal::WAL,
    op: &TupleInsertBulk,
) -> anyhow::Result<()> {
    for values in &op.tuples {
        tuple_insert(cache, wal, &TupleInsert { rel_oid: op.rel_oid, values: values.clone() }).await?;
    }
    Ok(())
}

pub async fn heap_scan(cache: &SharedBufferCache, rel_oid: u32) -> anyhow::Result<Vec<(crate::types::ItemPointerData, Vec<String>)>> {
    let state = cache.get_relation_state(Oid(rel_oid)).ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
    let rel_state = state.lock();
    let rel = &rel_state.relation;

    let mut rows = Vec::new();
    for (_page_idx, &page_id) in rel.pages.iter().enumerate() {
        let page = cache.storage.read_page(page_id)?;
        let mut offset = 0;
        while offset + 4 <= page.len() {
            let len = u32::from_le_bytes([page[offset], page[offset+1], page[offset+2], page[offset+3]]) as usize;
            if len == 0 || offset + 4 + len > page.len() {
                break;
            }
            if let Ok(tup) = bincode::deserialize::<Tuple>(&page[offset+4..offset+4+len]) {
                if !is_visible(&tup) {
                    offset += 4 + len;
                    continue;
                }
                let tid = crate::types::ItemPointerData {
                    page_id,
                    offset: offset as u16,
                };
                let values = decode_tuple_values(&tup, &rel.tuple_desc);
                rows.push((tid, values));
            }
            offset += 4 + len;
        }
    }
    Ok(rows)
}

pub async fn slow_scan(cache: &SharedBufferCache, op: &SlowScan) -> anyhow::Result<Vec<(crate::types::ItemPointerData, Vec<String>)>> {
    let state = cache.get_relation_state(op.rel_oid).ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
    let rel_state = state.lock();
    let rel = &rel_state.relation;

    let mut rows = Vec::new();
    for (_page_idx, &page_id) in rel.pages.iter().enumerate() {
        let page = cache.storage.read_page(page_id)?;
        let mut offset = 0;
        while offset + 4 <= page.len() {
            let len = u32::from_le_bytes([page[offset], page[offset+1], page[offset+2], page[offset+3]]) as usize;
            if len == 0 || offset + 4 + len > page.len() {
                break;
            }
            if let Ok(tup) = bincode::deserialize::<Tuple>(&page[offset+4..offset+4+len]) {
                if !is_visible(&tup) {
                    offset += 4 + len;
                    continue;
                }
                let row = decode_tuple_values(&tup, &rel.tuple_desc);
                
                if let Some(filter) = &op.filter {
                    let filter_col = filter.column as usize;
                    if filter_col < row.len() {
                        let expected = String::from_utf8_lossy(&filter.value);
                        if !row[filter_col].contains(&*expected) && row[filter_col] != expected.to_string() {
                            offset += 4 + len;
                            continue;
                        }
                    }
                }
                let tid = crate::types::ItemPointerData {
                    page_id,
                    offset: offset as u16,
                };
                rows.push((tid, row));
            }
            offset += 4 + len;
        }
    }
    Ok(rows)
}

fn decode_tuple_values(tup: &Tuple, desc: &TupleDesc) -> Vec<String> {
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
            let val = std::str::from_utf8(&tup.data[start..pos]).unwrap_or_default().to_string();
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
        } else if i < tup.slots.len() {
            values.push(String::new());
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

    let root_page = (1u32..=4096u32)
        .find_map(|pid| {
            storage
                .read_page(PageId(pid))
                .ok()
                .and_then(|data| {
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

    let _ = walk_btree(cache.storage.as_ref(), root, search_key, page_size, &mut results);

    Ok(results)
}

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
        crate::btree::page::BTreePageType::Root
        | crate::btree::page::BTreePageType::Internal => {
            let mut descended = false;
            for (key, child_page, _) in &entries {
                if search_key.is_empty() || search_key <= key.as_slice() {
                    let _ = walk_btree(
                        storage,
                        PageId(*child_page),
                        search_key,
                        page_size,
                        results,
                    );
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
    let state = cache.get_relation_state(rel_oid).ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
    let pages;
    let tuple_desc;
    {
        let rel_state = state.lock();
        pages = rel_state.relation.pages.clone();
        tuple_desc = rel_state.relation.tuple_desc.clone();
    }

    for &page_id in &pages {
        let page = cache.storage.read_page(page_id)?;
        let mut new_page = page.clone();
        let mut offset = 0;
        
        // First pass: find all tuples to update and mark them as deleted
        let mut tuples_to_update = Vec::new();
        while offset + 4 <= page.len() {
            let old_len = u32::from_le_bytes([page[offset], page[offset+1], page[offset+2], page[offset+3]]) as usize;
            if old_len == 0 || offset + 4 + old_len > page.len() {
                break;
            }
            if let Ok(tup) = bincode::deserialize::<Tuple>(&page[offset+4..offset+4+old_len]) {
                let row = decode_tuple_values(&tup, &tuple_desc);
                let should_update = filter.as_ref().map_or(true, |f| {
                    let filter_col = f.column;
                    filter_col < row.len() && row[filter_col] == String::from_utf8_lossy(&f.value).to_string()
                });
                
                if should_update {
                    if let Some(new_data) = build_updated_data(&tup.data, column_idx, new_value) {
                        tuples_to_update.push((offset, old_len, tup.clone(), new_data));
                    }
                }
            }
            offset += 4 + old_len;
        }
        
        // Second pass: apply updates
        for (old_offset, old_len, mut tup, new_data) in tuples_to_update {
            let xid = wal.allocate_xid();
            
            // Mark old tuple as deleted
            tup.xmax = xid;
            let old_encoded = bincode::serialize(&tup)?;
            new_page[old_offset + 4..old_offset + 4 + old_len].copy_from_slice(&old_encoded);
            
            // Insert new tuple version at end of page
            let mut end_offset = 0;
            while end_offset + 4 <= new_page.len() {
                let len = u32::from_le_bytes([new_page[end_offset], new_page[end_offset+1], new_page[end_offset+2], new_page[end_offset+3]]) as usize;
                if len == 0 || end_offset + 4 + len > new_page.len() {
                    break;
                }
                end_offset += 4 + len;
            }
            
            // Create new tuple with the updated data
            let mut new_tup = tup.clone();
            new_tup.xmin = xid;
            new_tup.xmax = 0;
            new_tup.data = new_data;
            let new_encoded = bincode::serialize(&new_tup)?;
            
            // Write length prefix + new tuple
            let len_bytes = (new_encoded.len() as u32).to_le_bytes();
            new_page[end_offset..end_offset + 4].copy_from_slice(&len_bytes);
            new_page[end_offset + 4..end_offset + 4 + new_encoded.len()].copy_from_slice(&new_encoded);
            
            cache.storage.write_page(page_id, &new_page)?;
            {
                let mut rel_state = state.lock();
                rel_state.dirty_buffers.push(page_id);
            }
            
            wal.append(&crate::wal::WALRecord::Update {
                rel_oid,
                page_id,
                old_tuple: tup,
                new_tuple: new_tup,
            }).await?;
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
    let state = cache.get_relation_state(rel_oid).ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
    let pages;
    let tuple_desc;
    {
        let rel_state = state.lock();
        pages = rel_state.relation.pages.clone();
        tuple_desc = rel_state.relation.tuple_desc.clone();
    }

    for &page_id in &pages {
        let page = cache.storage.read_page(page_id)?;
        let mut new_page = page.clone();
        let mut offset = 0;
        
        // First pass: find all tuples to delete
        let mut tuples_to_delete = Vec::new();
        while offset + 4 <= page.len() {
            let old_len = u32::from_le_bytes([page[offset], page[offset+1], page[offset+2], page[offset+3]]) as usize;
            if old_len == 0 || offset + 4 + old_len > page.len() {
                break;
            }
            if let Ok(tup) = bincode::deserialize::<Tuple>(&page[offset+4..offset+4+old_len]) {
                let row = decode_tuple_values(&tup, &tuple_desc);
                let should_delete = filter.as_ref().map_or(true, |f| {
                    let filter_col = f.column;
                    filter_col < row.len() && row[filter_col] == String::from_utf8_lossy(&f.value).to_string()
                });
                
                if should_delete {
                    tuples_to_delete.push((offset, old_len, tup.clone()));
                }
            }
            offset += 4 + old_len;
        }
        
        // Second pass: mark tuples as deleted
        for (del_offset, del_len, mut tup) in tuples_to_delete {
            let xid = wal.allocate_xid();
            tup.xmax = xid;
            let encoded = bincode::serialize(&tup)?;
            
            // Overwrite the tuple in place (keep same length, just update xmax)
            new_page[del_offset + 4..del_offset + 4 + del_len].copy_from_slice(&encoded);
            
            cache.storage.write_page(page_id, &new_page)?;
            {
                let mut rel_state = state.lock();
                rel_state.dirty_buffers.push(page_id);
            }
            
            wal.append(&crate::wal::WALRecord::Delete {
                rel_oid,
                page_id,
                tuple: tup,
            }).await?;
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
