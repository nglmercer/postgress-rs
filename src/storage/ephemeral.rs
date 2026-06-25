use crate::storage::StorageTrait;
use crate::types::PageId;
use parking_lot::RwLock;
use std::collections::HashMap;

pub struct EphemeralStorage {
    pages: RwLock<HashMap<PageId, Vec<u8>>>,
}

impl EphemeralStorage {
    pub fn new() -> Self {
        Self {
            pages: RwLock::new(HashMap::new()),
        }
    }
}

impl StorageTrait for EphemeralStorage {
    fn read_page(&self, page_id: PageId) -> anyhow::Result<Vec<u8>> {
        let pages = self.pages.read();
        Ok(pages
            .get(&page_id)
            .cloned()
            .unwrap_or_else(|| vec![0u8; 8192]))
    }

    fn write_page(&self, page_id: PageId, data: &[u8]) -> anyhow::Result<()> {
        let mut pages = self.pages.write();
        pages.insert(page_id, data.to_vec());
        Ok(())
    }

    fn sync_all(&self) -> anyhow::Result<()> {
        Ok(())
    }
}
