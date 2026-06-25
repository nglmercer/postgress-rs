#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanDirection {
    Forward,
    Backward,
}

pub struct BTreeScan {
    pub index_oid: u32,
    pub scan_key: Option<Vec<u8>>,
    pub direction: ScanDirection,
    pub skip_duplicates: bool,
    pub distinct_mode: bool,
}

impl BTreeScan {
    pub fn new(index_oid: u32, scan_key: Option<Vec<u8>>, direction: ScanDirection) -> Self {
        Self {
            index_oid,
            scan_key,
            direction,
            skip_duplicates: false,
            distinct_mode: false,
        }
    }

    pub fn with_skip_duplicates(mut self, skip: bool) -> Self {
        self.skip_duplicates = skip;
        self
    }

    pub fn with_distinct_mode(mut self, distinct: bool) -> Self {
        self.distinct_mode = distinct;
        self
    }

    pub fn should_skip(&self, current_key: &[u8], next_key: &[u8]) -> bool {
        if !self.skip_duplicates && !self.distinct_mode {
            return false;
        }
        current_key == next_key
    }
}

pub struct SkipScanIterator<'a> {
    entries: &'a [(Vec<u8>, u32, u16)],
    position: usize,
    skip_duplicates: bool,
}

impl<'a> SkipScanIterator<'a> {
    pub fn new(entries: &'a [(Vec<u8>, u32, u16)], skip_duplicates: bool) -> Self {
        Self {
            entries,
            position: 0,
            skip_duplicates,
        }
    }

    pub fn next_distinct(&mut self) -> Option<&'a (Vec<u8>, u32, u16)> {
        if self.position >= self.entries.len() {
            return None;
        }

        let current_key = self.entries[self.position].0.clone();

        if self.skip_duplicates {
            while self.position < self.entries.len() - 1 {
                let next_key = &self.entries[self.position + 1].0;
                if next_key.as_slice() == current_key.as_slice() {
                    self.position += 1;
                } else {
                    break;
                }
            }
        }

        let result = &self.entries[self.position];
        self.position += 1;
        Some(result)
    }

    pub fn reset(&mut self) {
        self.position = 0;
    }

    pub fn count_remaining(&self) -> usize {
        self.entries.len() - self.position
    }
}

pub struct DeduplicateResult {
    pub entries: Vec<(Vec<u8>, u32, u16)>,
    pub duplicates_removed: usize,
}

pub fn deduplicate_entries(entries: &[(Vec<u8>, u32, u16)]) -> DeduplicateResult {
    if entries.is_empty() {
        return DeduplicateResult {
            entries: Vec::new(),
            duplicates_removed: 0,
        };
    }

    let mut result = Vec::new();
    let mut duplicates_removed = 0;
    let mut i = 0;

    while i < entries.len() {
        let current_key = &entries[i].0;
        result.push(entries[i].clone());

        while i < entries.len() - 1 && entries[i + 1].0 == *current_key {
            i += 1;
            duplicates_removed += 1;
        }

        i += 1;
    }

    DeduplicateResult {
        entries: result,
        duplicates_removed,
    }
}

pub fn skip_to_next_key(entries: &[(Vec<u8>, u32, u16)], current_pos: usize) -> usize {
    if current_pos >= entries.len() {
        return entries.len();
    }

    let current_key = &entries[current_pos].0;
    let mut pos = current_pos + 1;

    while pos < entries.len() && entries[pos].0 == *current_key {
        pos += 1;
    }

    pos
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
        assert!(!scan.skip_duplicates);
        assert!(!scan.distinct_mode);
    }

    #[test]
    fn test_btree_scan_no_key() {
        let scan = BTreeScan::new(1, None, ScanDirection::Backward);
        assert!(scan.scan_key.is_none());
        assert_eq!(scan.direction, ScanDirection::Backward);
    }

    #[test]
    fn test_btree_scan_with_skip() {
        let scan = BTreeScan::new(1, None, ScanDirection::Forward)
            .with_skip_duplicates(true)
            .with_distinct_mode(true);
        assert!(scan.skip_duplicates);
        assert!(scan.distinct_mode);
    }

    #[test]
    fn test_should_skip() {
        let scan = BTreeScan::new(1, None, ScanDirection::Forward).with_skip_duplicates(true);

        assert!(scan.should_skip(b"key1", b"key1"));
        assert!(!scan.should_skip(b"key1", b"key2"));
    }

    #[test]
    fn test_skip_scan_iterator() {
        let entries = vec![
            (b"key1".to_vec(), 1, 0),
            (b"key1".to_vec(), 1, 1),
            (b"key2".to_vec(), 1, 2),
            (b"key2".to_vec(), 1, 3),
            (b"key3".to_vec(), 1, 4),
        ];

        let mut iter = SkipScanIterator::new(&entries, true);
        assert_eq!(iter.next_distinct().unwrap().0, b"key1");
        assert_eq!(iter.next_distinct().unwrap().0, b"key2");
        assert_eq!(iter.next_distinct().unwrap().0, b"key3");
        assert!(iter.next_distinct().is_none());
    }

    #[test]
    fn test_skip_scan_iterator_no_skip() {
        let entries = vec![
            (b"key1".to_vec(), 1, 0),
            (b"key1".to_vec(), 1, 1),
            (b"key2".to_vec(), 1, 2),
        ];

        let mut iter = SkipScanIterator::new(&entries, false);
        assert_eq!(iter.next_distinct().unwrap().0, b"key1");
        assert_eq!(iter.next_distinct().unwrap().0, b"key1");
        assert_eq!(iter.next_distinct().unwrap().0, b"key2");
        assert!(iter.next_distinct().is_none());
    }

    #[test]
    fn test_deduplicate_entries() {
        let entries = vec![
            (b"key1".to_vec(), 1, 0),
            (b"key1".to_vec(), 1, 1),
            (b"key2".to_vec(), 1, 2),
            (b"key2".to_vec(), 1, 3),
            (b"key3".to_vec(), 1, 4),
        ];

        let result = deduplicate_entries(&entries);
        assert_eq!(result.entries.len(), 3);
        assert_eq!(result.duplicates_removed, 2);
        assert_eq!(result.entries[0].0, b"key1");
        assert_eq!(result.entries[1].0, b"key2");
        assert_eq!(result.entries[2].0, b"key3");
    }

    #[test]
    fn test_deduplicate_empty() {
        let entries: Vec<(Vec<u8>, u32, u16)> = vec![];
        let result = deduplicate_entries(&entries);
        assert_eq!(result.entries.len(), 0);
        assert_eq!(result.duplicates_removed, 0);
    }

    #[test]
    fn test_skip_to_next_key() {
        let entries = vec![
            (b"key1".to_vec(), 1, 0),
            (b"key1".to_vec(), 1, 1),
            (b"key2".to_vec(), 1, 2),
        ];

        assert_eq!(skip_to_next_key(&entries, 0), 2);
        assert_eq!(skip_to_next_key(&entries, 2), 3);
    }

    #[test]
    fn test_skip_to_next_key_boundary() {
        let entries = vec![(b"key1".to_vec(), 1, 0)];

        assert_eq!(skip_to_next_key(&entries, 0), 1);
        assert_eq!(skip_to_next_key(&entries, 1), 1);
    }
}
