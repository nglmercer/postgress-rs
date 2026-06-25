use criterion::{criterion_group, criterion_main, Criterion};
use postgress_rs::buffer_cache::BufferPool;
use postgress_rs::storage::heap_page::{HeapPage, PAGE_SIZE};
use postgress_rs::storage::StorageTrait;
use postgress_rs::types::PageId;

mod common;

fn bench_ephemeral_read_page(c: &mut Criterion) {
    let storage = common::setup_storage();
    let page_data = vec![42u8; PAGE_SIZE];
    storage.write_page(PageId(0), &page_data).unwrap();

    c.bench_function("ephemeral_read_page", |b| {
        b.iter(|| storage.read_page(PageId(0)).unwrap())
    });
}

fn bench_ephemeral_write_page(c: &mut Criterion) {
    let storage = common::setup_storage();
    let page_data = vec![42u8; PAGE_SIZE];

    c.bench_function("ephemeral_write_page", |b| {
        b.iter(|| storage.write_page(PageId(0), &page_data).unwrap())
    });
}

fn bench_heap_page_add_tuple(c: &mut Criterion) {
    c.bench_function("heap_page_add_tuple", |b| {
        b.iter_batched(
            || HeapPage::new(),
            |mut page| {
                for i in 0..100 {
                    page.add_tuple(&format!("tuple_{}", i).into_bytes());
                }
                page
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

fn bench_heap_page_serialize(c: &mut Criterion) {
    let mut page = HeapPage::new();
    for i in 0..100 {
        page.add_tuple(&format!("tuple_{}", i).into_bytes());
    }

    c.bench_function("heap_page_serialize", |b| b.iter(|| page.serialize()));
}

fn bench_heap_page_deserialize(c: &mut Criterion) {
    let mut page = HeapPage::new();
    for i in 0..100 {
        page.add_tuple(&format!("tuple_{}", i).into_bytes());
    }
    let data = page.serialize();

    c.bench_function("heap_page_deserialize", |b| {
        b.iter(|| HeapPage::deserialize(&data))
    });
}

fn bench_heap_page_compact(c: &mut Criterion) {
    c.bench_function("heap_page_compact", |b| {
        b.iter_batched(
            || {
                let mut page = HeapPage::new();
                for i in 0..100 {
                    page.add_tuple(&format!("tuple_{}", i).into_bytes());
                }
                page
            },
            |mut page| {
                page.compact();
                page
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

fn bench_buffer_pool_fetch(c: &mut Criterion) {
    let storage = common::setup_storage();
    let pool = BufferPool::new(1024);
    let page_data = vec![42u8; PAGE_SIZE];
    storage.write_page(PageId(0), &page_data).unwrap();

    c.bench_function("buffer_pool_fetch", |b| {
        b.iter(|| pool.fetch_page(&*storage, PageId(0)).unwrap())
    });
}

fn bench_buffer_pool_eviction(c: &mut Criterion) {
    let storage = common::setup_storage();
    let pool = BufferPool::new(16);

    for i in 0..20 {
        let page_data = vec![i as u8; PAGE_SIZE];
        storage.write_page(PageId(i), &page_data).unwrap();
    }

    c.bench_function("buffer_pool_eviction", |b| {
        b.iter(|| {
            for i in 0..20 {
                pool.fetch_page(&*storage, PageId(i)).unwrap();
            }
        })
    });
}

fn bench_bincode_tuple_serialize(c: &mut Criterion) {
    let tuple = postgress_rs::types::Tuple {
        slots: vec![],
        data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        xmin: 1,
        xmax: 0,
        cmin: 0,
        cmax: 0,
        xvac: 0,
    };

    c.bench_function("bincode_tuple_serialize", |b| {
        b.iter(|| bincode::serialize(&tuple).unwrap())
    });
}

fn bench_bincode_tuple_deserialize(c: &mut Criterion) {
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

    c.bench_function("bincode_tuple_deserialize", |b| {
        b.iter(|| bincode::deserialize::<postgress_rs::types::Tuple>(&data).unwrap())
    });
}

criterion_group!(
    benches,
    bench_ephemeral_read_page,
    bench_ephemeral_write_page,
    bench_heap_page_add_tuple,
    bench_heap_page_serialize,
    bench_heap_page_deserialize,
    bench_heap_page_compact,
    bench_buffer_pool_fetch,
    bench_buffer_pool_eviction,
    bench_bincode_tuple_serialize,
    bench_bincode_tuple_deserialize
);

criterion_main!(benches);
