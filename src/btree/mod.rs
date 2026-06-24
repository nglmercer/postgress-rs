pub mod page;
pub mod scan;
pub mod insert;
pub mod search;

pub use page::{BTreePage, BTreePageType, IndexTuple, BTreeMetaPage};
pub use scan::{BTreeScan, ScanDirection};
pub use insert::btree_insert;
pub use search::btree_search;

pub struct BTreeIndex {
    pub index_oid: u32,
    pub rel_oid: u32,
    pub root_page: PageId,
    pub page_size: usize,
}

impl BTreeIndex {
    pub fn new(index_oid: u32, rel_oid: u32, root_page: PageId, page_size: usize) -> Self {
        Self { index_oid, rel_oid, root_page, page_size }
    }
}
