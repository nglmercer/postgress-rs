pub mod worker;

use parking_lot::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::Barrier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParallelPhase {
    ScanComplete,
    BuildComplete,
    ProbeComplete,
}

pub struct ParallelContext {
    pub n_workers: u32,
    pub shared_state: Arc<Mutex<ParallelSharedState>>,
    pub barrier: Arc<Barrier>,
}

pub struct ParallelSharedState {
    pub tuples: Vec<Vec<String>>,
    pub phase: ParallelPhase,
    pub error: Option<String>,
}

impl ParallelContext {
    pub fn new(n_workers: u32) -> Self {
        Self {
            n_workers,
            shared_state: Arc::new(Mutex::new(ParallelSharedState {
                tuples: Vec::new(),
                phase: ParallelPhase::ScanComplete,
                error: None,
            })),
            barrier: Arc::new(Barrier::new(n_workers as usize)),
        }
    }

    pub fn add_tuples(&self, tuples: Vec<Vec<String>>) {
        let mut state = self.shared_state.lock();
        state.tuples.extend(tuples);
    }

    pub fn get_tuples(&self) -> Vec<Vec<String>> {
        let state = self.shared_state.lock();
        state.tuples.clone()
    }

    pub fn set_error(&self, error: String) {
        let mut state = self.shared_state.lock();
        state.error = Some(error);
    }

    pub fn has_error(&self) -> bool {
        let state = self.shared_state.lock();
        state.error.is_some()
    }
}

pub struct ParallelWorker {
    pub worker_id: u32,
    pub join_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ParallelWorker {
    pub fn new(worker_id: u32) -> Self {
        Self {
            worker_id,
            join_handle: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_context_new() {
        let ctx = ParallelContext::new(4);
        assert_eq!(ctx.n_workers, 4);
        assert!(!ctx.has_error());
    }

    #[test]
    fn test_parallel_shared_state() {
        let ctx = ParallelContext::new(2);
        ctx.add_tuples(vec![vec!["a".to_string()], vec!["b".to_string()]]);
        let tuples = ctx.get_tuples();
        assert_eq!(tuples.len(), 2);
    }

    #[test]
    fn test_parallel_error() {
        let ctx = ParallelContext::new(2);
        assert!(!ctx.has_error());
        ctx.set_error("test error".to_string());
        assert!(ctx.has_error());
    }
}
