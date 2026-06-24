use postgress_rs::types::{Oid, PageId, Tuple, TupleDesc, Attribute, Relation};
use postgress_rs::storage::ephemeral::EphemeralStorage;
use postgress_rs::storage::{StorageTrait};
use postgress_rs::executor::heap::{tuple_insert, tuple_insert_bulk, TupleInsert, TupleInsertBulk};

#[test]
fn test_oid_eq() {
    assert_eq!(Oid(1), Oid(1));
    assert_ne!(Oid(1), Oid(2));
}

#[test]
fn test_relation_empty() {
    let rel = Relation::empty("users", vec![
        ("id", Oid(23)),  // INT4OID
        ("name", Oid(25)), // TEXTOID
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
    let _cache = postgress_rs::buffer_cache::SharedBufferCache::new(storage.clone());
    let wal = postgress_rs::wal::WAL::new(storage.clone(), 1024);
    let catalog = postgress_rs::catalog::Catalog::new(storage.clone());
    
    // Register cache so catalog can sync relations to buffer cache
    catalog.register_cache(_cache.clone());
    
    // Bootstrap a relation first
    let rel = Relation::empty("test_table", vec![
        ("id", Oid(23)),
        ("val", Oid(25)),
    ]);
    let rel_oid = catalog.create_relation(rel).await.unwrap();
    
    // Insert a tuple
    tuple_insert(
        &_cache,
        &wal,
        &TupleInsert {
            rel_oid,
            values: vec![postgress_rs::types::SlotId(1), postgress_rs::types::SlotId(2)],
        },
    ).await.unwrap();
    
    // Verify page was written
    let pages = catalog.get_relation(rel_oid).await.unwrap().unwrap().pages;
    assert!(!pages.is_empty());
    for page in &pages {
        let data = storage.read_page(*page).unwrap();
        assert!(data.iter().any(|&b| b != 0), "Page should contain non-zero data after insert");
    }
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
