# Postgres-RS Roadmap

## Current State (M1 – Skeleton)

What exists today in the codebase:

- **Runtime**: Tokio async runtime for I/O and task scheduling
- **Storage traits**: `Storage` trait with `EphemeralStorage` (in-memory) and `MmapStorage` (file-backed) implementations
- **WAL (partial)**: Segment-based append log; no record type serialization, no WAL-to-data ordering guarantee yet
- **Buffer cache**: `SharedBufferCache` with mutable relation-state tracking
- **System catalog (partial)**: Bootstrap logic exists but `pg_class`, `pg_attribute`, `pg_type`, `pg_index` are not fully populated with correct rows
- **Heap executor**: Tuple insert and bulk-insert paths; no full-table scan with pointer tuples or offset mapping
- **B-tree**: Stub/skeleton only; no multi-page tree, no insert/search/delete
- **Planner**: Hard-coded `SeqScan` and `IndexScan` nodes; no statistics, no cost model, no optimizer
- **SQL parser**: Simple hand-rolled parser covering a narrow subset of statements
- **TCP listener stub**: Accepts connections but no Postgres wire protocol
- **SuiteShell**: TUI and server-lifecycle management CLI

---

## Near-term Milestones (Next Steps)

### M2: Query Processing Foundation

- Full SQL parser: integrate `pg_query_rust` or implement a custom recursive-descent parser supporting the full DML/DQL grammar (SELECT, INSERT, UPDATE, DELETE, CREATE TABLE, DROP, ALTER)
- Proper catalog bootstrapping: `pg_class`, `pg_attribute`, `pg_type`, `pg_index` populated correctly during `initdb`; `pg_namespace` for schemas; `pg_database` for databases
- Heap executor: `HeapScan` with pointer tuples (`ItemPointerData`), offset number mapping, visibility-check integration (stub until MVCC)
- B-tree index implementation: single-page index, then multi-page with internal/leaf pages, insert, search, delete, and split logic
- Simple planner/optimizer: statistics-based index-vs-seq-scan decision; basic cost model using tuple count and index selectivity estimates

### M3: Transaction Engine

- MVCC tuple headers: `xmin`, `xmax`, `cmax`, `cmin`, `xvac` (or `xid`-based equivalents)
- Concurrency control: strict 2PL locking or SSI (Serializable Snapshot Isolation)
- Transaction isolation levels: `READ COMMITTED`, `REPEATABLE READ`, `SERIALIZABLE`
- Two-phase commit stub: `PREPARE TRANSACTION`, `COMMIT PREPARED`, `ROLLBACK PREPARED`

### M4: Write-Ahead Log

- WAL record types: `HEAP` (insert/update/delete), `BTREE` (page splits, leaf inserts), `COMMIT`, `ABORT`, `CHECKPOINT`
- Write-Ahead Logging guarantee: data pages are flushed to disk only after corresponding WAL records are persisted (`XLogWrite` / `XLogFlush` semantics)
- Archiving / PITR hooks: `archive_command`, `restore_command`, `recovery_target_*` GUC stubs for future point-in-time recovery

### M5: PostgreSQL Wire Protocol

- Full frontend/backend message set:
  - Frontend: `StartupMessage`, `Query`, `Parse`, `Bind`, `Execute`, `Sync`, `Close`, `Describe`, `Flush`
  - Backend: `RowDescription`, `DataRow`, `CommandComplete`, `ReadyForQuery`, `ErrorResponse`, `ParameterStatus`, `NoticeResponse`
- Binary and text parameter formats (`text`, `binary` in `Bind`)
- Simple extended query protocol support (not necessarily full `prepared statement` caching initially)

### M6: Connection & Concurrency

- Per-connection process model (Tokio task-per-connection or thread pool fallback)
- Authentication methods: `trust`, `md5`, `scram-sha-256` per `pg_hba.conf`
- SSL/TLS support: TLS 1.2/1.3 via `rustls` or `tokio-rustls`; `sslmode` handling (`require`, `prefer`, `disable`)

### M7: Storage Engine Hardening

- Relation forks: `main`, `fsm` (free space map), `vm` (visibility map)
- Free space map (`pg_freespace`): tracks free space within heap pages for efficient inserts
- Visibility map (`pg_visibility`): tracks all-visible pages to accelerate vacuum and seq scans
- TOAST tables: automatic overflow of values > 2 KB into a separate TOAST relation with `extern` or `extended` storage strategy
- `COPY` protocol support: `COPY FROM STDIN` and `COPY TO STDOUT` in both text and binary formats

### M8: Advanced Features

- Subqueries and CTEs (`WITH ...`)
- Window functions (`OVER`, `PARTITION BY`, `ORDER BY`)
- JOIN executor: `NestedLoop`, `HashJoin`, `MergeJoin`
- Aggregates and `GROUP BY` / `HAVING`
- `UPDATE` / `DELETE` with `RETURNING`
- Join types: `CROSS JOIN`, `[INNER] JOIN`, `LEFT [OUTER] JOIN`, `RIGHT [OUTER] JOIN`, `FULL [OUTER] JOIN`

### M9: PostgreSQL Compatibility Layer

- `psql` compatibility: tab-completion timing, `psql` variables (`:variable`, `\set`)
- `pg_dump` / `pg_restore` wire-compatible output (custom or directory format, schema + data dump)
- `pgbench` basic TCP protocol support
- SQL compliance: most of Core SQL:2023 except advanced OLAP features (e.g., `MATCH_RECOGNIZE`, `POLARITY`, `TOP`/`FETCH FIRST` variants beyond basic `LIMIT`)

### M10: Performance & Production

- Buffer pool with clock-sweep eviction (replace or augment current `SharedBufferCache` eviction)
- Background writer process: periodically flushes dirty buffers to reduce checkpoint I/O spikes
- Checkpoint scheduling: timed or `pg_stat_bgwriter`-driven checkpoints with `checkpoint_completion_target`
- Parallel query: parallel seq scan, parallel append (optional workers for partitioned/bulk reads)
- JIT compilation (optional): LLVM-based expression evaluation via `pg_jit`-style inlining

---

## Compatibility Target

The goal is **wire-protocol compatibility** with standard `psql`/`libpq` clients so that existing PostgreSQL tooling can connect without modification.

SQL feature parity target is **PostgreSQL 17 core**, excluding the following subsystems which are out of scope for the initial compatibility target:

- Stored procedures (`CREATE PROCEDURE` / `CALL`)
- Declarative partitioning
- Logical replication / logical decoding
- ICU collations (use libc/OS collations only initially)
- Incremental sort
- Advanced procedural languages (PL/pgSQL, PL/Rust, etc.)
- Table access methods beyond heap and b-tree
- Columnar / foreign-data wrappers

---

## File Structure Target

```
postgress-rs/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── ROADMAP.md
├── LICENSE
├── AGENTS.md
├── .kilo/
│   ├── command/
│   └── agent/
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── bin/
│   │   ├── suiteshell.rs
│   │   ├── pg_ctl.rs
│   │   └── initdb.rs
│   ├── runtime/
│   │   ├── mod.rs
│   │   ├── process.rs
│   │   └── config.rs
│   ├── tcp/
│   │   ├── mod.rs
│   │   ├── listener.rs
│   │   └── auth.rs
│   ├── protocol/
│   │   ├── mod.rs
│   │   ├── message.rs
│   │   ├── frontend.rs
│   │   └── backend.rs
│   ├── sql/
│   │   ├── mod.rs
│   │   ├── lexer.rs
│   │   ├── parser.rs
│   │   ├── ast.rs
│   │   └── rewrite.rs
│   ├── catalog/
│   │   ├── mod.rs
│   │   ├── schema.rs
│   │   ├── table.rs
│   │   ├── type.rs
│   │   ├── index.rs
│   │   └── bootstrap.rs
│   ├── executor/
│   │   ├── mod.rs
│   │   ├── result.rs
│   │   ├── heap_scan.rs
│   │   ├── index_scan.rs
│   │   ├── seq_scan.rs
│   │   ├── nested_loop.rs
│   │   ├── hash_join.rs
│   │   ├── merge_join.rs
│   │   ├── aggregate.rs
│   │   └── modify.rs
│   ├── planner/
│   │   ├── mod.rs
│   │   ├── planner.rs
│   │   ├── optimizer.rs
│   │   └── path.rs
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── trait.rs
│   │   ├── ephemeral.rs
│   │   ├── mmap.rs
│   │   ├── page.rs
│   │   ├── buffer.rs
│   │   ├── buffer_pool.rs
│   │   ├── relation.rs
│   │   ├── fork.rs
│   │   ├── fsm.rs
│   │   ├── vm.rs
│   │   ├── toast.rs
│   │   └── oid.rs
│   ├── wal/
│   │   ├── mod.rs
│   │   ├── record.rs
│   │   ├── segment.rs
│   │   ├── flush.rs
│   │   ├── archive.rs
│   │   └── recovery.rs
│   ├── transaction/
│   │   ├── mod.rs
│   │   ├── txn.rs
│   │   ├── mvcc.rs
│   │   ├── lock.rs
│   │   ├── isolation.rs
│   │   └── twophase.rs
│   ├── btree/
│   │   ├── mod.rs
│   │   ├── page.rs
│   │   ├── insert.rs
│   │   ├── search.rs
│   │   └── delete.rs
│   └── utils/
│       ├── mod.rs
│       ├── error.rs
│       ├── sync.rs
│       ├── timing.rs
│       └── checksum.rs
├── tests/
│   ├── pg_wire.rs
│   ├── sql_compat.rs
│   ├── pgbench.rs
│   └── regression/
│       ├── heap.test.rs
│       ├── btree.test.rs
│       └── mvcc.test.rs
└── benches/
    ├── tpcb.rs
    ├── seqscan.rs
    └── insert.rs
```
