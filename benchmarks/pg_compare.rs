use postgres::{Client, NoTls};
use std::time::Instant;

fn main() {
    println!("PostgreSQL Benchmark Comparison");
    println!("==============================\n");

    let mut client = match Client::connect("host=localhost user=postgres dbname=postgres", NoTls) {
        Ok(c) => c,
        Err(e) => {
            println!("Could not connect to PostgreSQL: {}", e);
            println!("Make sure PostgreSQL is running and accessible.");
            return;
        }
    };

    setup_tables(&mut client);

    println!("--- Query Benchmarks ---\n");

    bench_select_simple(&mut client);
    bench_select_where(&mut client);
    bench_select_join(&mut client);
    bench_select_aggregate(&mut client);
    bench_select_order_limit(&mut client);
    bench_insert_single(&mut client);
    bench_insert_bulk(&mut client);

    cleanup(&mut client);

    println!("\n--- Complete ---");
}

fn setup_tables(client: &mut Client) {
    println!("Setting up benchmark tables...");

    client
        .execute("DROP TABLE IF EXISTS bench_orders", &[])
        .unwrap();
    client
        .execute("DROP TABLE IF EXISTS bench_users", &[])
        .unwrap();

    client
        .execute(
            "CREATE TABLE bench_users (
        id SERIAL PRIMARY KEY,
        name VARCHAR(100),
        email TEXT,
        value INTEGER
    )",
            &[],
        )
        .unwrap();

    client
        .execute(
            "CREATE TABLE bench_orders (
        id SERIAL PRIMARY KEY,
        user_id INTEGER REFERENCES bench_users(id),
        amount DECIMAL(10,2),
        created_at TIMESTAMP DEFAULT NOW()
    )",
            &[],
        )
        .unwrap();

    for i in 0..10000 {
        client
            .execute(
                "INSERT INTO bench_users (name, email, value) VALUES ($1, $2, $3)",
                &[
                    &format!("User {}", i),
                    &format!("user{}@example.com", i),
                    &i,
                ],
            )
            .unwrap();
    }

    for i in 0..10000 {
        let user_id = (i % 10000) + 1;
        let amount = format!("{:.2}", (i as f64) * 1.5);
        client
            .execute(
                "INSERT INTO bench_orders (user_id, amount) VALUES ($1, $2)",
                &[&user_id, &amount],
            )
            .unwrap();
    }

    println!("Tables created with 10000 users and 10000 orders.\n");
}

fn bench_select_simple(client: &mut Client) {
    let iterations = 100;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = client
            .query("SELECT * FROM bench_users WHERE id = 5000", &[])
            .unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "SELECT * WHERE id = 5000: {:.3}ms avg ({} iterations)",
        elapsed.as_secs_f64() * 1000.0 / iterations as f64,
        iterations
    );
}

fn bench_select_where(client: &mut Client) {
    let iterations = 100;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = client
            .query("SELECT * FROM bench_users WHERE value > 5000", &[])
            .unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "SELECT * WHERE value > 5000: {:.3}ms avg ({} iterations)",
        elapsed.as_secs_f64() * 1000.0 / iterations as f64,
        iterations
    );
}

fn bench_select_join(client: &mut Client) {
    let iterations = 100;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = client.query(
            "SELECT u.name, o.amount FROM bench_users u JOIN bench_orders o ON u.id = o.user_id WHERE u.id < 100",
            &[],
        ).unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "JOIN query: {:.3}ms avg ({} iterations)",
        elapsed.as_secs_f64() * 1000.0 / iterations as f64,
        iterations
    );
}

fn bench_select_aggregate(client: &mut Client) {
    let iterations = 100;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = client
            .query(
                "SELECT COUNT(*), SUM(value), AVG(value) FROM bench_users",
                &[],
            )
            .unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "Aggregate query: {:.3}ms avg ({} iterations)",
        elapsed.as_secs_f64() * 1000.0 / iterations as f64,
        iterations
    );
}

fn bench_select_order_limit(client: &mut Client) {
    let iterations = 100;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = client
            .query(
                "SELECT * FROM bench_users ORDER BY value DESC LIMIT 10",
                &[],
            )
            .unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "ORDER BY + LIMIT: {:.3}ms avg ({} iterations)",
        elapsed.as_secs_f64() * 1000.0 / iterations as f64,
        iterations
    );
}

fn bench_insert_single(client: &mut Client) {
    let iterations = 1000;
    let start = Instant::now();
    for i in 0..iterations {
        client
            .execute(
                "INSERT INTO bench_users (name, email, value) VALUES ($1, $2, $3)",
                &[
                    &format!("bench_user_{}", 10000 + i),
                    &format!("bench{}@test.com", i),
                    &i,
                ],
            )
            .unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "Single INSERT: {:.3}ms avg ({} iterations)",
        elapsed.as_secs_f64() * 1000.0 / iterations as f64,
        iterations
    );
}

fn bench_insert_bulk(client: &mut Client) {
    let iterations = 10;
    let batch_size = 1000;
    let start = Instant::now();
    for _ in 0..iterations {
        let mut transaction = client.transaction().unwrap();
        for i in 0..batch_size {
            transaction
                .execute(
                    "INSERT INTO bench_users (name, email, value) VALUES ($1, $2, $3)",
                    &[
                        &format!("bulk_user_{}", i),
                        &format!("bulk{}@test.com", i),
                        &i,
                    ],
                )
                .unwrap();
        }
        transaction.commit().unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "Bulk INSERT ({} rows/batch): {:.3}ms avg ({} iterations)",
        batch_size,
        elapsed.as_secs_f64() * 1000.0 / iterations as f64,
        iterations
    );
}

fn cleanup(client: &mut Client) {
    println!("\nCleaning up...");
    client
        .execute("DROP TABLE IF EXISTS bench_orders", &[])
        .unwrap();
    client
        .execute("DROP TABLE IF EXISTS bench_users", &[])
        .unwrap();
}
