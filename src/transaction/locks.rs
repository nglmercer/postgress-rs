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
        match self {
            AccessShare => matches!(other, Exclusive | AccessExclusive),
            RowShare => matches!(other, RowExclusive | ShareUpdateExclusive | Exclusive | AccessExclusive),
            RowExclusive => matches!(other, ShareUpdateExclusive | Share | ShareRowExclusive | Exclusive | AccessExclusive),
            ShareUpdateExclusive => matches!(other, ShareUpdateExclusive | Share | ShareRowExclusive | Exclusive | AccessExclusive),
            Share => matches!(other, RowExclusive | ShareUpdateExclusive | ShareRowExclusive | Exclusive | AccessExclusive),
            ShareRowExclusive => matches!(other, RowExclusive | ShareUpdateExclusive | Share | ShareRowExclusive | Exclusive | AccessExclusive),
            Exclusive => matches!(other, RowShare | RowExclusive | ShareUpdateExclusive | Share | ShareRowExclusive | Exclusive | AccessExclusive),
            AccessExclusive => *other != AccessShare,
            ForUpdate => matches!(other, ForUpdate | ForShare | ForNoKeyUpdate | ForKeyShare),
            ForShare => matches!(other, ForUpdate | ForNoKeyUpdate | ForKeyShare),
            ForNoKeyUpdate => matches!(other, ForUpdate | ForNoKeyUpdate),
            ForKeyShare => matches!(other, ForUpdate),
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
}

impl LockManager {
    pub fn new() -> Self {
        Self {
            table_locks: Mutex::new(HashMap::new()),
            row_locks: Mutex::new(HashMap::new()),
            wait_queue: Mutex::new(VecDeque::new()),
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

        assert!(mgr.acquire_row_lock(xid1, rel_oid, 1, 0, LockMode::ForUpdate).is_ok());
        assert!(mgr.acquire_row_lock(xid2, rel_oid, 1, 0, LockMode::ForUpdate).is_err());
        assert!(mgr.acquire_row_lock(xid2, rel_oid, 1, 0, LockMode::ForKeyShare).is_ok());
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
}
