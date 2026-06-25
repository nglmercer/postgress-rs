use criterion::{criterion_group, criterion_main, Criterion};
use postgress_rs::types::{Oid, PageId, Tuple};
use postgress_rs::wal::{compute_crc, CheckpointRecord, ControlFile, WALRecord, XLogRecord, WAL};

mod common;

fn bench_wal_append_begin(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let wal = WAL::new(65536);

    c.bench_function("wal_append_begin", |b| {
        b.iter(|| {
            runtime.block_on(async { wal.append(&WALRecord::Begin { xid: 1 }).await.unwrap() })
        })
    });
}

fn bench_wal_append_insert_small(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let wal = WAL::new(65536);
    let tuple = Tuple {
        slots: vec![],
        data: vec![1, 2, 3, 4, 5],
        xmin: 1,
        xmax: 0,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };

    c.bench_function("wal_append_insert_small", |b| {
        b.iter(|| {
            runtime.block_on(async {
                wal.append(&WALRecord::Insert {
                    rel_oid: Oid(1),
                    page_id: PageId(0),
                    tuple: tuple.clone(),
                })
                .await
                .unwrap()
            })
        })
    });
}

fn bench_wal_append_insert_large(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let wal = WAL::new(65536);
    let tuple = Tuple {
        slots: vec![],
        data: vec![42u8; 10000],
        xmin: 1,
        xmax: 0,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };

    c.bench_function("wal_append_insert_large", |b| {
        b.iter(|| {
            runtime.block_on(async {
                wal.append(&WALRecord::Insert {
                    rel_oid: Oid(1),
                    page_id: PageId(0),
                    tuple: tuple.clone(),
                })
                .await
                .unwrap()
            })
        })
    });
}

fn bench_wal_append_update(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let wal = WAL::new(65536);
    let old_tuple = Tuple {
        slots: vec![],
        data: vec![1, 2, 3, 4, 5],
        xmin: 1,
        xmax: 0,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };
    let new_tuple = Tuple {
        slots: vec![],
        data: vec![6, 7, 8, 9, 10],
        xmin: 2,
        xmax: 0,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };

    c.bench_function("wal_append_update", |b| {
        b.iter(|| {
            runtime.block_on(async {
                wal.append(&WALRecord::Update {
                    rel_oid: Oid(1),
                    page_id: PageId(0),
                    old_tuple: old_tuple.clone(),
                    new_tuple: new_tuple.clone(),
                })
                .await
                .unwrap()
            })
        })
    });
}

fn bench_wal_flush(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let wal = WAL::new(65536);
    runtime.block_on(async {
        wal.append(&WALRecord::Begin { xid: 1 }).await.unwrap();
    });

    c.bench_function("wal_flush", |b| {
        b.iter(|| runtime.block_on(async { wal.flush().await.unwrap() }))
    });
}

fn bench_wal_crc32(c: &mut Criterion) {
    let data = vec![42u8; 1024];

    c.bench_function("wal_crc32_1k", |b| b.iter(|| compute_crc(&data)));

    let data_large = vec![42u8; 10240];
    c.bench_function("wal_crc32_10k", |b| b.iter(|| compute_crc(&data_large)));
}

fn bench_wal_xlog_record_serialize(c: &mut Criterion) {
    let record = XLogRecord::new(1, 0, 0, &[1, 2, 3, 4]);

    c.bench_function("wal_xlog_record_serialize", |b| {
        b.iter(|| record.serialize())
    });
}

fn bench_wal_checkpoint_record_roundtrip(c: &mut Criterion) {
    let checkpoint = CheckpointRecord {
        next_xid: 100,
        next_oid: 20000,
        next_multixact: 1,
        oldest_xid: 50,
        oldest_multixact: 1,
        oldest_commit_ts_xid: 50,
        new_commit_ts_xid: 100,
        checkpoint_lsn: 12345,
        redo_lsn: 12000,
        timeline_id: 1,
    };

    c.bench_function("wal_checkpoint_record_roundtrip", |b| {
        b.iter(|| {
            let bytes = checkpoint.serialize();
            CheckpointRecord::deserialize(&bytes)
        })
    });
}

fn bench_wal_control_file_roundtrip(c: &mut Criterion) {
    let control = ControlFile::create(12345678);

    c.bench_function("wal_control_file_roundtrip", |b| {
        b.iter(|| {
            let bytes = control.serialize();
            ControlFile::deserialize(&bytes)
        })
    });
}

criterion_group!(
    benches,
    bench_wal_append_begin,
    bench_wal_append_insert_small,
    bench_wal_append_insert_large,
    bench_wal_append_update,
    bench_wal_flush,
    bench_wal_crc32,
    bench_wal_xlog_record_serialize,
    bench_wal_checkpoint_record_roundtrip,
    bench_wal_control_file_roundtrip
);

criterion_main!(benches);
