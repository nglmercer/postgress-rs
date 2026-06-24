use postgress_rs::storage::ephemeral::EphemeralStorage;
use postgress_rs::buffer_cache::SharedBufferCache;
use postgress_rs::catalog::Catalog;
use postgress_rs::executor::heap::*;
use postgress_rs::wal::WAL;
use postgress_rs::types::{Oid, Relation};
use std::sync::Arc;

async fn setup() -> (Arc<EphemeralStorage>, Arc<WAL>, Arc<SharedBufferCache>, Arc<Catalog>) {
    let storage = Arc::new(EphemeralStorage::new());
    let wal = Arc::new(WAL::new(8192));
    let cache = Arc::new(SharedBufferCache::new(storage.clone()));
    let catalog = Arc::new(Catalog::new(storage.clone()));
    catalog.register_cache(cache.clone());
    (storage, wal, cache, catalog)
}

async fn create_test_table(_cache: &Arc<SharedBufferCache>, catalog: &Arc<Catalog>) -> Oid {
    let rel = Relation::empty("test", vec![
        ("id", Oid(23)),
        ("name", Oid(25)),
        ("value", Oid(23)),
    ]);
    catalog.create_relation(rel).await.unwrap()
}

#[tokio::test]
async fn test_tuple_insert_and_scan() {
    let (_, wal, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;

    tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid,
        values: vec![b"1".to_vec(), b"alice".to_vec(), b"100".to_vec()],
    }).await.unwrap();

    let rows = heap_scan(&cache, rel_oid.0).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1[0], "1");
    assert_eq!(rows[0].1[1], "alice");
    assert_eq!(rows[0].1[2], "100");
}

#[tokio::test]
async fn test_multiple_inserts_and_scan() {
    let (_, wal, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;

    for i in 0..10 {
        tuple_insert(&cache, &wal, &TupleInsert {
            rel_oid,
            values: vec![
                format!("{}", i).into_bytes(),
                format!("user_{}", i).into_bytes(),
                format!("{}", i * 10).into_bytes(),
            ],
        }).await.unwrap();
    }

    let rows = heap_scan(&cache, rel_oid.0).await.unwrap();
    assert_eq!(rows.len(), 10);
}

#[tokio::test]
async fn test_heap_scan_empty_table() {
    let (_, _, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;
    let rows = heap_scan(&cache, rel_oid.0).await.unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn test_tuple_insert_bulk() {
    let (_, wal, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;

    tuple_insert_bulk(&cache, &wal, &TupleInsertBulk {
        rel_oid,
        tuples: vec![
            vec![b"1".to_vec(), b"a".to_vec(), b"10".to_vec()],
            vec![b"2".to_vec(), b"b".to_vec(), b"20".to_vec()],
            vec![b"3".to_vec(), b"c".to_vec(), b"30".to_vec()],
        ],
    }).await.unwrap();

    let rows = heap_scan(&cache, rel_oid.0).await.unwrap();
    assert_eq!(rows.len(), 3);
}

#[tokio::test]
async fn test_tuple_update() {
    let (_, wal, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;

    tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid,
        values: vec![b"1".to_vec(), b"alice".to_vec(), b"100".to_vec()],
    }).await.unwrap();

    let updated = tuple_update(
        &cache,
        &wal,
        rel_oid,
        1, // column_idx = name
        b"ALICE",
        None,
    ).await.unwrap();

    assert_eq!(updated, 1);

    let rows = heap_scan(&cache, rel_oid.0).await.unwrap();
    assert_eq!(rows.len(), 1);
    // After update, old tuple is deleted and new one added
    // The visible tuple should have the new name
    assert_eq!(rows[0].1[1], "ALICE");
}

#[tokio::test]
async fn test_tuple_update_with_filter() {
    let (_, wal, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;

    tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid,
        values: vec![b"1".to_vec(), b"alice".to_vec(), b"100".to_vec()],
    }).await.unwrap();
    tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid,
        values: vec![b"2".to_vec(), b"bob".to_vec(), b"200".to_vec()],
    }).await.unwrap();

    let updated = tuple_update(
        &cache,
        &wal,
        rel_oid,
        2, // column_idx = value
        b"999",
        Some(Filter { column: 0, value: b"1".to_vec() }), // only where id=1
    ).await.unwrap();

    assert_eq!(updated, 1);

    let rows = heap_scan(&cache, rel_oid.0).await.unwrap();
    // Should have 2 visible tuples: updated row1 + unchanged row2
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn test_tuple_delete() {
    let (_, wal, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;

    tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid,
        values: vec![b"1".to_vec(), b"alice".to_vec(), b"100".to_vec()],
    }).await.unwrap();
    tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid,
        values: vec![b"2".to_vec(), b"bob".to_vec(), b"200".to_vec()],
    }).await.unwrap();

    let deleted = tuple_delete(
        &cache,
        &wal,
        rel_oid,
        None, // delete all
    ).await.unwrap();

    assert_eq!(deleted, 2);

    let rows = heap_scan(&cache, rel_oid.0).await.unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn test_tuple_delete_with_filter() {
    let (_, wal, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;

    tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid,
        values: vec![b"1".to_vec(), b"alice".to_vec(), b"100".to_vec()],
    }).await.unwrap();
    tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid,
        values: vec![b"2".to_vec(), b"bob".to_vec(), b"200".to_vec()],
    }).await.unwrap();

    let deleted = tuple_delete(
        &cache,
        &wal,
        rel_oid,
        Some(Filter { column: 0, value: b"1".to_vec() }), // delete where id=1
    ).await.unwrap();

    assert_eq!(deleted, 1);

    let rows = heap_scan(&cache, rel_oid.0).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1[0], "2");
}

#[tokio::test]
async fn test_slow_scan_all() {
    let (_, wal, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;

    tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid,
        values: vec![b"1".to_vec(), b"alice".to_vec(), b"100".to_vec()],
    }).await.unwrap();
    tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid,
        values: vec![b"2".to_vec(), b"bob".to_vec(), b"200".to_vec()],
    }).await.unwrap();

    let rows = slow_scan(&cache, &SlowScan { rel_oid, filter: None }).await.unwrap();
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn test_slow_scan_with_filter() {
    let (_, wal, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;

    tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid,
        values: vec![b"1".to_vec(), b"alice".to_vec(), b"100".to_vec()],
    }).await.unwrap();
    tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid,
        values: vec![b"2".to_vec(), b"bob".to_vec(), b"200".to_vec()],
    }).await.unwrap();

    let rows = slow_scan(&cache, &SlowScan {
        rel_oid,
        filter: Some(Filter { column: 1, value: b"alice".to_vec() }),
    }).await.unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1[1], "alice");
}

#[tokio::test]
async fn test_tuple_insert_nonexistent_relation() {
    let (_, wal, cache, _) = setup().await;
    let result = tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid: Oid(99999),
        values: vec![b"1".to_vec()],
    }).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_heap_scan_nonexistent_relation() {
    let (_, _, cache, _) = setup().await;
    let result = heap_scan(&cache, 99999).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_tuple_update_nonexistent_relation() {
    let (_, wal, cache, _) = setup().await;
    let result = tuple_update(&cache, &wal, Oid(99999), 0, b"x", None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_tuple_delete_nonexistent_relation() {
    let (_, wal, cache, _) = setup().await;
    let result = tuple_delete(&cache, &wal, Oid(99999), None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_tuple_insert_returns_item_pointer() {
    let (_, wal, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;

    let rows_before = heap_scan(&cache, rel_oid.0).await.unwrap();
    assert_eq!(rows_before.len(), 0);

    tuple_insert(&cache, &wal, &TupleInsert {
        rel_oid,
        values: vec![b"1".to_vec(), b"test".to_vec(), b"0".to_vec()],
    }).await.unwrap();

    let rows_after = heap_scan(&cache, rel_oid.0).await.unwrap();
    assert_eq!(rows_after.len(), 1);
    // Verify the ItemPointerData has a valid page_id
    assert_ne!(rows_after[0].0.page_id.0, 0);
}

#[tokio::test]
async fn test_update_on_empty_table() {
    let (_, wal, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;
    let updated = tuple_update(&cache, &wal, rel_oid, 0, b"x", None).await.unwrap();
    assert_eq!(updated, 0);
}

#[tokio::test]
async fn test_delete_on_empty_table() {
    let (_, wal, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;
    let deleted = tuple_delete(&cache, &wal, rel_oid, None).await.unwrap();
    assert_eq!(deleted, 0);
}

#[test]
fn test_mvcc_visible_committed_tuple() {
    use postgress_rs::transaction::{Snapshot, TransactionId};
    use postgress_rs::types::Tuple;

    let tup = Tuple {
        slots: vec![],
        data: vec![],
        xmin: 5,
        xmax: 0,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };
    let snapshot = Snapshot {
        xid: TransactionId(10),
        active_xids: vec![],
    };
    assert!(is_visible(&tup, &snapshot));
}

#[test]
fn test_mvcc_invisible_uncommitted_tuple() {
    use postgress_rs::transaction::{Snapshot, TransactionId};
    use postgress_rs::types::Tuple;

    let tup = Tuple {
        slots: vec![],
        data: vec![],
        xmin: 8,
        xmax: 0,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };
    let snapshot = Snapshot {
        xid: TransactionId(10),
        active_xids: vec![TransactionId(8)],
    };
    assert!(!is_visible(&tup, &snapshot));
}

#[test]
fn test_mvcc_invisible_deleted_tuple() {
    use postgress_rs::transaction::{Snapshot, TransactionId};
    use postgress_rs::types::Tuple;

    let tup = Tuple {
        slots: vec![],
        data: vec![],
        xmin: 5,
        xmax: 7,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };
    let snapshot = Snapshot {
        xid: TransactionId(10),
        active_xids: vec![],
    };
    assert!(!is_visible(&tup, &snapshot));
}

#[test]
fn test_mvcc_visible_tuple_with_active_delete_xid() {
    use postgress_rs::transaction::{Snapshot, TransactionId};
    use postgress_rs::types::Tuple;

    let tup = Tuple {
        slots: vec![],
        data: vec![],
        xmin: 5,
        xmax: 8,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };
    let snapshot = Snapshot {
        xid: TransactionId(10),
        active_xids: vec![TransactionId(8)],
    };
    assert!(is_visible(&tup, &snapshot));
}

#[test]
fn test_mvcc_invisible_zero_xmin() {
    use postgress_rs::transaction::{Snapshot, TransactionId};
    use postgress_rs::types::Tuple;

    let tup = Tuple {
        slots: vec![],
        data: vec![],
        xmin: 0,
        xmax: 0,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };
    let snapshot = Snapshot {
        xid: TransactionId(10),
        active_xids: vec![],
    };
    assert!(!is_visible(&tup, &snapshot));
}

#[test]
fn test_mvcc_invisible_future_xmin() {
    use postgress_rs::transaction::{Snapshot, TransactionId};
    use postgress_rs::types::Tuple;

    let tup = Tuple {
        slots: vec![],
        data: vec![],
        xmin: 15,
        xmax: 0,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };
    let snapshot = Snapshot {
        xid: TransactionId(10),
        active_xids: vec![],
    };
    assert!(!is_visible(&tup, &snapshot));
}

#[tokio::test]
async fn test_heap_scan_with_snapshot() {
    let (_, wal, cache, catalog) = setup().await;
    let rel_oid = create_test_table(&cache, &catalog).await;
    tuple_insert(&cache, &wal, &TupleInsert { rel_oid, values: vec![b"1".to_vec(), b"alice".to_vec(), b"100".to_vec()] }).await.unwrap();

    let snapshot = postgress_rs::transaction::Snapshot {
        xid: postgress_rs::transaction::TransactionId(u32::MAX),
        active_xids: vec![],
    };
    let rows = heap_scan_with_snapshot(&cache, rel_oid.0, &snapshot).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1[1], "alice");
}
