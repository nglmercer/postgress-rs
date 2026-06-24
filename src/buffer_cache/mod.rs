use crate::types::{PageId, Oid};
use crate::storage::StorageTrait;
use std::sync::Arc;
use parking_lot::{Mutex, RwLock};

pub struct Buffer {
    pub page_id: PageId,
    pub data: Vec<u8>,
    pub is_dirty: bool,
    pub usage_count: u32,
}

pub struct BufferPool {
    #[allow(dead_code)]
    pools: Vec<Mutex<Vec<Buffer>>>,
}

impl BufferPool {
    pub fn new(_pool_size: usize) -> Self {
        Self { pools: vec![] }
    }

    pub fn inspect(&self) -> Vec<String> {
        vec![]
    }

    pub fn evict(&self) {
        // placeholder
    }
}

pub struct MutableRelationState {
    pub relation: crate::types::Relation,
    pub dirty_buffers: Vec<PageId>,
}

pub struct SharedBufferCache {
    pub(crate) storage: Arc<dyn StorageTrait>,
    #[allow(dead_code)]
    pool: BufferPool,
    rels: RwLock<std::collections::HashMap<Oid, Arc<Mutex<MutableRelationState>>>>,
}

impl SharedBufferCache {
    pub fn new(storage: Arc<dyn StorageTrait>) -> Self {
        Self {
            storage,
            pool: BufferPool::new(1024),
            rels: RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub fn get_relation_state(&self, rel_oid: Oid) -> Option<Arc<Mutex<MutableRelationState>>> {
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

    pub fn unregister_relation(&self, rel_oid: Oid) {
        let mut rels = self.rels.write();
        rels.remove(&rel_oid);
    }
}
