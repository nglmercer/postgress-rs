use crate::types::PageId;

pub mod ephemeral;
pub mod mmap;

pub trait StorageTrait: Send + Sync {
    fn read_page(&self, page_id: PageId) -> anyhow::Result<Vec<u8>>;
    fn write_page(&self, page_id: PageId, data: &[u8]) -> anyhow::Result<()>;
}
