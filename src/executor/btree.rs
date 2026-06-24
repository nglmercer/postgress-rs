use crate::types::{PageId, Tuple, TupleDesc, Oid};
use crate::storage::StorageTrait;

pub struct BTreeScan {
    pub index_oid: Oid,
    pub scan_from: Vec<u8>,
}

pub struct BTreeInsert {
    pub index_oid: Oid,
    pub key: Vec<u8>,
    pub heap_tid: (PageId, usize),
}

pub async fn btree_insert(
    storage: &dyn StorageTrait,
    op: &BTreeInsert,
) -> anyhow::Result<()> {
    anyhow::bail!("btree_insert not implemented")
}

pub fn btree_scan(
    storage: &dyn StorageTrait,
    op: &BTreeScan,
) -> anyhow::Result<Vec<(Vec<u8>, (PageId, usize))>> {
    anyhow::bail!("btree_scan not implemented")
}