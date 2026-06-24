use crate::types::{Oid, PageId, Relation};
use crate::storage::StorageTrait;
use crate::buffer_cache::SharedBufferCache;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;

#[derive(Debug, Clone)]
pub struct IndexInfo {
    pub index_oid: Oid,
    pub rel_oid: Oid,
    pub column_name: String,
    pub root_page: PageId,
}

pub struct Catalog {
    #[allow(dead_code)]
    storage: Arc<dyn StorageTrait>,
    relations: RwLock<std::collections::HashMap<Oid, Relation>>,
    indexes: RwLock<Vec<IndexInfo>>,
    next_oid: AtomicU32,
    cache: RwLock<Option<Arc<SharedBufferCache>>>,
}

impl Catalog {
    pub fn new(storage: Arc<dyn StorageTrait>) -> Self {
        Self {
            storage,
            relations: RwLock::new(std::collections::HashMap::new()),
            indexes: RwLock::new(Vec::new()),
            next_oid: AtomicU32::new(10000),
            cache: RwLock::new(None),
        }
    }

    pub fn register_cache(&self, cache: Arc<SharedBufferCache>) {
        *self.cache.write() = Some(cache.clone());
        cache.sync_from_catalog(self);
    }

    pub fn allocate_oid(&self) -> Oid {
        Oid(self.next_oid.fetch_add(1, Ordering::Relaxed))
    }

    pub async fn create_relation(&self, mut rel: Relation) -> anyhow::Result<Oid> {
        if rel.rel_oid == Oid(0) {
            rel.rel_oid = self.allocate_oid();
        }
        let oid = rel.rel_oid;
        let mut rels = self.relations.write();
        rels.insert(rel.rel_oid, rel.clone());
        drop(rels);

        if let Some(cache) = self.cache.read().as_ref() {
            cache.register_relation(rel);
        }

        Ok(oid)
    }

    pub async fn get_relation(&self, rel_oid: Oid) -> anyhow::Result<Option<Relation>> {
        let rels = self.relations.read();
        Ok(rels.get(&rel_oid).cloned())
    }

    pub async fn delete_relation(&self, rel_oid: Oid) -> anyhow::Result<()> {
        let mut rels = self.relations.write();
        rels.remove(&rel_oid);
        drop(rels);
        if let Some(cache) = self.cache.read().as_ref() {
            cache.unregister_relation(rel_oid);
        }
        Ok(())
    }

    pub fn list_relations(&self) -> Vec<Relation> {
        let rels = self.relations.read();
        rels.values().cloned().collect()
    }

    pub fn get_relation_by_name(&self, name: &str) -> anyhow::Result<Option<Relation>> {
        let rels = self.relations.read();
        Ok(rels.values().find(|r| r.name == name).cloned())
    }

    pub async fn create_index(&self, name: &str, rel_oid: Oid, column_name: String) -> anyhow::Result<()> {
        let index_oid = self.allocate_oid();
        let info = IndexInfo {
            index_oid,
            rel_oid,
            column_name: column_name.clone(),
            root_page: PageId::default(),
        };
        self.register_index(info);
        let _ = rel_oid;
        let _ = name;
        let _ = column_name;
        Ok(())
    }

    pub fn register_index(&self, info: IndexInfo) {
        let mut indexes = self.indexes.write();
        indexes.push(info);
    }

    pub fn find_index(&self, rel_oid: Oid, column_name: &str) -> Option<IndexInfo> {
        let indexes = self.indexes.read();
        indexes.iter()
            .find(|i| i.rel_oid == rel_oid && i.column_name == column_name)
            .cloned()
    }

    pub async fn bootstrap(&self) -> anyhow::Result<()> {
        let pg_class = Relation::empty("pg_class", vec![
            ("oid", Oid(0)),
            ("relname", Oid(0)),
            ("relpages", Oid(0)),
        ]);
        let pg_class = Relation {
            rel_oid: Oid(1259),
            name: pg_class.name,
            tuple_desc: pg_class.tuple_desc,
            pages: pg_class.pages,
        };
        self.create_relation(pg_class).await?;

        let pg_attribute = Relation::empty("pg_attribute", vec![
            ("attrelid", Oid(0)),
            ("attname", Oid(0)),
            ("atttypid", Oid(0)),
            ("attnum", Oid(0)),
            ("attlen", Oid(0)),
        ]);
        let pg_attribute = Relation {
            rel_oid: Oid(1249),
            name: pg_attribute.name,
            tuple_desc: pg_attribute.tuple_desc,
            pages: pg_attribute.pages,
        };
        self.create_relation(pg_attribute).await?;

        let pg_type = Relation::empty("pg_type", vec![
            ("oid", Oid(0)),
            ("typname", Oid(0)),
            ("typtype", Oid(0)),
        ]);
        let pg_type = Relation {
            rel_oid: Oid(1247),
            name: pg_type.name,
            tuple_desc: pg_type.tuple_desc,
            pages: pg_type.pages,
        };
        self.create_relation(pg_type).await?;

        let pg_index = Relation::empty("pg_index", vec![
            ("indexrelid", Oid(0)),
            ("indrelid", Oid(0)),
            ("indnatts", Oid(0)),
        ]);
        let pg_index = Relation {
            rel_oid: Oid(1250),
            name: pg_index.name,
            tuple_desc: pg_index.tuple_desc,
            pages: pg_index.pages,
        };
        self.create_relation(pg_index).await?;

        Ok(())
    }
}
