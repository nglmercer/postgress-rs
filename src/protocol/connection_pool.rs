use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ConnectionPool {
    max_connections: usize,
    active_connections: Arc<Mutex<usize>>,
    idle_connections: Arc<Mutex<VecDeque<()>>>,
}

impl ConnectionPool {
    pub fn new(max_connections: usize) -> Self {
        Self {
            max_connections,
            active_connections: Arc::new(Mutex::new(0)),
            idle_connections: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub async fn acquire(&self) -> bool {
        let mut active = self.active_connections.lock().await;
        if *active < self.max_connections {
            *active += 1;
            true
        } else {
            false
        }
    }

    pub async fn release(&self) {
        let mut active = self.active_connections.lock().await;
        if *active > 0 {
            *active -= 1;
        }
    }

    pub async fn active_count(&self) -> usize {
        *self.active_connections.lock().await
    }

    pub fn max_connections(&self) -> usize {
        self.max_connections
    }

    pub async fn available(&self) -> usize {
        let active = self.active_count().await;
        self.max_connections.saturating_sub(active)
    }
}

impl Default for ConnectionPool {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_pool_new() {
        let pool = ConnectionPool::new(10);
        assert_eq!(pool.max_connections(), 10);
        assert_eq!(pool.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_connection_pool_acquire_release() {
        let pool = ConnectionPool::new(2);
        assert!(pool.acquire().await);
        assert!(pool.acquire().await);
        assert!(!pool.acquire().await);
        assert_eq!(pool.active_count().await, 2);

        pool.release().await;
        assert_eq!(pool.active_count().await, 1);
        assert!(pool.acquire().await);
    }

    #[tokio::test]
    async fn test_connection_pool_available() {
        let pool = ConnectionPool::new(5);
        assert_eq!(pool.available().await, 5);
        pool.acquire().await;
        assert_eq!(pool.available().await, 4);
    }
}
