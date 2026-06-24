use postgress_rs::types::{Oid, PageId, Relation};
use postgress_rs::storage::ephemeral::EphemeralStorage;
use postgress_rs::storage::StorageTrait;
use postgress_rs::buffer_cache::SharedBufferCache;
use postgress_rs::executor::heap::{tuple_insert, TupleInsert, heap_scan};

#[test]
fn test_oid_eq() {
    assert_eq!(Oid(1), Oid(1));
    assert_ne!(Oid(1), Oid(2));
}

#[test]
fn test_relation_empty() {
    let rel = Relation::empty("users", vec![
        ("id", Oid(23)),
        ("name", Oid(25)),
    ]);
    assert_eq!(rel.name, "users");
    assert_eq!(rel.tuple_desc.fields.len(), 2);
    assert_eq!(rel.tuple_desc.fields[0].name, "id");
    assert_eq!(rel.pages.len(), 0);
}

#[tokio::test]
async fn test_ephemeral_storage_roundtrip() {
    let storage = EphemeralStorage::new();
    let page = PageId(42);
    let data = b"hello database world";
    
    storage.write_page(page, data).unwrap();
    let read_back = storage.read_page(page).unwrap();
    
    assert_eq!(read_back, data);
}

#[tokio::test]
async fn test_tuple_insert_and_read() {
    let storage = std::sync::Arc::new(EphemeralStorage::new());
    let wal = postgress_rs::wal::WAL::new(1024);
    let catalog = postgress_rs::catalog::Catalog::new(storage.clone());
    let cache = std::sync::Arc::new(SharedBufferCache::new(storage.clone()));
    
    catalog.register_cache(cache.clone());
    
    let rel = Relation::empty("test_table", vec![
        ("id", Oid(23)),
        ("val", Oid(25)),
    ]);
    let rel_oid = catalog.create_relation(rel).await.unwrap();
    
    // Verify relation is in cache
    assert!(cache.get_relation_state(rel_oid).is_some(), "Relation should be in cache after create");
    
    tuple_insert(
        &cache,
        &wal,
        &TupleInsert {
            rel_oid,
            values: vec![b"1".to_vec(), b"hello".to_vec()],
        },
    ).await.unwrap();
    
    let rows = heap_scan(&cache, rel_oid.0).await.unwrap();
    assert_eq!(rows.len(), 1, "Expected 1 row, got {}", rows.len());
    assert_eq!(rows[0].1[0], "1");
    assert_eq!(rows[0].1[1], "hello");
}

#[test]
fn test_storage_trait_object() {
    let storage: std::sync::Arc<dyn StorageTrait> = std::sync::Arc::new(EphemeralStorage::new());
    let page = PageId(0);
    let data = vec![0u8; 100];
    storage.write_page(page, &data).unwrap();
    let read = storage.read_page(page).unwrap();
    assert_eq!(read, data);
}

#[tokio::test]
async fn test_multiple_inserts_scan() {
    let storage = std::sync::Arc::new(EphemeralStorage::new());
    let wal = postgress_rs::wal::WAL::new(1024);
    let catalog = postgress_rs::catalog::Catalog::new(storage.clone());
    let cache = std::sync::Arc::new(SharedBufferCache::new(storage.clone()));
    
    catalog.register_cache(cache.clone());
    
    let rel = Relation::empty("multi_table", vec![
        ("id", Oid(23)),
        ("name", Oid(25)),
    ]);
    let rel_oid = catalog.create_relation(rel).await.unwrap();
    
    for i in 0..5 {
        tuple_insert(
            &cache,
            &wal,
            &TupleInsert {
                rel_oid,
                values: vec![i.to_string().into_bytes(), format!("name_{}", i).into_bytes()],
            },
        ).await.unwrap();
    }
    
    let rows = heap_scan(&cache, rel_oid.0).await.unwrap();
    assert_eq!(rows.len(), 5);
}

#[tokio::test]
async fn test_heap_scan_empty() {
    let storage = std::sync::Arc::new(EphemeralStorage::new());
    let catalog = postgress_rs::catalog::Catalog::new(storage.clone());
    let cache = std::sync::Arc::new(SharedBufferCache::new(storage.clone()));
    
    catalog.register_cache(cache.clone());
    
    let rel = Relation::empty("empty_table", vec![
        ("id", Oid(23)),
    ]);
    let rel_oid = catalog.create_relation(rel).await.unwrap();
    
    let rows = heap_scan(&cache, rel_oid.0).await.unwrap();
    assert_eq!(rows.len(), 0);
}
