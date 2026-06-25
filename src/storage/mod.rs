use crate::types::{Oid, PageId};

pub mod ephemeral;
pub mod fork;
pub mod heap_page;
pub mod mmap;
pub mod toast;
pub mod visibility_map;

use fork::ForkType;

pub fn fork_page_id(rel_oid: Oid, fork: ForkType, block: u32) -> PageId {
    let fork_offset = match fork {
        ForkType::Main => 0,
        ForkType::Fsm => 1_000_000_000,
        ForkType::Vm => 2_000_000_000,
    };
    PageId(rel_oid.0.wrapping_add(fork_offset).wrapping_add(block))
}

pub trait StorageTrait: Send + Sync {
    fn read_page(&self, page_id: PageId) -> anyhow::Result<Vec<u8>>;
    fn write_page(&self, page_id: PageId, data: &[u8]) -> anyhow::Result<()>;
    fn sync_all(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn read_page_fork(&self, rel_oid: Oid, fork: ForkType, block: u32) -> anyhow::Result<Vec<u8>> {
        let page_id = fork_page_id(rel_oid, fork, block);
        self.read_page(page_id)
    }
    fn write_page_fork(
        &self,
        rel_oid: Oid,
        fork: ForkType,
        block: u32,
        data: &[u8],
    ) -> anyhow::Result<()> {
        let page_id = fork_page_id(rel_oid, fork, block);
        self.write_page(page_id, data)
    }
}
