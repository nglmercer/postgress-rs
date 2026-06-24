use crate::types::Oid;
use std::sync::atomic::{AtomicU32, Ordering};
use parking_lot::RwLock;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxnState {
    Active,
    Committed,
    Aborted,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub xid: TransactionId,
    pub active_xids: Vec<TransactionId>,
}

impl Snapshot {
    pub fn is_visible(&self, xmin: TransactionId, xmax: Option<TransactionId>) -> bool {
        if xmin.0 >= self.xid.0 {
            return false;
        }
        if self.active_xids.contains(&xmin) {
            return false;
        }
        if let Some(xmax) = xmax {
            if xmax.0 < self.xid.0 && !self.active_xids.contains(&xmax) {
                return true;
            }
            return false;
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub xid: TransactionId,
    pub isolation: IsolationLevel,
    pub state: TxnState,
    pub snapshot: Option<Snapshot>,
    #[allow(dead_code)]
    locks: Vec<Oid>,
}

impl Transaction {
    pub fn new(xid: TransactionId, isolation: IsolationLevel) -> Self {
        Self {
            xid,
            isolation,
            state: TxnState::Active,
            snapshot: None,
            locks: Vec::new(),
        }
    }

    pub fn commit(&mut self) {
        self.state = TxnState::Committed;
    }

    pub fn rollback(&mut self) {
        self.state = TxnState::Aborted;
    }

    pub fn take_snapshot(&mut self, snapshot: Snapshot) {
        self.snapshot = Some(snapshot);
    }
}

pub struct TransactionManager {
    next_xid: AtomicU32,
    active: RwLock<HashMap<TransactionId, Transaction>>,
}

impl TransactionManager {
    pub fn new() -> Self {
        Self {
            next_xid: AtomicU32::new(1),
            active: RwLock::new(HashMap::new()),
        }
    }

    pub fn begin(&self, isolation: IsolationLevel) -> TransactionId {
        let xid = TransactionId(self.next_xid.fetch_add(1, Ordering::SeqCst));
        let mut txn = Transaction::new(xid, isolation);

        let snapshot = self.create_snapshot(xid);
        match isolation {
            IsolationLevel::ReadUncommitted | IsolationLevel::ReadCommitted => {}
            IsolationLevel::RepeatableRead | IsolationLevel::Serializable => {
                txn.take_snapshot(snapshot);
            }
        }

        self.active.write().insert(xid, txn);
        xid
    }

    pub fn get_transaction(&self, xid: TransactionId) -> Option<Transaction> {
        self.active.read().get(&xid).cloned()
    }

    pub fn commit(&self, xid: TransactionId) -> anyhow::Result<()> {
        let mut active = self.active.write();
        if let Some(txn) = active.get_mut(&xid) {
            txn.commit();
            active.remove(&xid);
            Ok(())
        } else {
            anyhow::bail!("Transaction {:?} not found", xid);
        }
    }

    pub fn rollback(&self, xid: TransactionId) -> anyhow::Result<()> {
        let mut active = self.active.write();
        if let Some(txn) = active.get_mut(&xid) {
            txn.rollback();
            active.remove(&xid);
            Ok(())
        } else {
            anyhow::bail!("Transaction {:?} not found", xid);
        }
    }

    pub fn get_snapshot_for_statement(&self, xid: TransactionId) -> Snapshot {
        let active = self.active.read();
        let active_xids: Vec<TransactionId> = active.keys().cloned().collect();
        Snapshot { xid, active_xids }
    }

    fn create_snapshot(&self, current_xid: TransactionId) -> Snapshot {
        let active = self.active.read();
        let active_xids: Vec<TransactionId> = active.keys().cloned().collect();
        Snapshot { xid: current_xid, active_xids }
    }

    pub fn active_count(&self) -> usize {
        self.active.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_begin_and_commit() {
        let mgr = TransactionManager::new();
        let xid = mgr.begin(IsolationLevel::ReadCommitted);
        assert_eq!(xid.0, 1);
        assert_eq!(mgr.active_count(), 1);
        mgr.commit(xid).unwrap();
        assert_eq!(mgr.active_count(), 0);
    }

    #[test]
    fn test_begin_and_rollback() {
        let mgr = TransactionManager::new();
        let xid = mgr.begin(IsolationLevel::ReadCommitted);
        assert_eq!(mgr.active_count(), 1);
        mgr.rollback(xid).unwrap();
        assert_eq!(mgr.active_count(), 0);
    }

    #[test]
    fn test_snapshot_visibility() {
        let snapshot = Snapshot {
            xid: TransactionId(10),
            active_xids: vec![TransactionId(5), TransactionId(8)],
        };
        assert!(snapshot.is_visible(TransactionId(3), None));
        assert!(!snapshot.is_visible(TransactionId(5), None));
        assert!(!snapshot.is_visible(TransactionId(12), None));
        assert!(snapshot.is_visible(TransactionId(3), Some(TransactionId(7))));
        assert!(!snapshot.is_visible(TransactionId(3), Some(TransactionId(12))));
    }

    #[test]
    fn test_snapshot_with_xmax() {
        let snapshot = Snapshot {
            xid: TransactionId(10),
            active_xids: vec![],
        };
        assert!(snapshot.is_visible(TransactionId(5), Some(TransactionId(3))));
        assert!(!snapshot.is_visible(TransactionId(5), Some(TransactionId(12))));
    }

    #[test]
    fn test_get_snapshot_for_statement() {
        let mgr = TransactionManager::new();
        let xid1 = mgr.begin(IsolationLevel::ReadCommitted);
        let _xid2 = mgr.begin(IsolationLevel::ReadCommitted);
        let snap = mgr.get_snapshot_for_statement(xid1);
        assert_eq!(snap.xid, xid1);
        assert_eq!(snap.active_xids.len(), 2);
        mgr.commit(xid1).unwrap();
    }

    #[test]
    fn test_commit_nonexistent() {
        let mgr = TransactionManager::new();
        assert!(mgr.commit(TransactionId(999)).is_err());
    }

    #[test]
    fn test_rollback_nonexistent() {
        let mgr = TransactionManager::new();
        assert!(mgr.rollback(TransactionId(999)).is_err());
    }

    #[test]
    fn test_sequential_xids() {
        let mgr = TransactionManager::new();
        let x1 = mgr.begin(IsolationLevel::ReadCommitted);
        let x2 = mgr.begin(IsolationLevel::ReadCommitted);
        let x3 = mgr.begin(IsolationLevel::ReadCommitted);
        assert!(x1.0 < x2.0);
        assert!(x2.0 < x3.0);
        mgr.commit(x1).unwrap();
        mgr.commit(x2).unwrap();
        mgr.commit(x3).unwrap();
    }

    #[test]
    fn test_isolation_level_repeatable_read() {
        let mgr = TransactionManager::new();
        let xid = mgr.begin(IsolationLevel::RepeatableRead);
        let txn = mgr.get_transaction(xid).unwrap();
        assert!(txn.snapshot.is_some());
        assert_eq!(txn.isolation, IsolationLevel::RepeatableRead);
        mgr.commit(xid).unwrap();
    }
}
