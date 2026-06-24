use std::sync::Arc;
use super::ParallelContext;

pub struct ParallelExecutor {
    n_workers: u32,
    context: Arc<ParallelContext>,
}

impl ParallelExecutor {
    pub fn new(n_workers: u32) -> Self {
        Self {
            n_workers,
            context: Arc::new(ParallelContext::new(n_workers)),
        }
    }

    pub async fn execute_parallel<F>(
        &self,
        task: F,
    ) -> anyhow::Result<Vec<Vec<String>>>
    where
        F: Fn(u32, Arc<ParallelContext>) -> Vec<Vec<String>> + Send + Sync + Clone + 'static,
    {
        let mut handles = Vec::new();

        for worker_id in 0..self.n_workers {
            let ctx = Arc::clone(&self.context);
            let task = task.clone();
            let handle = tokio::spawn(async move {
                let result = task(worker_id, ctx.clone());
                ctx.add_tuples(result);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await?;
        }

        if self.context.has_error() {
            let state = self.context.shared_state.lock();
            return Err(anyhow::anyhow!("Parallel execution failed: {}", state.error.as_ref().unwrap()));
        }

        Ok(self.context.get_tuples())
    }

    pub fn get_context(&self) -> Arc<ParallelContext> {
        Arc::clone(&self.context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parallel_executor_basic() {
        let executor = ParallelExecutor::new(4);
        let result = executor.execute_parallel(|worker_id, _ctx| {
            vec![vec![format!("worker_{}", worker_id)]]
        }).await.unwrap();

        assert_eq!(result.len(), 4);
    }

    #[tokio::test]
    async fn test_parallel_executor_empty() {
        let executor = ParallelExecutor::new(2);
        let result = executor.execute_parallel(|_worker_id, _ctx| {
            vec![]
        }).await.unwrap();

        assert!(result.is_empty());
    }
}
