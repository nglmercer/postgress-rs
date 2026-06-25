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
        let pos = self
            .keys
            .binary_search_by(|t| t.key.as_slice().cmp(tuple.key.as_slice()))
            .unwrap_or_else(|p| p);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tuple(key: &[u8], page: u32, offset: u16) -> IndexTuple {
        IndexTuple {
            key: key.to_vec(),
            heap_pointer: (page, offset),
            heap_oid: Oid(0),
        }
    }

    #[test]
    fn test_new_page() {
        let page = BTreePage::new(BTreePageType::Leaf, PageId(1));
        assert_eq!(page.page_type, BTreePageType::Leaf);
        assert_eq!(page.page_id, PageId(1));
        assert_eq!(page.level, 0);
        assert!(page.keys.is_empty());
    }

    #[test]
    fn test_insert_sorted_single() {
        let mut page = BTreePage::new(BTreePageType::Leaf, PageId(1));
        page.insert_sorted(make_tuple(b"banana", 10, 0));
        assert_eq!(page.keys.len(), 1);
        assert_eq!(page.keys[0].key, b"banana");
    }

    #[test]
    fn test_insert_sorted_maintains_order() {
        let mut page = BTreePage::new(BTreePageType::Leaf, PageId(1));
        page.insert_sorted(make_tuple(b"cherry", 3, 0));
        page.insert_sorted(make_tuple(b"apple", 1, 0));
        page.insert_sorted(make_tuple(b"banana", 2, 0));
        page.insert_sorted(make_tuple(b"date", 4, 0));

        assert_eq!(page.keys.len(), 4);
        assert_eq!(page.keys[0].key, b"apple");
        assert_eq!(page.keys[1].key, b"banana");
        assert_eq!(page.keys[2].key, b"cherry");
        assert_eq!(page.keys[3].key, b"date");
    }

    #[test]
    fn test_insert_sorted_duplicate_key() {
        let mut page = BTreePage::new(BTreePageType::Leaf, PageId(1));
        page.insert_sorted(make_tuple(b"a", 1, 0));
        page.insert_sorted(make_tuple(b"a", 2, 0));
        assert_eq!(page.keys.len(), 2);
    }

    #[test]
    fn test_is_full() {
        let page_size = 128;
        let mut page = BTreePage::new(BTreePageType::Leaf, PageId(1));
        assert!(!page.is_full(page_size));
        // capacity = 128/64 = 2
        page.insert_sorted(make_tuple(b"a", 1, 0));
        assert!(!page.is_full(page_size));
        page.insert_sorted(make_tuple(b"b", 2, 0));
        assert!(page.is_full(page_size));
    }

    #[test]
    fn test_split_at() {
        let mut page = BTreePage::new(BTreePageType::Leaf, PageId(1));
        page.insert_sorted(make_tuple(b"a", 1, 0));
        page.insert_sorted(make_tuple(b"b", 2, 0));
        page.insert_sorted(make_tuple(b"c", 3, 0));
        page.insert_sorted(make_tuple(b"d", 4, 0));

        let right = page.split_at(2);
        assert_eq!(page.keys.len(), 2);
        assert_eq!(right.keys.len(), 2);
        assert_eq!(page.keys[0].key, b"a");
        assert_eq!(page.keys[1].key, b"b");
        assert_eq!(right.keys[0].key, b"c");
        assert_eq!(right.keys[1].key, b"d");
    }

    #[test]
    fn test_split_preserves_level() {
        let mut page = BTreePage::new(BTreePageType::Internal, PageId(1));
        page.level = 3;
        page.insert_sorted(make_tuple(b"x", 1, 0));

        let right = page.split_at(0);
        assert_eq!(right.level, 3);
    }

    #[test]
    fn test_split_at_zero() {
        let mut page = BTreePage::new(BTreePageType::Leaf, PageId(1));
        page.insert_sorted(make_tuple(b"a", 1, 0));
        let right = page.split_at(0);
        assert!(page.keys.is_empty());
        assert_eq!(right.keys.len(), 1);
    }

    #[test]
    fn test_split_at_len() {
        let mut page = BTreePage::new(BTreePageType::Leaf, PageId(1));
        page.insert_sorted(make_tuple(b"a", 1, 0));
        page.insert_sorted(make_tuple(b"b", 2, 0));
        let right = page.split_at(2);
        assert_eq!(page.keys.len(), 2);
        assert!(right.keys.is_empty());
    }

    #[test]
    fn test_meta_page() {
        let meta = BTreeMetaPage::new(PageId(1));
        assert_eq!(meta.magic, 0x805842);
        assert_eq!(meta.version, 1);
        assert_eq!(meta.root_page, PageId(1));
        assert!(meta.fast_root.is_none());
        assert_eq!(meta.height, 1);
        assert_eq!(meta.page_size, 8192);
    }
}
