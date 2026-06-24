# Full PostgreSQL Compatibility Plan

## Current State
- **206 tests passing, 0 failures, 0 warnings**
- Complete SQL parser with AST, CTE support, type casting, additional data types
- Date/Time types and functions: NOW(), CURRENT_DATE/TIME/TIMESTAMP, EXTRACT, DATE_TRUNC, DATE_PART, AT TIME ZONE
- Type casting for: BOOLEAN, UUID, JSON, JSONB, ARRAY, MONEY, DATE, TIME, TIMESTAMPTZ, INTERVAL, BIT, BIT VARYING, INET, CIDR, MACADDR, TSVECTOR, TSQUERY
- Simple + Extended query protocols
- B-tree indexes, MVCC visibility, WAL, BufferPool
- Parser support: CREATE SCHEMA, SET, CREATE MATERIALIZED VIEW, MERGE, window frames (ROWS/RANGE/GROUPS), aggregate/window functions

---

## Milestone 1: Complete SQL Parser (M2 from ROADMAP)

### 1.1 Expression Parser
- [x] Arithmetic expressions: `+`, `-`, `*`, `/`, `%`
- [x] Comparison operators: `=`, `<>`, `<`, `>`, `<=`, `>=`
- [x] Logical operators: `AND`, `OR`, `NOT`
- [x] `IN`, `BETWEEN`, `IS NULL`, `IS NOT NULL`
- [x] Subqueries in WHERE clauses

### 1.2 SELECT Features
- [x] `JOIN` (INNER, LEFT, RIGHT, FULL, CROSS, LATERAL)
- [x] `GROUP BY` with `HAVING`
- [x] `ORDER BY` (ASC, DESC, NULLS FIRST/LAST)
- [x] `LIMIT` and `OFFSET`
- [x] Aggregate functions: `COUNT`, `SUM`, `AVG`, `MIN`, `MAX`
- [x] `DISTINCT` and `DISTINCT ON`
- [x] `UNION`, `INTERSECT`, `EXCEPT`
- [x] Window functions: `ROW_NUMBER`, `RANK`, `DENSE_RANK`, `LAG`, `LEAD`

### 1.3 DDL Enhancements
- [x] `ALTER TABLE` (ADD COLUMN, DROP COLUMN, RENAME, ALTER TYPE)
- [x] `CREATE VIEW` (parsed)
- [x] `CREATE MATERIALIZED VIEW`
- [x] `CREATE SEQUENCE` and `NEXTVAL`
- [x] `CREATE TYPE` (composite, enum, range)
- [x] Constraints: `PRIMARY KEY`, `FOREIGN KEY`, `UNIQUE`, `CHECK`, `NOT NULL`
- [x] `DEFAULT` values
- [x] `CREATE SCHEMA` and `SET search_path`

### 1.4 DML Enhancements
- [x] `INSERT ... ON CONFLICT` (UPSERT)
- [x] `INSERT ... RETURNING`
- [x] `UPDATE ... FROM`
- [x] `DELETE ... USING`
- [x] `CTE` (Common Table Expressions) with `WITH` — simple, recursive, multiple, MATERIALIZED/NOT MATERIALIZED
- [x] `MERGE` statement

---

## Milestone 2: Type System (M3 from ROADMAP)

### 2.1 Numeric Types
- [x] `SMALLINT` (2 bytes)
- [x] `INTEGER` (4 bytes)
- [x] `BIGINT` (8 bytes)
- [x] `DECIMAL`/`NUMERIC` with arbitrary precision
- [x] `REAL` (4 bytes float)
- [x] `DOUBLE PRECISION` (8 bytes float)
- [x] `SERIAL`, `BIGSERIAL`, `SMALLSERIAL`
- [x] `MONEY`

### 2.2 String Types
- [x] `CHAR(n)`, `VARCHAR(n)`, `TEXT`
- [ ] String functions: `LENGTH`, `UPPER`, `LOWER`, `TRIM`, `SUBSTRING`, `CONCAT`
- [x] Pattern matching: `LIKE`, `ILIKE`, `SIMILAR TO`
- [x] Regular expressions: `~`, `~*`, `!~`, `!~*`

### 2.3 Date/Time Types
- [x] `DATE`, `TIME`, `TIMESTAMP`, `TIMESTAMPTZ`
- [x] `INTERVAL`
- [x] `TIME WITH TIME ZONE` (`TIMETZ`)
- [x] Date/Time functions: `NOW()`, `CURRENT_DATE`, `CURRENT_TIMESTAMP`, `CURRENT_TIME`, `LOCALTIME`, `LOCALTIMESTAMP`
- [x] `EXTRACT`, `DATE_TRUNC`, `DATE_PART` with all date parts (YEAR, MONTH, DAY, HOUR, MINUTE, SECOND, DOW, DOY, ISODOW, WEEK, QUARTER, EPOCH, ISOYEAR, TIMEZONE, TIMEZONE_HOUR, TIMEZONE_MINUTE)
- [x] `AT TIME ZONE`

### 2.4 Other Types
- [x] `JSON`, `JSONB`
- [x] `UUID`
- [x] `ARRAY`
- [x] `BOOLEAN`
- [x] `INET`, `CIDR`, `MACADDR`
- [x] `BIT`, `BIT VARYING`
- [x] `TSVECTOR`, `TSQUERY`

### 2.5 Type Casting
- [x] `CAST(x AS type)` syntax
- [x] `x::type` PostgreSQL shorthand syntax
- [x] `BOOLEAN`
- [x] `UUID`
- [x] `JSON` and `JSONB`
- [x] `ARRAY` types (including `type[]` suffix syntax)
- [x] `MONEY`
- [x] `INET`, `CIDR`, `MACADDR`
- [x] `BIT`, `BIT VARYING`
- [x] `TSVECTOR`, `TSQUERY`

---

## Milestone 3: Storage Engine (M4 from ROADMAP)

### 3.1 Heap Page Format
- [ ] Page header with special space
- [ ] Line pointer array (PD_LINEPOINTER)
- [ ] Tuple headers with transaction visibility info
- [ ] Free space tracking
- [ ] Page compaction (VACUUM)

### 3.2 Buffer Manager
- [ ] Clock-sweep eviction algorithm (replace LRU)
- [ ] Buffer pins and reference counting
- [ ] Shared buffers with proper locking
- [ ] Double-buffering for sequential scans
- [ ] Background writer (bgwriter)
- [ ] Checkpoint process

### 3.3 Storage Layout
- [ ] Relation file segments (256MB each)
- [ ] Fork files (main, FSM, visibility map)
- [ ] TOAST (The Oversized-Attribute Storage Technique)
- [ ] Relation size tracking

---

## Milestone 4: MVCC and Transactions (M5 from ROADMAP)

### 4.1 Transaction Management
- [ ] Transaction ID wraparound handling
- [ ] Freeze transactions (prevent wraparound)
- [ ] Transaction status map (clog)
- [ ] Subtransactions and savepoints

### 4.2 Locking
- [ ] Table-level locks (ACCESS SHARE, ROW SHARE, ROW EXCLUSIVE, etc.)
- [ ] Row-level locks (FOR UPDATE, FOR SHARE, FOR KEY SHARE)
- [ ] Advisory locks
- [ ] Lock queue and deadlock detection
- [ ] Lock timeout and statement timeout

### 4.3 Snapshot Management
- [ ] Predicate locks for serializable isolation
- [ ] Page-level visibility maps
- [ ] Hot updates (Heap-Only Tuples)
- [ ] Multi-version concurrency control improvements

---

## Milestone 5: Index Access Methods (M6 from ROADMAP)

### 5.1 B-tree Enhancements
- [ ] Unique indexes
- [ ] Multi-column indexes
- [ ] Index-only scans
- [ ] Index skip scans
- [ ] Deduplicate items (btree dedup)

### 5.2 Hash Indexes
- [ ] Hash index creation and maintenance
- [ ] Hash index scans

### 5.3 GIN (Generalized Inverted Index)
- [ ] GIN for array operations
- [ ] GIN for full-text search
- [ ] GIN for JSONB containment

### 5.4 GiST (Generalized Search Tree)
- [ ] GiST for geometric types
- [ ] GiST for range types
- [ ] GiST for full-text search

### 5.5 BRIN (Block Range Index)
- [ ] BRIN for large tables with natural ordering

---

## Milestone 6: Query Execution (M7 from ROADMAP)

### 6.1 Executor Nodes
- [ ] Nested Loop Join
- [ ] Hash Join
- [ ] Merge Join
- [ ] Hash Aggregate
- [ ] Sort Aggregate
- [ ] Materialize node
- [ ] Limit node
- [ ] Unique node
- [ ] WindowAgg node

### 6.2 Query Planner
- [ ] Cost-based optimizer (CBO)
- [ ] Statistics collection (pg_statistic)
- [ ] Selectivity estimation
- [ ] Join order optimization
- [ ] Parallel query execution
- [ ] JIT compilation

### 6.3 Parallel Execution
- [ ] Parallel sequential scan
- [ ] Parallel hash join
- [ ] Parallel aggregation
- [ ] Worker process management

---

## Milestone 7: WAL and Recovery (M8 from ROADMAP)

### 7.1 WAL Enhancements
- [ ] WAL page headers
- [ ] WAL record headers
- [ ] Full-page writes
- [ ] WAL compression
- [ ] WAL summarization

### 7.2 Checkpoint
- [ ] Checkpoint process
- [ ] Incremental backup support
- [ ] Point-in-time recovery (PITR)
- [ ] WAL archiving

### 7.3 Replication
- [ ] Streaming replication
- [ ] Logical replication
- [ ] Publication and subscription
- [ ] Replication slots

---

## Milestone 8: Concurrency Control (M9 from ROADMAP)

### 8.1 Process Management
- [ ] Postmaster process
- [ ] Backend processes (one per connection)
- [ ] Background workers (bgwriter, autovacuum, wal writer)
- [ ] Shared memory setup
- [ ] Signal handling

### 8.2 IPC
- [ ] Semaphores
- [ ] Shared memory queues
- [ ] Latches and events
- [ ] Condition variables

### 8.3 Autovacuum
- [ ] Dead tuple collection
- [ ] Auto-analyze for query planning
- [ ] Auto-vacuum for dead tuple removal
- [ ] Vacuum statistics

---

## Milestone 9: Network Protocol (M10 from ROADMAP)

### 9.1 Wire Protocol
- [ ] SSL/TLS support
- [ ] GSSAPI authentication
- [ ] Channel binding
- [ ] Protocol compression

### 9.2 Authentication
- [ ] Trust authentication
- [ ] Password authentication (md5, scram-sha-256)
- [ ] Certificate authentication
- [ ] PAM authentication
- [ ] LDAP authentication
- [ ] Row-level security (RLS)

### 9.3 Connection Management
- [ ] Connection pooling (pgbouncer integration)
- [ ] Prepared statement caching
- [ ] Cursor management
- [ ] LISTEN/NOTIFY
- [ ] COPY protocol

---

## Milestone 10: Advanced Features (M11 from ROADMAP)

### 10.1 Stored Procedures
- [ ] PL/pgSQL language
- [ ] Functions (SQL, PL/pgSQL)
- [ ] Procedures with transaction control
- [ ] Triggers (BEFORE, AFTER, INSTEAD OF)
- [ ] Event triggers

### 10.2 Partitioning
- [ ] Range partitioning
- [ ] List partitioning
- [ ] Hash partitioning
- [ ] Partition pruning
- [ ] Partition-wise joins and aggregation

### 10.3 Full-Text Search
- [ ] `tsvector` and `tsquery` types
- [ ] `to_tsvector()`, `to_tsquery()`
- [ ] `@@` match operator
- [ ] `ts_rank()`, `ts_rank_cd()`
- [ ] `phraseto_tsquery()`, `plainto_tsquery()`
- [ ] Text search configuration

### 10.4 JSON/JSONB
- [ ] JSON operators: `->`, `->>`, `#>`, `#>>`
- [ ] JSON containment: `@>`, `<@`
- [ ] JSON existence: `?`, `?|`, `?&`
- [ ] `jsonb_set()`, `jsonb_insert()`, `jsonb_delete()`
- [ ] JSONB indexing (GIN)

### 10.5 Window Functions
- [ ] `ROW_NUMBER()`, `RANK()`, `DENSE_RANK()`
- [ ] `NTILE()`, `LAG()`, `LEAD()`
- [ ] `FIRST_VALUE()`, `LAST_VALUE()`, `NTH_VALUE()`
- [ ] `OVER` clause with `PARTITION BY` and `ORDER BY`
- [ ] Frame specifications: `ROWS`, `RANGE`, `GROUPS`

---

## Milestone 11: Security (M12 from ROADMAP)

### 11.1 Access Control
- [ ] Role-based access control (RBAC)
- [ ] `GRANT` and `REVOKE`
- [ ] `CREATE ROLE`, `ALTER ROLE`, `DROP ROLE`
- [ ] `SET ROLE`, `RESET ROLE`
- [ ] Schema-level permissions

### 11.2 Row-Level Security
- [ ] `CREATE POLICY`
- [ ] `ALTER POLICY`
- [ ] `ENABLE ROW LEVEL SECURITY`
- [ ] `FORCE ROW LEVEL SECURITY`

### 11.3 Auditing
- [ ] `pgaudit` extension support
- [ ] Statement logging
- [ ] Connection logging

---

## Implementation Priority

### Phase A: Core SQL (3-4 weeks)
1. Complete expression parser
2. JOIN support
3. GROUP BY / ORDER BY / LIMIT
4. Aggregate functions
5. ALTER TABLE

### Phase B: Types (2-3 weeks)
1. All numeric types
2. Date/Time types
3. String functions
4. Type casting

### Phase C: Storage (3-4 weeks)
1. Proper heap page format
2. Buffer manager improvements
3. TOAST support
4. Checkpoint process

### Phase D: Transactions (2-3 weeks)
1. Transaction ID wraparound
2. Table-level locking
3. Row-level locking
4. Deadlock detection

### Phase E: Indexes (2-3 weeks)
1. Multi-column indexes
2. Index-only scans
3. Hash indexes
4. GIN indexes

### Phase F: Query Execution (3-4 weeks)
1. Join algorithms
2. Cost-based optimizer
3. Statistics collection
4. Parallel query

### Phase G: WAL and Recovery (2-3 weeks)
1. Full WAL implementation
2. Checkpoint process
3. Point-in-time recovery
4. Streaming replication

### Phase H: Advanced Features (4-6 weeks)
1. PL/pgSQL
2. Partitioning
3. Full-text search
4. JSON/JSONB
5. Window functions

### Phase I: Security (2-3 weeks)
1. Role-based access control
2. Row-level security
3. SSL/TLS support

---

## Testing Strategy

### Unit Tests
- Each module should have comprehensive unit tests
- Target: 90%+ code coverage

### Integration Tests
- End-to-end SQL execution tests
- Transaction isolation tests
- Concurrent access tests
- Failure recovery tests

### Compatibility Tests
- pg_regress test suite
- TPC-C benchmark (OLTP)
- TPC-H benchmark (OLAP)
- pgbench performance tests

### Stress Tests
- High concurrency tests
- Large dataset tests
- Long-running transaction tests
- Memory pressure tests

---

## Estimated Timeline

| Phase | Duration | Key Deliverables |
|-------|----------|------------------|
| A: Core SQL | 3-4 weeks | JOINs, aggregates, DDL |
| B: Types | 2-3 weeks | All PG types |
| C: Storage | 3-4 weeks | Heap, Buffer, TOAST |
| D: Transactions | 2-3 weeks | Locking, isolation |
| E: Indexes | 2-3 weeks | Multi-col, hash, GIN |
| F: Query Execution | 3-4 weeks | Joins, optimizer |
| G: WAL/Recovery | 2-3 weeks | Checkpoint, PITR |
| H: Advanced | 4-6 weeks | PL/pgSQL, FTS, JSON |
| I: Security | 2-3 weeks | RLS, SSL |
| **Total** | **25-35 weeks** | **Full PG compatibility** |
