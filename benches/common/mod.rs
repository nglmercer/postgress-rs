#![allow(dead_code)]

use postgress_rs::buffer_cache::SharedBufferCache;
use postgress_rs::catalog::Catalog;
use postgress_rs::storage::ephemeral::EphemeralStorage;
use postgress_rs::types::{Oid, Relation, TupleDesc, Attribute};
use std::sync::Arc;

pub fn setup_storage() -> Arc<EphemeralStorage> {
    Arc::new(EphemeralStorage::new())
}

pub fn setup_cache(storage: &Arc<EphemeralStorage>) -> Arc<SharedBufferCache> {
    Arc::new(SharedBufferCache::new(storage.clone()))
}

pub fn setup_catalog(storage: &Arc<EphemeralStorage>) -> Arc<Catalog> {
    Arc::new(Catalog::new(storage.clone()))
}

pub fn create_test_table(catalog: &Catalog, name: &str, columns: Vec<(&str, Oid)>) -> Oid {
    let tuple_desc = TupleDesc {
        fields: columns
            .into_iter()
            .enumerate()
            .map(|(i, (col_name, type_oid))| Attribute {
                name: col_name.to_string(),
                type_oid,
                attnum: i as i16,
                typmod: -1,
            })
            .collect(),
    };
    
    let rel = Relation {
        rel_oid: Oid(0),
        name: name.to_string(),
        tuple_desc,
        pages: vec![],
        relpages: 0,
        reltuples: 0.0,
        relfrozenxid: 0,
    };
    
    let oid = catalog.allocate_oid();
    let mut rel_with_oid = rel;
    rel_with_oid.rel_oid = oid;
    
    let catalog_clone = catalog;
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        catalog_clone.create_relation(rel_with_oid).await.unwrap();
    });
    
    oid
}

pub fn insert_test_data(cache: &SharedBufferCache, _catalog: &Catalog, rel_oid: Oid, count: usize) {
    let state = cache.get_relation_state(rel_oid).unwrap();
    let rel = {
        let rel_state = state.lock();
        rel_state.relation.clone()
    };
    
    let runtime = tokio::runtime::Runtime::new().unwrap();
    for i in 0..count {
        let values: Vec<String> = rel.tuple_desc.fields.iter().map(|attr| {
            match attr.type_oid.0 {
                23 => i.to_string(),
                25 => format!("value_{}", i),
                _ => format!("val_{}", i),
            }
        }).collect();
        
        let insert_op = postgress_rs::executor::heap::TupleInsert {
            rel_oid,
            values: values.iter().map(|v| v.as_bytes().to_vec()).collect(),
        };
        
        runtime.block_on(async {
            let wal = postgress_rs::wal::WAL::new(65536);
            postgress_rs::executor::heap::tuple_insert(cache, &wal, &insert_op).await.unwrap();
        });
    }
}
