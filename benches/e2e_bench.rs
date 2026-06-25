use criterion::{criterion_group, criterion_main, Criterion};
use postgress_rs::buffer_cache::SharedBufferCache;
use postgress_rs::catalog::Catalog;
use postgress_rs::storage::ephemeral::EphemeralStorage;
use postgress_rs::types::Oid;
use std::sync::Arc;

mod common;

fn setup_e2e_benchmark() -> (
    Arc<EphemeralStorage>,
    Arc<SharedBufferCache>,
    Arc<Catalog>,
    Oid,
) {
    let storage = common::setup_storage();
    let cache = common::setup_cache(&storage);
    let catalog = common::setup_catalog(&storage);

    let rel_oid = common::create_test_table(
        &catalog,
        "e2e_users",
        vec![("id", Oid(23)), ("name", Oid(25)), ("value", Oid(23))],
    );
    cache.sync_from_catalog(&catalog);

    (storage, cache, catalog, rel_oid)
}

fn bench_e2e_insert_1000(c: &mut Criterion) {
    let (_storage, cache, _catalog, rel_oid) = setup_e2e_benchmark();

    c.bench_function("e2e_insert_1000", |b| {
        b.iter(|| {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                for i in 0..1000 {
                    let wal = postgress_rs::wal::WAL::new(65536);
                    let insert_op = postgress_rs::executor::heap::TupleInsert {
                        rel_oid,
                        values: vec![
                            i.to_string().into_bytes(),
                            format!("user_{}", i).into_bytes(),
                            (i * 10).to_string().into_bytes(),
                        ],
                    };
                    postgress_rs::executor::heap::tuple_insert(&cache, &wal, &insert_op)
                        .await
                        .unwrap();
                }
            })
        })
    });
}

fn bench_e2e_insert_bulk(c: &mut Criterion) {
    let (_storage, cache, _catalog, rel_oid) = setup_e2e_benchmark();

    c.bench_function("e2e_insert_bulk", |b| {
        b.iter(|| {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                let tuples: Vec<Vec<Vec<u8>>> = (0..10000)
                    .map(|i| {
                        vec![
                            i.to_string().into_bytes(),
                            format!("user_{}", i).into_bytes(),
                            (i * 10).to_string().into_bytes(),
                        ]
                    })
                    .collect();

                let wal = postgress_rs::wal::WAL::new(65536);
                let insert_op = postgress_rs::executor::heap::TupleInsertBulk { rel_oid, tuples };
                postgress_rs::executor::heap::tuple_insert_bulk(&cache, &wal, &insert_op)
                    .await
                    .unwrap();
            })
        })
    });
}

fn bench_e2e_select_star(c: &mut Criterion) {
    let (_storage, cache, catalog, rel_oid) = setup_e2e_benchmark();
    common::insert_test_data(&cache, &catalog, rel_oid, 1000);

    c.bench_function("e2e_select_star_1k", |b| {
        b.iter(|| {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                postgress_rs::executor::heap::heap_scan(&cache, rel_oid.0)
                    .await
                    .unwrap()
            })
        })
    });
}

fn bench_e2e_select_where(c: &mut Criterion) {
    let (_storage, cache, catalog, rel_oid) = setup_e2e_benchmark();
    common::insert_test_data(&cache, &catalog, rel_oid, 1000);

    c.bench_function("e2e_select_where_1k", |b| {
        b.iter(|| {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                let rows = postgress_rs::executor::heap::heap_scan(&cache, rel_oid.0)
                    .await
                    .unwrap();
                rows.into_iter()
                    .filter(|(_, values)| values.first().map_or(false, |v| v == "500"))
                    .collect::<Vec<_>>()
            })
        })
    });
}

fn bench_e2e_parse_and_plan(c: &mut Criterion) {
    c.bench_function("e2e_parse_and_plan", |b| {
        b.iter(|| {
            postgress_rs::sql::Parser::parse("SELECT id, name FROM users WHERE id > 100 ORDER BY name LIMIT 10").unwrap()
        })
    });
}

fn bench_e2e_full_pipeline(c: &mut Criterion) {
    let (_storage, cache, catalog, rel_oid) = setup_e2e_benchmark();
    common::insert_test_data(&cache, &catalog, rel_oid, 1000);

    c.bench_function("e2e_full_pipeline", |b| {
        b.iter(|| {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                let _ctx = postgress_rs::executor::select::ExecContext::new(&cache, &catalog);
                let rows = postgress_rs::executor::heap::heap_scan(&cache, rel_oid.0)
                    .await
                    .unwrap();

                let filtered: Vec<_> = rows
                    .into_iter()
                    .filter(|(_, values)| {
                        values
                            .first()
                            .map_or(false, |v| v.parse::<i32>().unwrap_or(0) > 500)
                    })
                    .collect();

                let mut sorted = filtered;
                sorted.sort_by(|a, b| a.1[1].cmp(&b.1[1]));

                sorted.into_iter().take(10).collect::<Vec<_>>()
            })
        })
    });
}

fn bench_e2e_concurrent_reads(c: &mut Criterion) {
    let (_storage, cache, catalog, rel_oid) = setup_e2e_benchmark();
    common::insert_test_data(&cache, &catalog, rel_oid, 1000);

    c.bench_function("e2e_concurrent_reads_4", |b| {
        b.iter(|| {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                let mut handles = vec![];
                for _ in 0..4 {
                    let cache_clone = cache.clone();
                    let oid = rel_oid;
                    handles.push(tokio::spawn(async move {
                        postgress_rs::executor::heap::heap_scan(&cache_clone, oid.0)
                            .await
                            .unwrap()
                    }));
                }
                for handle in handles {
                    handle.await.unwrap();
                }
            })
        })
    });
}

fn bench_e2e_mixed_workload(c: &mut Criterion) {
    let (_storage, cache, catalog, rel_oid) = setup_e2e_benchmark();
    common::insert_test_data(&cache, &catalog, rel_oid, 100);

    c.bench_function("e2e_mixed_workload", |b| {
        b.iter(|| {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                for i in 0..100 {
                    if i % 5 == 0 {
                        let wal = postgress_rs::wal::WAL::new(65536);
                        let insert_op = postgress_rs::executor::heap::TupleInsert {
                            rel_oid,
                            values: vec![
                                (1000 + i).to_string().into_bytes(),
                                format!("new_user_{}", i).into_bytes(),
                                (i * 100).to_string().into_bytes(),
                            ],
                        };
                        postgress_rs::executor::heap::tuple_insert(&cache, &wal, &insert_op)
                            .await
                            .unwrap();
                    } else {
                        let _ = postgress_rs::executor::heap::heap_scan(&cache, rel_oid.0)
                            .await
                            .unwrap();
                    }
                }
            })
        })
    });
}

criterion_group!(
    benches,
    bench_e2e_insert_1000,
    bench_e2e_insert_bulk,
    bench_e2e_select_star,
    bench_e2e_select_where,
    bench_e2e_parse_and_plan,
    bench_e2e_full_pipeline,
    bench_e2e_concurrent_reads,
    bench_e2e_mixed_workload
);

criterion_main!(benches);
