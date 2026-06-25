use postgress_rs::buffer_cache::SharedBufferCache;
use postgress_rs::catalog::Catalog;
use postgress_rs::storage::ephemeral::EphemeralStorage;
use postgress_rs::types::{Oid, Relation};
use std::sync::Arc;

fn setup() -> (Arc<EphemeralStorage>, Arc<SharedBufferCache>, Arc<Catalog>) {
    let storage = Arc::new(EphemeralStorage::new());
    let cache = Arc::new(SharedBufferCache::new(storage.clone()));
    let catalog = Arc::new(Catalog::new(storage.clone()));
    catalog.register_cache(cache.clone());
    (storage, cache, catalog)
}

#[tokio::test]
async fn test_create_relation() {
    let (_, _, catalog) = setup();
    let rel = Relation::empty("users", vec![("id", Oid(23)), ("name", Oid(25))]);
    let oid = catalog.create_relation(rel).await.unwrap();
    assert_ne!(oid, Oid(0));
}

#[tokio::test]
async fn test_get_relation() {
    let (_, _, catalog) = setup();
    let rel = Relation::empty("test", vec![("col", Oid(23))]);
    let oid = catalog.create_relation(rel).await.unwrap();
    let found = catalog.get_relation(oid).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "test");
}

#[tokio::test]
async fn test_get_nonexistent_relation() {
    let (_, _, catalog) = setup();
    let found = catalog.get_relation(Oid(99999)).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_delete_relation() {
    let (_, _, catalog) = setup();
    let rel = Relation::empty("temp", vec![("x", Oid(23))]);
    let oid = catalog.create_relation(rel).await.unwrap();
    catalog.delete_relation(oid).await.unwrap();
    let found = catalog.get_relation(oid).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_list_relations_empty() {
    let (_, _, catalog) = setup();
    let rels = catalog.list_relations();
    assert!(rels.is_empty());
}

#[tokio::test]
async fn test_list_relations_multiple() {
    let (_, _, catalog) = setup();
    catalog
        .create_relation(Relation::empty("t1", vec![]))
        .await
        .unwrap();
    catalog
        .create_relation(Relation::empty("t2", vec![]))
        .await
        .unwrap();
    catalog
        .create_relation(Relation::empty("t3", vec![]))
        .await
        .unwrap();
    let rels = catalog.list_relations();
    assert_eq!(rels.len(), 3);
}

#[tokio::test]
async fn test_allocate_oid_unique() {
    let (_, _, catalog) = setup();
    let oid1 = catalog.allocate_oid();
    let oid2 = catalog.allocate_oid();
    let oid3 = catalog.allocate_oid();
    assert_ne!(oid1, oid2);
    assert_ne!(oid2, oid3);
    assert_ne!(oid1, oid3);
}

#[tokio::test]
async fn test_bootstrap() {
    let (_, _, catalog) = setup();
    catalog.bootstrap().await.unwrap();
    let rels = catalog.list_relations();
    assert_eq!(rels.len(), 4);
    let names: Vec<&str> = rels.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"pg_class"));
    assert!(names.contains(&"pg_attribute"));
    assert!(names.contains(&"pg_type"));
    assert!(names.contains(&"pg_index"));
}

#[tokio::test]
async fn test_create_relation_with_custom_oid() {
    let (_, _, catalog) = setup();
    let mut rel = Relation::empty("custom", vec![("x", Oid(23))]);
    rel.rel_oid = Oid(500);
    let oid = catalog.create_relation(rel).await.unwrap();
    assert_eq!(oid, Oid(500));
}

#[tokio::test]
async fn test_relation_visible_in_cache() {
    let (_, cache, catalog) = setup();
    let rel = Relation::empty("cached", vec![("a", Oid(23))]);
    let oid = catalog.create_relation(rel).await.unwrap();
    assert!(cache.get_relation_state(oid).is_some());
}

#[tokio::test]
async fn test_delete_relation_removes_from_cache() {
    let (_, cache, catalog) = setup();
    let rel = Relation::empty("gone", vec![("a", Oid(23))]);
    let oid = catalog.create_relation(rel).await.unwrap();
    assert!(cache.get_relation_state(oid).is_some());
    catalog.delete_relation(oid).await.unwrap();
    assert!(cache.get_relation_state(oid).is_none());
}
