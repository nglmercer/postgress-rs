use crate::types::{Oid, PageId};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForkType {
    Main,
    Fsm,
    Vm,
}

impl ForkType {
    pub fn suffix(&self) -> &str {
        match self {
            ForkType::Main => "",
            ForkType::Fsm => "_fsm",
            ForkType::Vm => "_vm",
        }
    }

    pub fn all() -> &'static [ForkType] {
        &[ForkType::Main, ForkType::Fsm, ForkType::Vm]
    }
}

#[derive(Debug, Clone)]
pub struct ForkFile {
    pub rel_oid: Oid,
    pub fork: ForkType,
    pub pages: Vec<PageId>,
}

impl ForkFile {
    pub fn new(rel_oid: Oid, fork: ForkType) -> Self {
        Self {
            rel_oid,
            fork,
            pages: Vec::new(),
        }
    }

    pub fn add_page(&mut self, page_id: PageId) {
        if !self.pages.contains(&page_id) {
            self.pages.push(page_id);
        }
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
}

#[derive(Debug, Clone, Default)]
pub struct RelationForks {
    pub forks: HashMap<ForkType, ForkFile>,
}

impl RelationForks {
    pub fn new(rel_oid: Oid) -> Self {
        let mut forks = HashMap::new();
        for fork in ForkType::all() {
            forks.insert(*fork, ForkFile::new(rel_oid, *fork));
        }
        Self { forks }
    }

    pub fn get_fork(&self, fork: ForkType) -> Option<&ForkFile> {
        self.forks.get(&fork)
    }

    pub fn get_fork_mut(&mut self, fork: ForkType) -> Option<&mut ForkFile> {
        self.forks.get_mut(&fork)
    }

    pub fn add_page(&mut self, fork: ForkType, page_id: PageId) {
        if let Some(fork_file) = self.forks.get_mut(&fork) {
            fork_file.add_page(page_id);
        }
    }

    pub fn main_pages(&self) -> &[PageId] {
        self.forks.get(&ForkType::Main).map(|f| f.pages.as_slice()).unwrap_or(&[])
    }

    pub fn fsm_pages(&self) -> &[PageId] {
        self.forks.get(&ForkType::Fsm).map(|f| f.pages.as_slice()).unwrap_or(&[])
    }

    pub fn vm_pages(&self) -> &[PageId] {
        self.forks.get(&ForkType::Vm).map(|f| f.pages.as_slice()).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fork_type_suffix() {
        assert_eq!(ForkType::Main.suffix(), "");
        assert_eq!(ForkType::Fsm.suffix(), "_fsm");
        assert_eq!(ForkType::Vm.suffix(), "_vm");
    }

    #[test]
    fn test_fork_file_new() {
        let ff = ForkFile::new(Oid(1), ForkType::Main);
        assert_eq!(ff.rel_oid, Oid(1));
        assert_eq!(ff.fork, ForkType::Main);
        assert!(ff.pages.is_empty());
    }

    #[test]
    fn test_fork_file_add_page() {
        let mut ff = ForkFile::new(Oid(1), ForkType::Main);
        ff.add_page(PageId(0));
        ff.add_page(PageId(1));
        ff.add_page(PageId(0)); // duplicate
        assert_eq!(ff.page_count(), 2);
        assert_eq!(ff.pages, vec![PageId(0), PageId(1)]);
    }

    #[test]
    fn test_relation_forks_new() {
        let rf = RelationForks::new(Oid(1));
        assert!(rf.get_fork(ForkType::Main).is_some());
        assert!(rf.get_fork(ForkType::Fsm).is_some());
        assert!(rf.get_fork(ForkType::Vm).is_some());
    }

    #[test]
    fn test_relation_forks_add_page() {
        let mut rf = RelationForks::new(Oid(1));
        rf.add_page(ForkType::Main, PageId(0));
        rf.add_page(ForkType::Main, PageId(1));
        rf.add_page(ForkType::Fsm, PageId(0));
        assert_eq!(rf.main_pages().len(), 2);
        assert_eq!(rf.fsm_pages().len(), 1);
        assert!(rf.vm_pages().is_empty());
    }

    #[test]
    fn test_fork_type_all() {
        assert_eq!(ForkType::all().len(), 3);
    }
}
