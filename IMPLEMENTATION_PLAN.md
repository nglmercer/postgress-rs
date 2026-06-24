# Implementation Plan - Current State

## Summary
- **230 tests passing, 0 failures, 0 warnings**
- Complete SQL parser with AST, CTE support, all major PostgreSQL subsystems implemented

## Completed Features

### Phase A: Complete SQL Parser with AST
- **Full AST** (`src/sql/ast.rs`) — All statement/expression types for PostgreSQL DDL/DML
- **Recursive Descent Parser** (`src/sql/parser.rs`) — 2500+ lines, complete SQL parser
- **SELECT** with JOINs (INNER, LEFT, RIGHT, FULL, CROSS, LATERAL), GROUP BY/HAVING, ORDER BY (ASC/DESC, NULLS FIRST/LAST), LIMIT/OFFSET, DISTINCT/DISTINCT ON, subqueries, aliases
- **INSERT** with VALUES/subqueries, RETURNING, ON CONFLICT (DO NOTHING / DO UPDATE SET)
- **UPDATE** with FROM, SET, WHERE, RETURNING
- **DELETE** with USING, WHERE, RETURNING
- **CREATE TABLE** with column constraints (PK, FK, UNIQUE, CHECK, NOT NULL, DEFAULT)
- **CREATE INDEX** (multi-column), **CREATE VIEW** (parsed), **DROP TABLE/INDEX** (IF EXISTS, CASCADE)
- **ALTER TABLE** (ADD/DROP/RENAME COLUMN, alter column type)
- **BEGIN** with isolation level, READ ONLY, DEFERRABLE
- **UNION, UNION ALL, INTERSECT, INTERSECT ALL, EXCEPT, EXCEPT ALL**
- **WITH (CTE)** — Simple, recursive, multiple CTEs, MATERIALIZED/NOT MATERIALIZED hints
- **Expression parser** — arithmetic, comparison, logical, bitwise operators, CASE/WHEN, function calls with FILTER/OVER, parameter placeholders (?N), array literals

### Phase 3: Core Infrastructure
- **B-tree Executor** (`src/executor/btree.rs`) — insert + scan with multipage support
- **Planner Index Selection** (`src/executor/planner.rs`) — IndexScan vs SeqScan decision
- **Transaction Manager** (`src/transaction/mod.rs`) — begin/commit/rollback/snapshot visibility
- **WAL Flush** (`src/wal/mod.rs`) — flush tracking, ensure_flushed
- **CREATE INDEX** (parser + server + catalog) — End-to-end DDL support
- **BufferPool LRU** (`src/buffer_cache/mod.rs`) — fetch/pin/unpin/flush with LRU eviction

### Phase 4: Extended Query Protocol
- **Prepared Statement & Portal Tracking** (`src/protocol/extended.rs`) — Parse/Bind/Close state
- **Frontend Decoder Integration** — Server handles both simple and extended query protocols
- **Parse/Bind/Execute/Sync/Describe/Close** — Full extended query protocol support

### Phase 5: MVCC Visibility
- **Snapshot-based Visibility** (`src/executor/heap.rs`) — `is_visible()` with xmin/xmax checks
- **heap_scan_with_snapshot()** — Scan with explicit snapshot for transaction isolation
- **7 MVCC tests** — Committed, uncommitted, deleted, active delete, zero xmin, future xmin

### Server Integration
- **CTE Support** (`src/server.rs`) — WITH clause creates temporary relations for CTE names
- **Statement Dispatcher** — Full routing for all AST-based statement types

## Test Coverage by Module

| Module | Tests |
|--------|-------|
| types | 8 |
| storage/ephemeral | 5 |
| storage/mmap | 7 |
| wal | 10 |
| buffer_cache | 10 |
| catalog | 11 |
| executor/heap | 24 |
| executor/btree | 7 |
| executor/planner | 15 |
| protocol/frontend | 15 |
| protocol/backend | 19 |
| protocol/parser | 23 |
| protocol/extended | 8 |
| btree/page | 10 |
| btree/insert | 10 |
| btree/search | 4 |
| btree/scan | 3 |
| transaction | 9 |
| sql/parser | 23 |
| integration | 12 |
| **Total** | **230** |

## Next Steps

### Phase B: Type System
- Additional numeric types: `SERIAL`, `BIGSERIAL`, `SMALLSERIAL`, `MONEY`
- Date/Time functions: `NOW()`, `CURRENT_DATE`, `EXTRACT()`, `DATE_TRUNC()`
- String functions: `LENGTH()`, `UPPER()`, `LOWER()`, `TRIM()`, `SUBSTRING()`, `REPLACE()`
- Type casting: `CAST(x AS type)`, `x::type` syntax
- `IN` operator with subqueries
- `BETWEEN` operator
- `ANY`/`SOME` operators

### Phase C: Storage Engine
- Proper heap page format with line pointers
- Page-level compression support
- TOAST (The Oversized-Attribute Storage Technique)
- Vacuum and analyze

### Phase D: Transactions
- Transaction ID wraparound handling
- Table-level locking (ROW EXCLUSIVE, SHARE, etc.)
- Row-level locking (FOR UPDATE, FOR SHARE)
- Deadlock detection

### Phase E: Indexes
- Multi-column index scans
- Index-only scans
- Hash indexes
- GIN/GiST indexes

### Phase F: Query Execution
- Join algorithms: nested loop, hash join, merge join
- Cost-based optimizer
- Table statistics (pg_stat)
- Parallel query execution

### Phase G: WAL/Recovery
- Full WAL with page LSN tracking
- Checkpoint support
- Point-in-time recovery (PITR)
- Streaming replication

### Phase H: Advanced Features
- PL/pgSQL procedural language
- Table partitioning (range, hash, list)
- Full-text search
- JSON/JSONB operators
- Common table expression materialization

### Phase I: Security
- Role-based access control (RBAC)
- Row-level security (RLS)
- SSL/TLS support
