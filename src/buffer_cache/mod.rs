use crate::types::PageId;
use crate::storage::StorageTrait;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::Mutex;

pub struct Buffer {
    pub page_id: PageId,
    pub data: Vec<u8>,
    pub is_dirty: bool,
    pub usage_count: u32,
    pub pin_count: u32,
}

pub struct BufferPool {
    pool_size: usize,
    buffers: Mutex<Vec<Buffer>>,
    page_map: Mutex<HashMap<PageId, usize>>,
    next_victim: Mutex<usize>,
}

impl BufferPool {
    pub fn new(pool_size: usize) -> Self {
        let mut buffers = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            buffers.push(Buffer {
                page_id: PageId(0),
                data: vec![0u8; 8192],
                is_dirty: false,
                usage_count: 0,
                pin_count: 0,
            });
        }
        Self {
            pool_size,
            buffers: Mutex::new(buffers),
            page_map: Mutex::new(HashMap::new()),
            next_victim: Mutex::new(0),
        }
    }

    pub fn fetch_page(
        &self,
        storage: &dyn StorageTrait,
        page_id: PageId,
    ) -> anyhow::Result<usize> {
        let map = self.page_map.lock();
        if let Some(&idx) = map.get(&page_id) {
            drop(map);
            let mut buffers = self.buffers.lock();
            buffers[idx].usage_count += 1;
            buffers[idx].pin_count += 1;
            return Ok(idx);
        }
        drop(map);

        let mut buffers = self.buffers.lock();
        let slot = self.find_victim(&mut buffers);
        let data = storage.read_page(page_id)?;

        if buffers[slot].is_dirty && buffers[slot].pin_count == 0 {
            let old_page = buffers[slot].page_id;
            storage.write_page(old_page, &buffers[slot].data)?;
            let mut map = self.page_map.lock();
            map.remove(&old_page);
        }

        buffers[slot].page_id = page_id;
        buffers[slot].data = data;
        buffers[slot].is_dirty = false;
        buffers[slot].usage_count = 1;
        buffers[slot].pin_count = 1;

        let mut map = self.page_map.lock();
        map.insert(page_id, slot);
        Ok(slot)
    }

    pub fn pin_page(&self, page_id: PageId) -> Option<usize> {
        let map = self.page_map.lock();
        map.get(&page_id).copied().map(|idx| {
            drop(map);
            let mut buffers = self.buffers.lock();
            buffers[idx].pin_count += 1;
            buffers[idx].usage_count += 1;
            idx
        })
    }

    pub fn unpin_page(&self, page_id: PageId, is_dirty: bool) {
        let map = self.page_map.lock();
        if let Some(&idx) = map.get(&page_id) {
            drop(map);
            let mut buffers = self.buffers.lock();
            buffers[idx].pin_count = buffers[idx].pin_count.saturating_sub(1);
            if is_dirty {
                buffers[idx].is_dirty = true;
            }
        }
    }

    pub fn flush_page(&self, storage: &dyn StorageTrait, page_id: PageId) -> anyhow::Result<()> {
        let map = self.page_map.lock();
        if let Some(&idx) = map.get(&page_id) {
            drop(map);
            let mut buffers = self.buffers.lock();
            if buffers[idx].is_dirty {
                storage.write_page(page_id, &buffers[idx].data)?;
                buffers[idx].is_dirty = false;
            }
        }
        Ok(())
    }

    pub fn flush_all(&self, storage: &dyn StorageTrait) -> anyhow::Result<()> {
        let map = self.page_map.lock().clone();
        for (page_id, _) in map {
            self.flush_page(storage, page_id)?;
        }
        Ok(())
    }

    fn find_victim(&self, buffers: &mut [Buffer]) -> usize {
        let mut hand_lock = self.next_victim.lock();
        let mut hand = *hand_lock;
        let pool_size = self.pool_size;

        for _ in 0..(pool_size * 2) {
            let buf = &mut buffers[hand];
            if buf.pin_count == 0 {
                if buf.usage_count > 0 {
                    buf.usage_count -= 1;
                } else {
                    let victim = hand;
                    *hand_lock = (hand + 1) % pool_size;
                    return victim;
                }
            }
            hand = (hand + 1) % pool_size;
        }

        for (i, buf) in buffers.iter().enumerate() {
            if buf.pin_count == 0 {
                *hand_lock = (i + 1) % pool_size;
                return i;
            }
        }

        0
    }

    pub fn inspect(&self) -> Vec<String> {
        let buffers = self.buffers.lock();
        buffers.iter()
            .filter(|b| b.pin_count > 0 || b.usage_count > 0)
            .map(|b| format!("Page({}) usage={} pin={} dirty={}", b.page_id.0, b.usage_count, b.pin_count, b.is_dirty))
            .collect()
    }

    pub fn pool_size(&self) -> usize {
        self.pool_size
    }

    pub fn dirty_count(&self) -> usize {
        let buffers = self.buffers.lock();
        buffers.iter().filter(|b| b.is_dirty).count()
    }

    /// Invalidate (evict) a page from the pool so the next fetch re-reads from storage.
    pub fn invalidate_page(&self, page_id: PageId) {
        let mut map = self.page_map.lock();
        if let Some(idx) = map.remove(&page_id) {
            let mut buffers = self.buffers.lock();
            // Reset the slot so it is treated as empty
            buffers[idx].is_dirty = false;
            buffers[idx].usage_count = 0;
            buffers[idx].pin_count = 0;
        }
    }
}

pub struct MutableRelationState {
    pub relation: crate::types::Relation,
    pub dirty_buffers: Vec<PageId>,
}

pub struct SharedBufferCache {
    pub(crate) storage: Arc<dyn StorageTrait>,
    pool: BufferPool,
    rels: parking_lot::RwLock<std::collections::HashMap<crate::types::Oid, Arc<Mutex<MutableRelationState>>>>,
}

impl SharedBufferCache {
    pub fn new(storage: Arc<dyn StorageTrait>) -> Self {
        Self {
            storage,
            pool: BufferPool::new(1024),
            rels: parking_lot::RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub fn get_pool(&self) -> &BufferPool {
        &self.pool
    }

    pub fn get_relation_state(&self, rel_oid: crate::types::Oid) -> Option<Arc<Mutex<MutableRelationState>>> {
        let rels = self.rels.read();
        rels.get(&rel_oid).cloned()
    }

    pub fn sync_from_catalog(&self, catalog: &crate::catalog::Catalog) {
        let mut rels = self.rels.write();
        for rel in catalog.list_relations() {
            rels.entry(rel.rel_oid).or_insert_with(|| {
                Arc::new(Mutex::new(MutableRelationState {
                    relation: rel,
                    dirty_buffers: vec![],
                }))
            });
        }
    }

    pub fn register_relation(&self, rel: crate::types::Relation) {
        let mut rels = self.rels.write();
        rels.entry(rel.rel_oid).or_insert_with(|| {
            Arc::new(Mutex::new(MutableRelationState {
                relation: rel,
                dirty_buffers: vec![],
            }))
        });
    }

    pub fn unregister_relation(&self, rel_oid: crate::types::Oid) {
        let mut rels = self.rels.write();
        rels.remove(&rel_oid);
    }

    pub fn fetch_page(&self, page_id: crate::types::PageId) -> anyhow::Result<std::sync::Arc<parking_lot::Mutex<Buffer>>> {
        let idx = self.pool.fetch_page(&*self.storage, page_id)?;
        let buffers = self.pool.buffers.lock();
        let buffer = &buffers[idx];
        let data = std::sync::Arc::new(parking_lot::Mutex::new(Buffer {
            page_id: buffer.page_id,
            data: buffer.data.clone(),
            is_dirty: buffer.is_dirty,
            usage_count: buffer.usage_count,
            pin_count: buffer.pin_count,
        }));
        Ok(data)
    }

    /// Remove a page from the buffer pool so subsequent fetches re-read from storage.
    pub fn invalidate_page(&self, page_id: crate::types::PageId) {
        self.pool.invalidate_page(page_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ephemeral::EphemeralStorage;

    #[test]
    fn test_buffer_pool_new() {
        let pool = BufferPool::new(16);
        assert_eq!(pool.pool_size(), 16);
        assert_eq!(pool.dirty_count(), 0);
    }

    #[test]
    fn test_fetch_page() {
        let storage = EphemeralStorage::new();
        let pool = BufferPool::new(4);
        let idx = pool.fetch_page(&storage, PageId(1)).unwrap();
        assert!(idx < 4);
        assert_eq!(pool.inspect().len(), 1);
    }

    #[test]
    fn test_fetch_same_page_twice() {
        let storage = EphemeralStorage::new();
        let pool = BufferPool::new(4);
        let idx1 = pool.fetch_page(&storage, PageId(1)).unwrap();
        let idx2 = pool.fetch_page(&storage, PageId(1)).unwrap();
        assert_eq!(idx1, idx2);
    }

    #[test]
    fn test_pin_and_unpin() {
        let storage = EphemeralStorage::new();
        let pool = BufferPool::new(4);
        pool.fetch_page(&storage, PageId(1)).unwrap();
        pool.pin_page(PageId(1));
        pool.unpin_page(PageId(1), false);
        pool.unpin_page(PageId(1), false);
    }

    #[test]
    fn test_unpin_dirty() {
        let storage = EphemeralStorage::new();
        let pool = BufferPool::new(4);
        pool.fetch_page(&storage, PageId(1)).unwrap();
        pool.unpin_page(PageId(1), true);
        assert_eq!(pool.dirty_count(), 1);
    }

    #[test]
    fn test_flush_page() {
        let storage = EphemeralStorage::new();
        let pool = BufferPool::new(4);
        pool.fetch_page(&storage, PageId(1)).unwrap();
        pool.unpin_page(PageId(1), true);
        pool.flush_page(&storage, PageId(1)).unwrap();
        assert_eq!(pool.dirty_count(), 0);
    }

    #[test]
    fn test_flush_all() {
        let storage = EphemeralStorage::new();
        let pool = BufferPool::new(4);
        pool.fetch_page(&storage, PageId(1)).unwrap();
        pool.unpin_page(PageId(1), true);
        pool.fetch_page(&storage, PageId(2)).unwrap();
        pool.unpin_page(PageId(2), true);
        assert_eq!(pool.dirty_count(), 2);
        pool.flush_all(&storage).unwrap();
        assert_eq!(pool.dirty_count(), 0);
    }

    #[test]
    fn test_eviction() {
        let storage = EphemeralStorage::new();
        let pool = BufferPool::new(2);
        pool.fetch_page(&storage, PageId(1)).unwrap();
        pool.unpin_page(PageId(1), false);
        pool.fetch_page(&storage, PageId(2)).unwrap();
        pool.unpin_page(PageId(2), false);
        pool.fetch_page(&storage, PageId(3)).unwrap();
        pool.unpin_page(PageId(3), false);
        let map = pool.page_map.lock();
        assert!(map.contains_key(&PageId(3)));
    }

    #[test]
    fn test_pin_prevents_eviction() {
        let storage = EphemeralStorage::new();
        let pool = BufferPool::new(2);
        pool.fetch_page(&storage, PageId(1)).unwrap();
        pool.fetch_page(&storage, PageId(2)).unwrap();
        pool.fetch_page(&storage, PageId(3)).unwrap();
        let map = pool.page_map.lock();
        assert!(map.contains_key(&PageId(1)));
        assert!(map.contains_key(&PageId(3)));
    }

    #[test]
    fn test_inspect() {
        let storage = EphemeralStorage::new();
        let pool = BufferPool::new(4);
        pool.fetch_page(&storage, PageId(1)).unwrap();
        let info = pool.inspect();
        assert_eq!(info.len(), 1);
        assert!(info[0].contains("Page(1)"));
    }
}
