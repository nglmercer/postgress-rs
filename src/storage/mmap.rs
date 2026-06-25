use crate::storage::StorageTrait;
use crate::types::PageId;
use parking_lot::Mutex;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub struct MmapStorage {
    file: Mutex<File>,
    page_size: usize,
}

impl MmapStorage {
    pub async fn open(path: impl AsRef<Path>, page_size: usize) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.set_len(1024 * 1024 * 1024)?;

        Ok(Self {
            file: Mutex::new(file),
            page_size,
        })
    }
}

impl StorageTrait for MmapStorage {
    fn read_page(&self, page_id: PageId) -> anyhow::Result<Vec<u8>> {
        let offset = (page_id.0 as usize) * self.page_size;
        let mut file = self.file.lock();
        file.seek(SeekFrom::Start(offset as u64))?;
        let mut buf = vec![0u8; self.page_size];
        file.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn write_page(&self, page_id: PageId, data: &[u8]) -> anyhow::Result<()> {
        if data.len() > self.page_size {
            anyhow::bail!("data too large for page");
        }
        let offset = (page_id.0 as usize) * self.page_size;
        let mut file = self.file.lock();
        file.seek(SeekFrom::Start(offset as u64))?;
        file.write_all(data)?;
        file.sync_all()?;
        Ok(())
    }

    fn sync_all(&self) -> anyhow::Result<()> {
        let file = self.file.lock();
        file.sync_all()?;
        Ok(())
    }
}
