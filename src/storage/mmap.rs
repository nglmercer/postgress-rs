use crate::types::PageId;
use crate::storage::StorageTrait;
use memmap2::Mmap;
use memmap2::MmapMut;
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use parking_lot::Mutex;

pub struct MmapStorage {
    file: Mutex<File>,
    mmap: Option<Mutex<Mmap>>,
    mmap_mut: Option<Mutex<MmapMut>>,
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
            .open(path)?;
        file.set_len(1024 * 1024 * 1024)?;
        
        // Create the mmap mapping
        let mmap = unsafe { Mmap::map(&file)? };
        let mmap_mut = unsafe { MmapMut::map_mut(&file)? };
        
        Ok(Self {
            file: Mutex::new(file),
            mmap: Some(Mutex::new(mmap)),
            mmap_mut: Some(Mutex::new(mmap_mut)),
            page_size,
        })
    }

    fn mmap(&self) -> &parking_lot::Mutex<memmap2::Mmap> {
        self.mmap.as_ref().expect("MmapStorage::open must initialize mmap")
    }

    fn mmap_mut(&self) -> &parking_lot::Mutex<memmap2::MmapMut> {
        self.mmap_mut.as_ref().expect("MmapStorage::open must initialize mmap_mut")
    }
}

impl StorageTrait for MmapStorage {
    fn read_page(&self, page_id: PageId) -> anyhow::Result<Vec<u8>> {
        let offset = (page_id.0 as usize) * self.page_size;
        let guard = self.mmap().lock();
        let end = std::cmp::min(offset + self.page_size, guard.len());
        Ok(guard[offset..end].to_vec())
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
        
        // Re-mmap to refresh the in-memory view
        drop(file);
        let new_mmap = unsafe { Mmap::map(&self.file.lock())? };
        let new_mmap_mut = unsafe { MmapMut::map_mut(&self.file.lock())? };
        *self.mmap() = new_mmap;
        *self.mmap_mut() = new_mmap_mut;
        Ok(())
    }
}
