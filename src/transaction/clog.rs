use std::collections::HashMap;
use parking_lot::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxnStatus {
    InProgress,
    Committed,
    Aborted,
}

impl TxnStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TxnStatus::InProgress => "in_progress",
            TxnStatus::Committed => "committed",
            TxnStatus::Aborted => "aborted",
        }
    }
}

pub struct CommitLog {
    statuses: RwLock<HashMap<u32, TxnStatus>>,
}

impl CommitLog {
    pub fn new() -> Self {
        Self {
            statuses: RwLock::new(HashMap::new()),
        }
    }

    /// Set the transaction status. If the entry already exists it is overwritten.
    #[inline]
    pub fn set_status(&self, xid: u32, status: TxnStatus) {
        self.statuses.write().insert(xid, status);
    }

    /// Returns `None` if the XID has never been seen (treated as InProgress in callers).
    #[inline]
    pub fn get_status(&self, xid: u32) -> Option<TxnStatus> {
        self.statuses.read().get(&xid).copied()
    }

    /// Returns `true` if the XID is committed.
    #[inline]
    pub fn is_committed(&self, xid: u32) -> bool {
        matches!(self.statuses.read().get(&xid), Some(TxnStatus::Committed))
    }

    /// Returns `true` if the XID is aborted.
    #[inline]
    pub fn is_aborted(&self, xid: u32) -> bool {
        matches!(self.statuses.read().get(&xid), Some(TxnStatus::Aborted))
    }

    /// Returns `true` if the XID is in-progress.
    #[inline]
    pub fn is_in_progress(&self, xid: u32) -> bool {
        matches!(self.statuses.read().get(&xid), Some(TxnStatus::InProgress))
    }

    /// Returns `true` if the XID is known (anything other than InProgress).
    #[inline]
    pub fn is_known(&self, xid: u32) -> bool {
        self.statuses.read().contains_key(&xid)
    }

    /// Truncate / remove statuses for XIDs that are no longer needed (older than `cutoff`).
    /// In a full implementation this would be triggered by vacuum. Here we simply remove
    /// committed/aborted entries below the cutoff to limit memory growth.
    pub fn truncate(&self, cutoff: u32) {
        let mut statuses = self.statuses.write();
        statuses.retain(|&xid, status| {
            let keep = xid >= cutoff || matches!(status, TxnStatus::InProgress);
            keep
        });
    }

    /// Return the number of tracked XIDs (for monitoring).
    #[inline]
    pub fn len(&self) -> usize {
        self.statuses.read().len()
    }

    /// Returns `true` if no XIDs are tracked.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.statuses.read().is_empty()
    }

    #[cfg(test)]
    pub fn for_test() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get_status() {
        let clog = CommitLog::new();
        clog.set_status(1, TxnStatus::InProgress);
        assert_eq!(clog.get_status(1), Some(TxnStatus::InProgress));
    }

    #[test]
    fn test_unknown_xid_returns_none() {
        let clog = CommitLog::new();
        assert!(clog.get_status(999).is_none());
    }

    #[test]
    fn test_is_committed() {
        let clog = CommitLog::new();
        clog.set_status(10, TxnStatus::Committed);
        assert!(clog.is_committed(10));
        assert!(!clog.is_aborted(10));
        assert!(!clog.is_in_progress(10));
    }

    #[test]
    fn test_is_aborted() {
        let clog = CommitLog::new();
        clog.set_status(20, TxnStatus::Aborted);
        assert!(clog.is_aborted(20));
        assert!(!clog.is_committed(20));
    }

    #[test]
    fn test_truncate() {
        let clog = CommitLog::new();
        clog.set_status(1, TxnStatus::Committed);
        clog.set_status(2, TxnStatus::Committed);
        clog.set_status(3, TxnStatus::InProgress);
        clog.set_status(100, TxnStatus::Committed);

        clog.truncate(3);
        assert_eq!(clog.get_status(1), None);
        assert_eq!(clog.get_status(2), None);
        assert_eq!(clog.get_status(3), Some(TxnStatus::InProgress));
        assert_eq!(clog.get_status(100), Some(TxnStatus::Committed));
    }

    #[test]
    fn test_truncate_keeps_in_progress() {
        let clog = CommitLog::new();
        clog.set_status(5, TxnStatus::InProgress);
        clog.truncate(10);
        assert_eq!(clog.get_status(5), Some(TxnStatus::InProgress));
    }
}
