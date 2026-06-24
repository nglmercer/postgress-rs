use crate::types::Oid;

pub const TOAST_THRESHOLD: usize = 2040;
pub const TOAST_MAX_CHUNK_SIZE: usize = 1996;

#[derive(Debug, Clone)]
pub struct ToastPointer {
    pub raw_size: u32,
    pub stored_size: u32,
    pub toast_table_oid: Oid,
    pub chunk_ids: Vec<u32>,
}

impl ToastPointer {
    pub fn is_toasted(&self) -> bool {
        self.raw_size > TOAST_THRESHOLD as u32
    }
}

pub struct ToastStorage;

impl ToastStorage {
    pub fn maybe_toast(data: &[u8], rel_oid: Oid) -> Option<ToastPointer> {
        if data.len() <= TOAST_THRESHOLD {
            return None;
        }

        let chunk_count = (data.len() + TOAST_MAX_CHUNK_SIZE - 1) / TOAST_MAX_CHUNK_SIZE;
        let toast_table_oid = Oid(rel_oid.0.wrapping_add(1));

        let mut chunk_ids = Vec::with_capacity(chunk_count);
        for i in 0..chunk_count {
            chunk_ids.push(i as u32);
        }

        Some(ToastPointer {
            raw_size: data.len() as u32,
            stored_size: data.len() as u32,
            toast_table_oid,
            chunk_ids,
        })
    }

    pub fn chunk_data(data: &[u8], chunk_index: usize) -> Option<&[u8]> {
        let start = chunk_index * TOAST_MAX_CHUNK_SIZE;
        if start >= data.len() {
            return None;
        }
        let end = std::cmp::min(start + TOAST_MAX_CHUNK_SIZE, data.len());
        Some(&data[start..end])
    }

    pub fn detoast_chunks(chunks: &[(u32, Vec<u8>)], raw_size: u32) -> Vec<u8> {
        let mut result = Vec::with_capacity(raw_size as usize);
        let mut sorted_chunks = chunks.to_vec();
        sorted_chunks.sort_by_key(|c| c.0);

        for (_, chunk_data) in sorted_chunks {
            result.extend_from_slice(&chunk_data);
        }
        result.truncate(raw_size as usize);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_toast_small_data() {
        let data = vec![0u8; 100];
        assert!(ToastStorage::maybe_toast(&data, Oid(1)).is_none());
    }

    #[test]
    fn test_toast_large_data() {
        let data = vec![0u8; 3000];
        let ptr = ToastStorage::maybe_toast(&data, Oid(1)).unwrap();
        assert!(ptr.is_toasted());
        assert_eq!(ptr.raw_size, 3000);
        assert_eq!(ptr.chunk_ids.len(), 2);
        assert_eq!(ptr.toast_table_oid, Oid(2));
    }

    #[test]
    fn test_chunk_data() {
        let data = vec![0u8; 4000];
        let chunk0 = ToastStorage::chunk_data(&data, 0).unwrap();
        assert_eq!(chunk0.len(), TOAST_MAX_CHUNK_SIZE);

        let chunk1 = ToastStorage::chunk_data(&data, 1).unwrap();
        assert_eq!(chunk1.len(), TOAST_MAX_CHUNK_SIZE);

        let chunk2 = ToastStorage::chunk_data(&data, 2).unwrap();
        assert_eq!(chunk2.len(), 4000 - 2 * TOAST_MAX_CHUNK_SIZE);

        assert!(ToastStorage::chunk_data(&data, 3).is_none());
    }

    #[test]
    fn test_detoast_chunks() {
        let chunks = vec![
            (1u32, vec![1, 2, 3]),
            (0u32, vec![4, 5, 6]),
        ];
        let result = ToastStorage::detoast_chunks(&chunks, 6);
        assert_eq!(result, vec![4, 5, 6, 1, 2, 3]);
    }

    #[test]
    fn test_toast_threshold_boundary() {
        let data_below = vec![0u8; TOAST_THRESHOLD];
        assert!(ToastStorage::maybe_toast(&data_below, Oid(1)).is_none());

        let data_above = vec![0u8; TOAST_THRESHOLD + 1];
        assert!(ToastStorage::maybe_toast(&data_above, Oid(1)).is_some());
    }
}
