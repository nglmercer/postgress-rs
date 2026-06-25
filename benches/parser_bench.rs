use criterion::{criterion_group, criterion_main, Criterion};
use postgress_rs::sql::Parser;

fn bench_parse_simple_select(c: &mut Criterion) {
    c.bench_function("parse_simple_select", |b| {
        b.iter(|| Parser::parse("SELECT * FROM users WHERE id = 1").unwrap())
    });
}

fn bench_parse_complex_join(c: &mut Criterion) {
    let query = "SELECT a.*, b.name FROM orders a JOIN users b ON a.user_id = b.id WHERE a.total > 100 GROUP BY b.name";
    c.bench_function("parse_complex_join", |b| {
        b.iter(|| Parser::parse(query).unwrap())
    });
}

fn bench_parse_cte(c: &mut Criterion) {
    let query = "WITH cte AS (SELECT id, name FROM users WHERE active = true) SELECT * FROM cte WHERE id > 10";
    c.bench_function("parse_cte", |b| b.iter(|| Parser::parse(query).unwrap()));
}

fn bench_parse_window(c: &mut Criterion) {
    let query =
        "SELECT RANK() OVER (PARTITION BY department ORDER BY salary DESC) as rank FROM employees";
    c.bench_function("parse_window", |b| b.iter(|| Parser::parse(query).unwrap()));
}

fn bench_parse_subquery(c: &mut Criterion) {
    let query = "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders WHERE total > 1000)";
    c.bench_function("parse_subquery", |b| {
        b.iter(|| Parser::parse(query).unwrap())
    });
}

fn bench_parse_insert(c: &mut Criterion) {
    let query = "INSERT INTO users (name, email, age) VALUES ('John', 'john@example.com', 30)";
    c.bench_function("parse_insert", |b| b.iter(|| Parser::parse(query).unwrap()));
}

fn bench_parse_create_table(c: &mut Criterion) {
    let query = "CREATE TABLE users (id SERIAL PRIMARY KEY, name VARCHAR(100) NOT NULL, email TEXT UNIQUE, created_at TIMESTAMP DEFAULT NOW())";
    c.bench_function("parse_create_table", |b| {
        b.iter(|| Parser::parse(query).unwrap())
    });
}

fn bench_parse_update(c: &mut Criterion) {
    let query = "UPDATE users SET name = 'Jane', email = 'jane@example.com' WHERE id = 1";
    c.bench_function("parse_update", |b| b.iter(|| Parser::parse(query).unwrap()));
}

fn bench_parse_delete(c: &mut Criterion) {
    let query = "DELETE FROM users WHERE id = 1";
    c.bench_function("parse_delete", |b| b.iter(|| Parser::parse(query).unwrap()));
}

criterion_group!(
    benches,
    bench_parse_simple_select,
    bench_parse_complex_join,
    bench_parse_cte,
    bench_parse_window,
    bench_parse_subquery,
    bench_parse_insert,
    bench_parse_create_table,
    bench_parse_update,
    bench_parse_delete
);

criterion_main!(benches);
