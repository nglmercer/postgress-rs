use crate::types::{Oid, PageId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreePageType {
    Meta,
    Root,
    Internal,
    Leaf,
    Overflow,
}

#[derive(Debug, Clone)]
pub struct IndexTuple {
    pub key: Vec<u8>,
    pub heap_pointer: (u32, u16), // (page_id, offset)
    pub heap_oid: Oid,
}

#[derive(Debug)]
pub struct BTreePage {
    pub page_type: BTreePageType,
    pub page_id: PageId,
    pub level: u16,
    pub keys: Vec<IndexTuple>,
    pub left_sibling: Option<PageId>,
    pub right_sibling: Option<PageId>,
}

impl BTreePage {
    pub fn new(page_type: BTreePageType, page_id: PageId) -> Self {
        Self {
            page_type,
            page_id,
            level: 0,
            keys: Vec::new(),
            left_sibling: None,
            right_sibling: None,
        }
    }

    pub fn is_full(&self, page_size: usize) -> bool {
        self.keys.len() >= (page_size / 64) // conservative
    }

    pub fn insert_sorted(&mut self, tuple: IndexTuple) {
        let pos = self.keys.binary_search_by(|t| t.key.as_slice().cmp(tuple.key.as_slice())).unwrap_or_else(|p| p);
        self.keys.insert(pos, tuple);
    }

    pub fn split_at(&mut self, split_pos: usize) -> BTreePage {
        let mut right = BTreePage::new(self.page_type, PageId(0));
        right.keys = self.keys.split_off(split_pos);
        right.level = self.level;
        self.keys.truncate(split_pos);
        right
    }
}

#[derive(Debug)]
pub struct BTreeMetaPage {
    pub magic: u32,
    pub version: u32,
    pub root_page: PageId,
    pub fast_root: Option<PageId>,
    pub height: u16,
    pub page_size: u32,
}

impl BTreeMetaPage {
    pub fn new(root_page: PageId) -> Self {
        Self {
            magic: 0x805842u32, // 'B' 'T' 'R' 'E' in little-endian
            version: 1,
            root_page,
            fast_root: None,
            height: 1,
            page_size: 8192,
        }
    }
}
