
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_direction_variants() {
        assert_ne!(ScanDirection::Forward, ScanDirection::Backward);
    }

    #[test]
    fn test_btree_scan_new() {
        let scan = BTreeScan::new(42, Some(b"key".to_vec()), ScanDirection::Forward);
        assert_eq!(scan.index_oid, 42);
        assert_eq!(scan.scan_key.as_deref(), Some(b"key".as_ref()));
        assert_eq!(scan.direction, ScanDirection::Forward);
    }

    #[test]
    fn test_btree_scan_no_key() {
        let scan = BTreeScan::new(1, None, ScanDirection::Backward);
        assert!(scan.scan_key.is_none());
        assert_eq!(scan.direction, ScanDirection::Backward);
    }
}
