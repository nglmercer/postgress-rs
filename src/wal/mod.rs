use crate::types::{PageId, Oid, Tuple};
use crate::storage::ephemeral::EphemeralStorage;
use crate::storage::StorageTrait;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WALRecord {
    Begin {
        xid: u64,
    },
    Insert {
        rel_oid: Oid,
        page_id: PageId,
        tuple: Tuple,
    },
    Update {
        rel_oid: Oid,
        page_id: PageId,
        old_tuple: Tuple,
        new_tuple: Tuple,
    },
    Delete {
        rel_oid: Oid,
        page_id: PageId,
        tuple: Tuple,
    },
    Commit {
        xid: u64,
    },
    Abort {
        xid: u64,
    },
}

pub struct WAL {
    storage: Arc<dyn StorageTrait>,
    current_lsn: Mutex<u64>,
    flushed_lsn: Mutex<u64>,
    current_xid: AtomicU64,
    segment_size: u32,
    flush_count: std::sync::atomic::AtomicU64,
}

impl WAL {
    pub fn new(segment_size: u32) -> Self {
        Self {
            storage: Arc::new(EphemeralStorage::new()),
            current_lsn: Mutex::new(0),
            flushed_lsn: Mutex::new(0),
            current_xid: AtomicU64::new(1),
            segment_size,
            flush_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn allocate_xid(&self) -> u64 {
        self.current_xid.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn append(&self, record: &WALRecord) -> anyhow::Result<u64> {
        let mut lsn = self.current_lsn.lock().await;
        let current = *lsn;
        *lsn += 1;
        drop(lsn);

        let data = bincode::serialize(record)?;
        let page_id = PageId((1 + (current / (self.segment_size as u64))) as u32);
        let mut page = self.storage.read_page(page_id)?;
        let offset = (current % (self.segment_size as u64)) as usize;
        if offset + data.len() > page.len() {
            anyhow::bail!("WAL record too large for segment");
        }
        page[offset..offset + data.len()].copy_from_slice(&data);
        self.storage.write_page(page_id, &page)?;
        Ok(current)
    }

    pub async fn flush(&self) -> anyhow::Result<()> {
        let lsn = self.current_lsn.lock().await;
        let mut flushed = self.flushed_lsn.lock().await;
        *flushed = *lsn;
        drop(flushed);
        drop(lsn);
        self.flush_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    pub async fn get_flushed_lsn(&self) -> u64 {
        *self.flushed_lsn.lock().await
    }

    pub fn get_flush_count(&self) -> u64 {
        self.flush_count.load(Ordering::SeqCst)
    }

    pub async fn ensure_flushed(&self, lsn: u64) -> anyhow::Result<()> {
        let current = *self.current_lsn.lock().await;
        let flushed = self.get_flushed_lsn().await;
        if current > flushed && lsn >= flushed {
            self.flush().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_wal_new() {
        let wal = WAL::new(4096);
        assert_eq!(wal.segment_size, 4096);
    }

    #[tokio::test]
    async fn test_wal_allocate_xid_sequential() {
        let wal = WAL::new(8192);
        let x1 = wal.allocate_xid();
        let x2 = wal.allocate_xid();
        let x3 = wal.allocate_xid();
        assert_eq!(x1, 1);
        assert_eq!(x2, 2);
        assert_eq!(x3, 3);
    }

    #[tokio::test]
    async fn test_wal_append_returns_sequential_lsn() {
        let wal = WAL::new(8192);
        let lsn1 = wal.append(&WALRecord::Begin { xid: 1 }).await.unwrap();
        let lsn2 = wal.append(&WALRecord::Commit { xid: 1 }).await.unwrap();
        let lsn3 = wal.append(&WALRecord::Begin { xid: 2 }).await.unwrap();
        assert_eq!(lsn1, 0);
        assert_eq!(lsn2, 1);
        assert_eq!(lsn3, 2);
    }

    #[tokio::test]
    async fn test_wal_flush_advances_flushed_lsn() {
        let wal = WAL::new(8192);
        let _lsn1 = wal.append(&WALRecord::Begin { xid: 1 }).await.unwrap();
        let lsn2 = wal.append(&WALRecord::Commit { xid: 1 }).await.unwrap();
        assert_eq!(wal.get_flushed_lsn().await, 0);
        assert_eq!(wal.get_flush_count(), 0);
        wal.flush().await.unwrap();
        assert_eq!(wal.get_flushed_lsn().await, lsn2 + 1);
        assert_eq!(wal.get_flush_count(), 1);
    }

    #[tokio::test]
    async fn test_wal_ensure_flushed() {
        let wal = WAL::new(8192);
        let lsn = wal.append(&WALRecord::Begin { xid: 1 }).await.unwrap();
        wal.ensure_flushed(lsn).await.unwrap();
        assert_eq!(wal.get_flushed_lsn().await, lsn + 1);
    }

    #[tokio::test]
    async fn test_wal_ensure_flushed_already_flushed() {
        let wal = WAL::new(8192);
        wal.flush().await.unwrap();
        wal.ensure_flushed(0).await.unwrap();
        assert_eq!(wal.get_flush_count(), 1);
    }
}
