use crate::types::Oid;
use crate::transaction::TransactionId;
use std::collections::{HashMap, VecDeque};
use parking_lot::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockMode {
    // Table-level locks
    AccessShare,
    RowShare,
    RowExclusive,
    ShareUpdateExclusive,
    Share,
    ShareRowExclusive,
    Exclusive,
    AccessExclusive,
    // Row-level locks
    ForUpdate,
    ForShare,
    ForNoKeyUpdate,
    ForKeyShare,
}

impl LockMode {
    pub fn is_table_lock(&self) -> bool {
        !matches!(self, LockMode::ForUpdate | LockMode::ForShare | LockMode::ForNoKeyUpdate | LockMode::ForKeyShare)
    }

    pub fn conflicts_with(&self, other: &LockMode) -> bool {
        use LockMode::*;
        match (self, other) {
            // Table-level conflicts
            (AccessShare, Exclusive) | (Exclusive, AccessShare) => true,
            (AccessShare, AccessExclusive) | (AccessExclusive, AccessShare) => true,
            (RowShare, RowExclusive) | (RowExclusive, RowShare) => true,
            (RowShare, ShareUpdateExclusive) | (ShareUpdateExclusive, RowShare) => true,
            (RowShare, Exclusive) | (Exclusive, RowShare) => true,
            (RowShare, AccessExclusive) | (AccessExclusive, RowShare) => true,
            (RowExclusive, ShareUpdateExclusive) | (ShareUpdateExclusive, RowExclusive) => true,
            (RowExclusive, Share) | (Share, RowExclusive) => true,
            (RowExclusive, ShareRowExclusive) | (ShareRowExclusive, RowExclusive) => true,
            (RowExclusive, Exclusive) | (Exclusive, RowExclusive) => true,
            (RowExclusive, AccessExclusive) | (AccessExclusive, RowExclusive) => true,
            (ShareUpdateExclusive, Share) | (Share, ShareUpdateExclusive) => true,
            (ShareUpdateExclusive, ShareRowExclusive) | (ShareRowExclusive, ShareUpdateExclusive) => true,
            (ShareUpdateExclusive, Exclusive) | (Exclusive, ShareUpdateExclusive) => true,
            (ShareUpdateExclusive, AccessExclusive) | (AccessExclusive, ShareUpdateExclusive) => true,
            (Share, ShareRowExclusive) | (ShareRowExclusive, Share) => true,
            (Share, Exclusive) | (Exclusive, Share) => true,
            (Share, AccessExclusive) | (AccessExclusive, Share) => true,
            (ShareRowExclusive, Exclusive) | (Exclusive, ShareRowExclusive) => true,
            (ShareRowExclusive, AccessExclusive) | (AccessExclusive, ShareRowExclusive) => true,
            (Exclusive, AccessExclusive) | (AccessExclusive, Exclusive) => true,
            // Row-level conflicts
            (ForUpdate, ForUpdate) | (ForUpdate, ForShare) | (ForShare, ForUpdate) => true,
            (ForUpdate, ForNoKeyUpdate) | (ForNoKeyUpdate, ForUpdate) => true,
            (ForUpdate, ForKeyShare) | (ForKeyShare, ForUpdate) => true,
            (ForShare, ForNoKeyUpdate) | (ForNoKeyUpdate, ForShare) => true,
            (ForShare, ForKeyShare) | (ForKeyShare, ForShare) => true,
            (ForNoKeyUpdate, ForNoKeyUpdate) => true,
            (ForKeyShare, ForKeyShare) => false,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LockRequest {
    pub xid: TransactionId,
    pub mode: LockMode,
    pub relation_oid: Oid,
    pub page_id: Option<u32>,
    pub tuple_offset: Option<u16>,
}

#[derive(Debug)]
struct LockEntry {
    holder: TransactionId,
    mode: LockMode,
}

#[derive(Debug)]
pub struct LockManager {
    table_locks: Mutex<HashMap<Oid, Vec<LockEntry>>>,
    row_locks: Mutex<HashMap<(Oid, u32, u16), Vec<LockEntry>>>,
    wait_queue: Mutex<VecDeque<LockRequest>>,
    /// Advisory locks: key -> list of (holder_xid, mode, is_exclusive)
    advisory_locks: Mutex<HashMap<i64, Vec<AdvisoryLockEntry>>>,
}

/// Mode for an advisory lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvisoryLockMode {
    /// Exclusive advisory lock – only one holder permitted at a time.
    Exclusive,
    /// Shared advisory lock – multiple shared holders are allowed concurrently.
    Shared,
}

#[derive(Debug, Clone)]
struct AdvisoryLockEntry {
    holder: TransactionId,
    mode: AdvisoryLockMode,
}

impl LockManager {
    pub fn new() -> Self {
        Self {
            table_locks: Mutex::new(HashMap::new()),
            row_locks: Mutex::new(HashMap::new()),
            wait_queue: Mutex::new(VecDeque::new()),
            advisory_locks: Mutex::new(HashMap::new()),
        }
    }

    pub fn acquire_table_lock(
        &self,
        xid: TransactionId,
        relation_oid: Oid,
        mode: LockMode,
    ) -> anyhow::Result<()> {
        let mut table_locks = self.table_locks.lock();
        let locks = table_locks.entry(relation_oid).or_insert_with(Vec::new);

        // Check for conflicts with existing locks
        for existing in locks.iter() {
            if existing.holder != xid && mode.conflicts_with(&existing.mode) {
                // Conflict detected - add to wait queue
                drop(table_locks);
                let request = LockRequest {
                    xid,
                    mode,
                    relation_oid,
                    page_id: None,
                    tuple_offset: None,
                };
                self.wait_queue.lock().push_back(request);
                return Err(anyhow::anyhow!("Lock conflict on relation {:?}", relation_oid));
            }
        }

        locks.push(LockEntry { holder: xid, mode });
        Ok(())
    }

    pub fn release_table_lock(&self, xid: TransactionId, relation_oid: Oid) {
        let mut table_locks = self.table_locks.lock();
        if let Some(locks) = table_locks.get_mut(&relation_oid) {
            locks.retain(|l| l.holder != xid);
            if locks.is_empty() {
                table_locks.remove(&relation_oid);
            }
        }
    }

    pub fn acquire_row_lock(
        &self,
        xid: TransactionId,
        relation_oid: Oid,
        page_id: u32,
        tuple_offset: u16,
        mode: LockMode,
    ) -> anyhow::Result<()> {
        let mut row_locks = self.row_locks.lock();
        let key = (relation_oid, page_id, tuple_offset);
        let locks = row_locks.entry(key).or_insert_with(Vec::new);

        // Check for conflicts
        for existing in locks.iter() {
            if existing.holder != xid && mode.conflicts_with(&existing.mode) {
                drop(row_locks);
                let request = LockRequest {
                    xid,
                    mode,
                    relation_oid,
                    page_id: Some(page_id),
                    tuple_offset: Some(tuple_offset),
                };
                self.wait_queue.lock().push_back(request);
                return Err(anyhow::anyhow!("Row lock conflict on {:?}:{:?}", page_id, tuple_offset));
            }
        }

        locks.push(LockEntry { holder: xid, mode });
        Ok(())
    }

    pub fn release_row_lock(&self, xid: TransactionId, relation_oid: Oid, page_id: u32, tuple_offset: u16) {
        let mut row_locks = self.row_locks.lock();
        let key = (relation_oid, page_id, tuple_offset);
        if let Some(locks) = row_locks.get_mut(&key) {
            locks.retain(|l| l.holder != xid);
            if locks.is_empty() {
                row_locks.remove(&key);
            }
        }
    }

    pub fn release_all_locks(&self, xid: TransactionId) {
        // Release all table locks
        let mut table_locks = self.table_locks.lock();
        for locks in table_locks.values_mut() {
            locks.retain(|l| l.holder != xid);
        }
        table_locks.retain(|_, locks| !locks.is_empty());

        // Release all row locks
        let mut row_locks = self.row_locks.lock();
        for locks in row_locks.values_mut() {
            locks.retain(|l| l.holder != xid);
        }
        row_locks.retain(|_, locks| !locks.is_empty());
    }

    pub fn detect_deadlock(&self) -> Option<Vec<TransactionId>> {
        let wait_queue = self.wait_queue.lock();
        if wait_queue.is_empty() {
            return None;
        }

        // Build wait-for graph
        let mut wait_for: HashMap<TransactionId, Vec<TransactionId>> = HashMap::new();
        let table_locks = self.table_locks.lock();
        let row_locks = self.row_locks.lock();

        for request in wait_queue.iter() {
            let mut blockers = Vec::new();

            if request.page_id.is_some() {
                // Row lock - check row locks
                let key = (request.relation_oid, request.page_id.unwrap(), request.tuple_offset.unwrap());
                if let Some(locks) = row_locks.get(&key) {
                    for lock in locks {
                        if lock.holder != request.xid && request.mode.conflicts_with(&lock.mode) {
                            blockers.push(lock.holder);
                        }
                    }
                }
            } else {
                // Table lock
                if let Some(locks) = table_locks.get(&request.relation_oid) {
                    for lock in locks {
                        if lock.holder != request.xid && request.mode.conflicts_with(&lock.mode) {
                            blockers.push(lock.holder);
                        }
                    }
                }
            }

            if !blockers.is_empty() {
                wait_for.entry(request.xid).or_default().extend(blockers);
            }
        }

        // DFS for cycle detection
        let mut visited = std::collections::HashSet::new();
        let mut stack = std::collections::HashSet::new();

        for &xid in wait_for.keys() {
            if let Some(cycle) = self.detect_cycle(xid, &wait_for, &mut visited, &mut stack) {
                return Some(cycle);
            }
        }

        None
    }

    fn detect_cycle(
        &self,
        xid: TransactionId,
        wait_for: &HashMap<TransactionId, Vec<TransactionId>>,
        visited: &mut std::collections::HashSet<TransactionId>,
        stack: &mut std::collections::HashSet<TransactionId>,
    ) -> Option<Vec<TransactionId>> {
        visited.insert(xid);
        stack.insert(xid);

        if let Some(blockers) = wait_for.get(&xid) {
            for &blocker in blockers {
                if !visited.contains(&blocker) {
                    if let Some(cycle) = self.detect_cycle(blocker, wait_for, visited, stack) {
                        return Some(cycle);
                    }
                } else if stack.contains(&blocker) {
                    return Some(vec![xid, blocker]);
                }
            }
        }

        stack.remove(&xid);
        None
    }

    pub fn table_lock_holders(&self, relation_oid: Oid) -> Vec<(TransactionId, LockMode)> {
        let table_locks = self.table_locks.lock();
        table_locks.get(&relation_oid)
            .map(|locks| locks.iter().map(|l| (l.holder, l.mode)).collect())
            .unwrap_or_default()
    }

    pub fn row_lock_holders(&self, relation_oid: Oid, page_id: u32, tuple_offset: u16) -> Vec<(TransactionId, LockMode)> {
        let row_locks = self.row_locks.lock();
        let key = (relation_oid, page_id, tuple_offset);
        row_locks.get(&key)
            .map(|locks| locks.iter().map(|l| (l.holder, l.mode)).collect())
            .unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // Advisory lock API
    // -----------------------------------------------------------------------

    /// Acquire an advisory lock on `key` for transaction `xid`.
    ///
    /// * `mode = Exclusive` – blocks if any other holder (shared or exclusive)
    ///   currently holds a lock on this key.
    /// * `mode = Shared`   – blocks only if an *exclusive* holder exists.
    ///
    /// Returns `Ok(())` on success or `Err` on conflict (non-blocking).
    pub fn acquire_advisory_lock(
        &self,
        xid: TransactionId,
        key: i64,
        mode: AdvisoryLockMode,
    ) -> anyhow::Result<()> {
        let mut advisory = self.advisory_locks.lock();
        let entries = advisory.entry(key).or_default();

        // Check conflicts with existing holders (other than ourselves).
        for entry in entries.iter() {
            if entry.holder == xid {
                continue; // Re-entrant lock by the same xid is fine.
            }
            let conflicts = match (mode, entry.mode) {
                (AdvisoryLockMode::Exclusive, _) => true,        // Exclusive blocks everyone.
                (AdvisoryLockMode::Shared, AdvisoryLockMode::Exclusive) => true,  // Shared blocks exclusive.
                (AdvisoryLockMode::Shared, AdvisoryLockMode::Shared) => false,    // Shared + Shared OK.
            };
            if conflicts {
                return Err(anyhow::anyhow!(
                    "Advisory lock conflict on key {} (mode {:?})",
                    key, mode
                ));
            }
        }

        entries.push(AdvisoryLockEntry { holder: xid, mode });
        Ok(())
    }

    /// Non-blocking variant of `acquire_advisory_lock`.
    /// Returns `true` if the lock was acquired, `false` on conflict.
    pub fn try_acquire_advisory_lock(
        &self,
        xid: TransactionId,
        key: i64,
        mode: AdvisoryLockMode,
    ) -> bool {
        self.acquire_advisory_lock(xid, key, mode).is_ok()
    }

    /// Release one advisory lock on `key` held by `xid`.
    /// If the same xid acquired the lock multiple times, only one entry is removed.
    /// Returns `true` if a lock was found and removed.
    pub fn release_advisory_lock(
        &self,
        xid: TransactionId,
        key: i64,
    ) -> bool {
        let mut advisory = self.advisory_locks.lock();
        if let Some(entries) = advisory.get_mut(&key) {
            if let Some(pos) = entries.iter().position(|e| e.holder == xid) {
                entries.remove(pos);
                if entries.is_empty() {
                    advisory.remove(&key);
                }
                return true;
            }
        }
        false
    }

    /// Release **all** advisory locks held by `xid` (called at transaction end).
    pub fn release_all_advisory_locks(&self, xid: TransactionId) {
        let mut advisory = self.advisory_locks.lock();
        for entries in advisory.values_mut() {
            entries.retain(|e| e.holder != xid);
        }
        advisory.retain(|_, entries| !entries.is_empty());
    }

    /// List all advisory lock holders for a given key.
    pub fn advisory_lock_holders(&self, key: i64) -> Vec<(TransactionId, AdvisoryLockMode)> {
        let advisory = self.advisory_locks.lock();
        advisory
            .get(&key)
            .map(|entries| entries.iter().map(|e| (e.holder, e.mode)).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_mode_conflicts() {
        assert!(LockMode::AccessShare.conflicts_with(&LockMode::Exclusive));
        assert!(LockMode::AccessShare.conflicts_with(&LockMode::AccessExclusive));
        assert!(!LockMode::AccessShare.conflicts_with(&LockMode::Share));
        assert!(!LockMode::AccessShare.conflicts_with(&LockMode::RowShare));
    }

    #[test]
    fn test_acquire_table_lock() {
        let mgr = LockManager::new();
        let xid1 = TransactionId(1);
        let xid2 = TransactionId(2);
        let rel_oid = Oid(100);

        // First lock should succeed
        assert!(mgr.acquire_table_lock(xid1, rel_oid, LockMode::AccessShare).is_ok());

        // Conflicting lock should fail
        assert!(mgr.acquire_table_lock(xid2, rel_oid, LockMode::Exclusive).is_err());

        // Non-conflicting lock should succeed
        assert!(mgr.acquire_table_lock(xid2, rel_oid, LockMode::AccessShare).is_ok());
    }

    #[test]
    fn test_release_table_lock() {
        let mgr = LockManager::new();
        let xid = TransactionId(1);
        let rel_oid = Oid(100);

        mgr.acquire_table_lock(xid, rel_oid, LockMode::AccessShare).unwrap();
        assert_eq!(mgr.table_lock_holders(rel_oid).len(), 1);

        mgr.release_table_lock(xid, rel_oid);
        assert_eq!(mgr.table_lock_holders(rel_oid).len(), 0);
    }

    #[test]
    fn test_row_lock() {
        let mgr = LockManager::new();
        let xid1 = TransactionId(1);
        let xid2 = TransactionId(2);
        let rel_oid = Oid(100);

        // ForUpdate conflicts with ForUpdate
        assert!(mgr.acquire_row_lock(xid1, rel_oid, 1, 0, LockMode::ForUpdate).is_ok());
        assert!(mgr.acquire_row_lock(xid2, rel_oid, 1, 0, LockMode::ForUpdate).is_err());
        // ForUpdate conflicts with ForKeyShare
        assert!(mgr.acquire_row_lock(xid2, rel_oid, 1, 0, LockMode::ForKeyShare).is_err());
        // Different row should not conflict
        assert!(mgr.acquire_row_lock(xid2, rel_oid, 1, 1, LockMode::ForUpdate).is_ok());
    }

    #[test]
    fn test_release_all_locks() {
        let mgr = LockManager::new();
        let xid = TransactionId(1);

        mgr.acquire_table_lock(xid, Oid(1), LockMode::AccessShare).unwrap();
        mgr.acquire_table_lock(xid, Oid(2), LockMode::RowShare).unwrap();
        mgr.acquire_row_lock(xid, Oid(1), 1, 0, LockMode::ForUpdate).unwrap();

        mgr.release_all_locks(xid);

        assert_eq!(mgr.table_lock_holders(Oid(1)).len(), 0);
        assert_eq!(mgr.table_lock_holders(Oid(2)).len(), 0);
        assert_eq!(mgr.row_lock_holders(Oid(1), 1, 0).len(), 0);
    }

    // -------------------------------------------------------------------
    // Advisory lock tests
    // -------------------------------------------------------------------

    #[test]
    fn test_advisory_lock_exclusive_blocks_exclusive() {
        let mgr = LockManager::new();
        let xid1 = TransactionId(1);
        let xid2 = TransactionId(2);

        assert!(mgr.acquire_advisory_lock(xid1, 42, AdvisoryLockMode::Exclusive).is_ok());
        // Second xid must not get exclusive on the same key.
        assert!(mgr.acquire_advisory_lock(xid2, 42, AdvisoryLockMode::Exclusive).is_err());
    }

    #[test]
    fn test_advisory_lock_exclusive_blocks_shared() {
        let mgr = LockManager::new();
        let xid1 = TransactionId(1);
        let xid2 = TransactionId(2);

        assert!(mgr.acquire_advisory_lock(xid1, 99, AdvisoryLockMode::Exclusive).is_ok());
        // Shared must not be granted while exclusive holder exists.
        assert!(mgr.acquire_advisory_lock(xid2, 99, AdvisoryLockMode::Shared).is_err());
    }

    #[test]
    fn test_advisory_lock_shared_allows_shared() {
        let mgr = LockManager::new();
        let xid1 = TransactionId(1);
        let xid2 = TransactionId(2);

        assert!(mgr.acquire_advisory_lock(xid1, 7, AdvisoryLockMode::Shared).is_ok());
        // Another shared holder on the same key should succeed.
        assert!(mgr.acquire_advisory_lock(xid2, 7, AdvisoryLockMode::Shared).is_ok());
        assert_eq!(mgr.advisory_lock_holders(7).len(), 2);
    }

    #[test]
    fn test_advisory_lock_shared_blocks_exclusive() {
        let mgr = LockManager::new();
        let xid1 = TransactionId(1);
        let xid2 = TransactionId(2);

        assert!(mgr.acquire_advisory_lock(xid1, 7, AdvisoryLockMode::Shared).is_ok());
        // Exclusive must be blocked by existing shared holder.
        assert!(mgr.acquire_advisory_lock(xid2, 7, AdvisoryLockMode::Exclusive).is_err());
    }

    #[test]
    fn test_advisory_lock_reentrant_same_xid() {
        let mgr = LockManager::new();
        let xid = TransactionId(1);

        // The same transaction may acquire the same key multiple times (re-entrancy).
        assert!(mgr.acquire_advisory_lock(xid, 55, AdvisoryLockMode::Exclusive).is_ok());
        assert!(mgr.acquire_advisory_lock(xid, 55, AdvisoryLockMode::Exclusive).is_ok());
        assert_eq!(mgr.advisory_lock_holders(55).len(), 2);
    }

    #[test]
    fn test_advisory_lock_different_keys_dont_conflict() {
        let mgr = LockManager::new();
        let xid1 = TransactionId(1);
        let xid2 = TransactionId(2);

        assert!(mgr.acquire_advisory_lock(xid1, 1, AdvisoryLockMode::Exclusive).is_ok());
        // A completely different key must not be affected.
        assert!(mgr.acquire_advisory_lock(xid2, 2, AdvisoryLockMode::Exclusive).is_ok());
    }

    #[test]
    fn test_try_acquire_advisory_lock() {
        let mgr = LockManager::new();
        let xid1 = TransactionId(1);
        let xid2 = TransactionId(2);

        assert!(mgr.try_acquire_advisory_lock(xid1, 10, AdvisoryLockMode::Exclusive));
        // Non-blocking try must return false on conflict.
        assert!(!mgr.try_acquire_advisory_lock(xid2, 10, AdvisoryLockMode::Exclusive));
    }

    #[test]
    fn test_release_advisory_lock() {
        let mgr = LockManager::new();
        let xid1 = TransactionId(1);
        let xid2 = TransactionId(2);

        mgr.acquire_advisory_lock(xid1, 20, AdvisoryLockMode::Exclusive).unwrap();
        assert!(mgr.acquire_advisory_lock(xid2, 20, AdvisoryLockMode::Exclusive).is_err());

        // After release the key should be available.
        assert!(mgr.release_advisory_lock(xid1, 20));
        assert!(mgr.acquire_advisory_lock(xid2, 20, AdvisoryLockMode::Exclusive).is_ok());
    }

    #[test]
    fn test_release_advisory_lock_returns_false_when_not_held() {
        let mgr = LockManager::new();
        let xid = TransactionId(1);
        // Nothing held — should return false.
        assert!(!mgr.release_advisory_lock(xid, 999));
    }

    #[test]
    fn test_release_all_advisory_locks() {
        let mgr = LockManager::new();
        let xid = TransactionId(1);
        let xid2 = TransactionId(2);

        mgr.acquire_advisory_lock(xid, 100, AdvisoryLockMode::Exclusive).unwrap();
        mgr.acquire_advisory_lock(xid, 200, AdvisoryLockMode::Shared).unwrap();

        mgr.release_all_advisory_locks(xid);

        assert!(mgr.advisory_lock_holders(100).is_empty());
        assert!(mgr.advisory_lock_holders(200).is_empty());
        // After release another xid should be free to lock.
        assert!(mgr.acquire_advisory_lock(xid2, 100, AdvisoryLockMode::Exclusive).is_ok());
    }
}
