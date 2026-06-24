use crate::types::PageId;
use crate::btree::page::BTreePage;
use crate::storage::StorageTrait;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanDirection {
    Forward,
    Backward,
}

pub struct BTreeScan {
    pub index_oid: u32,
    pub scan_key: Option<Vec<u8>>,
    pub direction: ScanDirection,
}

impl BTreeScan {
    pub fn new(index_oid: u32, scan_key: Option<Vec<u8>>, direction: ScanDirection) -> Self {
        Self { index_oid, scan_key, direction }
    }
}
