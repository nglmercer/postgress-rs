# Implementation Plan

## Current State Assessment

### What Works (Verified by 163 Tests)
- **Storage**: EphemeralStorage + MmapStorage fully functional
- **Heap**: Insert, scan, update, delete all working with MVCC-style tuple headers
- **B-tree**: Multi-page insert with recursive split, search with descent, serialize/deserialize
- **WAL**: Record serialization, XID allocation, segment-based append
- **Catalog**: Relation CRUD, bootstrap, OID allocation, cache sync
- **Protocol**: Frontend message decode (all PG wire types), backend encode (all 14 message types)
- **Parser**: SELECT, INSERT, UPDATE, DELETE, CREATE TABLE, DROP TABLE, BEGIN, COMMIT, ROLLBACK
- **Planner**: SeqScan selection, cost estimation stubs

### Known Stubs/Incomplete
| Component | Location | Status |
|-----------|----------|--------|
| `executor::btree::btree_insert` | `src/executor/btree.rs` | Always returns error |
| `executor::btree::btree_scan` | `src/executor/btree.rs` | Always returns error |
| `WAL::flush()` | `src/wal/mod.rs` | No-op (returns Ok) |
| `BufferPool` | `src/buffer_cache/mod.rs` | No-op (empty vecs, no eviction) |
| `Planner` | `src/executor/planner.rs` | Always SeqScan, no index selection |
| `Catalog::bootstrap()` | `src/catalog/mod.rs` | Creates tables but no real data rows |

---

## M2: Query Processing Foundation (Next Priority)

### 2.1 Wire the B-tree into the Executor
**Files**: `src/executor/btree.rs`
- Implement `btree_insert()` to call `btree::insert::btree_insert_multipage()`
- Implement `btree_scan()` to call `btree::search::btree_search()` and return results
- Wire `server.rs` INSERT to create/update B-tree index entries

### 2.2 Planner: Index-vs-SeqScan Decision
**Files**: `src/executor/planner.rs`
- Implement `Planner::plan()` to check for available indexes via catalog
- Use `seq_scan_cost()` vs `index_scan_cost()` to choose plan
- Add `IndexScan` execution path in `server.rs`

### 2.3 Full Table Scan with Filter Pushdown
**Files**: `src/executor/heap.rs`, `src/server.rs`
- Ensure `slow_scan` filter matches are correct for all data types
- Add type-aware comparison (numeric vs string)

---

## M3: Transaction Engine

### 3.1 Transaction Manager
**New files**: `src/transaction/mod.rs`, `src/transaction/txn.rs`
- `Transaction` struct: xid, isolation_level, snapshot, locks
- `TransactionManager`: begin, commit, rollback, get_snapshot
- Track active transactions and their XIDs

### 3.2 MVCC Visibility
**Files**: `src/executor/heap.rs`
- Implement proper `is_visible()` using snapshot-based visibility
- `tuple_visible(tup, snapshot)` checks xmin committed + xmax not committed
- Add `xmin_committed()` and `xmax_committed()` checks via transaction manager

### 3.3 Isolation Levels
**Files**: `src/transaction/isolation.rs`
- `READ COMMITTED`: new snapshot per statement
- `REPEATABLE READ`: same snapshot per transaction
- `SERIALIZABLE`: predicate locking (stub)

---

## M4: Write-Ahead Log

### 4.1 WAL Flush
**Files**: `src/wal/mod.rs`
- Implement actual `flush()`: sync WAL segments to "disk" (storage)
- Add WAL page dirty tracking

### 4.2 WAL-to-Data Ordering
**Files**: `src/executor/heap.rs`
- Before writing data pages, ensure WAL records are flushed
- Add `WAL::ensure_flushed(lsn)` method

---

## M5: PostgreSQL Wire Protocol

### 5.1 Use Frontend Decoder in Server
**Files**: `src/server.rs`
- Replace hand-rolled `Parser` with `Message::decode()` for incoming data
- Handle `Parse`, `Bind`, `Execute` messages for extended query protocol

### 5.2 Extended Query Protocol
**Files**: `src/server.rs`
- Implement prepared statement caching
- Portal-based execution

---

## M6: Connection & Concurrency

### 6.1 Per-Connection State
**Files**: `src/server.rs`
- Track transaction state per connection (Idle/InTransaction/Failed)
- Send `ReadyForQuery` with correct transaction status

### 6.2 Basic Locking
**Files**: `src/transaction/lock.rs`
- Row-level locks: `FOR UPDATE`, `FOR SHARE`
- Lock table on DDL operations

---

## Implementation Order

### Phase 3a: Wire B-tree + Planner (Week 1)
1. Implement `executor::btree::btree_insert` and `btree_scan`
2. Add index creation tracking to catalog
3. Implement planner index selection
4. Wire INSERT to update B-tree indexes

### Phase 3b: Transaction Basics (Week 2)
1. Create transaction manager
2. Implement snapshot-based visibility
3. Wire BEGIN/COMMIT/ROLLBACK to transaction manager
4. Add per-connection transaction state

### Phase 3c: WAL Hardening (Week 3)
1. Implement WAL flush
2. Add WAL-before-data ordering
3. Implement checkpoint logic (stub)

### Phase 3d: Wire Protocol (Week 4)
1. Integrate frontend decoder into server
2. Implement extended query protocol basics
3. Add `psql` compatibility features

---

## Test Coverage Summary

| Module | Tests | Status |
|--------|-------|--------|
| types | 8 | ✅ Complete |
| storage/ephemeral | 5 | ✅ Complete |
| storage/mmap | 7 | ✅ Complete |
| wal | 14 | ✅ Complete |
| buffer_cache | 0 (indirect) | Needs tests |
| catalog | 11 | ✅ Complete |
| executor/heap | 17 | ✅ Complete |
| executor/btree | 0 (stubs) | Needs impl + tests |
| executor/planner | 12 | ✅ Complete |
| protocol/frontend | 15 | ✅ Complete |
| protocol/backend | 19 | ✅ Complete |
| protocol/parser | 21 | ✅ Complete |
| btree/page | 10 | ✅ Complete |
| btree/insert | 10 | ✅ Complete |
| btree/search | 4 | ✅ Complete |
| btree/scan | 3 | ✅ Complete |
| **Total** | **163** | **All passing** |
