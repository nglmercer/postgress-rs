use criterion::{criterion_group, criterion_main, Criterion};
use postgress_rs::buffer_cache::SharedBufferCache;
use postgress_rs::catalog::Catalog;
use postgress_rs::storage::ephemeral::EphemeralStorage;
use postgress_rs::storage::StorageTrait;
use postgress_rs::types::Oid;
use std::sync::Arc;

mod common;

fn setup_benchmark() -> (Arc<EphemeralStorage>, Arc<SharedBufferCache>, Arc<Catalog>, Oid) {
    let storage = common::setup_storage();
    let cache = common::setup_cache(&storage);
    let catalog = common::setup_catalog(&storage);
    
    let rel_oid = common::create_test_table(&catalog, "bench_users", vec![
        ("id", Oid(23)),
        ("name", Oid(25)),
        ("value", Oid(23)),
    ]);
    cache.sync_from_catalog(&catalog);

    common::insert_test_data(&cache, &catalog, rel_oid, 1000);
    
    (storage, cache, catalog, rel_oid)
}

fn bench_seq_scan_1000(c: &mut Criterion) {
    let (_storage, cache, _catalog, rel_oid) = setup_benchmark();
    
    c.bench_function("seq_scan_1000", |b| {
        b.iter(|| {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                postgress_rs::executor::heap::heap_scan(&cache, rel_oid.0).await.unwrap()
            })
        })
    });
}

fn bench_insert_single(c: &mut Criterion) {
    let (_storage, cache, _catalog, rel_oid) = setup_benchmark();
    
    c.bench_function("insert_single", |b| {
        b.iter(|| {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                let wal = postgress_rs::wal::WAL::new(65536);
                let insert_op = postgress_rs::executor::heap::TupleInsert {
                    rel_oid,
                    values: vec![b"test".to_vec(), b"value".to_vec()],
                };
                postgress_rs::executor::heap::tuple_insert(&cache, &wal, &insert_op).await.unwrap();
            })
        })
    });
}

fn bench_heap_page_serialize(c: &mut Criterion) {
    let mut page = postgress_rs::storage::heap_page::HeapPage::new();
    for i in 0..50 {
        page.add_tuple(&bincode::serialize(&format!("tuple_{}", i)).unwrap());
    }
    
    c.bench_function("heap_page_serialize", |b| {
        b.iter(|| {
            page.serialize()
        })
    });
}

fn bench_heap_page_deserialize(c: &mut Criterion) {
    let mut page = postgress_rs::storage::heap_page::HeapPage::new();
    for i in 0..50 {
        page.add_tuple(&bincode::serialize(&format!("tuple_{}", i)).unwrap());
    }
    let data = page.serialize();
    
    c.bench_function("heap_page_deserialize", |b| {
        b.iter(|| {
            postgress_rs::storage::heap_page::HeapPage::deserialize(&data)
        })
    });
}

fn bench_tuple_serialize(c: &mut Criterion) {
    let tuple = postgress_rs::types::Tuple {
        slots: vec![],
        data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        xmin: 1,
        xmax: 0,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };
    
    c.bench_function("tuple_serialize", |b| {
        b.iter(|| {
            bincode::serialize(&tuple).unwrap()
        })
    });
}

fn bench_tuple_deserialize(c: &mut Criterion) {
    let tuple = postgress_rs::types::Tuple {
        slots: vec![],
        data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        xmin: 1,
        xmax: 0,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };
    let data = bincode::serialize(&tuple).unwrap();
    
    c.bench_function("tuple_deserialize", |b| {
        b.iter(|| {
            bincode::deserialize::<postgress_rs::types::Tuple>(&data).unwrap()
        })
    });
}

fn bench_ephemeral_read(c: &mut Criterion) {
    let storage = common::setup_storage();
    let page_data = vec![42u8; 8192];
    storage.write_page(postgress_rs::types::PageId(1), &page_data).unwrap();
    
    c.bench_function("ephemeral_read_page", |b| {
        b.iter(|| {
            storage.read_page(postgress_rs::types::PageId(1)).unwrap()
        })
    });
}

fn bench_ephemeral_write(c: &mut Criterion) {
    let storage = common::setup_storage();
    let page_data = vec![42u8; 8192];
    
    c.bench_function("ephemeral_write_page", |b| {
        b.iter(|| {
            storage.write_page(postgress_rs::types::PageId(1), &page_data).unwrap()
        })
    });
}

criterion_group!(
    benches,
    bench_seq_scan_1000,
    bench_insert_single,
    bench_heap_page_serialize,
    bench_heap_page_deserialize,
    bench_tuple_serialize,
    bench_tuple_deserialize,
    bench_ephemeral_read,
    bench_ephemeral_write
);

criterion_main!(benches);
