# postgress-rs

A PostgreSQL-compatible database server implemented from scratch in Rust, for educational purposes.

## Overview

postgress-rs reimplements core PostgreSQL subsystems: a complete SQL parser, heap storage engine with MVCC, buffer management, WAL with crash recovery, multiple index types, a query executor with joins/aggregations/window functions, and the PostgreSQL wire protocol.

## Quick Start

```bash
# Build
cargo build --release

# Run server (default port 5433)
cargo run --release

# Connect with psql
psql -h localhost -p 5433 -U postgres -d postgres
```

## Features

- **SQL Parser** -- SELECT (JOINs, CTEs, subqueries, window functions, set operations), INSERT/UPDATE/DELETE (with UPSERT, RETURNING), DDL, MERGE, transactions
- **Storage Engine** -- PostgreSQL-compatible heap pages, line pointers, HOT updates, VACUUM, TOAST, visibility map
- **Buffer Cache** -- Clock-sweep eviction, pin/unpin reference counting, double-buffering for sequential scans, background writer, checkpointing
- **WAL & Recovery** -- Write-ahead log with CRC32, archiving, point-in-time recovery (PITR)
- **MVCC Transactions** -- All four isolation levels, snapshot-based visibility, commit log
- **Locking** -- Table-level (8 modes), row-level (4 modes), deadlock detection, advisory locks
- **Index Types** -- B-tree (unique, multi-column, index-only scans), Hash, GIN, GiST, BRIN
- **Query Execution** -- Nested Loop / Hash / Merge joins, Sort/Hash aggregates, window functions, partition pruning (Range/List/Hash), parallel execution
- **Wire Protocol** -- Simple and Extended query protocol, SCRAM-SHA-256 / MD5 auth, connection pooling, cursors, LISTEN/NOTIFY, COPY
- **Security** -- RBAC (roles, privileges, GRANT/REVOKE), Row-Level Security, audit logging
- **Full-Text Search** -- TSVECTOR/TSQUERY types, ranking functions
- **JSONB** -- Operators for containment, existence, path queries

## Project Structure

```
src/
  main.rs                 # TCP server entry point
  lib.rs                  # Library root
  server/                 # Connection handling, query dispatch, expression evaluation
  sql/                    # Tokenizer, AST, full SQL parser
  storage/                # Heap pages, ephemeral/mmap backends, TOAST, visibility map
  buffer_cache/           # Buffer pool, bgwriter, checkpointer
  btree/                  # B-tree, Hash, GIN, GiST, BRIN indexes
  executor/               # Planner, heap ops, joins, aggregations, window functions, partitioning
  transaction/            # Transaction manager, locks, CLOG, timeouts
  wal/                    # WAL records, archiving, recovery
  protocol/               # Wire protocol, auth, extended query, LISTEN/NOTIFY, COPY
  catalog/                # System catalog (pg_class, pg_attribute, pg_type, pg_index)
  security/               # RBAC, row-level security, audit logging
  concurrency/            # Latch, condition variable, autovacuum daemon
  types/                  # Core types, JSONB, TSVECTOR, TSQUERY
benches/                  # Criterion benchmarks
benchmarks/               # Comparison binary against real PostgreSQL
```

## Usage

### Server Options

```bash
cargo run --release -- --port 5433 --data-dir ./data
```

### In-Memory Mode

Connect with `:memory:` as the data directory for a temporary in-memory database.

### Run Benchmarks

```bash
# Criterion benchmarks
cargo bench

# Compare against real PostgreSQL (requires local PG instance)
cargo run --bin pg_compare
```

### Run Tests

```bash
cargo test
```

## Building

Requires Rust 2021 edition or later.

```bash
cargo build --release
```

## License

Educational project -- no license specified.
