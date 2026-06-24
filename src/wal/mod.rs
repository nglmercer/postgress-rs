use crate::types::{PageId, Oid, Tuple};
use crate::storage::StorageTrait;
use std::sync::Arc;
use parking_lot::Mutex;

pub enum WALRecord {
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
    current_xid: Mutex<u64>,
    segment_size: u32,
}

impl WAL {
    pub fn new(storage: Arc<dyn StorageTrait>, segment_size: u32) -> Self {
        Self {
            storage,
            current_lsn: Mutex::new(0),
            current_xid: Mutex::new(1),
            segment_size,
        }
    }

    pub fn allocate_xid(&self) -> u64 {
        let mut xid = self.current_xid.lock();
        let current = *xid;
        *xid += 1;
        current
    }

    pub async fn append(&self, record: &WALRecord) -> anyhow::Result<u64> {
        let mut lsn = self.current_lsn.lock();
        let current = *lsn;
        *lsn += 1;
        drop(lsn);

        let data = bincode::serialize(record)?;
        let page_id = PageId(1 + (current / (self.segment_size as u64)));
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
        Ok(())
    }
}
