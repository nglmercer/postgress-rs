use crate::types::{PageId, Oid, Tuple};
use crate::storage::ephemeral::EphemeralStorage;
use crate::storage::StorageTrait;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;

use serde::{Deserialize, Serialize};

pub mod recovery;
pub mod archiving;

pub const XLOG_PAGE_MAGIC: u16 = 0xD106;
pub const XLP_FIRST_IS_CONTRECORD: u16 = 0x0001;
pub const XLP_LONG_HEADER: u16 = 0x0002;

#[derive(Debug, Clone)]
#[repr(C)]
pub struct XLogPageHeader {
    pub xlp_magic: u16,
    pub xlp_flags: u16,
    pub xlp_tli: u32,
    pub xlp_page_addr: u64,
    pub xlp_max_block: u32,
    pub xlp_checkpoint_copy: u64,
}

impl XLogPageHeader {
    pub fn new(page_addr: u64) -> Self {
        Self {
            xlp_magic: XLOG_PAGE_MAGIC,
            xlp_flags: 0,
            xlp_tli: 1,
            xlp_page_addr: page_addr,
            xlp_max_block: 0,
            xlp_checkpoint_copy: 0,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(26);
        buf.extend_from_slice(&self.xlp_magic.to_le_bytes());
        buf.extend_from_slice(&self.xlp_flags.to_le_bytes());
        buf.extend_from_slice(&self.xlp_tli.to_le_bytes());
        buf.extend_from_slice(&self.xlp_page_addr.to_le_bytes());
        buf.extend_from_slice(&self.xlp_max_block.to_le_bytes());
        buf.extend_from_slice(&self.xlp_checkpoint_copy.to_le_bytes());
        buf
    }

    pub fn deserialize(data: &[u8]) -> Self {
        Self {
            xlp_magic: u16::from_le_bytes(data[0..2].try_into().unwrap()),
            xlp_flags: u16::from_le_bytes(data[2..4].try_into().unwrap()),
            xlp_tli: u32::from_le_bytes(data[4..8].try_into().unwrap()),
            xlp_page_addr: u64::from_le_bytes(data[8..16].try_into().unwrap()),
            xlp_max_block: u32::from_le_bytes(data[16..20].try_into().unwrap()),
            xlp_checkpoint_copy: u64::from_le_bytes(data[20..28].try_into().unwrap()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmgrId {
    Heap = 0,
    Xlog = 1,
    Clog = 2,
    Btree = 3,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct XLogRecord {
    pub xl_tot_len: u32,
    pub xl_xid: u32,
    pub xl_info: u32,
    pub xl_rmid: u8,
    pub xl_pad: [u8; 3],
    pub xl_crc: u32,
}

impl XLogRecord {
    pub fn new(xl_xid: u32, xl_info: u32, xl_rmid: u8, data: &[u8]) -> Self {
        let xl_tot_len = 24 + data.len() as u32;
        let xl_crc = compute_crc(data);
        Self {
            xl_tot_len,
            xl_xid,
            xl_info,
            xl_rmid,
            xl_pad: [0; 3],
            xl_crc,
        }
    }

    pub fn serialize(&self) -> [u8; 24] {
        let mut buf = [0u8; 24];
        buf[0..4].copy_from_slice(&self.xl_tot_len.to_le_bytes());
        buf[4..8].copy_from_slice(&self.xl_xid.to_le_bytes());
        buf[8..12].copy_from_slice(&self.xl_info.to_le_bytes());
        buf[12] = self.xl_rmid;
        buf[13..16].copy_from_slice(&self.xl_pad);
        buf[16..20].copy_from_slice(&self.xl_crc.to_le_bytes());
        buf
    }

    pub fn deserialize(data: &[u8]) -> Self {
        Self {
            xl_tot_len: u32::from_le_bytes(data[0..4].try_into().unwrap()),
            xl_xid: u32::from_le_bytes(data[4..8].try_into().unwrap()),
            xl_info: u32::from_le_bytes(data[8..12].try_into().unwrap()),
            xl_rmid: data[12],
            xl_pad: [data[13], data[14], data[15]],
            xl_crc: u32::from_le_bytes(data[16..20].try_into().unwrap()),
        }
    }

    pub fn verify_crc(&self, data: &[u8]) -> bool {
        self.xl_crc == compute_crc(data)
    }
}

pub fn compute_crc(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        crc = crc ^ (byte as u32);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFFFFFF
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub next_xid: u32,
    pub next_oid: u32,
    pub next_multixact: u32,
    pub oldest_xid: u32,
    pub oldest_multixact: u32,
    pub oldest_commit_ts_xid: u32,
    pub new_commit_ts_xid: u32,
    pub checkpoint_lsn: u64,
    pub redo_lsn: u64,
    pub timeline_id: u32,
}

impl CheckpointRecord {
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(44);
        buf.extend_from_slice(&self.next_xid.to_le_bytes());
        buf.extend_from_slice(&self.next_oid.to_le_bytes());
        buf.extend_from_slice(&self.next_multixact.to_le_bytes());
        buf.extend_from_slice(&self.oldest_xid.to_le_bytes());
        buf.extend_from_slice(&self.oldest_multixact.to_le_bytes());
        buf.extend_from_slice(&self.oldest_commit_ts_xid.to_le_bytes());
        buf.extend_from_slice(&self.new_commit_ts_xid.to_le_bytes());
        buf.extend_from_slice(&self.checkpoint_lsn.to_le_bytes());
        buf.extend_from_slice(&self.redo_lsn.to_le_bytes());
        buf.extend_from_slice(&self.timeline_id.to_le_bytes());
        buf
    }

    pub fn deserialize(data: &[u8]) -> Self {
        Self {
            next_xid: u32::from_le_bytes(data[0..4].try_into().unwrap()),
            next_oid: u32::from_le_bytes(data[4..8].try_into().unwrap()),
            next_multixact: u32::from_le_bytes(data[8..12].try_into().unwrap()),
            oldest_xid: u32::from_le_bytes(data[12..16].try_into().unwrap()),
            oldest_multixact: u32::from_le_bytes(data[16..20].try_into().unwrap()),
            oldest_commit_ts_xid: u32::from_le_bytes(data[20..24].try_into().unwrap()),
            new_commit_ts_xid: u32::from_le_bytes(data[24..28].try_into().unwrap()),
            checkpoint_lsn: u64::from_le_bytes(data[28..36].try_into().unwrap()),
            redo_lsn: u64::from_le_bytes(data[36..44].try_into().unwrap()),
            timeline_id: u32::from_le_bytes(data[44..48].try_into().unwrap()),
        }
    }
}

pub const PG_CONTROL_VERSION: u32 = 170000;

#[derive(Debug, Clone)]
pub struct ControlFile {
    pub system_identifier: u64,
    pub pg_control_version: u32,
    pub catalog_version_no: u32,
    pub check_point_lsn: u64,
    pub redo_lsn: u64,
    pub timeline: u32,
    pub next_xid: u32,
    pub next_oid: u32,
    pub oldest_xid: u32,
    pub oldest_commit_ts_xid: u32,
    pub next_multixact_id: u32,
    pub oldest_multixact_id: u32,
    pub time_checkpoint: i64,
}

impl ControlFile {
    pub fn create(system_id: u64) -> Self {
        Self {
            system_identifier: system_id,
            pg_control_version: PG_CONTROL_VERSION,
            catalog_version_no: 202401011,
            check_point_lsn: 0,
            redo_lsn: 0,
            timeline: 1,
            next_xid: 1,
            next_oid: 10000,
            oldest_xid: 1,
            oldest_commit_ts_xid: 1,
            next_multixact_id: 1,
            oldest_multixact_id: 1,
            time_checkpoint: 0,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(128);
        buf.extend_from_slice(&self.system_identifier.to_le_bytes());
        buf.extend_from_slice(&self.pg_control_version.to_le_bytes());
        buf.extend_from_slice(&self.catalog_version_no.to_le_bytes());
        buf.extend_from_slice(&self.check_point_lsn.to_le_bytes());
        buf.extend_from_slice(&self.redo_lsn.to_le_bytes());
        buf.extend_from_slice(&self.timeline.to_le_bytes());
        buf.extend_from_slice(&self.next_xid.to_le_bytes());
        buf.extend_from_slice(&self.next_oid.to_le_bytes());
        buf.extend_from_slice(&self.oldest_xid.to_le_bytes());
        buf.extend_from_slice(&self.oldest_commit_ts_xid.to_le_bytes());
        buf.extend_from_slice(&self.next_multixact_id.to_le_bytes());
        buf.extend_from_slice(&self.oldest_multixact_id.to_le_bytes());
        buf.extend_from_slice(&self.time_checkpoint.to_le_bytes());
        buf
    }

    pub fn deserialize(data: &[u8]) -> Self {
        Self {
            system_identifier: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            pg_control_version: u32::from_le_bytes(data[8..12].try_into().unwrap()),
            catalog_version_no: u32::from_le_bytes(data[12..16].try_into().unwrap()),
            check_point_lsn: u64::from_le_bytes(data[16..24].try_into().unwrap()),
            redo_lsn: u64::from_le_bytes(data[24..32].try_into().unwrap()),
            timeline: u32::from_le_bytes(data[32..36].try_into().unwrap()),
            next_xid: u32::from_le_bytes(data[36..40].try_into().unwrap()),
            next_oid: u32::from_le_bytes(data[40..44].try_into().unwrap()),
            oldest_xid: u32::from_le_bytes(data[44..48].try_into().unwrap()),
            oldest_commit_ts_xid: u32::from_le_bytes(data[48..52].try_into().unwrap()),
            next_multixact_id: u32::from_le_bytes(data[52..56].try_into().unwrap()),
            oldest_multixact_id: u32::from_le_bytes(data[56..60].try_into().unwrap()),
            time_checkpoint: i64::from_le_bytes(data[60..68].try_into().unwrap()),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.pg_control_version == PG_CONTROL_VERSION
    }
}

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
    Checkpoint {
        record: CheckpointRecord,
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
        let data = bincode::serialize(record)?;
        let record_size = data.len() as u64;

        let mut lsn = self.current_lsn.lock().await;
        let current = *lsn;
        *lsn += record_size;
        drop(lsn);

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
        assert_eq!(lsn1, 0);
        assert!(lsn2 > lsn1);
    }

    #[tokio::test]
    async fn test_wal_flush_advances_flushed_lsn() {
        let wal = WAL::new(8192);
        let _lsn1 = wal.append(&WALRecord::Begin { xid: 1 }).await.unwrap();
        let lsn2 = wal.append(&WALRecord::Commit { xid: 1 }).await.unwrap();
        assert_eq!(wal.get_flushed_lsn().await, 0);
        assert_eq!(wal.get_flush_count(), 0);
        wal.flush().await.unwrap();
        assert!(wal.get_flushed_lsn().await > lsn2);
        assert_eq!(wal.get_flush_count(), 1);
    }

    #[tokio::test]
    async fn test_wal_ensure_flushed() {
        let wal = WAL::new(8192);
        let lsn = wal.append(&WALRecord::Begin { xid: 1 }).await.unwrap();
        wal.ensure_flushed(lsn).await.unwrap();
        assert!(wal.get_flushed_lsn().await > lsn);
    }

    #[tokio::test]
    async fn test_wal_ensure_flushed_already_flushed() {
        let wal = WAL::new(8192);
        wal.flush().await.unwrap();
        wal.ensure_flushed(0).await.unwrap();
        assert_eq!(wal.get_flush_count(), 1);
    }

    #[test]
    fn test_xlog_page_header_roundtrip() {
        let header = XLogPageHeader::new(1024);
        let bytes = header.serialize();
        let deserialized = XLogPageHeader::deserialize(&bytes);
        assert_eq!(header.xlp_magic, deserialized.xlp_magic);
        assert_eq!(header.xlp_page_addr, deserialized.xlp_page_addr);
        assert_eq!(header.xlp_tli, deserialized.xlp_tli);
    }

    #[test]
    fn test_xlog_record_roundtrip() {
        let data = vec![1, 2, 3, 4];
        let record = XLogRecord::new(100, 0, RmgrId::Heap as u8, &data);
        let bytes = record.serialize();
        let deserialized = XLogRecord::deserialize(&bytes);
        assert_eq!(record.xl_tot_len, deserialized.xl_tot_len);
        assert_eq!(record.xl_xid, deserialized.xl_xid);
        assert_eq!(record.xl_crc, deserialized.xl_crc);
    }

    #[test]
    fn test_xlog_record_crc_verification() {
        let data = vec![1, 2, 3, 4];
        let record = XLogRecord::new(100, 0, RmgrId::Heap as u8, &data);
        assert!(record.verify_crc(&data));
        assert!(!record.verify_crc(&vec![1, 2, 3, 5]));
    }

    #[test]
    fn test_compute_crc() {
        let data = b"hello world";
        let crc1 = compute_crc(data);
        let crc2 = compute_crc(data);
        assert_eq!(crc1, crc2);
        let crc3 = compute_crc(b"hello worle");
        assert_ne!(crc1, crc3);
    }

    #[test]
    fn test_checkpoint_record_roundtrip() {
        let record = CheckpointRecord {
            next_xid: 100,
            next_oid: 20000,
            next_multixact: 1,
            oldest_xid: 50,
            oldest_multixact: 1,
            oldest_commit_ts_xid: 50,
            new_commit_ts_xid: 100,
            checkpoint_lsn: 12345,
            redo_lsn: 12000,
            timeline_id: 1,
        };
        let bytes = record.serialize();
        let deserialized = CheckpointRecord::deserialize(&bytes);
        assert_eq!(record.next_xid, deserialized.next_xid);
        assert_eq!(record.checkpoint_lsn, deserialized.checkpoint_lsn);
        assert_eq!(record.redo_lsn, deserialized.redo_lsn);
    }

    #[test]
    fn test_control_file_roundtrip() {
        let mut control = ControlFile::create(12345678);
        control.check_point_lsn = 99999;
        control.next_xid = 500;
        let bytes = control.serialize();
        let deserialized = ControlFile::deserialize(&bytes);
        assert!(deserialized.is_valid());
        assert_eq!(control.system_identifier, deserialized.system_identifier);
        assert_eq!(control.check_point_lsn, deserialized.check_point_lsn);
        assert_eq!(control.next_xid, deserialized.next_xid);
    }

    #[test]
    fn test_control_file_invalid_version() {
        let mut control = ControlFile::create(1);
        control.pg_control_version = 0;
        assert!(!control.is_valid());
    }
}
