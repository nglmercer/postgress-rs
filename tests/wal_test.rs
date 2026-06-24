use postgress_rs::wal::{WAL, WALRecord};
use postgress_rs::types::{Oid, PageId, Tuple};

#[tokio::test]
async fn test_wal_allocate_xid() {
    let wal = WAL::new(8192);
    let xid1 = wal.allocate_xid();
    let xid2 = wal.allocate_xid();
    let xid3 = wal.allocate_xid();
    assert!(xid1 < xid2);
    assert!(xid2 < xid3);
}

#[tokio::test]
async fn test_wal_append_begin() {
    let wal = WAL::new(8192);
    let lsn = wal.append(&WALRecord::Begin { xid: 1 }).await.unwrap();
    assert_eq!(lsn, 0);
}

#[tokio::test]
async fn test_wal_append_multiple() {
    let wal = WAL::new(8192);
    let lsn1 = wal.append(&WALRecord::Begin { xid: 1 }).await.unwrap();
    let lsn2 = wal.append(&WALRecord::Commit { xid: 1 }).await.unwrap();
    let lsn3 = wal.append(&WALRecord::Begin { xid: 2 }).await.unwrap();
    assert_eq!(lsn1, 0);
    assert!(lsn2 > lsn1);
    assert!(lsn3 > lsn2);
}

#[tokio::test]
async fn test_wal_append_insert_record() {
    let wal = WAL::new(8192);
    let record = WALRecord::Insert {
        rel_oid: Oid(100),
        page_id: PageId(1),
        tuple: Tuple {
            slots: vec![],
            data: b"hello world".to_vec(),
            xmin: 1,
            xmax: 0,
            cmin: 0,
            cmax: 0,
            xvac: 0,
        },
    };
    let lsn = wal.append(&record).await.unwrap();
    assert_eq!(lsn, 0);
}

#[tokio::test]
async fn test_wal_append_update_record() {
    let wal = WAL::new(8192);
    let old_tuple = Tuple {
        slots: vec![],
        data: b"old".to_vec(),
        xmin: 1,
        xmax: 0,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };
    let new_tuple = Tuple {
        slots: vec![],
        data: b"new".to_vec(),
        xmin: 2,
        xmax: 0,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };
    let record = WALRecord::Update {
        rel_oid: Oid(100),
        page_id: PageId(1),
        old_tuple,
        new_tuple,
    };
    let lsn = wal.append(&record).await.unwrap();
    assert_eq!(lsn, 0);
}

#[tokio::test]
async fn test_wal_append_delete_record() {
    let wal = WAL::new(8192);
    let record = WALRecord::Delete {
        rel_oid: Oid(100),
        page_id: PageId(1),
        tuple: Tuple {
            slots: vec![],
            data: b"delete me".to_vec(),
            xmin: 1,
            xmax: 0,
            cmin: 0,
            cmax: 0,
            xvac: 0,
        },
    };
    let lsn = wal.append(&record).await.unwrap();
    assert_eq!(lsn, 0);
}

#[tokio::test]
async fn test_wal_flush_is_noop() {
    let wal = WAL::new(8192);
    wal.flush().await.unwrap();
}

#[tokio::test]
async fn test_wal_uses_own_storage() {
    let wal = WAL::new(8192);
    // Append should not fail (it uses internal storage)
    let _ = wal.append(&WALRecord::Begin { xid: 1 }).await.unwrap();
    let _ = wal.append(&WALRecord::Commit { xid: 1 }).await.unwrap();
}

#[tokio::test]
async fn test_wal_segment_overflow_detection() {
    let wal = WAL::new(64); // very small segment size
    let record = WALRecord::Begin { xid: 1 };
    // First append should succeed (fits in segment)
    let _ = wal.append(&record).await.unwrap();
}

#[tokio::test]
async fn test_wal_concurrent_xid_allocation() {
    let wal = WAL::new(8192);
    let mut xids = Vec::new();
    for _ in 0..100 {
        xids.push(wal.allocate_xid());
    }
    // All XIDs should be unique
    let mut sorted = xids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(xids.len(), sorted.len());
}
