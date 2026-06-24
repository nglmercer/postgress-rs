//use crate::types::*;

pub const PAGE_SIZE: usize = 8192;
pub const PAGE_HEADER_SIZE: usize = 24;
pub const LINE_POINTER_SIZE: usize = 4;
pub const MAX_OFFSET: u16 = PAGE_SIZE as u16;

#[derive(Debug, Clone)]
#[repr(C)]
pub struct PageHeader {
    pub pd_lsn: u64,
    pub pd_checksum: u32,
    pub pd_flags: u16,
    pub pd_lower: u16,
    pub pd_upper: u16,
    pub pd_special: u16,
    pub pd_pagesize_version: u16,
    pub pd_prune_xid: u32,
}

impl PageHeader {
    pub fn new() -> Self {
        Self {
            pd_lsn: 0,
            pd_checksum: 0,
            pd_flags: 0,
            pd_lower: PAGE_HEADER_SIZE as u16,
            pd_upper: PAGE_SIZE as u16,
            pd_special: PAGE_SIZE as u16,
            pd_pagesize_version: 0x0802,
            pd_prune_xid: 0,
        }
    }

    pub fn serialize(&self) -> [u8; PAGE_HEADER_SIZE] {
        let mut buf = [0u8; PAGE_HEADER_SIZE];
        buf[0..8].copy_from_slice(&self.pd_lsn.to_le_bytes());
        buf[8..12].copy_from_slice(&self.pd_checksum.to_le_bytes());
        buf[12..14].copy_from_slice(&self.pd_flags.to_le_bytes());
        buf[14..16].copy_from_slice(&self.pd_lower.to_le_bytes());
        buf[16..18].copy_from_slice(&self.pd_upper.to_le_bytes());
        buf[18..20].copy_from_slice(&self.pd_special.to_le_bytes());
        buf[20..22].copy_from_slice(&self.pd_pagesize_version.to_le_bytes());
        buf
    }

    pub fn deserialize(data: &[u8]) -> Self {
        Self {
            pd_lsn: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            pd_checksum: u32::from_le_bytes(data[8..12].try_into().unwrap()),
            pd_flags: u16::from_le_bytes(data[12..14].try_into().unwrap()),
            pd_lower: u16::from_le_bytes(data[14..16].try_into().unwrap()),
            pd_upper: u16::from_le_bytes(data[16..18].try_into().unwrap()),
            pd_special: u16::from_le_bytes(data[18..20].try_into().unwrap()),
            pd_pagesize_version: u16::from_le_bytes(data[20..22].try_into().unwrap()),
            pd_prune_xid: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct LinePointer {
    pub lp_offset: u16,
    pub lp_flags: u16,
}

pub const LP_NORMAL: u16 = 0;
pub const LP_REDIRECT: u16 = 1;
pub const LP_DEAD: u16 = 2;

impl LinePointer {
    pub fn new(offset: u16) -> Self {
        Self {
            lp_offset: offset,
            lp_flags: LP_NORMAL,
        }
    }

    pub fn serialize(&self) -> [u8; LINE_POINTER_SIZE] {
        let mut buf = [0u8; LINE_POINTER_SIZE];
        buf[0..2].copy_from_slice(&self.lp_offset.to_le_bytes());
        buf[2..4].copy_from_slice(&self.lp_flags.to_le_bytes());
        buf
    }

    pub fn deserialize(data: &[u8]) -> Self {
        Self {
            lp_offset: u16::from_le_bytes(data[0..2].try_into().unwrap()),
            lp_flags: u16::from_le_bytes(data[2..4].try_into().unwrap()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HeapPage {
    pub header: PageHeader,
    pub line_pointers: Vec<LinePointer>,
    pub tuples: Vec<Vec<u8>>,
}

impl HeapPage {
    pub fn new() -> Self {
        Self {
            header: PageHeader::new(),
            line_pointers: Vec::new(),
            tuples: Vec::new(),
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut page = vec![0u8; PAGE_SIZE];

        // Write tuples (from end of page backwards) and compute new line pointers
        let mut line_pointers = self.line_pointers.clone();
        let mut tuple_offset = PAGE_SIZE;
        for i in 0..line_pointers.len() {
            let lp = &mut line_pointers[i];
            if lp.lp_flags == LP_NORMAL {
                if i < self.tuples.len() {
                    let tuple_data = &self.tuples[i];
                    if !tuple_data.is_empty() {
                        tuple_offset -= tuple_data.len();
                        lp.lp_offset = tuple_offset as u16;
                        page[tuple_offset..tuple_offset + tuple_data.len()].copy_from_slice(tuple_data);
                    }
                }
            } else {
                lp.lp_offset = 0;
            }
        }

        // Update header fields before serializing the header
        let mut header = self.header.clone();
        header.pd_lower = (PAGE_HEADER_SIZE + line_pointers.len() * LINE_POINTER_SIZE) as u16;
        header.pd_upper = tuple_offset as u16;

        // Write header
        let header_bytes = header.serialize();
        page[0..PAGE_HEADER_SIZE].copy_from_slice(&header_bytes);

        // Write line pointers
        let mut offset = PAGE_HEADER_SIZE;
        for lp in &line_pointers {
            let lp_bytes = lp.serialize();
            page[offset..offset + LINE_POINTER_SIZE].copy_from_slice(&lp_bytes);
            offset += LINE_POINTER_SIZE;
        }

        page
    }

    pub fn deserialize(data: &[u8]) -> Self {
        let header = PageHeader::deserialize(data);

        let num_line_pointers =
            ((header.pd_lower as usize - PAGE_HEADER_SIZE) / LINE_POINTER_SIZE) as usize;
        let mut line_pointers = Vec::with_capacity(num_line_pointers);

        for i in 0..num_line_pointers {
            let offset = PAGE_HEADER_SIZE + i * LINE_POINTER_SIZE;
            let lp = LinePointer::deserialize(&data[offset..offset + LINE_POINTER_SIZE]);
            line_pointers.push(lp);
        }

        let mut tuples = vec![Vec::new(); line_pointers.len()];
        for i in 0..line_pointers.len() {
            let lp = &line_pointers[i];
            if lp.lp_flags == LP_NORMAL {
                let tuple_start = lp.lp_offset as usize;
                let mut tuple_end = header.pd_special as usize;
                for j in (0..i).rev() {
                    if line_pointers[j].lp_flags == LP_NORMAL {
                        tuple_end = line_pointers[j].lp_offset as usize;
                        break;
                    }
                }
                if tuple_start < tuple_end && tuple_end <= PAGE_SIZE {
                    let tuple_data = data[tuple_start..tuple_end].to_vec();
                    tuples[i] = tuple_data;
                }
            }
        }

        Self {
            header,
            line_pointers,
            tuples,
        }
    }

    pub fn add_tuple(&mut self, tuple_data: &[u8]) -> Option<u16> {
        let tuple_len = tuple_data.len();
        
        let mut slot_index = None;
        for i in 0..self.line_pointers.len() {
            if self.line_pointers[i].lp_flags == LP_DEAD {
                slot_index = Some(i as u16);
                break;
            }
        }

        let new_lower = if slot_index.is_none() {
            self.header.pd_lower as usize + LINE_POINTER_SIZE
        } else {
            self.header.pd_lower as usize
        };
        let new_upper = self.header.pd_upper as usize - tuple_len;

        if new_lower > new_upper {
            return None;
        }

        let slot = if let Some(idx) = slot_index {
            self.line_pointers[idx as usize] = LinePointer::new(new_upper as u16);
            if (idx as usize) < self.tuples.len() {
                self.tuples[idx as usize] = tuple_data.to_vec();
            }
            idx
        } else {
            let idx = self.line_pointers.len() as u16;
            let lp = LinePointer::new(new_upper as u16);
            self.line_pointers.push(lp);
            self.tuples.push(tuple_data.to_vec());
            self.header.pd_lower = new_lower as u16;
            idx
        };

        self.header.pd_upper = new_upper as u16;
        Some(slot)
    }

    pub fn compact(&mut self) {
        for i in 0..self.line_pointers.len() {
            let lp = &self.line_pointers[i];
            if lp.lp_flags == LP_DEAD {
                if i < self.tuples.len() {
                    self.tuples[i] = Vec::new();
                }
            }
        }

        self.header.pd_lower = (PAGE_HEADER_SIZE + self.line_pointers.len() * LINE_POINTER_SIZE) as u16;

        let mut total_len = 0;
        for i in 0..self.line_pointers.len() {
            if self.line_pointers[i].lp_flags == LP_NORMAL {
                if i < self.tuples.len() {
                    total_len += self.tuples[i].len();
                }
            }
        }
        self.header.pd_upper = (PAGE_SIZE - total_len) as u16;
    }

    pub fn free_space(&self) -> usize {
        (self.header.pd_upper - self.header.pd_lower) as usize
    }

    pub fn tuple_count(&self) -> usize {
        self.line_pointers.len()
    }

    pub fn get_tuple(&self, slot: u16) -> Option<&[u8]> {
        if (slot as usize) < self.line_pointers.len() {
            let lp = &self.line_pointers[slot as usize];
            if lp.lp_flags == LP_NORMAL {
                if (slot as usize) < self.tuples.len() {
                    Some(&self.tuples[slot as usize])
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_header_roundtrip() {
        let header = PageHeader::new();
        let bytes = header.serialize();
        let deserialized = PageHeader::deserialize(&bytes);
        assert_eq!(header.pd_lsn, deserialized.pd_lsn);
        assert_eq!(header.pd_lower, deserialized.pd_lower);
        assert_eq!(header.pd_upper, deserialized.pd_upper);
    }

    #[test]
    fn test_line_pointer_roundtrip() {
        let lp = LinePointer::new(100);
        let bytes = lp.serialize();
        let deserialized = LinePointer::deserialize(&bytes);
        assert_eq!(lp.lp_offset, deserialized.lp_offset);
        assert_eq!(lp.lp_flags, deserialized.lp_flags);
    }

    #[test]
    fn test_heap_page_add_tuple() {
        let mut page = HeapPage::new();
        let tuple_data = vec![1, 2, 3, 4, 5];
        let slot = page.add_tuple(&tuple_data);
        assert!(slot.is_some());
        assert_eq!(slot.unwrap(), 0);
        assert_eq!(page.tuple_count(), 1);
    }

    #[test]
    fn test_heap_page_multiple_tuples() {
        let mut page = HeapPage::new();
        for i in 0..5 {
            let tuple_data = vec![i; 100];
            let slot = page.add_tuple(&tuple_data);
            assert!(slot.is_some());
        }
        assert_eq!(page.tuple_count(), 5);
    }

    #[test]
    fn test_heap_page_roundtrip() {
        let mut page = HeapPage::new();
        page.add_tuple(&vec![1, 2, 3]);
        page.add_tuple(&vec![4, 5, 6]);

        let serialized = page.serialize();
        let deserialized = HeapPage::deserialize(&serialized);

        assert_eq!(deserialized.tuple_count(), 2);
    }

    #[test]
    fn test_heap_page_compaction_and_reuse() {
        let mut page = HeapPage::new();
        let s0 = page.add_tuple(&vec![1, 1, 1]).unwrap();
        let s1 = page.add_tuple(&vec![2, 2, 2]).unwrap();
        let s2 = page.add_tuple(&vec![3, 3, 3]).unwrap();
        
        assert_eq!(page.free_space(), PAGE_SIZE - PAGE_HEADER_SIZE - 3 * LINE_POINTER_SIZE - 9);

        // Mark s1 as dead
        page.line_pointers[s1 as usize].lp_flags = LP_DEAD;
        
        // Compact
        page.compact();
        
        // Verify free space has reclaimed the 3 bytes of tuple 2 (but line pointer slot is still kept)
        assert_eq!(page.free_space(), PAGE_SIZE - PAGE_HEADER_SIZE - 3 * LINE_POINTER_SIZE - 6);

        // Serialize and deserialize, verify it maintains the dead slot
        let bytes = page.serialize();
        let mut deserialized = HeapPage::deserialize(&bytes);
        assert_eq!(deserialized.line_pointers[s1 as usize].lp_flags, LP_DEAD);

        // Now add another tuple, verifying it reuses the dead slot s1
        let s3 = deserialized.add_tuple(&vec![4, 4, 4, 4]).unwrap();
        assert_eq!(s3, s1);
        assert_eq!(deserialized.line_pointers[s3 as usize].lp_flags, LP_NORMAL);
        assert_eq!(deserialized.get_tuple(s3).unwrap(), &vec![4, 4, 4, 4]);
    }
}
