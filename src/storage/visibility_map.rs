const BITS_PER_BYTE: u32 = 8;
const PAGES_PER_BYTE: u32 = BITS_PER_BYTE;

#[derive(Debug, Clone)]
pub struct VisibilityMap {
    data: Vec<u8>,
    num_pages: u32,
}

impl VisibilityMap {
    pub fn new(num_pages: u32) -> Self {
        let bytes_needed = num_pages.div_ceil(PAGES_PER_BYTE);
        Self {
            data: vec![0u8; bytes_needed as usize],
            num_pages,
        }
    }

    pub fn is_page_all_visible(&self, page_idx: u32) -> bool {
        if page_idx >= self.num_pages {
            return false;
        }
        let byte_idx = (page_idx / PAGES_PER_BYTE) as usize;
        let bit_idx = page_idx % PAGES_PER_BYTE;
        (self.data[byte_idx] & (1 << bit_idx)) != 0
    }

    pub fn mark_all_visible(&mut self, page_idx: u32) {
        if page_idx >= self.num_pages {
            return;
        }
        let byte_idx = (page_idx / PAGES_PER_BYTE) as usize;
        let bit_idx = page_idx % PAGES_PER_BYTE;
        self.data[byte_idx] |= 1 << bit_idx;
    }

    pub fn clear_all_visible(&mut self, page_idx: u32) {
        if page_idx >= self.num_pages {
            return;
        }
        let byte_idx = (page_idx / PAGES_PER_BYTE) as usize;
        let bit_idx = page_idx % PAGES_PER_BYTE;
        self.data[byte_idx] &= !(1 << bit_idx);
    }

    pub fn num_pages(&self) -> u32 {
        self.num_pages
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(4 + self.data.len());
        buf.extend_from_slice(&self.num_pages.to_le_bytes());
        buf.extend_from_slice(&self.data);
        buf
    }

    pub fn deserialize(data: &[u8]) -> Self {
        if data.len() < 4 {
            return Self::new(0);
        }
        let num_pages = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let vm_data = data[4..].to_vec();
        Self {
            data: vm_data,
            num_pages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visibility_map_new() {
        let vm = VisibilityMap::new(64);
        assert_eq!(vm.num_pages(), 64);
        assert!(!vm.is_page_all_visible(0));
    }

    #[test]
    fn test_mark_and_check() {
        let mut vm = VisibilityMap::new(100);
        vm.mark_all_visible(0);
        vm.mark_all_visible(7);
        vm.mark_all_visible(63);
        vm.mark_all_visible(64);
        vm.mark_all_visible(99);

        assert!(vm.is_page_all_visible(0));
        assert!(vm.is_page_all_visible(7));
        assert!(vm.is_page_all_visible(63));
        assert!(vm.is_page_all_visible(64));
        assert!(vm.is_page_all_visible(99));
        assert!(!vm.is_page_all_visible(1));
        assert!(!vm.is_page_all_visible(50));
    }

    #[test]
    fn test_clear() {
        let mut vm = VisibilityMap::new(10);
        vm.mark_all_visible(5);
        assert!(vm.is_page_all_visible(5));
        vm.clear_all_visible(5);
        assert!(!vm.is_page_all_visible(5));
    }

    #[test]
    fn test_out_of_bounds() {
        let mut vm = VisibilityMap::new(10);
        vm.mark_all_visible(10);
        assert!(!vm.is_page_all_visible(10));
    }

    #[test]
    fn test_roundtrip() {
        let mut vm = VisibilityMap::new(200);
        vm.mark_all_visible(0);
        vm.mark_all_visible(50);
        vm.mark_all_visible(199);

        let serialized = vm.serialize();
        let deserialized = VisibilityMap::deserialize(&serialized);

        assert!(deserialized.is_page_all_visible(0));
        assert!(deserialized.is_page_all_visible(50));
        assert!(deserialized.is_page_all_visible(199));
        assert!(!deserialized.is_page_all_visible(1));
    }
}
