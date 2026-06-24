use crate::types::PageId;
use crate::storage::StorageTrait;
use std::collections::HashMap;
use parking_lot::RwLock;

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
        pages.get(&page_id).cloned().ok_or_else(|| {
            anyhow::anyhow!("Page {} not found in ephemeral storage", page_id.0)
        })
    }

    fn write_page(&self, page_id: PageId, data: &[u8]) -> anyhow::Result<()> {
        let mut pages = self.pages.write();
        pages.insert(page_id, data.to_vec());
        Ok(())
    }
}
